//! Applying invalidations, reconciling on reconnect, and reporting pull conflict.

use crate::cache::{compare, CacheVersionStore, EntityState, InvalidationEvent, VersionUpdate};
use crate::entity::{EntityKey, EntityRegistry};
use crate::error::Error;

/// How many pending records a classified conflict scan will read.
///
/// Matches [`DEFAULT_BATCH_LIMIT`](crate::runner::DEFAULT_BATCH_LIMIT) deliberately: the scan is
/// answering a question about the same queue a sync pass reads, and two different defaults would
/// invite the assumption that one of them is a coincidence.
pub const DEFAULT_CONFLICT_SCAN: usize = crate::runner::DEFAULT_BATCH_LIMIT;

mod conflict;
mod report;

pub use report::{ClassifiedConflict, InvalidationReport, PendingConflict, StaleEntity};

/// Applies invalidations to a version store.
///
/// Store and registry are generic parameters rather than trait objects, for the same reason
/// [`SyncRunner`](crate::runner::SyncRunner) takes them that way: `async fn` in trait is not
/// dyn-compatible.
pub struct InvalidationRunner<S, R> {
    store: S,
    registry: R,
    conflict_scan: usize,
}

impl<S, R> InvalidationRunner<S, R>
where
    S: CacheVersionStore,
    R: EntityRegistry,
{
    /// Build a runner over a version store and the application's registry.
    pub fn new(store: S, registry: R) -> Self {
        Self {
            store,
            registry,
            conflict_scan: DEFAULT_CONFLICT_SCAN,
        }
    }

    /// Set how many pending records a classified conflict scan reads.
    ///
    /// A limit of zero would attribute nothing, so it is clamped to one.
    #[must_use]
    pub fn with_conflict_scan(mut self, limit: usize) -> Self {
        self.conflict_scan = limit.max(1);
        self
    }

    /// The version store this runner drives.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The registry this runner parses wire names with.
    pub fn registry(&self) -> &R {
        &self.registry
    }

    /// Apply invalidation events.
    ///
    /// Events for the same entity collapse: the highest version wins, which is what makes applying
    /// a burst of events equivalent to applying the last one. The whole batch commits in a single
    /// [`put`](CacheVersionStore::put).
    ///
    /// # Errors
    ///
    /// A storage failure. An unparseable entity name is not one — it is reported in
    /// [`InvalidationReport::unknown`].
    pub async fn apply(
        &self,
        events: &[InvalidationEvent],
    ) -> Result<InvalidationReport<R::Key>, Error> {
        let mut named = Vec::with_capacity(events.len());
        let mut report = InvalidationReport::default();

        for event in events {
            match self.registry.parse(&event.entity) {
                Some(key) => named.push((key, event.version)),
                None => report.unknown.push(event.entity.clone()),
            }
        }

        self.reconcile_pairs(named, &mut report).await?;
        Ok(report)
    }

    /// Reconcile against the server's full version map, as after a reconnect.
    ///
    /// Takes wire names because that is what a `GET /versions`-shaped endpoint returns. Unknown
    /// names are reported rather than dropped, which is the divergence from the source: its
    /// reconnect path filters them out of the map with no log at all, so a client whose registry
    /// has drifted reconciles against a silently truncated view and reports success.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn reconcile(
        &self,
        server_versions: &[(String, u64)],
    ) -> Result<InvalidationReport<R::Key>, Error> {
        let events: Vec<InvalidationEvent> = server_versions
            .iter()
            .map(|(entity, version)| InvalidationEvent::new(entity.clone(), *version))
            .collect();
        self.apply(&events).await
    }

    /// Apply already-parsed `(key, version)` pairs.
    async fn reconcile_pairs(
        &self,
        pairs: Vec<(R::Key, u64)>,
        report: &mut InvalidationReport<R::Key>,
    ) -> Result<(), Error> {
        // Collapse repeats to the highest version first, so a burst of events for one entity is one
        // decision and one write rather than a sequence whose intermediate states could be
        // interrupted by a crash.
        let mut highest: Vec<(R::Key, u64)> = Vec::with_capacity(pairs.len());
        for (key, version) in pairs {
            match highest.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, current)) => *current = (*current).max(version),
                None => highest.push((key, version)),
            }
        }

        let mut updates = Vec::with_capacity(highest.len());
        for (key, version) in highest {
            let local = self.store.state(key.as_str()).await?;
            match compare(local, version) {
                VersionUpdate::Updated => {
                    updates.push((key.as_str().to_string(), EntityState::stale_at(version)));
                    report.marked_stale.push(key);
                }
                // Reset to zero rather than down to the server's number. Both ends can agree on
                // zero; adopting a version the client has already passed cannot be distinguished
                // later from having legitimately reached it.
                VersionUpdate::NeedsReset => {
                    updates.push((key.as_str().to_string(), EntityState::stale_at(0)));
                    report.needs_reset.push(key);
                }
                // Deliberately writes nothing. An entity already stale at this version stays
                // stale — the refetch is still owed, and rewriting the row would only risk
                // clearing it.
                VersionUpdate::NoChange => report.unchanged.push(key),
            }
        }

        if !updates.is_empty() {
            self.store.put(&updates).await?;
        }
        Ok(())
    }

    /// Record that a refetch for `key` completed.
    ///
    /// Clears staleness and leaves the version alone, which is the source's `mark_fresh` semantics
    /// and the reason staleness has to be durable: the version already matches the server, so
    /// nothing else remembers whether the data behind it was actually fetched.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn mark_fresh(&self, key: &R::Key) -> Result<(), Error> {
        let state = self.store.state(key.as_str()).await?;
        self.store
            .put(&[(
                key.as_str().to_string(),
                EntityState::fresh_at(state.version),
            )])
            .await
    }

    /// Mark every *registered* entity stale.
    ///
    /// The source's error-recovery hammer, and the operation that makes
    /// [`EntityRegistry::all`](crate::entity::EntityRegistry::all) necessary — a bare string key has
    /// no way to enumerate what "every entity" means.
    ///
    /// Entities outside the registry are not marked, which is consistent rather than a gap: the
    /// application cannot refetch what it does not model.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn mark_all_stale(&self) -> Result<(), Error> {
        let mut updates = Vec::with_capacity(self.registry.all().len());
        for key in self.registry.all() {
            let state = self.store.state(key.as_str()).await?;
            updates.push((
                key.as_str().to_string(),
                EntityState::stale_at(state.version),
            ));
        }
        if !updates.is_empty() {
            self.store.put(&updates).await?;
        }
        Ok(())
    }
}
