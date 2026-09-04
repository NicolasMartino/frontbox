//! The invalidation half: the registry, the sources, and one round of polling.
//!
//! **The first application code that has ever called `InvalidationRunner::apply`.** D2 built the
//! cache runtime in 2026-08-27 and D5 gave it two durable backends; until D4d, nothing outside the
//! crates that define it had held it. Decision 038 is explicit that this is the point — the seam
//! above it is built here, against a real application, before anything is promoted into core.

use frontbox::{
    CacheVersionStore, EntityRegistry, EntityState, Error, InvalidationEvent, InvalidationRunner,
    SliceRegistry,
};

use super::identity::USER;
use super::{TodoApp, TODO};
use crate::backend::VersionStore;
use crate::invalidation::{Dropped, InvalidationSource, InvalidationTick};
use crate::trace;

/// The registry this application models.
///
/// Two entities, which is what makes per-source semantics testable at all: one source can drop and
/// the other's entity has to stay fresh. With one entity, "only this source's entities" and "every
/// entity" are the same set and a bug between them is invisible.
pub fn registry() -> SliceRegistry<&'static str> {
    SliceRegistry::new([TODO, USER])
}

/// The runner over this scope's durable cache versions.
pub type Cache = InvalidationRunner<VersionStore, SliceRegistry<&'static str>>;

impl TodoApp {
    /// The cache runtime, for a caller that wants to ask what is stale.
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Poll every source whose staleness budget has run out.
    ///
    /// What a cadence loop calls. See [`refresh_now`](TodoApp::refresh_now) for what a button
    /// calls.
    ///
    /// # Errors
    ///
    /// A storage failure. A source that could not answer is not one — it is reported in
    /// [`InvalidationTick::dropped`].
    pub async fn poll_invalidation(&self) -> Result<InvalidationTick, Error> {
        let due = self.sources.due(self.clock.now_ms());
        trace::log(format!("invalidation due sources={}", due.len()));
        self.poll_sources(due).await
    }

    /// Poll every source, due or not.
    ///
    /// # The refresh button, and where the plan and the build disagreed
    ///
    /// `wiki/plans/d4d-multi-domain-trial.plan.md` calls the refresh button "the second of the two
    /// sources this deliverable builds". It is not one, and the attempt to make it one is what
    /// showed why: **a button has no versions to deliver.** A source's job is to answer *what
    /// changed*, and the only honest answer a button can give is "go and look" — which is a
    /// trigger, not a delivery.
    ///
    /// So the two source implementations are [`ManualSource`] and [`VersionPollSource`], which is
    /// what decision 038's bar actually asks for — push-shaped and pull-shaped — and the button
    /// drives the same seam from outside rather than sitting inside it. The observation the plan
    /// wanted still holds and is still worth having: pressing refresh converges through exactly the
    /// path the cadence uses, with no second code path to keep in step.
    ///
    /// [`ManualSource`]: crate::invalidation::ManualSource
    /// [`VersionPollSource`]: crate::invalidation::VersionPollSource
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn refresh_now(&self) -> Result<InvalidationTick, Error> {
        trace::log("invalidation refresh_now");
        self.poll_sources(self.sources.all()).await
    }

    async fn poll_sources(&self, which: Vec<usize>) -> Result<InvalidationTick, Error> {
        let now = self.clock.now_ms();
        let mut tick = InvalidationTick::default();
        let mut events: Vec<InvalidationEvent> = Vec::new();
        let mut observed_known = Vec::new();

        for index in which {
            let Some(source) = self.sources.get(index) else {
                continue;
            };
            // Stamped before the poll, and whether it succeeds or not. A source that is down would
            // otherwise be due on every tick, turning an outage into a request flood exactly when
            // the service can least afford one.
            self.sources.mark_polled(index, now);
            let source_name = source.name().to_owned();
            trace::log(format!(
                "invalidation poll source={} entities={:?}",
                source_name,
                source.entities()
            ));
            tick.polled.push(source_name.clone());

            match source.poll().await {
                Ok(mut delivered) => {
                    trace::log(format!(
                        "invalidation source={} delivered={}",
                        source_name,
                        delivered.len()
                    ));
                    for event in &delivered {
                        if self.cache.registry().parse(&event.entity).is_some()
                            && !observed_known.contains(&event.entity)
                        {
                            observed_known.push(event.entity.clone());
                        }
                    }
                    events.append(&mut delivered);
                }
                Err(failure) => {
                    let entities: Vec<String> =
                        source.entities().iter().map(|e| (*e).to_owned()).collect();
                    // **Only this source's entities.** The other service's are still being vouched
                    // for by its own source, and marking them would turn one outage into a full
                    // refetch — which is the per-source reconnect semantic decision 038 names as
                    // the reason more than one origin is a promotion criterion.
                    self.mark_stale(&entities).await?;
                    trace::log(format!(
                        "invalidation source={} dropped reason={} stale={:?}",
                        source_name, failure, entities
                    ));
                    tick.dropped.push(Dropped {
                        source: source_name,
                        reason: failure.to_string(),
                        entities,
                    });
                }
            }
        }

        let report = self.cache.apply(&events).await?;
        tick.changed = report
            .marked_stale
            .iter()
            .map(|entity| (*entity).to_owned())
            .collect();
        for entity in observed_known {
            if !tick.changed.contains(&entity) {
                tick.changed.push(entity);
            }
        }
        tick.unknown = report.unknown;
        trace::log(format!(
            "invalidation applied changed={:?} unknown={:?} dropped={}",
            tick.changed,
            tick.unknown,
            tick.dropped.len()
        ));
        Ok(tick)
    }

    /// Flag entities stale without changing what version they were last known at.
    ///
    /// # Why not `mark_all_stale`
    ///
    /// Core has one, and it is the wrong tool here: it flags every entity the registry models,
    /// which is exactly the over-reach a per-source drop must avoid. There is no core call for
    /// "these entities", and this needs none — reading each state and writing it back with `stale`
    /// set is four lines against `CacheVersionStore`, and it keeps the version, which is the part
    /// that matters. A drop means *I can no longer vouch for this*, not *I have forgotten what I
    /// knew*.
    async fn mark_stale(&self, entities: &[String]) -> Result<(), Error> {
        let mut updates = Vec::with_capacity(entities.len());
        for entity in entities {
            let state = self.cache.store().state(entity).await?;
            updates.push((entity.clone(), EntityState::from_parts(state.version, true)));
        }
        self.cache.store().put(&updates).await
    }
}
