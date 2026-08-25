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
#[derive(Debug, Clone, PartialEq)]
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
    /// Records stamped with a different [`ScopeKey`] are never returned.
    ///
    /// `limit` bounds the blast radius of any one server verdict. The source loads the entire
    /// outbox in one batch, so a single terminal rejection can block every later correlated
    /// mutation at once.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;

    /// Count decodable pending records in this store's scope.
    ///
    /// Quarantined rows are excluded and reported by [`QuarantineStore::count`], so a wedged record
    /// can never hide inside a pending total. Records under another scope are never counted.
    async fn pending_count(&self) -> Result<usize, Error>;

    /// Move rows that cannot be decoded into quarantine, returning how many moved.
    ///
    /// This is the id-less path. A row whose `mutation_id` is missing or unparseable cannot be
    /// named by an [`Outcome`], so nothing outside the backend can find it — and a row that is
    /// silently skipped on every read stays in storage, keeps occupying space, is never sent, is
    /// never deleted, and never reaches a user-visible view. The source has five such
    /// silent-drop paths.
    async fn sweep_corrupt(&self) -> Result<usize, Error>;

    /// Apply every outcome atomically.
    ///
    /// Deletions, dead-letter inserts, and quarantine transitions either all commit or none do.
    /// The source performs this as an insert followed by a separate delete whose result it
    /// discards; when the delete fails, the record is both pending and dead-lettered, gets re-sent
    /// on every subsequent sync, and its `rejected_at` keeps moving forward so retention never
    /// purges it. The trait makes that intermediate state unrepresentable for implementors.
    ///
    /// Every outcome must name a record this store holds in its own scope. The runner guarantees
    /// that; a store handed an unknown or out-of-scope id returns [`Error::Protocol`] and commits
    /// nothing.
    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error>;
}

/// Mutations the server terminally refused.
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
