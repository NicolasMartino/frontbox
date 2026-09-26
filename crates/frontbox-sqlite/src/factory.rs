//! The conformance factory, so this backend runs the same cases the in-memory one does.

use frontbox::testing::{
    CorruptKind, FaultInjection, RowStoreFactory, StoreFactory, VersionStoreFactory,
};
use frontbox::{Error, ManualClock, ScopeKey};
use rusqlite::params;

use crate::backend::{storage, SqliteBackend, SqliteStore};
use crate::SqliteVersionStore;

/// Opens scopes on one shared in-memory SQLite database.
///
/// In-memory rather than a temp file, and that is not a shortcut: it is the same SQL, the same
/// transactions and the same schema, so what the suite exercises is this backend's logic rather
/// than the filesystem's. What it does *not* cover is a real restart, which is why
/// [`SqliteBackend::open`] exists and why D4b is where a surviving queue gets proven.
///
/// Every scope shares one database, which is what the suite's sharing contract requires: the
/// isolation cases have to prove that scope enforcement holds when both scopes live in one
/// physical store, not merely that two files cannot see each other.
pub struct SqliteFactory {
    backend: SqliteBackend,
    clock: ManualClock,
}

impl SqliteFactory {
    /// Build a factory over a fresh database.
    ///
    /// # Panics
    ///
    /// If the schema cannot be applied, which would make every case meaningless anyway.
    #[must_use]
    pub fn new() -> Self {
        let clock = ManualClock::new(1_700_000_000_000);
        let backend = SqliteBackend::in_memory(clock.clone()).expect("schema");
        Self { backend, clock }
    }
}

impl Default for SqliteFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreFactory for SqliteFactory {
    type Store = SqliteStore;

    fn clock(&self) -> ManualClock {
        self.clock.clone()
    }

    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error> {
        Ok(self.backend.open_scope(scope))
    }

    async fn insert_corrupt_row(&self, scope: &ScopeKey, kind: CorruptKind) -> Result<(), Error> {
        let (raw_id, body, created_at) = match kind {
            CorruptKind::UnparseableId => ("not-a-uuid".to_owned(), r#"{"ok":true}"#, 1),
            CorruptKind::InvalidBody { id } => (id.to_string(), "{ not json", 1),
            CorruptKind::UnrepresentableCreatedAt { id } => {
                (id.to_string(), r#"{"ok":true}"#, i64::MAX)
            }
        };
        let connection = self.backend.connection().borrow();
        // Written through the same table the real path uses, which is only possible because the
        // schema stores the identifier and the body as `TEXT`. A schema that parsed them into typed
        // columns could not hold a corrupt row at all, and decision 006's whole position — that
        // corruption is visible rather than silently dropped — would be untestable here.
        connection
            .execute(
                "INSERT INTO outbox (scope, mutation_id, method, path, raw_body, created_at, \
                 attempts) VALUES (?1, ?2, 'POST', '/api/v1/things', ?3, ?4, 0)",
                params![scope.as_str(), raw_id, body, created_at],
            )
            .map_err(storage)?;
        Ok(())
    }
}

impl RowStoreFactory for SqliteFactory {
    // The same type as `Store`: `merge_rows` has to read the queue to know which rows it is
    // protecting, and here both live in one database behind one connection.
    type Rows = SqliteStore;

    async fn open_rows(&self, scope: ScopeKey) -> Result<Self::Rows, Error> {
        Ok(self.backend.open_scope(scope))
    }
}

impl VersionStoreFactory for SqliteFactory {
    type Versions = SqliteVersionStore;

    async fn open_versions(&self, scope: ScopeKey) -> Result<Self::Versions, Error> {
        Ok(self.backend.open_versions(scope))
    }
}

impl FaultInjection for SqliteFactory {
    async fn fail_next_apply_outcomes(&self) {
        self.backend.arm_apply_failure();
    }
}

impl frontbox::testing::MigrationFaultInjection for SqliteFactory {
    async fn fail_next_migration(&self) {
        self.backend.arm_migration_failure();
    }
}
