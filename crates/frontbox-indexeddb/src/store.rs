//! The outbox, dead-letter and quarantine traits over IndexedDB.

use std::collections::HashSet;

use frontbox::{
    truncate_error, CoalescingEnqueue, CoalescingPolicy, DeadLetterRecord, DeadLetterStore,
    Disposition, Error, MutationId, MutationIntent, OutboxRecord, OutboxStore, Outcome,
    QuarantineStore, QuarantinedRecord, ScopeKey,
};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use crate::backend::{IdbStore, DEAD_LETTERS, OUTBOX, QUARANTINE};
use crate::convert::{to_js, DeadLetterRow, OutboxRow, QuarantineRow};
use crate::request::{await_request, js_error};
use crate::scan::all_in_scope;

impl IdbStore {
    /// Every outbox row in this scope, in `seq` order, still undecoded.
    async fn raw_rows(&self) -> Result<Vec<OutboxRow>, Error> {
        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readonly)?;
        let store = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        // `decodable` is right here and only because of the sweep: `pending_batch` and
        // `pending_count` are specified to exclude rows that will not decode, and `sweep_corrupt`
        // below is what turns "excluded" into "quarantined and visible".
        let mut rows: Vec<OutboxRow> = all_in_scope(&store, &self.scope).await?.decodable();
        // The scope index gives no ordering guarantee across its own entries, so the sort is what
        // makes `seq` the replay order rather than an accident of insertion.
        rows.sort_by_key(|row| row.seq.unwrap_or_default());
        Ok(rows)
    }
}

impl OutboxStore for IdbStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    /// Decision 031's cross-realm half, through Web Locks. See `crate::locks`.
    async fn claim_drain(&self) -> Result<Option<frontbox::DrainLease>, Error> {
        crate::locks::claim(&self.scope).await
    }

    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error> {
        let row = OutboxRow::from_intent(&intent, &self.scope)?;
        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        // `seq` is omitted from the value, so the store's own generator assigns it inside this
        // request — decision 016's "inside the insert's own transaction", met by the platform.
        await_request(store.add(&to_js(&row)?).map_err(js_error)?).await?;
        transaction.commit().await
    }

    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error> {
        let rows = self.raw_rows().await?;
        let mut records: Vec<OutboxRecord> = rows
            .iter()
            .filter_map(|row| row.decode(&self.scope).ok())
            .collect();
        records.truncate(limit);
        Ok(records)
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

    async fn pending_count(&self) -> Result<usize, Error> {
        Ok(self
            .raw_rows()
            .await?
            .iter()
            .filter(|row| row.decode(&self.scope).is_ok())
            .count())
    }

    async fn sweep_corrupt(&self) -> Result<usize, Error> {
        let now = self.backend.now();
        let transaction = self
            .backend
            .transaction(&[OUTBOX, QUARANTINE], IdbTransactionMode::Readwrite)?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        let quarantine = transaction
            .inner()
            .object_store(QUARANTINE)
            .map_err(js_error)?;

        let scan = all_in_scope::<OutboxRow>(&outbox, &self.scope).await?;
        // Two kinds of corruption, one destination. A row that parsed but whose contents are
        // unusable is the case this always handled; a stored object that does not match the row
        // type at all is the case that used to disappear before it got here.
        let corrupt: Vec<(JsValue, QuarantinedRecord)> = scan
            .rows
            .iter()
            .filter_map(|entry| {
                entry.row.decode(&self.scope).err().map(|reason| {
                    (
                        // The key the scan read, not one rebuilt from `seq`. A row whose `seq` is
                        // absent used to reconstruct as `0` here, which is a real key belonging to
                        // a real and possibly healthy record.
                        entry.key.clone(),
                        entry.row.quarantined(&self.scope, reason, now),
                    )
                })
            })
            .chain(scan.undecodable.iter().map(|entry| {
                (
                    entry.key.clone(),
                    // No identifier, no method and no path, because the object that would carry
                    // them is the object that did not parse. `QuarantinedRecord` is built for
                    // exactly this: an unreadable `raw_mutation_id` leaves `mutation_id` as `None`,
                    // which is the id-less path no `Outcome` can name and only a sweep can reach
                    // (`src/record/terminal.rs`). The stored text is kept verbatim as the body, so
                    // whatever was in there is still inspectable afterwards.
                    QuarantinedRecord::from_raw(
                        String::new(),
                        String::new(),
                        String::new(),
                        entry.text.clone(),
                        0,
                        self.scope.clone(),
                        "stored row does not match the outbox schema".to_owned(),
                        now,
                    ),
                )
            }))
            .collect();

        for (key, record) in &corrupt {
            let stored = QuarantineRow::from_record(record);
            await_request(quarantine.add(&to_js(&stored)?).map_err(js_error)?).await?;
            await_request(outbox.delete(key).map_err(js_error)?).await?;
        }
        transaction.commit().await?;
        Ok(corrupt.len())
    }

    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error> {
        let now = self.backend.now();
        // **One transaction over all three stores.** Opening three would leave the outbox delete
        // and the dead-letter insert independently committable, which is the torn state the trait
        // exists to make unrepresentable. Everything below awaits IDB requests and nothing else,
        // which is what keeps this transaction alive across the whole sequence.
        let transaction = self.backend.transaction(
            &[OUTBOX, DEAD_LETTERS, QUARANTINE],
            IdbTransactionMode::Readwrite,
        )?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        let dead_letters = transaction
            .inner()
            .object_store(DEAD_LETTERS)
            .map_err(js_error)?;
        let quarantine = transaction
            .inner()
            .object_store(QUARANTINE)
            .map_err(js_error)?;

        // Sweep-reachable, like every other outbox read: what this drops is quarantinable. The
        // keys are kept because every disposition below except `Retain` deletes the row it names,
        // and the key the scan read is the only one that is certainly the row's own.
        let rows = all_in_scope::<OutboxRow>(&outbox, &self.scope).await?.rows;

        let mut seen: HashSet<MutationId> = HashSet::with_capacity(outcomes.len());
        let mut targets = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            if !seen.insert(outcome.id) {
                return Err(Error::protocol(format!(
                    "outcome set names {} more than once",
                    outcome.id
                )));
            }
            let row = rows
                .iter()
                .find(|entry| entry.row.mutation_id == outcome.id.to_string())
                .ok_or_else(|| {
                    Error::protocol(format!(
                        "outcome for {} names no pending record in scope {}",
                        outcome.id, self.scope
                    ))
                })?;
            targets.push((row, outcome));
        }

        for (entry, outcome) in targets {
            match &outcome.disposition {
                Disposition::Retain { reason } => {
                    let mut updated = entry.row.clone();
                    updated.attempts = updated.attempts.saturating_add(1);
                    updated.last_error = reason.as_deref().map(truncate_error);
                    // A `Retain` is a verdict, and a verdict cannot exist without a request. Set
                    // here as well as in `read_for_send`, because `apply_outcomes` is public and a
                    // direct caller may have done its own sending (conformance case 80).
                    updated.transport_started = true;
                    // No explicit key: `seq` is the store's key path, so a `put` of the round-tripped
                    // value replaces the row in place.
                    await_request(outbox.put(&to_js(&updated)?).map_err(js_error)?).await?;
                }
                Disposition::Delete => {
                    delete_key(&outbox, &entry.key).await?;
                }
                Disposition::DeadLetter { reason } => {
                    let record = entry
                        .row
                        .decode(&self.scope)
                        .map_err(|reason| Error::corrupt(outcome.id, reason))?;
                    let letter = DeadLetterRecord::from_record(record, now, reason.clone());
                    let stored = DeadLetterRow::from_record(&letter)?;
                    await_request(dead_letters.add(&to_js(&stored)?).map_err(js_error)?).await?;
                    delete_key(&outbox, &entry.key).await?;
                }
                Disposition::Quarantine { reason } => {
                    let record = entry.row.quarantined(&self.scope, reason.clone(), now);
                    let stored = QuarantineRow::from_record(&record);
                    await_request(quarantine.add(&to_js(&stored)?).map_err(js_error)?).await?;
                    delete_key(&outbox, &entry.key).await?;
                }
                other => {
                    return Err(Error::protocol(format!(
                        "unrecognised disposition {other:?}"
                    )))
                }
            }
        }

        if self.backend.take_apply_failure() {
            // Aborted rather than merely returned from: dropping the handle would let the browser
            // commit what has already been written. The abort is what makes the rollback real, and
            // it is what case 09 and case 50 are actually checking on this backend.
            transaction.abort();
            return Err(Error::storage_opaque());
        }
        transaction.commit().await
    }
}

/// # What happens to a dead letter that does not match its row type
///
/// **It is dropped from the listing, and that is a real loss rather than a tidy default.** A
/// terminal store has no sweep — a dead letter has no next state, and quarantining a quarantine
/// entry names nothing new — so unlike the outbox there is nowhere for such a row to become
/// visible. It stays in storage and nothing here can show it.
///
/// It is dropped rather than raised because the alternative is worse in the direction that matters:
/// the trait returns `Vec<DeadLetterRecord>`, so reporting the bad row means failing the whole
/// call, and this listing is what a person reads to find out what the server refused. One
/// unreadable entry would hide every readable one beside it, turning a lost record into a lost
/// view.
///
/// Fixing it properly needs a listing that can carry per-row failures, which is a core trait change
/// affecting every backend. Recorded in `wiki/compatibility/indexeddb-adapter.compat.md` and as
/// entry 25 of `wiki/references/open-decisions.reference.md`.
impl DeadLetterStore for IdbStore {
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error> {
        let transaction = self
            .backend
            .transaction(&[DEAD_LETTERS], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(DEAD_LETTERS)
            .map_err(js_error)?;
        // Dropped, not raised — see the note on this `impl`.
        let rows: Vec<DeadLetterRow> = all_in_scope(&store, &self.scope).await?.decodable();
        let mut found: Vec<DeadLetterRecord> = rows
            .iter()
            .filter_map(|row| row.decode(&self.scope))
            .collect();
        // Sorted before truncating, because the index hands rows back in key order and the trait
        // asks for timestamp order. Truncating first would return a different *set* of records
        // than the other backends, not merely a different sequence — see `DeadLetterStore::list`.
        found.sort_by_key(|record| (record.rejected_at, record.mutation_id));
        found.truncate(limit);
        Ok(found)
    }

    async fn count(&self) -> Result<usize, Error> {
        let transaction = self
            .backend
            .transaction(&[DEAD_LETTERS], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(DEAD_LETTERS)
            .map_err(js_error)?;
        // Dropped, not raised — see the note on this `impl`.
        let rows: Vec<DeadLetterRow> = all_in_scope(&store, &self.scope).await?.decodable();
        Ok(rows.len())
    }

    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error> {
        let transaction = self
            .backend
            .transaction(&[DEAD_LETTERS], IdbTransactionMode::Readwrite)?;
        let store = transaction
            .inner()
            .object_store(DEAD_LETTERS)
            .map_err(js_error)?;

        // The same scan every other read here uses, rather than a second hand-rolled copy of it.
        // It was one: `get_all`, `get_all_keys`, zip by position — which is `all_in_scope`'s whole
        // body, so the two could drift on the property that makes either of them correct.
        //
        // An unreadable row is left alone rather than purged. It has no readable `rejected_at`, so
        // deleting it would be purging on age without knowing the age — and this is a purge, not a
        // sweep: the row is unreadable, not expired.
        let scan = all_in_scope::<DeadLetterRow>(&store, &self.scope).await?;
        let mut removed = 0;
        for entry in &scan.rows {
            if entry.row.rejected_at < cutoff_ms {
                delete_key(&store, &entry.key).await?;
                removed += 1;
            }
        }
        transaction.commit().await?;
        Ok(removed)
    }
}

impl QuarantineStore for IdbStore {
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error> {
        let transaction = self
            .backend
            .transaction(&[QUARANTINE], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(QUARANTINE)
            .map_err(js_error)?;
        // The same terminal-store limit as the dead letters above: dropped, not raised, so one
        // unreadable entry cannot hide the rest of the quarantine.
        let rows: Vec<QuarantineRow> = all_in_scope(&store, &self.scope).await?.decodable();
        let mut found: Vec<QuarantinedRecord> =
            rows.iter().map(|row| row.decode(&self.scope)).collect();
        // As on dead letters: the trait states the order, so the backend sorts rather than
        // inheriting whatever the scope index produced.
        found.sort_by(|left, right| {
            (left.quarantined_at, &left.raw_mutation_id)
                .cmp(&(right.quarantined_at, &right.raw_mutation_id))
        });
        found.truncate(limit);
        Ok(found)
    }

    async fn count(&self) -> Result<usize, Error> {
        let transaction = self
            .backend
            .transaction(&[QUARANTINE], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(QUARANTINE)
            .map_err(js_error)?;
        // The same terminal-store limit as the dead letters above: dropped, not raised, so one
        // unreadable entry cannot hide the rest of the quarantine.
        let rows: Vec<QuarantineRow> = all_in_scope(&store, &self.scope).await?.decodable();
        Ok(rows.len())
    }
}

/// Remove one row by the primary key the scan read off the store.
async fn delete_key(store: &IdbObjectStore, key: &JsValue) -> Result<(), Error> {
    await_request(store.delete(key).map_err(js_error)?).await?;
    Ok(())
}
