//! The read-only halves of the in-memory backend: dead letters and quarantine.
//!
//! Neither trait has an `insert`. Both stores are reached only as a transition out of the outbox,
//! so everything that writes to them lives in `super::outbox`.

use crate::error::Error;
use crate::record::{DeadLetterRecord, QuarantinedRecord};
use crate::store::{DeadLetterStore, QuarantineStore};

use super::{InMemoryStore, StoreOp};

impl DeadLetterStore for InMemoryStore {
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error> {
        self.backend.check(StoreOp::DeadLetterList)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<DeadLetterRecord> = state
            .dead_letters
            .iter()
            .filter(|record| record.scope == self.scope)
            .cloned()
            .collect();
        records.sort_by_key(|record| (record.rejected_at, record.mutation_id));
        records.truncate(limit);
        Ok(records)
    }

    async fn count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::DeadLetterCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .dead_letters
            .iter()
            .filter(|record| record.scope == self.scope)
            .count())
    }

    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error> {
        self.backend.check(StoreOp::DeadLetterPurge)?;

        let mut state = self.backend.state.borrow_mut();
        let before = state.dead_letters.len();
        let scope = self.scope.clone();
        state
            .dead_letters
            .retain(|record| record.scope != scope || record.rejected_at >= cutoff_ms);
        Ok(before - state.dead_letters.len())
    }
}

impl QuarantineStore for InMemoryStore {
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error> {
        self.backend.check(StoreOp::QuarantineList)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<QuarantinedRecord> = state
            .quarantine
            .iter()
            .filter(|record| record.scope == self.scope)
            .cloned()
            .collect();
        records.sort_by_key(|record| (record.quarantined_at, record.raw_mutation_id.clone()));
        records.truncate(limit);
        Ok(records)
    }

    async fn count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::QuarantineCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .quarantine
            .iter()
            .filter(|record| record.scope == self.scope)
            .count())
    }
}
