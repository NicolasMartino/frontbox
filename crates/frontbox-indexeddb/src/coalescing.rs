//! The two methods queued-write coalescing adds, over IndexedDB.
//!
//! Split from [`super::store`] for the reason everything in this crate splits: the four-hundred-line
//! cap. `IdbStore` is one type with `OutboxStore` implemented across two files.
//!
//! # Both are one read-write transaction, and that is the interlock
//!
//! IndexedDB serializes read-write transactions that overlap an object store, so opening both of
//! these `Readwrite` over `OUTBOX` is what makes them exclude each other. It is a requirement rather
//! than an accident of style: a replacement landing after a batch was read would rewrite a body
//! already on its way to the server, and the server's verdict on the *old* body would then delete
//! the new one without ever applying it.
//!
//! A read-only transaction for the eligibility check would lose exactly that, because IndexedDB runs
//! read-only transactions concurrently with read-write ones. `read_for_send` is `Readwrite` for the
//! same reason even though only its mark writes.
//!
//! Nothing here awaits anything but an IDB request. A transaction closes the moment the event loop
//! turns, so a `.await` on anything else would leave the writes below running against a dead
//! transaction.

use frontbox::{
    CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal, Error, MutationIntent, OutboxRecord,
};
use web_sys::IdbTransactionMode;

use crate::backend::{IdbStore, OUTBOX};
use crate::convert::{to_js, OutboxRow};
use crate::request::{await_request, js_error};
use crate::scan::{all_in_scope, Decoded};

/// Whether a stored row is the one this intent is a newer version of.
///
/// All three of row, method, and path. A store matching on the row alone would coalesce a `PUT` of a
/// profile into a `DELETE` of it.
fn matches(row: &OutboxRow, intent: &MutationIntent) -> bool {
    let Some(bound) = intent.row.as_ref() else {
        return false;
    };
    row.row_entity.as_deref() == Some(bound.entity.as_str())
        && row.row_id.as_deref() == Some(bound.row_id.as_str())
        && row.method == intent.method
        && row.path == intent.path
}

impl IdbStore {
    pub(crate) async fn enqueue_coalescing_impl(
        &self,
        intent: MutationIntent,
        policy: CoalescingPolicy,
    ) -> Result<CoalescingEnqueue, Error> {
        let mutation_id = intent.mutation_id;
        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readwrite)?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;

        // Sweep-reachable, like every other outbox read: an undecodable row is not a candidate for
        // replacement and `sweep_corrupt` is what makes it visible.
        let rows = all_in_scope::<OutboxRow>(&outbox, &self.scope).await?.rows;
        let mut found = rows
            .iter()
            .filter(|entry: &&Decoded<OutboxRow>| matches(&entry.row, &intent))
            .filter(|entry| entry.row.decode(&self.scope).is_ok());

        let refusal = match (intent.row.is_some(), found.next(), found.next()) {
            (false, _, _) => CoalescingRefusal::Unbound,
            (true, None, _) => CoalescingRefusal::MissingMatch,
            (true, Some(_), Some(_)) => CoalescingRefusal::AmbiguousMatch,
            (true, Some(entry), None) if entry.row.transport_started => {
                CoalescingRefusal::TransportStarted
            }
            (true, Some(entry), None) => {
                // The queued slot and its guard stay: `seq`, `mutation_id`, and `precondition` are
                // not touched. Everything written here describes the body, and the body is being
                // replaced. `attempts` and `last_error` need no rule — this row is not
                // transport-started, so it has received no verdict.
                let mut updated = entry.row.clone();
                updated.raw_body =
                    serde_json::to_string(&intent.body).map_err(Error::serialization)?;
                updated.op_name = intent.op.as_ref().map(|op| op.name.clone());
                updated.op_version = intent.op.as_ref().and_then(|op| op.version.clone());
                updated.traceparent = intent.traceparent.clone();
                updated.created_at = intent.created_at;

                let kept = updated
                    .mutation_id
                    .parse()
                    .expect("a decodable row has a parseable identifier");
                // No explicit key: `seq` is the store's key path and the round-tripped value still
                // carries it, so this replaces the row in place rather than adding a second one.
                await_request(outbox.put(&to_js(&updated)?).map_err(js_error)?).await?;
                transaction.commit().await?;
                return Ok(CoalescingEnqueue::Replaced {
                    kept,
                    discarded: mutation_id,
                });
            }
        };

        match policy {
            // Appending is what `enqueue` would have done, and the caller has said it holds a
            // precondition valid now. Handing back an `Ok` for a dropped write would be worse than
            // either.
            CoalescingPolicy::AppendIfMissing => {
                let row = OutboxRow::from_intent(&intent, &self.scope)?;
                // `seq` omitted, so the store's generator assigns it inside this request.
                await_request(outbox.add(&to_js(&row)?).map_err(js_error)?).await?;
                transaction.commit().await?;
                Ok(CoalescingEnqueue::Appended { mutation_id })
            }
            CoalescingPolicy::RequireExisting => {
                // Nothing was written, so there is nothing to commit; the transaction ends on its
                // own with no changes.
                Ok(CoalescingEnqueue::NotQueued {
                    mutation_id,
                    reason: refusal,
                })
            }
            // `CoalescingPolicy` is `#[non_exhaustive]`, so a future variant reaches here compiled
            // against an older backend. The two policies are opposite instructions about the
            // caller's write, so guessing either silently does the wrong thing.
            other => Err(Error::protocol(format!(
                "unrecognised coalescing policy {other:?}"
            ))),
        }
    }

    pub(crate) async fn read_for_send_impl(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboxRecord>, Error> {
        // Before the transaction, not inside it. A zero limit marks nothing and returns nothing
        // either way, but opening `Readwrite` over the outbox to discover that would block a
        // concurrent `enqueue_coalescing` for the length of a full scan — the two are meant to
        // exclude each other only when one of them is actually going to write.
        if limit == 0 {
            return Ok(Vec::new());
        }
        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readwrite)?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;

        let mut rows = all_in_scope::<OutboxRow>(&outbox, &self.scope).await?.rows;
        // The scope index gives no ordering guarantee across its own entries, so the sort is what
        // makes `seq` the replay order rather than an accident of insertion.
        rows.sort_by_key(|entry| entry.row.seq.unwrap_or_default());

        let mut records: Vec<OutboxRecord> = Vec::with_capacity(limit);
        let mut marked: Vec<OutboxRow> = Vec::with_capacity(limit);
        for entry in rows {
            if records.len() == limit {
                break;
            }
            if let Ok(record) = entry.row.decode(&self.scope) {
                let mut updated = entry.row;
                updated.transport_started = true;
                marked.push(updated);
                records.push(record);
            }
        }

        // Written after the scan rather than during it, so the read is one `get_all` request and the
        // writes are a bounded run of `put`s — the shape that keeps this transaction alive.
        for row in &marked {
            await_request(outbox.put(&to_js(row)?).map_err(js_error)?).await?;
        }
        transaction.commit().await?;
        Ok(records)
    }
}
