//! Decision 032's row store over IndexedDB.

use std::collections::HashSet;

use frontbox::{Error, RowRef, RowStore, StoredRow};
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

use crate::backend::{IdbStore, OUTBOX, ROWS};
use crate::convert::{from_js, to_js, OutboxRow, RowRecord};
use crate::request::{await_request, js_error};
use crate::scan::all_in_scope;

impl IdbStore {
    /// The composite key IndexedDB stores a row under.
    fn row_key(&self, row: &RowRef) -> js_sys::Array {
        let key = js_sys::Array::new();
        key.push(&JsValue::from_str(self.scope.as_str()));
        key.push(&JsValue::from_str(&row.entity));
        key.push(&JsValue::from_str(&row.row_id));
        key
    }
}

impl RowStore for IdbStore {
    async fn get_row(&self, row: &RowRef) -> Result<Option<StoredRow>, Error> {
        let transaction = self
            .backend
            .transaction(&[ROWS], IdbTransactionMode::Readonly)?;
        let store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        let value = await_request(store.get(&self.row_key(row)).map_err(js_error)?).await?;
        Ok(from_js::<RowRecord>(&value).map(|record| record.decode()))
    }

    async fn list_rows(&self, entity: &str, limit: usize) -> Result<Vec<StoredRow>, Error> {
        let transaction = self
            .backend
            .transaction(&[ROWS], IdbTransactionMode::Readonly)?;
        let store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        // The index is on `scope` alone, so the entity filter happens here. A compound index would
        // be faster and is deliberately not the first version: the correctness this backend has to
        // establish is the merge rule, and an index that is wrong is harder to notice than a filter.
        // A row record that does not match its type is dropped from the projection. It is not
        // lost the way an outbox row was — nothing deletes it and the next schema-compatible read
        // returns it — but it is invisible while it lasts. See the adapter compatibility page.
        let mut rows: Vec<RowRecord> = all_in_scope(&store, &self.scope).await?.decodable();
        rows.retain(|record| record.entity == entity);
        rows.sort_by(|a, b| a.row_id.cmp(&b.row_id));
        rows.truncate(limit);
        Ok(rows.iter().map(RowRecord::decode).collect())
    }

    async fn put_rows(&self, rows: &[StoredRow]) -> Result<(), Error> {
        let transaction = self
            .backend
            .transaction(&[ROWS], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        for stored in rows {
            let record = RowRecord::from_stored(stored, &self.scope)?;
            await_request(store.put(&to_js(&record)?).map_err(js_error)?).await?;
        }
        transaction.commit().await
    }

    async fn merge_rows(&self, rows: &[StoredRow]) -> Result<Vec<RowRef>, Error> {
        // **The method the whole crate's transaction discipline exists for.** It reads the queue
        // and writes the rows, and those two have to be one unit: a mutation enqueued between the
        // read and the write would have its row overwritten by the server's copy, which is exactly
        // the failure the merge rule prevents, reintroduced as a race.
        //
        // Both stores are named in one transaction, and every await below is an IDB request. Awaiting
        // anything else here — a timer, a fetch, a channel — would let the transaction go inactive
        // and the writes would throw `TransactionInactiveError` partway through.
        let transaction = self
            .backend
            .transaction(&[ROWS, OUTBOX], IdbTransactionMode::Readwrite)?;
        let rows_store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;

        // Sweep-reachable: this is the outbox, so what does not decode is quarantinable.
        let queued: Vec<OutboxRow> = all_in_scope(&outbox, &self.scope).await?.decodable();
        // A set, not a list: the membership test runs once per incoming row, so a scan makes a
        // hydration quadratic in the size of the queue.
        let protected: HashSet<RowRef> = queued.iter().filter_map(OutboxRow::bound_row).collect();

        let mut skipped = Vec::new();
        for stored in rows {
            if protected.contains(&stored.row) {
                skipped.push(stored.row.clone());
                continue;
            }
            let record = RowRecord::from_stored(stored, &self.scope)?;
            await_request(rows_store.put(&to_js(&record)?).map_err(js_error)?).await?;
        }
        transaction.commit().await?;
        Ok(skipped)
    }

    async fn delete_rows(&self, rows: &[RowRef]) -> Result<usize, Error> {
        let transaction = self
            .backend
            .transaction(&[ROWS], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        let mut removed = 0;
        for row in rows {
            // Checked before deleting, because IndexedDB's `delete` succeeds whether or not
            // anything was there and the count has to mean "removed".
            let key = self.row_key(row);
            let existing = await_request(store.get(&key).map_err(js_error)?).await?;
            if !existing.is_undefined() && !existing.is_null() {
                await_request(store.delete(&key).map_err(js_error)?).await?;
                removed += 1;
            }
        }
        transaction.commit().await?;
        Ok(removed)
    }

    async fn set_stale(&self, rows: &[RowRef], stale: bool) -> Result<usize, Error> {
        let transaction = self
            .backend
            .transaction(&[ROWS], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(ROWS).map_err(js_error)?;
        let mut changed = 0;
        for row in rows {
            let key = self.row_key(row);
            let value = await_request(store.get(&key).map_err(js_error)?).await?;
            let Some(mut record) = from_js::<RowRecord>(&value) else {
                continue;
            };
            // Only a real change counts, which is what the trait promises and what the other two
            // backends do.
            if record.stale != stale {
                record.stale = stale;
                await_request(store.put(&to_js(&record)?).map_err(js_error)?).await?;
                changed += 1;
            }
        }
        transaction.commit().await?;
        Ok(changed)
    }
}
