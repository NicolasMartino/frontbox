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
use crate::protocol::RemoteRejection;
use crate::record::{DeadLetterRecord, MutationIntent, OutboxRecord, QuarantinedRecord};
use crate::scope::ScopeKey;

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
    /// Move it to the dead-letter store. The server terminally refused it.
    DeadLetter {
        /// The server's verdict, when it supplied one.
        error: Option<RemoteRejection>,
    },
    /// Leave it queued, unmodified.
    Retain,
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
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait OutboxStore {
    /// The scope this store was opened under.
    fn scope(&self) -> &ScopeKey;

    /// Queue a mutation, stamping it with this store's scope.
    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error>;

    /// Read up to `limit` pending records, oldest first by `(created_at, mutation_id)`.
    ///
    /// The compound key makes the order total and reproducible across backends without adding a
    /// schema column. `created_at` alone does not: it is a client clock reading, and the source's
    /// two backends break same-millisecond ties differently, so they can disagree about the order
    /// of simultaneous writes.
    ///
    /// This buys determinism, not causality. Real causal ordering needs a monotonic sequence
    /// assigned at enqueue, which is a durable-storage concern and is not claimed here.
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
    /// Read up to `limit` dead letters in this store's scope.
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error>;

    /// Count dead letters in this store's scope.
    async fn count(&self) -> Result<usize, Error>;

    /// Drop dead letters rejected before `cutoff_ms`, returning how many were dropped.
    ///
    /// The caller computes the cutoff from its own clock, so retention is deterministic and
    /// testable rather than depending on wall-clock time inside the store.
    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error>;
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
    /// Read up to `limit` quarantined rows in this store's scope.
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error>;

    /// Count quarantined rows in this store's scope.
    async fn count(&self) -> Result<usize, Error>;
}
