//! The read-model row store.
//!
//! Split from `super` for length, along the line `src/record/row.rs` already draws over the types
//! these methods move.

use crate::error::Error;
use crate::record::{RowRef, StoredRow};

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
/// Reads here are scoped exactly as on [`OutboxStore`](super::OutboxStore).
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
    /// A coalesced record protects its row exactly as the record it replaced did: replacement keeps
    /// the row binding along with the queue position, so nothing here changes when a body is
    /// rewritten (see [`enqueue_coalescing`](super::OutboxStore::enqueue_coalescing)).
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
