//! The outbox trait over SQLite. The two terminal stores are in `super::terminal`.

use std::collections::HashSet;

use frontbox::{
    truncate_error, CoalescingEnqueue, CoalescingPolicy, DeadLetterRecord, Disposition, Error,
    MutationId, MutationIntent, OutboxRecord, OutboxStore, Outcome, QuarantinedRecord, ScopeKey,
};
use rusqlite::{params, OptionalExtension};

use crate::backend::{storage, SqliteStore};
use crate::convert::{
    decode, insert_intent, quarantine_from, raw_page_on, read_raw, RawRow, OUTBOX_COLUMNS,
};
use crate::terminal::{insert_dead_letter, insert_quarantine};

impl SqliteStore {
    /// One window of outbox rows in this scope after `after_seq`, in replay order, still undecoded.
    ///
    /// Decoding happens in Rust rather than in SQL because two of the three corruption kinds —
    /// an unparseable identifier and a body that is not JSON — are not things a `WHERE` clause can
    /// express. So reads load the raw rows and filter, which is also what makes
    /// `insert_corrupt_row` possible: the schema must be able to hold a row that will not decode.
    ///
    /// The window is keyed on `seq` rather than on `OFFSET` because rows leave the outbox between
    /// pages in principle, and an offset that shifts under a deletion skips a record silently.
    /// `seq` is monotonic and never reused (`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`),
    /// so "everything after the last one I saw" is stable whatever else happens.
    fn raw_page(&self, after_seq: i64, window: usize) -> Result<Vec<RawRow>, Error> {
        let connection = self.backend.connection().borrow();
        raw_page_on(&connection, &self.scope, after_seq, window)
    }
}

impl OutboxStore for SqliteStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error> {
        let connection = self.backend.connection().borrow();
        insert_intent(&connection, &self.scope, &intent)
    }

    /// # Why this pages rather than over-reading
    ///
    /// Corrupt rows are skipped after decoding, so a single `LIMIT limit` window can come back
    /// short — or empty — while the queue holds plenty of sendable work. The first attempt at this
    /// asked SQL for `limit * 4` rows and hoped, which is a guess about how much corruption sits
    /// ahead of the next good record, and there is no number that is right. With more than
    /// `3 * limit` corrupt rows at the head, every row in the window was discarded and
    /// `pending_batch` returned nothing while [`pending_count`](Self::pending_count) correctly
    /// reported work outstanding — a store claiming to have records and refusing to name any.
    ///
    /// The runner never saw it because [`SyncRunner::sync_once`](frontbox::SyncRunner) sweeps
    /// before it reads, so the corrupt rows are in quarantine by the time the batch is taken. A
    /// direct caller does not sweep, and `TodoApp::refresh_pending` is one.
    ///
    /// So the window is a loop, not a multiplier: it keeps asking for the next page until it has
    /// `limit` decodable records or the scope runs out. In the ordinary case — no corruption — that
    /// is exactly one query, the same cost as before. Conformance case 69 seeds more corrupt rows
    /// than any fixed multiplier would survive.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut records: Vec<OutboxRecord> = Vec::with_capacity(limit);
        // `seq` is `AUTOINCREMENT` and so starts at 1; nothing is skipped by starting below it.
        let mut after_seq: i64 = 0;
        loop {
            let page = self.raw_page(after_seq, limit)?;
            let Some(last) = page.last() else {
                return Ok(records);
            };
            after_seq = last.seq as i64;
            for raw in &page {
                if let Ok(record) = decode(raw, &self.scope) {
                    records.push(record);
                    if records.len() == limit {
                        return Ok(records);
                    }
                }
            }
        }
    }

    /// # Why this reads every row
    ///
    /// The count is of *decodable* records, and decodability is not a predicate SQL can evaluate:
    /// it means the `mutation_id` parses and the body is JSON. `COUNT(*)` would count the corrupt
    /// rows that [`pending_batch`](Self::pending_batch) refuses to return, and the trait says those
    /// two numbers describe the same records.
    ///
    /// It pages for the same reason `pending_batch` does — so a large queue is never materialised
    /// in one allocation — and decodes into nothing, keeping one page live at a time.
    async fn pending_count(&self) -> Result<usize, Error> {
        const WINDOW: usize = 256;
        let mut count = 0;
        let mut after_seq: i64 = 0;
        loop {
            let page = self.raw_page(after_seq, WINDOW)?;
            let Some(last) = page.last() else {
                return Ok(count);
            };
            after_seq = last.seq as i64;
            count += page
                .iter()
                .filter(|raw| decode(raw, &self.scope).is_ok())
                .count();
        }
    }

    async fn enqueue_coalescing(
        &self,
        intent: MutationIntent,
        policy: CoalescingPolicy,
    ) -> Result<CoalescingEnqueue, Error> {
        self.enqueue_coalescing_impl(intent, policy).await
    }

    async fn read_for_send(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        self.read_for_send_impl(limit).await
    }

    async fn sweep_corrupt(&self) -> Result<usize, Error> {
        let now = self.backend.now();
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;

        let corrupt: Vec<(u64, QuarantinedRecord)> = {
            let sql = format!("SELECT {OUTBOX_COLUMNS} FROM outbox WHERE scope = ?1 ORDER BY seq");
            let mut statement = transaction.prepare(&sql).map_err(storage)?;
            let raws: Vec<RawRow> = statement
                .query_map(params![self.scope.as_str()], read_raw)
                .map_err(storage)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(storage)?;
            raws.iter()
                .filter_map(|raw| {
                    decode(raw, &self.scope)
                        .err()
                        .map(|reason| (raw.seq, quarantine_from(raw, &self.scope, reason, now)))
                })
                .collect()
        };

        for (seq, record) in &corrupt {
            insert_quarantine(&transaction, &self.scope, record)?;
            transaction
                .execute("DELETE FROM outbox WHERE seq = ?1", params![*seq as i64])
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(corrupt.len())
    }

    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        let now = self.backend.now();
        let mut connection = self.backend.connection().borrow_mut();
        // `IMMEDIATE` takes the write lock up front. Deferred would acquire it partway through and
        // could fail after work was already applied — the intermediate state the trait exists to
        // make unrepresentable.
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;

        let mut seen: HashSet<MutationId> = HashSet::with_capacity(outcomes.len());
        let mut targets = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            if !seen.insert(outcome.id) {
                return Err(Error::protocol(format!(
                    "outcome set names {} more than once",
                    outcome.id
                )));
            }
            let sql = format!(
                "SELECT {OUTBOX_COLUMNS} FROM outbox WHERE scope = ?1 AND mutation_id = ?2"
            );
            let raw = transaction
                .query_row(
                    &sql,
                    params![self.scope.as_str(), outcome.id.to_string()],
                    read_raw,
                )
                .optional()
                .map_err(storage)?
                .ok_or_else(|| {
                    Error::protocol(format!(
                        "outcome for {} names no pending record in scope {}",
                        outcome.id, self.scope
                    ))
                })?;
            targets.push((raw, outcome));
        }

        for (raw, outcome) in targets {
            match &outcome.disposition {
                Disposition::Retain { reason } => {
                    // `transport_started` too: a `Retain` is a verdict, and a verdict cannot exist
                    // without a request. `apply_outcomes` is public, so a direct caller that read
                    // with `pending_batch` and sent the batch itself must not leave the record
                    // coalescible (conformance case 80).
                    transaction
                        .execute(
                            "UPDATE outbox SET attempts = attempts + 1, last_error = ?2, \
                             transport_started = 1 WHERE seq = ?1",
                            params![raw.seq as i64, reason.as_deref().map(truncate_error)],
                        )
                        .map_err(storage)?;
                }
                Disposition::Delete => {
                    delete_row(&transaction, raw.seq)?;
                }
                Disposition::DeadLetter { reason } => {
                    let record = decode(&raw, &self.scope)
                        .map_err(|reason| Error::corrupt(outcome.id, reason))?;
                    let letter = DeadLetterRecord::from_record(record, now, reason.clone());
                    insert_dead_letter(&transaction, &self.scope, &letter)?;
                    delete_row(&transaction, raw.seq)?;
                }
                Disposition::Quarantine { reason } => {
                    let record = quarantine_from(&raw, &self.scope, reason.clone(), now);
                    insert_quarantine(&transaction, &self.scope, &record)?;
                    delete_row(&transaction, raw.seq)?;
                }
                // `Disposition` is `#[non_exhaustive]`; the trait says to refuse rather than guess.
                other => {
                    return Err(Error::protocol(format!(
                        "unrecognised disposition {other:?}"
                    )))
                }
            }
        }
        // Injected *after* every write and *before* the commit, which is the only placement that
        // proves anything. A backend that checked a flag first would demonstrate that it can
        // decline to start; failing here demonstrates that a transaction which has already
        // inserted dead letters, moved quarantine rows and deleted from the outbox rolls all of it
        // back. The in-memory backend cannot make this claim — its own docs say so, because it
        // commits by replacing state in one assignment.
        if self.backend.take_apply_failure() {
            return Err(Error::storage_opaque());
        }
        transaction.commit().map_err(storage)?;
        Ok(())
    }
}

fn delete_row(transaction: &rusqlite::Transaction<'_>, seq: u64) -> Result<(), Error> {
    transaction
        .execute("DELETE FROM outbox WHERE seq = ?1", params![seq as i64])
        .map_err(storage)?;
    Ok(())
}
