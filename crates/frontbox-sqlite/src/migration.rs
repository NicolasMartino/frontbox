//! Application storage migrations update existing rows inside one write transaction.

use std::collections::HashSet;

use frontbox::{
    DrainLease, Error, MigrationReport, MigrationStore, RowRef, StorageMigration, StoredRow,
};
use rusqlite::{params, TransactionBehavior};

use crate::backend::{storage, SqliteStore};
use crate::convert::{body_text, decode, raw_page_on, read_dead_letter};

impl MigrationStore for SqliteStore {
    async fn migrate(
        &self,
        migration: &impl StorageMigration,
    ) -> Result<Option<MigrationReport>, Error> {
        let Some(_lease) = DrainLease::claim(self).await? else {
            return Ok(None);
        };
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let pending = raw_page_on(&transaction, &self.scope, 0, i64::MAX as usize)?
            .iter()
            .map(|raw| decode(raw, &self.scope).map_err(Error::protocol))
            .collect::<Result<Vec<_>, _>>()?;
        let letters =
            {
                let mut statement = transaction.prepare(
                "SELECT mutation_id, method, path, raw_body, created_at, op_name, op_version, \
                 traceparent, precondition, attempts, rejected_at, reason_kind, reason_body, id \
                 FROM dead_letters WHERE scope = ?1 ORDER BY id"
            ).map_err(storage)?;
                let found = statement
                    .query_map([self.scope.as_str()], |row| {
                        Ok((row.get::<_, i64>(13)?, read_dead_letter(row, &self.scope)?))
                    })
                    .map_err(storage)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(storage)?;
                found
            };
        let cached = {
            let mut statement = transaction
                .prepare("SELECT entity, row_id, blob, stale FROM rows_store WHERE scope = ?1")
                .map_err(storage)?;
            let found = statement
                .query_map([self.scope.as_str()], |row| {
                    Ok((
                        RowRef::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)? != 0,
                    ))
                })
                .map_err(storage)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(storage)?;
            found
        };
        let mut ids: HashSet<_> = pending
            .iter()
            .map(|row| row.mutation_id)
            .chain(letters.iter().map(|(_, row)| row.mutation_id))
            .collect();
        let mut pending_updates = Vec::new();
        let mut letter_updates = Vec::new();
        let mut row_updates = Vec::new();
        for record in pending {
            if let Some(update) = migration.pending(&record)? {
                if let Some(id) = update.replacement_id {
                    if !ids.insert(id) {
                        return Err(Error::protocol("migration identifier collision"));
                    }
                }
                let body = body_text(&update.payload.body)?;
                pending_updates.push((record, update, body));
            }
        }
        for (key, record) in letters {
            if let Some(update) = migration.dead_letter(&record)? {
                let body = body_text(&update.body)?;
                letter_updates.push((key, update, body));
            }
        }
        for (key, text, stale) in cached {
            let blob = serde_json::from_str(&text).map_err(Error::serialization)?;
            let row = StoredRow::new(key, blob).with_stale(stale);
            if let Some(blob) = migration.row(&row)? {
                row_updates.push((row.row, body_text(&blob)?));
            }
        }
        let mut report = MigrationReport::default();
        for (record, update, body) in pending_updates {
            transaction.execute(
                "UPDATE outbox SET mutation_id = ?1, raw_body = ?2, op_name = ?3, op_version = ?4, \
                 precondition = ?5 WHERE scope = ?6 AND seq = ?7",
                params![update.replacement_id.unwrap_or(record.mutation_id).to_string(), body,
                    update.payload.op.as_ref().map(|op| &op.name),
                    update.payload.op.as_ref().and_then(|op| op.version.as_ref()),
                    update.payload.precondition, self.scope.as_str(), record.seq as i64],
            ).map_err(storage)?;
            report.pending += 1;
        }
        for (key, update, body) in letter_updates {
            transaction
                .execute(
                    "UPDATE dead_letters SET raw_body = ?1, op_name = ?2, op_version = ?3, \
                 precondition = ?4 WHERE scope = ?5 AND id = ?6",
                    params![
                        body,
                        update.op.as_ref().map(|op| &op.name),
                        update.op.as_ref().and_then(|op| op.version.as_ref()),
                        update.precondition,
                        self.scope.as_str(),
                        key
                    ],
                )
                .map_err(storage)?;
            report.dead_letters += 1;
        }
        for (key, blob) in row_updates {
            transaction.execute(
                "UPDATE rows_store SET blob = ?1 WHERE scope = ?2 AND entity = ?3 AND row_id = ?4",
                params![blob, self.scope.as_str(), key.entity, key.row_id],
            ).map_err(storage)?;
            report.rows += 1;
        }
        #[cfg(feature = "testing")]
        if self.backend.take_migration_failure() {
            return Err(Error::storage_message(
                "injected migration failure before commit",
            ));
        }
        transaction.commit().map_err(storage)?;
        Ok(Some(report))
    }
}
