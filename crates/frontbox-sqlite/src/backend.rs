//! The connection, and the handle a scope is opened through.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use frontbox::{Clock, Error, ScopeKey};
use rusqlite::Connection;

use crate::schema::{migrate, SCHEMA};

/// A SQLite database holding every scope's storage.
///
/// Cloning is cheap and shares the connection, so two clones are two handles on the same rows —
/// the same contract [`InMemoryBackend`](frontbox::InMemoryBackend) offers, and what the
/// conformance suite's sharing rule requires: two scopes must live in the same physical store or
/// the isolation cases prove nothing.
#[derive(Clone)]
pub struct SqliteBackend {
    connection: Rc<RefCell<Connection>>,
    clock: Rc<dyn Clock>,
    /// Whether the next `apply_outcomes` should fail after doing its work.
    ///
    /// Shared with every clone, so a factory can arm it and a store opened later still sees it.
    fail_next_apply: Rc<Cell<bool>>,
    #[cfg(feature = "testing")]
    fail_next_migration: Rc<Cell<bool>>,
}

impl std::fmt::Debug for SqliteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SqliteBackend(..)")
    }
}

impl SqliteBackend {
    /// Open a database file, creating the schema if it is not there.
    ///
    /// # Errors
    ///
    /// The file cannot be opened, or the schema cannot be applied.
    pub fn open(path: impl AsRef<Path>, clock: impl Clock + 'static) -> Result<Self, Error> {
        let connection = Connection::open(path).map_err(storage)?;
        Self::from_connection(connection, clock)
    }

    /// Open a private in-memory database.
    ///
    /// Used by the conformance suite, and by an application that wants the durable code path
    /// without a file. It is still this backend — same SQL, same transactions — which is what makes
    /// it worth having rather than reaching for the in-memory backend instead.
    ///
    /// # Errors
    ///
    /// The schema cannot be applied.
    pub fn in_memory(clock: impl Clock + 'static) -> Result<Self, Error> {
        let connection = Connection::open_in_memory().map_err(storage)?;
        Self::from_connection(connection, clock)
    }

    fn from_connection(connection: Connection, clock: impl Clock + 'static) -> Result<Self, Error> {
        connection.execute_batch(SCHEMA).map_err(storage)?;
        // `SCHEMA` creates what is absent; this changes what is present. An existing database
        // predating a column gets it here, because `CREATE TABLE IF NOT EXISTS` would leave it
        // exactly as it found it (`crates/frontbox-sqlite/src/schema.rs`).
        migrate(&connection).map_err(storage)?;
        Ok(Self {
            connection: Rc::new(RefCell::new(connection)),
            clock: Rc::new(clock),
            fail_next_apply: Rc::new(Cell::new(false)),
            #[cfg(feature = "testing")]
            fail_next_migration: Rc::new(Cell::new(false)),
        })
    }

    /// Open a handle scoped to `scope`.
    ///
    /// Cheap: the scope is a column, and every read filters on it. Decision 024 asks for
    /// injectivity from `ScopeKey` to physical storage, which a column satisfies without an
    /// encoding — two distinct keys are two distinct strings, and no sanitiser is in a position to
    /// collapse them.
    pub fn open_scope(&self, scope: ScopeKey) -> SqliteStore {
        SqliteStore {
            backend: self.clone(),
            scope,
        }
    }

    /// Open a cache version handle scoped to `scope`.
    ///
    /// Separate from [`open_scope`](Self::open_scope) because `CacheVersionStore` and `OutboxStore`
    /// both declare `scope()`; one type implementing both would make every existing `store.scope()`
    /// call ambiguous. Same split as the in-memory backend, and same shared connection underneath,
    /// so the two handles are two views of one database.
    pub fn open_versions(&self, scope: ScopeKey) -> crate::SqliteVersionStore {
        crate::SqliteVersionStore::new(self.clone(), scope)
    }

    pub(crate) fn connection(&self) -> &Rc<RefCell<Connection>> {
        &self.connection
    }

    pub(crate) fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Arm a single `apply_outcomes` failure.
    ///
    /// Only the conformance factory calls this, so it is gated: a consumer that depends on this
    /// crate without `testing` should not carry fault-injection scaffolding, and without the gate
    /// it is dead code in every real build.
    #[cfg(feature = "testing")]
    pub(crate) fn arm_apply_failure(&self) {
        self.fail_next_apply.set(true);
    }

    #[cfg(feature = "testing")]
    pub(crate) fn arm_migration_failure(&self) {
        self.fail_next_migration.set(true);
    }

    #[cfg(feature = "testing")]
    pub(crate) fn take_migration_failure(&self) -> bool {
        self.fail_next_migration.replace(false)
    }

    /// Consume the armed failure, if there is one.
    pub(crate) fn take_apply_failure(&self) -> bool {
        self.fail_next_apply.replace(false)
    }
}

/// A store scoped to one [`ScopeKey`].
///
/// Implements every storage trait core defines. Reads are filtered on the scope column, so a record
/// written under one key is never returned, batched, or counted by a store opened under another
/// (`wiki/decisions/009-local-scope-identity.decision.md`).
#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) backend: SqliteBackend,
    pub(crate) scope: ScopeKey,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore")
            .field("scope", &self.scope.as_str())
            .finish()
    }
}

/// Turn a `rusqlite` failure into core's opaque storage error.
///
/// Opaque on purpose: a caller cannot act on a SQLite result code, and leaking one would make the
/// backend's identity part of the error surface that decision 002 keeps generic.
pub(crate) fn storage(source: rusqlite::Error) -> Error {
    Error::storage(source)
}
