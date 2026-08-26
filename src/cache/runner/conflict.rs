//! Reporting what a refetch would cost.
//!
//! Split from the rest of the runner because it is the half that reads the *outbox* rather than the
//! version store — the two questions decision 014 insists on answering together, from two different
//! stores.

use super::{ClassifiedConflict, InvalidationRunner, PendingConflict, StaleEntity};
use crate::cache::{CacheVersionStore, EntityState};
use crate::entity::EntityRegistry;
use crate::error::Error;
use crate::record::OutboxRecord;
use crate::store::OutboxStore;

impl<S, R> InvalidationRunner<S, R>
where
    S: CacheVersionStore,
    R: EntityRegistry,
{
    /// Every stale entity, and whether refetching it would discard unsent local work.
    ///
    /// Without a classifier core cannot attribute pending records to entities, so a non-empty
    /// outbox yields [`PendingConflict::Unattributed`] for every stale entity. Use
    /// [`stale_classified`](InvalidationRunner::stale_classified) to narrow it.
    ///
    /// The two facts arrive together on purpose. The source's listener refetches eagerly and never
    /// consults the outbox, so nothing in its call path *could* have decided otherwise; making the
    /// cost visible at the moment staleness is read means an application has to actively ignore it
    /// to hit the same bug.
    ///
    /// # Errors
    ///
    /// A storage failure from either store.
    pub async fn stale<O: OutboxStore>(
        &self,
        outbox: &O,
    ) -> Result<Vec<StaleEntity<R::Key>>, Error> {
        let pending = outbox.pending_count().await?;
        let conflict = if pending == 0 {
            PendingConflict::None
        } else {
            PendingConflict::Unattributed { pending }
        };

        Ok(self
            .stale_states()
            .await?
            .into_iter()
            .map(|(key, state)| StaleEntity {
                key,
                version: state.version,
                conflict,
            })
            .collect())
    }

    /// Every stale entity, with conflict attributed per entity by `classify`.
    ///
    /// `classify` is handed each pending record and returns the entity it would affect, or `None`
    /// if it cannot tell. Core never interprets the record itself — the body is uninterpreted JSON
    /// by `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, so `method`, `path`, and
    /// `op` are the caller's to read.
    ///
    /// # When the queue is larger than the scan
    ///
    /// If more records are pending than [`with_conflict_scan`](InvalidationRunner::with_conflict_scan)
    /// allows, every entity's conflict degrades to [`PendingConflict::Unattributed`] rather than
    /// reporting a clean result from a partial view. A false "nothing is queued" is the answer that
    /// loses data, so an incomplete scan declines to give one.
    ///
    /// # Errors
    ///
    /// A storage failure from either store.
    pub async fn stale_classified<O, F>(
        &self,
        outbox: &O,
        classify: F,
    ) -> Result<Vec<StaleEntity<R::Key>>, Error>
    where
        O: OutboxStore,
        F: Fn(&OutboxRecord) -> Option<R::Key>,
    {
        let classified = self.classify(outbox, classify).await?;
        let stale = self.stale_states().await?;

        Ok(stale
            .into_iter()
            .map(|(key, state)| {
                let conflict = if !classified.complete {
                    PendingConflict::Unattributed {
                        pending: classified.unattributed,
                    }
                } else {
                    match classified.attributed.iter().find(|(k, _)| *k == key) {
                        Some((_, pending)) => PendingConflict::ForEntity { pending: *pending },
                        None if classified.unattributed > 0 => PendingConflict::Unattributed {
                            pending: classified.unattributed,
                        },
                        None => PendingConflict::None,
                    }
                };
                StaleEntity {
                    key,
                    version: state.version,
                    conflict,
                }
            })
            .collect())
    }

    /// Group the pending queue by entity, using a caller-supplied classifier.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn classify<O, F>(
        &self,
        outbox: &O,
        classify: F,
    ) -> Result<ClassifiedConflict<R::Key>, Error>
    where
        O: OutboxStore,
        F: Fn(&OutboxRecord) -> Option<R::Key>,
    {
        let total = outbox.pending_count().await?;
        // One over the limit, so that "there is more than I looked at" is distinguishable from
        // "I looked at exactly all of it".
        let records = outbox.pending_batch(self.conflict_scan).await?;
        let complete = total <= self.conflict_scan && records.len() == total;

        let mut attributed: Vec<(R::Key, usize)> = Vec::new();
        let mut unattributed = 0;
        for record in &records {
            match classify(record) {
                Some(key) => match attributed.iter_mut().find(|(seen, _)| *seen == key) {
                    Some((_, count)) => *count += 1,
                    None => attributed.push((key, 1)),
                },
                None => unattributed += 1,
            }
        }

        // An incomplete scan cannot claim anything is unattributed-free, so it reports the whole
        // queue as the unattributed count.
        if !complete {
            unattributed = total;
        }

        Ok(ClassifiedConflict {
            attributed,
            unattributed,
            complete,
        })
    }

    /// Every entity currently owing a refetch, parsed back into the application's key type.
    ///
    /// Stored names the registry no longer models are skipped rather than reported. A durable store
    /// outlives the build that wrote it, so a row for a retired entity is expected — and the
    /// application could not act on one anyway, which is the same reasoning that makes an unknown
    /// name on the wire a report rather than an error
    /// (`wiki/decisions/013-unknown-entity-name.decision.md`).
    async fn stale_states(&self) -> Result<Vec<(R::Key, EntityState)>, Error> {
        Ok(self
            .store
            .all_states()
            .await?
            .into_iter()
            .filter(|(_, state)| state.stale)
            .filter_map(|(entity, state)| self.registry.parse(&entity).map(|key| (key, state)))
            .collect())
    }
}
