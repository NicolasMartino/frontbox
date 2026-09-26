//! Application format upgrades keep the original object-store keys and audit fields.

use std::collections::HashSet;

use frontbox::{DrainLease, Error, MigrationReport, MigrationStore, StorageMigration};
use web_sys::IdbTransactionMode;

use crate::backend::{IdbStore, DEAD_LETTERS, OUTBOX, ROWS};
use crate::convert::{to_js, DeadLetterRow, OutboxRow, RowRecord};
use crate::request::{await_request, js_error};
use crate::scan::all_in_scope;

impl MigrationStore for IdbStore {
    async fn migrate(
        &self,
        migration: &impl StorageMigration,
    ) -> Result<Option<MigrationReport>, Error> {
        let Some(_lease) = DrainLease::claim(self).await? else {
            return Ok(None);
        };
        let transaction = self
            .backend
            .transaction(&[OUTBOX, DEAD_LETTERS, ROWS], IdbTransactionMode::Readwrite)?;
        let outbox = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        let dead_letters = transaction
            .inner()
            .object_store(DEAD_LETTERS)
            .map_err(js_error)?;
        let rows = transaction.inner().object_store(ROWS).map_err(js_error)?;
        let pending = all_in_scope::<OutboxRow>(&outbox, &self.scope).await?;
        let letters = all_in_scope::<DeadLetterRow>(&dead_letters, &self.scope).await?;
        let cached = all_in_scope::<RowRecord>(&rows, &self.scope).await?;
        if !pending.undecodable.is_empty()
            || !letters.undecodable.is_empty()
            || !cached.undecodable.is_empty()
        {
            return Err(Error::protocol(
                "undecodable record prevents storage migration",
            ));
        }
        // Decode everything before invoking callbacks. Unlike projection reads, a migration
        // cannot quietly omit a row and then report that the scope is ready for the new codec.
        let pending = pending
            .rows
            .into_iter()
            .map(|entry| {
                let record = entry.row.decode(&self.scope).map_err(Error::protocol)?;
                if entry.row.seq.is_none() {
                    return Err(Error::protocol("missing migration sequence"));
                }
                Ok((entry, record))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let letters = letters
            .rows
            .into_iter()
            .map(|entry| {
                let record = entry.row.decode(&self.scope).ok_or_else(|| {
                    Error::protocol("undecodable rejected edit prevents storage migration")
                })?;
                Ok((entry, record))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let mut ids: HashSet<_> = pending
            .iter()
            .map(|(_, record)| record.mutation_id)
            .chain(letters.iter().map(|(_, record)| record.mutation_id))
            .collect();
        let mut writes = Vec::new();
        let mut report = MigrationReport::default();
        for (mut entry, record) in pending {
            if let Some(update) = migration.pending(&record)? {
                if let Some(id) = update.replacement_id {
                    if !ids.insert(id) {
                        return Err(Error::protocol("migration identifier collision"));
                    }
                    entry.row.mutation_id = id.to_string();
                }
                entry.row.raw_body =
                    serde_json::to_string(&update.payload.body).map_err(Error::serialization)?;
                entry.row.op_name = update.payload.op.as_ref().map(|op| op.name.clone());
                entry.row.op_version = update.payload.op.and_then(|op| op.version);
                entry.row.precondition = update.payload.precondition;
                // Outbox uses an inline key. It must still identify exactly the entry read.
                let key = to_js(&entry.row.seq.expect("validated above"))?;
                if key != entry.key {
                    return Err(Error::protocol(
                        "migration sequence differs from stored key",
                    ));
                }
                writes.push((outbox.clone(), None, to_js(&entry.row)?));
                report.pending += 1;
            }
        }
        for (mut entry, record) in letters {
            if let Some(update) = migration.dead_letter(&record)? {
                entry.row.raw_body =
                    serde_json::to_string(&update.body).map_err(Error::serialization)?;
                entry.row.op_name = update.op.as_ref().map(|op| op.name.clone());
                entry.row.op_version = update.op.and_then(|op| op.version);
                entry.row.precondition = update.precondition;
                // Dead letters have out-of-line auto-increment keys. Reusing the actual key
                // preserves the record, including unknown rejection fields encoded by the backend.
                writes.push((dead_letters.clone(), Some(entry.key), to_js(&entry.row)?));
                report.dead_letters += 1;
            }
        }
        for mut entry in cached.rows {
            let blob = serde_json::from_str(&entry.row.blob).map_err(Error::serialization)?;
            let mut record = entry.row.decode();
            record.blob = blob;
            if let Some(blob) = migration.row(&record)? {
                entry.row.blob = serde_json::to_string(&blob).map_err(Error::serialization)?;
                writes.push((rows.clone(), None, to_js(&entry.row)?));
                report.rows += 1;
            }
        }
        // No storage write starts until every callback and serialization has succeeded.
        // Every await below belongs to this transaction; cancellation aborts through Txn::drop.
        for (store, key, value) in writes {
            let request = match key {
                Some(key) => store.put_with_key(&value, &key),
                None => store.put(&value),
            }
            .map_err(js_error)?;
            await_request(request).await?;
        }
        #[cfg(feature = "testing")]
        if self.backend.take_migration_failure() {
            return Err(Error::storage_message(
                "injected migration failure before commit",
            ));
        }
        transaction.commit().await?;
        Ok(Some(report))
    }
}
