//! The two stores a record leaves the outbox *into*.
//!
//! Split from `super` for length, along the line `src/record/terminal.rs` already draws over the
//! record types these return. Neither trait has an `insert`, for the reason stated on each: a
//! terminal record is a transition out of the outbox, made where the backend still holds the
//! pending row and can move it atomically, never a free-standing write.

use crate::error::Error;
use crate::record::{DeadLetterRecord, QuarantinedRecord};

/// Mutations the server terminally refused.
///
/// Reads here are scoped and exclude other scopes' records, exactly as on
/// [`OutboxStore`](super::OutboxStore).
///
/// There is no `insert`. A dead letter is a transition out of the outbox after a server verdict,
/// produced by [`OutboxStore::apply_outcomes`](super::OutboxStore::apply_outcomes) where the backend
/// still holds the pending record and can move it atomically — never a free-standing write.
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
    /// oldest unresolved parking is the one to look at. [`MutationId`](crate::id::MutationId) breaks
    /// the tie because it is the only field guaranteed present, unique, and stable, and it is
    /// compared by its bytes: a backend storing it as text must use a binary collation, since a
    /// case-insensitive one orders hex differently (`src/id.rs`).
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
/// Reads here are scoped and exclude other scopes' records, exactly as on
/// [`OutboxStore`](super::OutboxStore).
///
/// No `insert`, for the same reason [`DeadLetterStore`] has none: a quarantine entry is a
/// transition out of the outbox, produced by
/// [`OutboxStore::apply_outcomes`](super::OutboxStore::apply_outcomes) or
/// [`OutboxStore::sweep_corrupt`](super::OutboxStore::sweep_corrupt).
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
