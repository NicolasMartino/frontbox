//! The outbox half of the in-memory backend.
//!
//! Split from `super` for length only; `InMemoryStore` is one type with its storage traits
//! implemented across three files. The atomicity argument lives on `apply_outcomes` below.

use std::collections::HashSet;

use crate::error::Error;
use crate::id::MutationId;
use crate::record::{truncate_error, DeadLetterRecord, MutationIntent, OutboxRecord};
use crate::scope::ScopeKey;
use crate::store::{Disposition, OutboxStore, Outcome};

use super::{decode, quarantine_from, InMemoryStore, Row, StoreOp};

impl OutboxStore for InMemoryStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error> {
        self.backend.check(StoreOp::Enqueue)?;

        let raw_body = serde_json::to_string(&intent.body).map_err(Error::serialization)?;
        let mut state = self.backend.state.borrow_mut();

        // Issued and recorded under one borrow, which is this backend's stand-in for "inside the
        // insert's own transaction". A durable backend must make the same guarantee, or two
        // concurrent enqueues can read the same next value and produce a duplicate order key.
        let seq = state.next_seq;
        state.next_seq += 1;

        let row = Row {
            raw_mutation_id: intent.mutation_id.to_string(),
            method: intent.method,
            path: intent.path,
            raw_body,
            created_at: intent.created_at,
            op: intent.op,
            traceparent: intent.traceparent,
            precondition: intent.precondition,
            // The store stamps its own scope. Nothing the caller passed can influence this.
            scope: self.scope.clone(),
            seq,
            attempts: 0,
            row: intent.row,
            last_error: None,
        };
        state.outbox.push(row);
        Ok(())
    }

    /// Undecodable rows in this scope are skipped, not returned and not reported here. See the
    /// trait's contract: `sweep_corrupt` is the only thing that surfaces them.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        self.backend.check(StoreOp::PendingBatch)?;

        let state = self.backend.state.borrow();
        let mut records: Vec<OutboxRecord> = state
            .outbox
            .iter()
            .filter(|row| row.scope == self.scope)
            .filter_map(|row| decode(row).ok())
            .collect();
        records.sort_by_key(OutboxRecord::order_key);
        records.truncate(limit);
        Ok(records)
    }

    /// Counts decodable rows only. An undecodable row is neither counted here nor visible through
    /// `QuarantineStore` until `sweep_corrupt` moves it.
    async fn pending_count(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::PendingCount)?;

        let state = self.backend.state.borrow();
        Ok(state
            .outbox
            .iter()
            .filter(|row| row.scope == self.scope)
            .filter(|row| decode(row).is_ok())
            .count())
    }

    async fn sweep_corrupt(&self) -> Result<usize, Error> {
        self.backend.check(StoreOp::SweepCorrupt)?;

        let quarantined_at = self.backend.clock.now_ms();
        let mut state = self.backend.state.borrow_mut();

        let mut kept = Vec::with_capacity(state.outbox.len());
        let mut moved = Vec::new();
        for row in state.outbox.drain(..) {
            match (row.scope == self.scope, decode(&row)) {
                (true, Err(reason)) => moved.push(quarantine_from(&row, reason, quarantined_at)),
                _ => kept.push(row),
            }
        }

        state.outbox = kept;
        let count = moved.len();
        state.quarantine.extend(moved);
        Ok(count)
    }

    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        self.backend.check(StoreOp::ApplyOutcomes)?;

        let now = self.backend.clock.now_ms();
        let mut state = self.backend.state.borrow_mut();

        // Resolve every outcome against a record this store actually holds, before touching
        // anything. An outcome naming an unknown or out-of-scope id fails the whole call, and so
        // does a second outcome for an id already named: two outcomes resolve to the same row, so
        // a pair of `DeadLetter` dispositions would write two dead letters for one record and the
        // count would then disagree with reality. There is also no basis for choosing between two
        // dispositions for one record.
        let mut targets = Vec::with_capacity(outcomes.len());
        let mut named: HashSet<MutationId> = HashSet::with_capacity(outcomes.len());
        for outcome in outcomes {
            if !named.insert(outcome.id) {
                return Err(Error::protocol(format!(
                    "outcome set names {} more than once",
                    outcome.id
                )));
            }
            // The identifier is rendered once per outcome rather than once per row scanned.
            // `position` calls its closure for every row, so `outcome.id.to_string()` inside it
            // allocated a `String` per row per outcome — quadratic in the queue for a batch, and
            // for no reason: the value does not depend on the row.
            let wanted = outcome.id.to_string();
            let index = state
                .outbox
                .iter()
                .position(|row| row.scope == self.scope && row.raw_mutation_id == wanted)
                .ok_or_else(|| {
                    Error::protocol(format!(
                        "outcome for {} names no pending record in scope {}",
                        outcome.id, self.scope
                    ))
                })?;
            targets.push((index, outcome));
        }

        // Build the whole next state, then commit it in one step. Nothing is observable in
        // between, which is the property `apply_outcomes` promises implementors must provide.
        let mut removed = vec![false; state.outbox.len()];
        let mut retained: Vec<(usize, Option<String>)> = Vec::new();
        let mut new_dead_letters = Vec::new();
        let mut new_quarantine = Vec::new();

        for (index, outcome) in targets {
            let row = &state.outbox[index];
            match &outcome.disposition {
                // A write, not a no-op: the record was sent and is still queued, which is what an
                // attempt is. Staged with everything else so decision 003's atomicity covers the
                // increment for free (`wiki/decisions/017-bounded-retention.decision.md`).
                Disposition::Retain { reason } => retained.push((index, reason.clone())),
                Disposition::Delete => removed[index] = true,
                Disposition::DeadLetter { reason } => {
                    let record =
                        decode(row).map_err(|reason| Error::corrupt(outcome.id, reason))?;
                    new_dead_letters.push(DeadLetterRecord::from_record(
                        record,
                        now,
                        reason.clone(),
                    ));
                    removed[index] = true;
                }
                Disposition::Quarantine { reason } => {
                    new_quarantine.push(quarantine_from(row, reason.clone(), now));
                    removed[index] = true;
                }
            }
        }

        for (index, reason) in retained {
            state.outbox[index].attempts = state.outbox[index].attempts.saturating_add(1);
            // Written in the same step as the increment, so the two always describe the same
            // verdict, and truncated because core states the bound and the store keeps it
            // (`wiki/decisions/033-last-error-on-the-record.decision.md`).
            state.outbox[index].last_error = reason.map(|reason| truncate_error(&reason));
        }

        let mut index = 0;
        state.outbox.retain(|_| {
            let keep = !removed[index];
            index += 1;
            keep
        });
        state.dead_letters.extend(new_dead_letters);
        state.quarantine.extend(new_quarantine);
        Ok(())
    }
}
