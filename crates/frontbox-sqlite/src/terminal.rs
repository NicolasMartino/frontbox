//! The two terminal stores over SQLite: dead letters and quarantine.
//!
//! Split from `super::store` for length, along the line the traits already draw. Everything here is
//! a record that has left the outbox and will not be sent again; everything there is still pending.
//!
//! # The `ORDER BY` is the contract, not a convenience
//!
//! Both `list` methods order by the timestamp then the identifier, because
//! [`DeadLetterStore::list`] and [`QuarantineStore::list`] now say so. They used to order by the
//! rowid — insertion order — which agrees with the stated order right up until a caller supplies a
//! clock that does not advance monotonically, and then silently disagrees under a truncating
//! `limit`. Conformance case 68 arranges exactly that disagreement.
//!
//! `COLLATE BINARY` on the identifier columns is what makes the SQL order match the in-memory
//! backend's `sort_by_key`: Rust compares `MutationId` by its bytes and `String` by its bytes, and
//! only a binary collation does the same (`crate::schema`).

use frontbox::{
    DeadLetterRecord, DeadLetterStore, Error, QuarantineStore, QuarantinedRecord, ScopeKey,
};
use rusqlite::params;

use crate::backend::{storage, SqliteStore};
use crate::convert::{body_text, read_dead_letter, read_quarantined, reason_columns};

impl DeadLetterStore for SqliteStore {
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error> {
        let connection = self.backend.connection().borrow();
        let mut statement = connection
            .prepare(
                "SELECT mutation_id, method, path, raw_body, created_at, op_name, op_version, \
                 traceparent, precondition, attempts, rejected_at, reason_kind, reason_body \
                 FROM dead_letters WHERE scope = ?1 ORDER BY rejected_at, mutation_id LIMIT ?2",
            )
            .map_err(storage)?;
        let found = statement
            .query_map(params![self.scope.as_str(), limit as i64], |row| {
                read_dead_letter(row, &self.scope)
            })
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage);
        found
    }

    async fn count(&self) -> Result<usize, Error> {
        let connection = self.backend.connection().borrow();
        connection
            .query_row(
                "SELECT COUNT(*) FROM dead_letters WHERE scope = ?1",
                params![self.scope.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
            .map_err(storage)
    }

    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error> {
        let connection = self.backend.connection().borrow();
        connection
            .execute(
                "DELETE FROM dead_letters WHERE scope = ?1 AND rejected_at < ?2",
                params![self.scope.as_str(), cutoff_ms],
            )
            .map_err(storage)
    }
}

impl QuarantineStore for SqliteStore {
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error> {
        let connection = self.backend.connection().borrow();
        let mut statement = connection
            .prepare(
                "SELECT raw_mutation_id, method, path, raw_body, created_at, op_name, op_version, \
                 reason, quarantined_at FROM quarantine WHERE scope = ?1 \
                 ORDER BY quarantined_at, raw_mutation_id LIMIT ?2",
            )
            .map_err(storage)?;
        let found = statement
            .query_map(params![self.scope.as_str(), limit as i64], |row| {
                read_quarantined(row, &self.scope)
            })
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage);
        found
    }

    async fn count(&self) -> Result<usize, Error> {
        let connection = self.backend.connection().borrow();
        connection
            .query_row(
                "SELECT COUNT(*) FROM quarantine WHERE scope = ?1",
                params![self.scope.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
            .map_err(storage)
    }
}

pub(crate) fn insert_dead_letter(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ScopeKey,
    letter: &DeadLetterRecord,
) -> Result<(), Error> {
    let (kind, body) = reason_columns(&letter.reason);
    let (op_name, op_version) = match &letter.op {
        Some(op) => (Some(op.name.clone()), op.version.clone()),
        None => (None, None),
    };
    transaction
        .execute(
            "INSERT INTO dead_letters (scope, mutation_id, method, path, raw_body, created_at, \
             op_name, op_version, traceparent, precondition, attempts, rejected_at, reason_kind, \
             reason_body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                scope.as_str(),
                letter.mutation_id.to_string(),
                letter.method,
                letter.path,
                body_text(&letter.body)?,
                letter.created_at,
                op_name,
                op_version,
                letter.traceparent,
                letter.precondition,
                letter.attempts as i64,
                letter.rejected_at,
                kind,
                body,
            ],
        )
        .map_err(storage)?;
    Ok(())
}

pub(crate) fn insert_quarantine(
    transaction: &rusqlite::Transaction<'_>,
    scope: &ScopeKey,
    record: &QuarantinedRecord,
) -> Result<(), Error> {
    let (op_name, op_version) = match &record.op {
        Some(op) => (Some(op.name.clone()), op.version.clone()),
        None => (None, None),
    };
    transaction
        .execute(
            "INSERT INTO quarantine (scope, raw_mutation_id, method, path, raw_body, created_at, \
             op_name, op_version, reason, quarantined_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                scope.as_str(),
                record.raw_mutation_id,
                record.method,
                record.path,
                record.raw_body,
                record.created_at,
                op_name,
                op_version,
                record.reason,
                record.quarantined_at,
            ],
        )
        .map_err(storage)?;
    Ok(())
}
