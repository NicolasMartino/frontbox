//! The inbound invalidation seam, built here rather than in core.
//!
//! # Why this lives in an example crate
//!
//! Decision 038. Core and the adapter gain nothing yet: no `InvalidationSource` trait, no polling
//! loop, no resume cursor, no server contract for a versions endpoint. **Invalidation has had zero
//! consumers, ever** — `InvalidationRunner` and `InvalidationEvent` appear nowhere outside the
//! crates that define them — and designing the inbound seam in the abstract would be the one place
//! this project guessed at an API with no application holding it.
//!
//! What core already owns and keeps: the event type, comparison by equality (decision 021), the
//! durable `(version, stale)` pair (decision 015), and the pending-write conflict report
//! (decision 014). This module is the part above those, and promotion is a later decision made
//! against the bar decision 038 wrote down.
//!
//! # A staleness budget, not a poll schedule
//!
//! A caller means "this entity may be up to N seconds out of date". It does *not* mean "issue a
//! request every N seconds", and the difference is forced by the servers: one
//! `GET /api/v1/versions` answers for **every** entity a service owns, so a literal per-entity
//! schedule is not satisfiable without N times the requests.
//!
//! A budget is satisfiable by any source, and over-delivering against it is never wrong. So a
//! source polls at the tightest budget among its own entities and the rest are simply fresher than
//! they were promised (`wiki/proposals/invalidation-delivery.proposal.md`).

use std::cell::RefCell;

use frontbox::{CacheVersion, Error, InvalidationEvent};

use crate::trace;

/// Where invalidation events come from.
///
/// # Two implementations, because a seam with one is a wrapper
///
/// [`ManualSource`] and [`VersionPollSource`]. That is what decision 038 asks D4d to produce, and
/// deliberately not three: a stream is the third, and it is the criterion D4d **cannot** close on
/// its own — if a polling source cannot be expressed cleanly here, the seam is stream-shaped and
/// should be named one.
#[allow(async_fn_in_trait)] // The application runtime is `!Send` by decision 001, like core's.
pub trait InvalidationSource {
    /// A name for reports and logs.
    fn name(&self) -> &str;

    /// Which entities this source speaks for.
    ///
    /// **The unit of per-source reconnect semantics.** When this source drops, only these entities
    /// may be marked stale — the other service's are still being vouched for by its own source, and
    /// marking them would turn one outage into a full refetch.
    fn entities(&self) -> &[&'static str];

    /// How far out of date this source's entities may be, in milliseconds.
    ///
    /// The tightest budget among its entities. See this module's header for why one number per
    /// source rather than one per entity.
    fn budget_ms(&self) -> i64;

    /// Ask for what has changed.
    ///
    /// # Errors
    ///
    /// Whatever prevented this source from answering. A failure marks this source's entities stale
    /// and nothing else.
    async fn poll(&self) -> Result<Vec<InvalidationEvent>, Error>;
}

/// A source fed by hand.
///
/// **The push shape, standing in for a stream.** A future SSE source differs from this one only in
/// where the events come from: something outside hands them over and `poll` drains what has
/// arrived. That is exactly what makes it worth having in D4d — it is the cheapest thing that
/// exercises the seam from the *other* direction from polling, and if the seam only fitted pull it
/// would show up here.
pub struct ManualSource {
    name: String,
    entities: Vec<&'static str>,
    queued: RefCell<Vec<InvalidationEvent>>,
}

impl ManualSource {
    /// Build a manual source over the entities it speaks for.
    pub fn new(name: impl Into<String>, entities: Vec<&'static str>) -> Self {
        Self {
            name: name.into(),
            entities,
            queued: RefCell::new(Vec::new()),
        }
    }

    /// Hand this source an event to deliver on its next poll.
    pub fn offer(&self, entity: &str, version: impl Into<CacheVersion>) {
        self.queued
            .borrow_mut()
            .push(InvalidationEvent::new(entity.to_owned(), version));
    }
}

impl InvalidationSource for ManualSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn entities(&self) -> &[&'static str] {
        &self.entities
    }

    /// Never due on its own.
    ///
    /// A push source has no schedule: it delivers when something delivers to it. `i64::MAX` says
    /// that in the vocabulary the budget already has, rather than adding an `Option` that every
    /// caller would have to unwrap to learn the same thing.
    fn budget_ms(&self) -> i64 {
        i64::MAX
    }

    async fn poll(&self) -> Result<Vec<InvalidationEvent>, Error> {
        let delivered = std::mem::take(&mut *self.queued.borrow_mut());
        trace::log(format!(
            "invalidation manual source={} delivered={}",
            self.name,
            delivered.len()
        ));
        Ok(delivered)
    }
}

/// A source that asks one service for the versions of everything it owns.
///
/// Emits an event per entity whose version differs from the last one this source saw — not per
/// entity in the response. A source that emitted everything every time would work, because
/// `InvalidationRunner::apply` compares before it writes, but it would make "nothing changed"
/// indistinguishable from "everything changed" in the returned report, and the report is what the
/// application renders.
pub struct VersionPollSource {
    name: String,
    entities: Vec<&'static str>,
    budget_ms: i64,
    fetch: Box<dyn Fn() -> BoxedVersions>,
    seen: RefCell<Vec<(String, CacheVersion)>>,
}

/// What a fetch returns: entity name to opaque version, or the reason it could not be had.
type BoxedVersions =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, Error>>>>;

impl VersionPollSource {
    /// Build a polling source over a fetch of one service's versions endpoint.
    pub fn new(
        name: impl Into<String>,
        entities: Vec<&'static str>,
        budget_ms: i64,
        fetch: impl Fn() -> BoxedVersions + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            entities,
            budget_ms,
            fetch: Box::new(fetch),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn last_seen(&self, entity: &str) -> Option<CacheVersion> {
        self.seen
            .borrow()
            .iter()
            .find(|(name, _)| name == entity)
            .map(|(_, version)| version.clone())
    }

    fn remember(&self, entity: &str, version: CacheVersion) {
        let mut seen = self.seen.borrow_mut();
        match seen.iter_mut().find(|(name, _)| name == entity) {
            Some(entry) => entry.1 = version,
            None => seen.push((entity.to_owned(), version)),
        }
    }
}

impl InvalidationSource for VersionPollSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn entities(&self) -> &[&'static str] {
        &self.entities
    }

    fn budget_ms(&self) -> i64 {
        self.budget_ms
    }

    async fn poll(&self) -> Result<Vec<InvalidationEvent>, Error> {
        let versions = (self.fetch)().await?;
        let mut events = Vec::new();
        for (entity, raw) in versions {
            let version = CacheVersion::from(raw);
            if self.last_seen(&entity).as_ref() == Some(&version) {
                trace::log(format!(
                    "invalidation version unchanged source={} entity={entity}",
                    self.name
                ));
                continue;
            }
            self.remember(&entity, version.clone());
            trace::log(format!(
                "invalidation version changed source={} entity={entity}",
                self.name
            ));
            events.push(InvalidationEvent::new(entity, version));
        }
        Ok(events)
    }
}

/// What one round of polling produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InvalidationTick {
    /// Sources that were asked this round.
    pub polled: Vec<String>,
    /// Entities this application instance should hydrate, in the servers' spelling.
    ///
    /// Usually this is the same as core's stale report. In a browser, though, two tabs share the
    /// durable version store while each tab has its own in-memory polling source. If another tab
    /// has already written the new version, this tab still needs to hydrate the entity whose
    /// source just observed that version transition.
    pub changed: Vec<String>,
    /// Sources that could not answer, and the entities marked stale as a result.
    pub dropped: Vec<Dropped>,
    /// Entity names no source's registry recognised.
    ///
    /// Reported rather than dropped, which is the divergence from the source system: its reconnect
    /// path filters unknown names out of the version map with no log at all, so a client whose
    /// registry has drifted reconciles against a silently truncated view and reports success.
    pub unknown: Vec<String>,
}

/// A source that failed, and what that cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    /// Which source.
    pub source: String,
    /// Why it could not answer.
    pub reason: String,
    /// The entities marked stale — **this source's, and only this source's.**
    pub entities: Vec<String>,
}

/// One application's sources, and which of them are due.
///
/// # An enum, and that is a finding rather than a convenience
///
/// The obvious shape is `Vec<Box<dyn InvalidationSource>>`. It does not compile:
/// [`InvalidationSource::poll`] is an `async fn` in a trait, and an `async fn` in a trait is not
/// dyn-compatible — the returned future has no named type to put behind a pointer. The ways out are
/// to box the future in the trait's own signature, to depend on `async-trait`, or to enumerate the
/// implementations.
///
/// **This bears directly on decision 038's promotion bar**, so it is recorded here rather than
/// discovered again later. Core has exactly this constraint everywhere — `SyncTransport`,
/// `OutboxStore` and `CacheVersionStore` are all `#[allow(async_fn_in_trait)]` and all used
/// statically, which works because an application has *one* of each. An invalidation source is the
/// first seam where one application plainly wants **several at once, of different types**. A core
/// `InvalidationSource` would therefore have to answer a question no existing core trait has had
/// to: box the future and take an allocation per poll, or make the set generic and cap it at
/// compile time.
///
/// The trial answers it the cheapest way, because two implementations enumerate fine. That answer
/// does not generalise, and saying so is the point of writing it down.
pub enum Source {
    /// Fed by hand; the push shape.
    Manual(ManualSource),
    /// Asks a service for its versions.
    Polling(VersionPollSource),
}

impl InvalidationSource for Source {
    fn name(&self) -> &str {
        match self {
            Self::Manual(source) => source.name(),
            Self::Polling(source) => source.name(),
        }
    }

    fn entities(&self) -> &[&'static str] {
        match self {
            Self::Manual(source) => source.entities(),
            Self::Polling(source) => source.entities(),
        }
    }

    fn budget_ms(&self) -> i64 {
        match self {
            Self::Manual(source) => source.budget_ms(),
            Self::Polling(source) => source.budget_ms(),
        }
    }

    async fn poll(&self) -> Result<Vec<InvalidationEvent>, Error> {
        match self {
            Self::Manual(source) => source.poll().await,
            Self::Polling(source) => source.poll().await,
        }
    }
}

/// Every source an application runs, with the last time each was asked.
#[derive(Default)]
pub struct Sources {
    sources: Vec<Source>,
    /// Wall-clock milliseconds at which each source was last polled; `None` until its first poll.
    last_polled: RefCell<Vec<Option<i64>>>,
}

impl Sources {
    /// Add a source.
    #[must_use]
    pub fn with(mut self, source: Source) -> Self {
        self.sources.push(source);
        self.last_polled.borrow_mut().push(None);
        self
    }

    /// How many sources there are.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The sources, by index.
    pub fn get(&self, index: usize) -> Option<&Source> {
        self.sources.get(index)
    }

    /// Which sources are past their staleness budget at `now`.
    ///
    /// A source that has never been polled is due immediately: an application that has just opened
    /// has no idea how stale anything is, which is the same condition as a budget that has run out
    /// and deserves the same answer.
    ///
    /// **Except a source with no schedule at all.** `ManualSource` reports `i64::MAX` and its own
    /// documentation says it is "never due on its own" — but "never polled is due" outranked that,
    /// so it was due exactly once, on the first round, and never again. Being polled once is not
    /// what a push source wants and it is not nothing either: it put the source into
    /// `InvalidationTick::polled`, where the trial's cache line picked it up and rendered it ageing
    /// beside two services that really are on a clock. A push source delivers when something
    /// delivers to it, and `refresh_now` asks every source regardless — which is the path
    /// observation 9 exercises and the one a handed-in event actually arrives on.
    pub fn due(&self, now_ms: i64) -> Vec<usize> {
        let last = self.last_polled.borrow();
        (0..self.sources.len())
            .filter(|index| {
                let budget = self.sources[*index].budget_ms();
                match last[*index] {
                    None => budget != i64::MAX,
                    // `saturating_sub` because `ManualSource` reports `i64::MAX` and a subtraction
                    // that wrapped would make a source that is never due read as always due.
                    Some(at) => now_ms.saturating_sub(at) >= budget,
                }
            })
            .collect()
    }

    /// Every source, due or not. What a refresh button asks for.
    pub fn all(&self) -> Vec<usize> {
        (0..self.sources.len()).collect()
    }

    /// Record that a source was asked at `now_ms`.
    ///
    /// Stamped whether the poll succeeded or failed, deliberately: a source that is down would
    /// otherwise be retried on every single tick, turning an outage into a request flood exactly
    /// when the service can least afford one.
    pub fn mark_polled(&self, index: usize, now_ms: i64) {
        self.last_polled.borrow_mut()[index] = Some(now_ms);
    }
}
