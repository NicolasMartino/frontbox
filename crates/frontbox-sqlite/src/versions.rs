//! Decision 015's cache version state over SQLite.
//!
//! # A separate handle rather than another `impl` on `SqliteStore`
//!
//! [`CacheVersionStore`] and [`OutboxStore`](frontbox::OutboxStore) both declare `scope()`, so a
//! type implementing both would make every existing `store.scope()` call ambiguous and force
//! disambiguation at sites that have nothing to do with the cache. The in-memory backend split the
//! two for the same reason, and the split says something true besides: the outbox holds raw HTTP
//! envelopes and knows nothing about entities, while this table holds entities and knows nothing
//! about HTTP.
//!
//! Both handles share one [`SqliteBackend`], so the scope filter they enforce is one rule applied
//! twice rather than two rules that have to agree.

use frontbox::{CacheVersion, CacheVersionStore, EntityState, Error, ScopeKey};
use rusqlite::{params, OptionalExtension};

use crate::backend::{storage, SqliteBackend};

/// A handle on one scope's cache version state.
#[derive(Clone)]
pub struct SqliteVersionStore {
    backend: SqliteBackend,
    scope: ScopeKey,
}

impl std::fmt::Debug for SqliteVersionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteVersionStore")
            .field("scope", &self.scope.as_str())
            .finish()
    }
}

impl SqliteVersionStore {
    pub(crate) fn new(backend: SqliteBackend, scope: ScopeKey) -> Self {
        Self { backend, scope }
    }
}

impl CacheVersionStore for SqliteVersionStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn state(&self, entity: &str) -> Result<EntityState, Error> {
        let connection = self.backend.connection().borrow();
        let found = connection
            .query_row(
                "SELECT version, stale FROM cache_versions WHERE scope = ?1 AND entity = ?2",
                params![self.scope.as_str(), entity],
                read_state,
            )
            .optional()
            .map_err(storage)?;
        // Absent is not an error. Never having heard a version is a normal starting condition, and
        // the trait says so explicitly.
        Ok(found.unwrap_or_else(EntityState::unknown))
    }

    async fn all_states(&self) -> Result<Vec<(String, EntityState)>, Error> {
        let connection = self.backend.connection().borrow();
        let mut statement = connection
            .prepare("SELECT entity, version, stale FROM cache_versions WHERE scope = ?1")
            .map_err(storage)?;
        let found = statement
            .query_map(params![self.scope.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, read_state_offset(row)?))
            })
            .map_err(storage)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage)?;
        Ok(found)
    }

    async fn put(&self, updates: &[(String, EntityState)]) -> Result<(), Error> {
        // Rejected before anything is written, so a duplicate leaves the table untouched rather
        // than half-applied. The trait rejects a repeated name because the two entries may
        // disagree and picking one would make argument order the arbiter.
        for (index, (entity, _)) in updates.iter().enumerate() {
            if updates[..index].iter().any(|(seen, _)| seen == entity) {
                return Err(Error::protocol(format!(
                    "one entity named twice in a single put: {entity:?}"
                )));
            }
        }

        let mut connection = self.backend.connection().borrow_mut();
        // `IMMEDIATE` for the same reason `apply_outcomes` uses it: the write lock is taken up
        // front, so the transaction cannot fail partway through work already applied. Decision 015
        // makes version and staleness one atomically written unit, and a torn write here leaves a
        // client believing invalidated data is fresh with nothing to detect it.
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        for (entity, state) in updates {
            transaction
                .execute(
                    "INSERT INTO cache_versions (scope, entity, version, stale) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT (scope, entity) DO UPDATE SET \
                     version = excluded.version, stale = excluded.stale",
                    params![
                        self.scope.as_str(),
                        entity,
                        // `None` becomes SQL `NULL`, never `''`. The two are different states and
                        // the schema's own note says why collapsing them is decision 021's bug.
                        state.version.as_ref().map(CacheVersion::as_str),
                        state.stale as i64,
                    ],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(())
    }
}

/// Build a state from a `SELECT version, stale`.
fn read_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntityState> {
    build(row, 0)
}

/// The same, from a `SELECT entity, version, stale`.
fn read_state_offset(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntityState> {
    build(row, 1)
}

/// # Why this is `from_parts` and not a match on the three named constructors
///
/// A null version with `stale = 1` is a real row —
/// [`mark_all_stale`](frontbox::InvalidationRunner::mark_all_stale) writes one for every registered
/// entity no invalidation has ever named. Reconstructing through `unknown()`/`stale_at`/`fresh_at`
/// covers the other three combinations and would have to answer `unknown()` here, dropping the
/// staleness and cancelling the refetch the caller asked for. `from_parts` exists because writing
/// this function is what found that.
fn build(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<EntityState> {
    let version = row.get::<_, Option<String>>(offset)?.map(CacheVersion::new);
    Ok(EntityState::from_parts(
        version,
        row.get::<_, i64>(offset + 1)? != 0,
    ))
}
