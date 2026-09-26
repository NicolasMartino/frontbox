//! Scoped format changes are staged before any in-memory state is replaced.

use std::collections::HashSet;

use crate::{DrainLease, Error, MigrationReport, MigrationStore, StorageMigration};

use super::{decode, InMemoryStore, StoreOp};

impl MigrationStore for InMemoryStore {
    async fn migrate(
        &self,
        migration: &impl StorageMigration,
    ) -> Result<Option<MigrationReport>, Error> {
        let Some(_lease) = DrainLease::claim(self).await? else {
            return Ok(None);
        };
        let mut state = self.backend.state.borrow_mut();
        let mut pending = state
            .outbox
            .iter()
            .enumerate()
            .filter(|(_, row)| row.scope == self.scope)
            .map(|(index, row)| Ok((index, row.clone(), decode(row).map_err(Error::protocol)?)))
            .collect::<Result<Vec<_>, Error>>()?;
        let mut letters: Vec<_> = state
            .dead_letters
            .iter()
            .enumerate()
            .filter(|(_, row)| row.scope == self.scope)
            .map(|(index, row)| (index, row.clone()))
            .collect();
        let mut rows: Vec<_> = state
            .rows
            .iter()
            .filter(|((scope, _), _)| *scope == self.scope)
            .map(|(key, row)| (key.clone(), row.clone()))
            .collect();
        let mut ids: HashSet<_> = pending
            .iter()
            .map(|(_, _, row)| row.mutation_id)
            .chain(letters.iter().map(|(_, row)| row.mutation_id))
            .collect();
        let mut report = MigrationReport::default();
        for (_, raw, record) in &mut pending {
            if let Some(update) = migration.pending(record)? {
                if let Some(id) = update.replacement_id {
                    if !ids.insert(id) {
                        return Err(Error::protocol("migration identifier collision"));
                    }
                    raw.raw_mutation_id = id.to_string();
                }
                raw.raw_body =
                    serde_json::to_string(&update.payload.body).map_err(Error::serialization)?;
                raw.op = update.payload.op;
                raw.precondition = update.payload.precondition;
                report.pending += 1;
            }
        }
        for (_, record) in &mut letters {
            if let Some(update) = migration.dead_letter(record)? {
                record.body = update.body;
                record.op = update.op;
                record.precondition = update.precondition;
                report.dead_letters += 1;
            }
        }
        for (_, row) in &mut rows {
            if let Some(blob) = migration.row(row)? {
                row.blob = blob;
                report.rows += 1;
            }
        }
        if let Some(error) = state.pending_failures.remove(&StoreOp::Migrate) {
            return Err(error);
        }
        for (index, raw, _) in pending {
            state.outbox[index] = raw;
        }
        for (index, record) in letters {
            state.dead_letters[index] = record;
        }
        state.rows.extend(rows);
        Ok(Some(report))
    }
}
