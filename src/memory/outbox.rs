//! The outbox half of the in-memory backend.
//!
//! Split from `super` for length only; `InMemoryStore` is one type with its storage traits
//! implemented across three files. The atomicity argument lives on `apply_outcomes` below.

use std::collections::HashSet;

use crate::error::Error;
use crate::id::MutationId;
use crate::record::{truncate_error, DeadLetterRecord, MutationIntent, OutboxRecord};
use crate::scope::ScopeKey;
use crate::store::{
    CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal, Disposition, OutboxStore, Outcome,
};

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
            transport_started: false,
        };
        state.outbox.push(row);
        Ok(())
    }

    async fn enqueue_coalescing(
        &self,
        intent: MutationIntent,
        policy: CoalescingPolicy,
    ) -> Result<CoalescingEnqueue, Error> {
        self.backend.check(StoreOp::EnqueueCoalescing)?;

        let mutation_id = intent.mutation_id;
        let raw_body = serde_json::to_string(&intent.body).map_err(Error::serialization)?;

        // The borrow is confined to this block: the append path below awaits `enqueue`, and holding
        // a `RefCell` across an await is how a re-entrant caller panics on an already-borrowed
        // state. One borrow covers the match and the write, which is this backend's stand-in for
        // "one transaction" — a durable backend owes the same, and owes it against `read_for_send`
        // too.
        let decision = {
            let mut state = self.backend.state.borrow_mut();
            match &intent.row {
                None => Err(CoalescingRefusal::Unbound),
                Some(row) => {
                    let mut matches = state.outbox.iter_mut().filter(|candidate| {
                        candidate.scope == self.scope
                            && candidate.row.as_ref() == Some(row)
                            && candidate.method == intent.method
                            && candidate.path == intent.path
                            && decode(candidate).is_ok()
                    });
                    match (matches.next(), matches.next()) {
                        (None, _) => Err(CoalescingRefusal::MissingMatch),
                        (Some(_), Some(_)) => Err(CoalescingRefusal::AmbiguousMatch),
                        (Some(found), None) if found.transport_started => {
                            Err(CoalescingRefusal::TransportStarted)
                        }
                        (Some(found), None) => {
                            // The queued slot and its guard stay; everything describing the body is
                            // the caller's newer one. `attempts` and `last_error` need no rule: a
                            // row that is not transport-started has received no verdict.
                            let kept = found
                                .raw_mutation_id
                                .parse()
                                .expect("a decodable row has a parseable identifier");
                            found.raw_body = raw_body;
                            found.op = intent.op.clone();
                            found.traceparent = intent.traceparent.clone();
                            found.created_at = intent.created_at;
                            Ok(kept)
                        }
                    }
                }
            }
        };

        let reason = match decision {
            Ok(kept) => {
                return Ok(CoalescingEnqueue::Replaced {
                    kept,
                    discarded: mutation_id,
                })
            }
            Err(reason) => reason,
        };

        match policy {
            // Every non-replacement case queues the write. A caller reaching for this policy has
            // said it holds a precondition valid now, so appending is what `enqueue` would have
            // done and is never worse than handing back an `Ok` for a dropped write.
            CoalescingPolicy::AppendIfMissing => {
                self.enqueue(intent).await?;
                Ok(CoalescingEnqueue::Appended { mutation_id })
            }
            CoalescingPolicy::RequireExisting => Ok(CoalescingEnqueue::NotQueued {
                mutation_id,
                reason,
            }),
        }
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

    /// The same read as `pending_batch`, plus the durable mark, under one borrow.
    ///
    /// Marked before the caller can send, never cleared: see the trait's contract for why the fact
    /// is recorded ahead of the request instead of inferred from how the request failed.
    async fn read_for_send(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        self.backend.check(StoreOp::ReadForSend)?;

        let mut state = self.backend.state.borrow_mut();
        let mut found: Vec<(usize, OutboxRecord)> = state
            .outbox
            .iter()
            .enumerate()
            .filter(|(_, row)| row.scope == self.scope)
            .filter_map(|(index, row)| decode(row).ok().map(|record| (index, record)))
            .collect();
        found.sort_by_key(|(_, record)| record.order_key());
        found.truncate(limit);

        for (index, _) in &found {
            state.outbox[*index].transport_started = true;
        }
        Ok(found.into_iter().map(|(_, record)| record).collect())
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
            // A `Retain` *is* a verdict, and a verdict cannot exist without a request — so whoever
            // applied this had the record sent, whether or not they went through `read_for_send`.
            // `apply_outcomes` is public and this trait says a backend cannot assume the runner is
            // its only caller, so the mark is set here too or a direct caller's queue stays
            // coalescible after the server has seen it (conformance case 80).
            state.outbox[index].transport_started = true;
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
