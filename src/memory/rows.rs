//! The read-model half of the in-memory backend.
//!
//! Decision 032's store: blobs in, blobs out, nothing parsed. The one piece of logic here is the
//! merge skip, and it is the reason the rows live beside the queue at all.

use std::collections::HashSet;

use crate::error::Error;
use crate::record::{RowRef, StoredRow};
use crate::store::RowStore;

use super::{decode, InMemoryStore, StoreOp};

impl RowStore for InMemoryStore {
    async fn get_row(&self, row: &RowRef) -> Result<Option<StoredRow>, Error> {
        self.backend.check(StoreOp::GetRow)?;
        let state = self.backend.state.borrow();
        Ok(state.rows.get(&(self.scope.clone(), row.clone())).cloned())
    }

    async fn list_rows(&self, entity: &str, limit: usize) -> Result<Vec<StoredRow>, Error> {
        self.backend.check(StoreOp::ListRows)?;
        let state = self.backend.state.borrow();
        let mut found: Vec<StoredRow> = state
            .rows
            .iter()
            .filter(|((scope, key), _)| *scope == self.scope && key.entity == entity)
            .map(|(_, stored)| stored.clone())
            .collect();
        // `HashMap` iteration order is not stable, so the sort is what makes two calls agree. A
        // durable backend gets this from its index; here it has to be asked for.
        found.sort_by(|a, b| a.row.row_id.cmp(&b.row.row_id));
        found.truncate(limit);
        Ok(found)
    }

    async fn put_rows(&self, rows: &[StoredRow]) -> Result<(), Error> {
        self.backend.check(StoreOp::PutRows)?;
        let mut state = self.backend.state.borrow_mut();
        for stored in rows {
            state
                .rows
                .insert((self.scope.clone(), stored.row.clone()), stored.clone());
        }
        Ok(())
    }

    async fn merge_rows(&self, rows: &[StoredRow]) -> Result<Vec<RowRef>, Error> {
        self.backend.check(StoreOp::MergeRows)?;

        // Which rows the queue is currently protecting. Read before anything is written, so the
        // decision is made against one consistent view — a durable backend must do the same inside
        // one transaction, which is the whole of what this method asks of it.
        //
        // Undecodable rows are skipped rather than raised, exactly as `pending_batch` skips them:
        // a corrupt row must not be able to block a hydration any more than it blocks a read
        // (`wiki/decisions/006-corrupt-record-policy.decision.md`).
        // A set rather than a list: the membership test below runs once per incoming row, so a
        // linear scan makes a hydration quadratic in the size of the queue — and a large queue is
        // exactly the state a client hydrates from.
        let protected: HashSet<RowRef> = {
            let state = self.backend.state.borrow();
            state
                .outbox
                .iter()
                .filter(|row| row.scope == self.scope)
                .filter(|row| decode(row).is_ok())
                .filter_map(|row| row.row.clone())
                .collect()
        };

        let mut state = self.backend.state.borrow_mut();
        let mut skipped = Vec::new();
        for stored in rows {
            if protected.contains(&stored.row) {
                skipped.push(stored.row.clone());
                continue;
            }
            state
                .rows
                .insert((self.scope.clone(), stored.row.clone()), stored.clone());
        }
        Ok(skipped)
    }

    async fn delete_rows(&self, rows: &[RowRef]) -> Result<usize, Error> {
        self.backend.check(StoreOp::DeleteRows)?;
        let mut state = self.backend.state.borrow_mut();
        let mut removed = 0;
        for row in rows {
            // The marker goes with the row, because the marker *is* a field on it. Decision 023
            // owed an unbounded-growth policy only because the two were separable.
            if state
                .rows
                .remove(&(self.scope.clone(), row.clone()))
                .is_some()
            {
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn set_stale(&self, rows: &[RowRef], stale: bool) -> Result<usize, Error> {
        self.backend.check(StoreOp::SetStale)?;
        let mut state = self.backend.state.borrow_mut();
        let mut changed = 0;
        for row in rows {
            if let Some(stored) = state.rows.get_mut(&(self.scope.clone(), row.clone())) {
                if stored.stale != stale {
                    stored.stale = stale;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }
}
