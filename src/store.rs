//! Durable storage traits.
//!
//! Every read on every trait here is scoped. A record written under one [`ScopeKey`] is never
//! returned, never batched, and never counted by a store opened under another
//! (`wiki/decisions/009-local-scope-identity.decision.md`). Enforcement lives here, on the read
//! path, because that is where it has to be: fresh per-send authentication is correct and
//! insufficient — a valid token for one principal applied to another principal's queued record
//! produces an *authorized* wrong write.

use crate::error::Error;
use crate::id::MutationId;
use crate::record::{
    DeadLetterReason, DeadLetterRecord, MutationIntent, OutboxRecord, QuarantinedRecord, RowRef,
    StoredRow,
};
use crate::scope::ScopeKey;

/// Exclusive drain rights for one scope, held for the length of one pass.
///
/// Released on drop, whatever ends the pass — returning, erroring, or being cancelled. Core never
/// looks inside: a backend puts whatever it needs to release in the closure, and a backend with
/// nothing to release supplies none.
pub struct DrainLease(Option<Box<dyn FnOnce()>>);

impl DrainLease {
    /// A lease with nothing to release.
    ///
    /// What the default [`claim_drain`](OutboxStore::claim_drain) hands back, and what a
    /// single-realm backend should return.
    #[must_use]
    pub fn granted() -> Self {
        Self(None)
    }

    /// A lease that runs `release` when it is dropped.
    #[must_use]
    pub fn held(release: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl Drop for DrainLease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

impl std::fmt::Debug for DrainLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "DrainLease(held)",
            None => "DrainLease(granted)",
        })
    }
}

/// What should happen to one pending record.
///
/// `#[non_exhaustive]`: a future disposition is additive, so implementors match with a wildcard.
/// A backend that meets an unrecognised disposition should return [`Error::Protocol`] and commit
/// nothing, rather than guess.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Disposition {
    /// Remove it. The server applied it, or had already seen it.
    Delete,
    /// Move it to the dead-letter store. It will not be sent again.
    DeadLetter {
        /// Why, so a reader does not have to infer it from what is absent.
        ///
        /// Core produces two of the three kinds — a server verdict and its own retention bound.
        /// The third exists so a caller parking a record for a reason of its own can say what it
        /// was, rather than being forced into a silence indistinguishable from the bound
        /// (`wiki/decisions/027-dead-letter-reason.decision.md`).
        reason: DeadLetterReason,
    },
    /// Leave it queued, with one more attempt against it.
    ///
    /// Not a no-op: the store increments `attempts` and records `reason`, which is what makes
    /// decision 017's bound reachable and decision 033's diagnosis possible.
    Retain {
        /// Why this verdict left the record queued, in the server's own words.
        ///
        /// Truncated by the store to [`LAST_ERROR_MAX`](crate::record::LAST_ERROR_MAX) and never
        /// parsed. `None` when the caller has nothing to say — an offline pass produces no verdict
        /// at all and so never reaches here.
        reason: Option<String>,
    },
    /// Move it to quarantine. The record is identifiable but unusable.
    Quarantine {
        /// What made the record unusable.
        reason: String,
    },
}

/// One record's disposition, addressed by id.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Outcome {
    /// Which record this is about.
    pub id: MutationId,
    /// What should happen to it.
    pub disposition: Disposition,
}

impl Outcome {
    /// Pair a record with its disposition.
    pub fn new(id: MutationId, disposition: Disposition) -> Self {
        Self { id, disposition }
    }
}

/// The pending mutation queue.
///
/// # One drain per scope
///
/// **At most one drain may be in flight per scope, across every handle on it.** Core holds that
/// within a realm: [`SyncRunner::sync_once`](crate::runner::SyncRunner::sync_once) claims the
/// scope rather than the runner, so a second handle opened under the same key reports
/// [`SyncPass::AlreadyRunning`](crate::runner::SyncPass) instead of sending. Conformance case 61
/// asserts it.
///
/// **A backend whose storage can be opened from more than one realm must extend that exclusion
/// across realms.** Core cannot: a second browser tab is a second wasm instance with its own
/// memory, and nothing in this process is visible to it. Two tabs on one origin are two realms over
/// one IndexedDB database, each with its own runner, drawing from one queue — which needs nobody to
/// make a mistake. The mechanism is the backend's, because only the backend knows its concurrency
/// model: Web Locks (`navigator.locks.request`, available in window *and* worker scopes) for
/// IndexedDB, usually nothing at all for SQLite in a one-process native application.
///
/// A drain pass is *read the batch* → *POST* → *apply outcomes*: two transactions with a network
/// round trip between them. **Transactional atomicity cannot supply this**, because the two
/// transactions never overlap — whatever excludes the second drainer has to outlive a transaction.
///
/// What a violation costs is specific, and worth knowing before treating this as optional.
/// Idempotency survives, because the server dedupes on `mutation_id`. Ordering does not: at
/// `batch_limit = 1` the second drainer ships record 2 while record 1 is still in flight, which is
/// the guarantee [`pending_batch`](OutboxStore::pending_batch)'s order key exists to provide. The
/// retention bound does not either, since two drainers spend one record's `attempts` budget in
/// parallel and a wedged head reaches its bound in half the wall clock the bound was chosen to
/// represent (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
///
/// Case 61 cannot enforce this half: a conformance suite drives a backend through one process, and
/// a second realm is not something a case can open. It is stated here for the same reason decision
/// 024's injectivity obligation is stated on [`ScopeKey`] — the test pins the half it can reach,
/// and the prose has to carry the rest.
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait OutboxStore {
    /// The scope this store was opened under.
    fn scope(&self) -> &ScopeKey;

    /// Claim exclusive drain rights for this scope, for as long as the returned lease lives.
    ///
    /// `Ok(None)` means someone else holds them and this pass must stand down — the runner turns
    /// that into [`SyncPass::AlreadyRunning`](crate::runner::SyncPass::AlreadyRunning), exactly as
    /// it does for the in-process claim.
    ///
    /// # Why this exists, and why it defaults to granted
    ///
    /// Decision 031's in-realm half is core's and is already built: a scope registry means two
    /// handles in one process cannot drain together. **The cross-realm half is not reachable from
    /// core at all** — two browser tabs are two wasm instances with two registries, and neither can
    /// see the other. Only something with a view of the storage layer can arbitrate, which is the
    /// backend.
    ///
    /// So core states the obligation here and names no mechanism, exactly as decisions 024 and 025
    /// do for storage naming and quarantine. The default grants immediately, which is the honest
    /// answer for a backend that has one realm by construction: an in-memory store dies with its
    /// process, and a SQLite file is normally opened by one.
    ///
    /// # What a lease must guarantee
    ///
    /// It is released when dropped, including on cancellation — a pass abandoned mid-flight must
    /// not leave a scope claimed forever, which is the failure
    /// [`case_30`](crate::testing::cases::case_30_a_cancelled_pass_does_not_wedge_the_runner)
    /// pins down for the in-process claim.
    async fn claim_drain(&self) -> Result<Option<DrainLease>, Error> {
        Ok(Some(DrainLease::granted()))
    }

    /// Queue a mutation, stamping it with this store's scope.
    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error>;

    /// Read up to `limit` pending records in enqueue order, by
    /// [`order_key`](crate::record::OutboxRecord::order_key).
    ///
    /// **The order is the sequence the store assigned at enqueue**, not the client clock. D1 and D2
    /// ordered by `(created_at, mutation_id)`, which was total and reproducible but not faithful:
    /// `created_at` is a clock reading, and same-millisecond ties broke on a random
    /// [`MutationId`], so three rapid writes could be drained in an order the
    /// application never wrote in — a set before the session it belongs to. Under batching the
    /// server received the whole batch and evaluated it in that same wrong order; at
    /// `batch_limit = 1` a wrong order is a wrong write
    /// (`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`).
    ///
    /// This buys faithfulness, not causality. It replays the order the caller enqueued in; it
    /// cannot make that order correct if the caller enqueued a child before its parent.
    ///
    /// Records stamped with a different [`ScopeKey`] are never returned, and neither are rows that
    /// will not decode — see [`sweep_corrupt`](OutboxStore::sweep_corrupt).
    ///
    /// `limit` bounds the blast radius of any one server verdict. The source loads the entire
    /// outbox in one batch, so a single terminal rejection can block every later correlated
    /// mutation at once.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;

    /// Count decodable pending records in this store's scope.
    ///
    /// Quarantined rows are excluded and reported by [`QuarantineStore::count`], so a wedged record
    /// can never hide inside a pending total. Records under another scope are never counted.
    ///
    /// Undecodable rows that have not yet been swept are not counted either. See
    /// [`sweep_corrupt`](OutboxStore::sweep_corrupt) for why reads behave this way and what a
    /// caller has to do about it.
    async fn pending_count(&self) -> Result<usize, Error>;

    /// Move rows that cannot be decoded into quarantine, returning how many moved.
    ///
    /// This is the id-less path. A row whose `mutation_id` is missing or unparseable cannot be
    /// named by an [`Outcome`], so nothing outside the backend can find it — and a row that is
    /// silently skipped on every read stays in storage, keeps occupying space, is never sent, is
    /// never deleted, and never reaches a user-visible view. The source has five such
    /// silent-drop paths.
    ///
    /// # Reads skip corruption; only this surfaces it
    ///
    /// [`pending_batch`](OutboxStore::pending_batch) returns `OutboxRecord` values and
    /// [`pending_count`](OutboxStore::pending_count) counts them, so neither can represent a row
    /// that will not decode. Between a row becoming corrupt and a sweep finding it, that row is
    /// invisible to every read — the same silence this crate criticises the source for, narrowed to
    /// a window rather than made permanent.
    ///
    /// Two alternatives were considered and are worse. Sweeping inside the read makes a read
    /// mutate storage, which is surprising and rules out read-only transactions. Returning
    /// [`Error::CorruptRecord`] from a read lets one bad row block every read, wedging a queue that
    /// is otherwise healthy — the outcome decision 006 exists to prevent.
    ///
    /// So the contract is: **a caller that reads without ever syncing must call this itself.**
    /// [`SyncRunner`](crate::runner::SyncRunner) sweeps at the start of every pass and reports the
    /// count in [`SyncReport::quarantined`](crate::runner::SyncReport::quarantined), so an
    /// application that syncs at all closes the window on its own.
    async fn sweep_corrupt(&self) -> Result<usize, Error>;

    /// Apply every outcome atomically.
    ///
    /// Deletions, dead-letter inserts, and quarantine transitions either all commit or none do.
    /// The source performs this as an insert followed by a separate delete whose result it
    /// discards; when the delete fails, the record is both pending and dead-lettered, gets re-sent
    /// on every subsequent sync, and its `rejected_at` keeps moving forward so retention never
    /// purges it. The trait makes that intermediate state unrepresentable for implementors.
    ///
    /// # What the outcome set must satisfy
    ///
    /// Every outcome must name a record this store holds in its own scope, and **no id may appear
    /// twice**. A store handed an unknown id, an out-of-scope id, or a repeated id returns
    /// [`Error::Protocol`] and commits nothing.
    ///
    /// The duplicate rule is not pedantry. Two outcomes for one id resolve to the same record, so a
    /// pair of `DeadLetter` dispositions writes two dead letters for one row and the dead-letter
    /// count stops matching reality; a `Delete` paired with a `DeadLetter` has no defensible
    /// winner. Rejecting the set is the only answer that cannot silently corrupt a count.
    ///
    /// [`SyncRunner`](crate::runner::SyncRunner) satisfies both rules already: it drops verdicts
    /// for ids it did not send, and drops *every* verdict for an id the same response named more
    /// than once, reporting both kinds as anomalies. A store therefore never has to arbitrate — but
    /// it still has to reject, because `apply_outcomes` is public and a backend cannot assume the
    /// runner is its only caller.
    ///
    /// This rule is uniform across backends. Conformance case 31 asserts it, so the in-memory
    /// store, SQLite, and IndexedDB all fail the suite if any of them silently applies one of a
    /// repeated pair instead.
    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error>;
}

/// Mutations the server terminally refused.
///
/// Reads here are scoped and exclude other scopes' records, exactly as on [`OutboxStore`].
///
/// There is no `insert`. A dead letter is a transition out of the outbox after a server verdict,
/// produced by [`OutboxStore::apply_outcomes`] where the backend still holds the pending record and
/// can move it atomically — never a free-standing write.
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait DeadLetterStore {
    /// Read up to `limit` dead letters in this store's scope, **ordered by
    /// `(rejected_at, mutation_id)`**.
    ///
    /// # Why the order is stated rather than left to the backend
    ///
    /// It was left to the backend, and three backends chose three orders: the in-memory store
    /// sorted by `(rejected_at, mutation_id)`, SQLite by its insertion rowid, and IndexedDB by
    /// whatever its scope index handed back. All three agree as long as records are parked in
    /// timestamp order — which is why the conformance suite passed on all three — and they diverge
    /// the moment they are not. An injected [`Clock`](crate::Clock) makes that routine, and a real
    /// one makes it possible: nothing requires the clock behind a retention sweep to run ahead of
    /// the clock behind a server rejection.
    ///
    /// The divergence only becomes visible when `limit` truncates, and then it is a different *set*
    /// of records per backend, not merely a different sequence. That is the failure this crate
    /// exists to prevent, so the trait states the order and conformance case 68 pins it.
    ///
    /// `rejected_at` first, because a human triaging a queue asks *what broke, and when* — the
    /// oldest unresolved parking is the one to look at. [`MutationId`] breaks the tie because it is
    /// the only field guaranteed present, unique, and stable, and it is compared by its bytes: a
    /// backend storing it as text must use a binary collation, since a case-insensitive one orders
    /// hex differently (`src/id.rs`).
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error>;

    /// Count dead letters in this store's scope.
    async fn count(&self) -> Result<usize, Error>;

    /// Drop dead letters rejected before `cutoff_ms`, returning how many were dropped.
    ///
    /// The caller computes the cutoff from its own clock, so retention is deterministic and
    /// testable rather than depending on wall-clock time inside the store.
    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error>;
}

/// Read-model rows, stored as values core never reads.
///
/// # The boundary this draws
///
/// frontbox holds the bytes and the bookkeeping; the application holds the meaning. There is no
/// query language here, no index over the application's fields, and no subscription surface —
/// those belong to the local-first database cohort, and going there means competing with mature
/// systems on data this crate understands nothing about
/// (`wiki/decisions/032-opaque-row-store.decision.md`).
///
/// What it replaces is worse: before this, every application wanting offline reads opened a second
/// durable store beside frontbox's, and re-derived the merge rule below by hand.
///
/// Reads here are scoped exactly as on [`OutboxStore`].
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait RowStore {
    /// Read one row, if this scope holds it.
    async fn get_row(&self, row: &RowRef) -> Result<Option<StoredRow>, Error>;

    /// Read up to `limit` rows of one entity, in `row_id` order.
    async fn list_rows(&self, entity: &str, limit: usize) -> Result<Vec<StoredRow>, Error>;

    /// Write rows, replacing any this scope already holds under the same keys.
    ///
    /// The application's own writes: an optimistic projection, or the result of a fetch it chose
    /// to trust. Nothing is skipped, because the caller is stating what it wants stored.
    async fn put_rows(&self, rows: &[StoredRow]) -> Result<(), Error>;

    /// Write rows from the server, **skipping any row with queued work**.
    ///
    /// # The rule, and why it is here rather than in every application
    ///
    /// A client that reloads with unsent writes has local rows the server has not seen. Writing
    /// the server's list over them drops exactly the work the user is waiting on — it reappears a
    /// drain later, so nothing is lost, but the screen lies in the meantime, and for an offline
    /// client "the meantime" is unbounded.
    ///
    /// The skip is decidable only by something that can see both the rows and the queue, which is
    /// why decision 032 brought the rows inside. A row is skipped when a pending mutation in this
    /// scope is bound to it — see [`MutationIntent::with_row`](crate::record::MutationIntent::with_row).
    /// Unbound mutations protect nothing, so a caller that never binds gets a plain write.
    ///
    /// Returns the rows that were skipped, so a caller can say why its screen still disagrees with
    /// the server.
    async fn merge_rows(&self, rows: &[StoredRow]) -> Result<Vec<RowRef>, Error>;

    /// Forget rows, and the staleness markers that describe them.
    ///
    /// The marker dying with the row is the point: decision 023 owed an unbounded-growth policy
    /// precisely because frontbox could not see the deletions that made its markers garbage.
    async fn delete_rows(&self, rows: &[RowRef]) -> Result<usize, Error>;

    /// Mark rows out of step with the server, returning how many changed.
    async fn set_stale(&self, rows: &[RowRef], stale: bool) -> Result<usize, Error>;
}

/// Locally stored rows that could not be used.
///
/// Reads here are scoped and exclude other scopes' records, exactly as on [`OutboxStore`].
///
/// No `insert`, for the same reason [`DeadLetterStore`] has none: a quarantine entry is a
/// transition out of the outbox, produced by [`OutboxStore::apply_outcomes`] or
/// [`OutboxStore::sweep_corrupt`].
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait QuarantineStore {
    /// Read up to `limit` quarantined rows in this store's scope, **ordered by
    /// `(quarantined_at, raw_mutation_id)`**.
    ///
    /// The same rule as [`DeadLetterStore::list`], and stated for the same reason — see there for
    /// the argument. The tie-break is the *raw* identifier, compared as bytes, because a
    /// quarantined row is one whose identifier may not parse; that string is all there is.
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error>;

    /// Count quarantined rows in this store's scope.
    async fn count(&self) -> Result<usize, Error>;
}
