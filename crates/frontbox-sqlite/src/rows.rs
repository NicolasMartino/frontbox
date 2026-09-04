//! Decision 032's row store over SQLite.

use std::collections::HashSet;

use frontbox::{Error, RowRef, RowStore, StoredRow};
use rusqlite::{params, OptionalExtension};

use crate::backend::{storage, SqliteStore};

impl RowStore for SqliteStore {
    async fn get_row(&self, row: &RowRef) -> Result<Option<StoredRow>, Error> {
        let connection = self.backend.connection().borrow();
        let found = connection
            .query_row(
                "SELECT blob, stale FROM rows_store \
                 WHERE scope = ?1 AND entity = ?2 AND row_id = ?3",
                params![self.scope.as_str(), row.entity, row.row_id],
                |sql_row| read_row(row.clone(), sql_row),
            )
            .optional()
            .map_err(storage);
        found
    }

    async fn list_rows(&self, entity: &str, limit: usize) -> Result<Vec<StoredRow>, Error> {
        let connection = self.backend.connection().borrow();
        let mut statement = connection
            .prepare(
                "SELECT row_id, blob, stale FROM rows_store \
                 WHERE scope = ?1 AND entity = ?2 ORDER BY row_id LIMIT ?3",
            )
            .map_err(storage)?;
        let found = statement
            .query_map(
                params![self.scope.as_str(), entity, limit as i64],
                |sql_row| {
                    let key = RowRef::new(entity, sql_row.get::<_, String>(0)?);
                    read_row_offset(key, sql_row)
                },
            )
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage);
        found
    }

    async fn put_rows(&self, rows: &[StoredRow]) -> Result<(), Error> {
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        for stored in rows {
            upsert(&transaction, self.scope.as_str(), stored)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(())
    }

    async fn merge_rows(&self, rows: &[StoredRow]) -> Result<Vec<RowRef>, Error> {
        let mut connection = self.backend.connection().borrow_mut();
        // One transaction covering the read of the queue *and* the writes to the rows. The skip
        // decision has to be made against a view that cannot change underneath it, or a mutation
        // enqueued mid-merge would have its row overwritten by the server's copy — the exact
        // failure the rule exists to prevent, reintroduced by a race.
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;

        // A set, not a list. The membership test runs once per incoming row, so a scan makes a
        // hydration quadratic in the size of the queue — and hydrating with a large queue is the
        // case the skip rule exists for.
        let protected: HashSet<(String, String)> = {
            let mut statement = transaction
                .prepare(
                    "SELECT DISTINCT row_entity, row_id FROM outbox \
                     WHERE scope = ?1 AND row_entity IS NOT NULL AND row_id IS NOT NULL",
                )
                .map_err(storage)?;
            let found = statement
                .query_map(params![self.scope.as_str()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(storage)?
                .collect::<rusqlite::Result<HashSet<_>>>()
                .map_err(storage)?;
            found
        };

        let mut skipped = Vec::new();
        for stored in rows {
            if protected.contains(&(stored.row.entity.clone(), stored.row.row_id.clone())) {
                skipped.push(stored.row.clone());
                continue;
            }
            upsert(&transaction, self.scope.as_str(), stored)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(skipped)
    }

    async fn delete_rows(&self, rows: &[RowRef]) -> Result<usize, Error> {
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let mut removed = 0;
        for row in rows {
            removed += transaction
                .execute(
                    "DELETE FROM rows_store WHERE scope = ?1 AND entity = ?2 AND row_id = ?3",
                    params![self.scope.as_str(), row.entity, row.row_id],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(removed)
    }

    async fn set_stale(&self, rows: &[RowRef], stale: bool) -> Result<usize, Error> {
        let mut connection = self.backend.connection().borrow_mut();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let mut changed = 0;
        for row in rows {
            // `AND stale <> ?4` is what makes the count mean "changed" rather than "matched",
            // which is what the trait promises and what the in-memory backend does.
            changed += transaction
                .execute(
                    "UPDATE rows_store SET stale = ?4 \
                     WHERE scope = ?1 AND entity = ?2 AND row_id = ?3 AND stale <> ?4",
                    params![self.scope.as_str(), row.entity, row.row_id, stale as i64],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(changed)
    }
}

fn upsert(
    transaction: &rusqlite::Transaction<'_>,
    scope: &str,
    stored: &StoredRow,
) -> Result<(), Error> {
    transaction
        .execute(
            "INSERT INTO rows_store (scope, entity, row_id, blob, stale) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT (scope, entity, row_id) DO UPDATE SET \
             blob = excluded.blob, stale = excluded.stale",
            params![
                scope,
                stored.row.entity,
                stored.row.row_id,
                serde_json::to_string(&stored.blob).map_err(Error::serialization)?,
                stored.stale as i64,
            ],
        )
        .map_err(storage)?;
    Ok(())
}

/// Read a row whose key is already known, from a `SELECT blob, stale`.
fn read_row(key: RowRef, row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    build(key, row, 0)
}

/// The same, from a `SELECT row_id, blob, stale`.
fn read_row_offset(key: RowRef, row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRow> {
    build(key, row, 1)
}

fn build(key: RowRef, row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<StoredRow> {
    // A blob that will not parse is stored text this backend wrote and something else corrupted.
    // `Value::Null` keeps the row readable rather than failing the whole list, which is the same
    // posture `pending_batch` takes toward an undecodable envelope.
    let blob: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(offset)?).unwrap_or(serde_json::Value::Null);
    Ok(StoredRow::new(key, blob).with_stale(row.get::<_, i64>(offset + 1)? != 0))
}
