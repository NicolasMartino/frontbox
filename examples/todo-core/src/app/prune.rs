//! The one delete hydration performs, and the queue scan that guards it.
//!
//! Split out of [`super`] when it passed the four-hundred-line cap `AGENTS.md` sets, and the line
//! is a seam rather than a length: everything next door decides what a *write* means before
//! anything durable happens, and this decides what a *read* is allowed to remove. It is the only
//! place in this crate that deletes a row the user can still see.

use frontbox::{Error, OutboxStore, RowRef, RowStore};

use super::{TodoApp, ROW_LOAD_LIMIT};

/// What one [`TodoApp::prune_rows_absent`] pass did, for the hydration trace line.
///
/// `suppressed` is carried rather than inferred from `deleted == 0`: a hydration that deleted
/// nothing because the server named everything and one that deleted nothing because it refused to
/// look are different facts, and only the second is worth noticing in a log.
pub(super) struct Prune {
    pub(super) deleted: usize,
    pub(super) queued: usize,
    pub(super) suppressed: bool,
}

impl TodoApp {
    /// Delete the rows of `entity` the server did not mention, protecting anything still queued.
    ///
    /// # Why both hydrations call one function
    ///
    /// This was the same twenty lines in `sync.rs` and in `identity.rs`, and the duplication was
    /// not the problem — *the decision being made twice* was. It is the only place in this crate
    /// that deletes a row the user can still see, and getting it wrong costs data rather than a
    /// render. One copy is one thing to audit.
    ///
    /// # Why a capped scan suppresses the delete instead of narrowing it
    ///
    /// The protection used to be `pending_batch(self.pending_scan_limit)` and nothing else, so a
    /// queue longer than the limit left its tail unprotected: those rows are queued, the server has
    /// never seen them, they are absent from its list — and they were deleted, which is the queued
    /// write disappearing off the screen while it waits to drain.
    ///
    /// `pending_batch` returns the first `limit` records and offers no way to ask for the next
    /// page, so a scan that comes back exactly full is indistinguishable from one that was
    /// truncated. **Treating full as truncated is the safe side of that ambiguity**: the cost of
    /// guessing "complete" wrongly is deleted user data, and the cost of guessing "truncated"
    /// wrongly is a row the server has forgotten lingering until the next hydration with a shorter
    /// queue. Those are not comparable, so this does not weigh them — it refuses to delete.
    ///
    /// `keep` is what the caller already knows to protect: the ids the server named, and the rows
    /// `merge_rows` skipped because a local write was newer.
    pub(super) async fn prune_rows_absent(
        &self,
        entity: &str,
        mut keep: std::collections::HashSet<String>,
    ) -> Result<Prune, Error> {
        let limit = self.pending_scan_limit;
        let scanned = self.runner.store().pending_batch(limit).await?;
        let complete = scanned.len() < limit;
        let queued: Vec<RowRef> = scanned
            .into_iter()
            .filter_map(|record| record.row)
            .filter(|row| row.entity == entity)
            .collect();

        if !complete {
            return Ok(Prune {
                deleted: 0,
                queued: queued.len(),
                suppressed: true,
            });
        }

        keep.extend(queued.iter().map(|row| row.row_id.clone()));
        let gone: Vec<RowRef> = self
            .runner
            .store()
            .list_rows(entity, ROW_LOAD_LIMIT)
            .await?
            .into_iter()
            .map(|stored| stored.row)
            .filter(|row| !keep.contains(row.row_id.as_str()))
            .collect();
        if !gone.is_empty() {
            self.runner.store().delete_rows(&gone).await?;
        }
        Ok(Prune {
            deleted: gone.len(),
            queued: queued.len(),
            suppressed: false,
        })
    }
}
