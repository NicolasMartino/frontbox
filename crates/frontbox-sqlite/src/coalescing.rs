//! The two methods queued-write coalescing adds, over SQLite.
//!
//! Split from `super::store` for length; `SqliteStore` is one type with `OutboxStore` implemented
//! across two files, and no public path changed.
//!
//! Both methods run inside an `Immediate` transaction, which is what makes them serialize against
//! each other. That is a requirement rather than an implementation detail: a replacement landing
//! after a batch was read would rewrite a body already on its way to the server, and the server's
//! `Applied` for the *old* body would then delete the new one without ever applying it.

use frontbox::{
    CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal, Error, MutationIntent, OutboxRecord,
};
use rusqlite::params;

use crate::backend::{storage, SqliteStore};
use crate::convert::{
    body_text, decode, insert_intent, raw_page_on, read_raw, RawRow, OUTBOX_COLUMNS,
};

impl SqliteStore {
    /// Every decodable pending row in this scope matching the intent's row, method, and path.
    ///
    /// The row binding is matched in SQL because both halves are columns; decodability is not a
    /// predicate SQL can evaluate, so that half is filtered after reading — the same split
    /// `pending_batch` makes, for the same reason.
    fn coalescing_matches(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        intent: &MutationIntent,
    ) -> Result<Vec<RawRow>, Error> {
        let Some(row) = intent.row.as_ref() else {
            return Ok(Vec::new());
        };
        let sql = format!(
            "SELECT {OUTBOX_COLUMNS} FROM outbox \
             WHERE scope = ?1 AND row_entity = ?2 AND row_id = ?3 AND method = ?4 AND path = ?5 \
             ORDER BY seq"
        );
        let mut statement = transaction.prepare(&sql).map_err(storage)?;
        let raws: Vec<RawRow> = statement
            .query_map(
                params![
                    self.scope.as_str(),
                    row.entity,
                    row.row_id,
                    intent.method,
                    intent.path
                ],
                read_raw,
            )
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage)?;
        Ok(raws
            .into_iter()
            .filter(|raw| decode(raw, &self.scope).is_ok())
            .collect())
    }
}

impl SqliteStore {
    pub(crate) async fn enqueue_coalescing_impl(
        &self,
        intent: MutationIntent,
        policy: CoalescingPolicy,
    ) -> Result<CoalescingEnqueue, Error> {
        let mutation_id = intent.mutation_id;
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;

        let matches = self.coalescing_matches(&transaction, &intent)?;
        let refusal = match (intent.row.is_some(), matches.as_slice()) {
            (false, _) => CoalescingRefusal::Unbound,
            (true, []) => CoalescingRefusal::MissingMatch,
            (true, [_, _, ..]) => CoalescingRefusal::AmbiguousMatch,
            (true, [found]) if found.transport_started => CoalescingRefusal::TransportStarted,
            (true, [found]) => {
                // The queued slot and its guard stay — `seq`, `mutation_id`, `precondition` are
                // untouched. Everything named here describes the body, and the body is being
                // replaced. `attempts` and `last_error` need no rule: this row is not
                // transport-started, so it has received no verdict and they are already 0 and NULL.
                let (op_name, op_version) = match &intent.op {
                    Some(op) => (Some(op.name.clone()), op.version.clone()),
                    None => (None, None),
                };
                let replaced = transaction
                    .execute(
                        "UPDATE outbox SET raw_body = ?2, op_name = ?3, op_version = ?4, \
                         traceparent = ?5, created_at = ?6 WHERE seq = ?1",
                        params![
                            found.seq as i64,
                            body_text(&intent.body)?,
                            op_name,
                            op_version,
                            intent.traceparent,
                            intent.created_at,
                        ],
                    )
                    .map_err(storage)?;
                // Cannot fire today: `seq` was read from this same `Immediate` transaction moments
                // ago. It is checked because the alternative to a loud failure here is a silent one
                // — `Replaced` returned for a row that was never written, telling the caller its
                // newer body is queued when the older one still is. If a future change to the match
                // query or the key ever breaks that assumption, this says so instead of losing an
                // edit.
                if replaced != 1 {
                    // Uncommitted, so the transaction rolls back on drop and the UPDATE is undone.
                    return Err(Error::storage_message(format!(
                        "coalescing replacement matched {replaced} rows, expected exactly 1"
                    )));
                }
                let kept = found
                    .raw_mutation_id
                    .parse()
                    .expect("a decodable row has a parseable identifier");
                transaction.commit().map_err(storage)?;
                return Ok(CoalescingEnqueue::Replaced {
                    kept,
                    discarded: mutation_id,
                });
            }
        };

        match policy {
            // Appending is what `enqueue` would have done, and the caller has said it holds a
            // precondition valid now. Returning "not queued" here would hand back an `Ok` for a
            // write that was silently dropped.
            CoalescingPolicy::AppendIfMissing => {
                insert_intent(&transaction, &self.scope, &intent)?;
                transaction.commit().map_err(storage)?;
                Ok(CoalescingEnqueue::Appended { mutation_id })
            }
            // Nothing was written, so there is nothing to commit. The transaction rolls back on
            // drop, which is the same outcome and one fewer thing to get wrong.
            CoalescingPolicy::RequireExisting => Ok(CoalescingEnqueue::NotQueued {
                mutation_id,
                reason: refusal,
            }),
            // `CoalescingPolicy` is `#[non_exhaustive]`, so a future variant reaches here compiled
            // against an older backend. Refusing is the only safe answer: appending and refusing
            // are opposite instructions about the caller's write, and guessing either one silently
            // does the wrong thing. Same rule as `Disposition`'s.
            _ => Err(Error::protocol(
                "unrecognised CoalescingPolicy; this backend cannot honour it",
            )),
        }
    }

    pub(crate) async fn read_for_send_impl(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboxRecord>, Error> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;

        // The same paging loop `pending_batch` runs, and for the same reason: corrupt rows are
        // skipped after decoding, so one `LIMIT` window can come back short while the queue holds
        // plenty of sendable work.
        let mut records: Vec<OutboxRecord> = Vec::with_capacity(limit);
        let mut marks: Vec<i64> = Vec::with_capacity(limit);
        let mut after_seq: i64 = 0;
        'paging: loop {
            let page = raw_page_on(&transaction, &self.scope, after_seq, limit)?;
            let Some(last) = page.last() else {
                break 'paging;
            };
            after_seq = last.seq as i64;
            for raw in &page {
                if let Ok(record) = decode(raw, &self.scope) {
                    marks.push(raw.seq as i64);
                    records.push(record);
                    if records.len() == limit {
                        break 'paging;
                    }
                }
            }
        }

        // One statement rather than one per row: the batch is bounded by `limit`, but a caller may
        // set that high and a hundred round trips through the same transaction buys nothing.
        if !marks.is_empty() {
            let placeholders = vec!["?"; marks.len()].join(", ");
            transaction
                .execute(
                    &format!(
                        "UPDATE outbox SET transport_started = 1 WHERE seq IN ({placeholders})"
                    ),
                    rusqlite::params_from_iter(marks.iter()),
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(records)
    }
}
