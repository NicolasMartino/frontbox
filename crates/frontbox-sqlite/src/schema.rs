//! The durable schema, and the four choices in it that are load-bearing.

/// Every table, created if absent.
///
/// # `AUTOINCREMENT` is not decoration
///
/// Plain `INTEGER PRIMARY KEY` reuses the largest deleted rowid, so a queue that drains to empty
/// and is written to again reissues sequence numbers that have already been used. Decision 016
/// made `seq` the primary sort key precisely so that replay follows enqueue order; a reused number
/// sorts a later write ahead of an earlier one and inverts exactly the pairs the decision exists to
/// order. `AUTOINCREMENT` is what makes the counter monotonic for the lifetime of the database.
///
/// # `COLLATE BINARY` is stated rather than assumed
///
/// It is already the default for `TEXT`. It is written out because decision 009 requires one
/// textual form for `mutation_id` under a binary collation, and a default is a thing a later
/// `ALTER` or a differently-configured connection can change without anybody noticing. A stated
/// collation fails loudly if it is ever contradicted.
///
/// # Scope is a column, not a database
///
/// Decision 024 requires that two distinct `ScopeKey`s never reach one physical store; it does not
/// require separate files. A column with every read filtered on it satisfies injectivity trivially
/// — two different keys are two different strings — and it keeps the conformance suite's sharing
/// contract, which needs two scopes to live in the same physical store or the isolation cases
/// prove nothing.
///
/// # `cache_versions.version` is nullable, and that is the whole of decision 021
///
/// SQL `NULL` carries "never heard a version"; the empty string carries "the server said the empty
/// string". They are different rows and they compare unequal, which is exactly the distinction
/// [`CacheVersion`](frontbox::CacheVersion) exists to preserve: under XOR set hashing an emptied
/// collection hashes to zero, so a client that has never synced and a collection the server
/// legitimately emptied would otherwise share one identity and never refetch. A `NOT NULL DEFAULT
/// ''` column would collapse them silently and every case in the suite would still pass except
/// `case_44`, which is why that case exists.
///
/// `version` and `stale` are columns of one row, so a single `UPSERT` writes both or neither.
/// Decision 015 makes that pairing a requirement rather than a convenience — a crash between
/// advancing a version and setting its staleness leaves the client believing invalidated data is
/// fresh, permanently, with nothing to detect it.
/// # No column carries a `DEFAULT`
///
/// Every `INSERT` in this crate names every column, so a `DEFAULT` could only ever supply a value
/// for a statement written against a *different* schema — and this crate has never shipped one.
/// Dropping them makes a column added without updating its writers fail loudly at the insert
/// instead of silently storing a zero, which for `attempts` would mean decision 017's retention
/// bound quietly restarting and for `stale` would mean an invalidated row reading as fresh.
///
/// `cache_versions.version` and `rows_store.version` stay nullable, which is a different thing
/// entirely and is argued above: `NULL` and `''` are distinguishable values that decision 021
/// needs to stay distinguishable.
///
/// `outbox.transport_started` is nullable for a third reason, and it does not weaken the rule. It
/// carries no `DEFAULT`, so every writer still names it and a writer that forgets still fails. What
/// `NULL` means is *"written before this column existed"*, and [`migrate`] cannot give those rows a
/// value without an `UPDATE` over every user's queue — so the read supplies the conservative one
/// (`true`, meaning "the transport may already have seen this"). A `NOT NULL DEFAULT 0` column
/// would have been the opposite: it would have told every pre-existing row that it had never been
/// sent, which is the one answer that permits an unsafe rewrite.
pub const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS outbox (
    seq           INTEGER PRIMARY KEY AUTOINCREMENT,
    scope         TEXT NOT NULL COLLATE BINARY,
    mutation_id   TEXT NOT NULL COLLATE BINARY,
    method        TEXT NOT NULL,
    path          TEXT NOT NULL,
    raw_body      TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    op_name       TEXT,
    op_version    TEXT,
    traceparent   TEXT,
    precondition  TEXT,
    row_entity    TEXT,
    row_id        TEXT,
    attempts      INTEGER NOT NULL,
    last_error    TEXT,
    transport_started INTEGER
);
CREATE INDEX IF NOT EXISTS outbox_scope_seq ON outbox (scope, seq);

CREATE TABLE IF NOT EXISTS dead_letters (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    scope         TEXT NOT NULL COLLATE BINARY,
    mutation_id   TEXT NOT NULL COLLATE BINARY,
    method        TEXT NOT NULL,
    path          TEXT NOT NULL,
    raw_body      TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    op_name       TEXT,
    op_version    TEXT,
    traceparent   TEXT,
    precondition  TEXT,
    attempts      INTEGER NOT NULL,
    rejected_at   INTEGER NOT NULL,
    reason_kind   TEXT NOT NULL,
    reason_body   TEXT
);
CREATE INDEX IF NOT EXISTS dead_letters_scope ON dead_letters (scope, rejected_at);

CREATE TABLE IF NOT EXISTS quarantine (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    scope          TEXT NOT NULL COLLATE BINARY,
    raw_mutation_id TEXT NOT NULL,
    method         TEXT NOT NULL,
    path           TEXT NOT NULL,
    raw_body       TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    op_name        TEXT,
    op_version     TEXT,
    reason         TEXT NOT NULL,
    quarantined_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS quarantine_scope ON quarantine (scope, quarantined_at);

CREATE TABLE IF NOT EXISTS rows_store (
    scope    TEXT NOT NULL COLLATE BINARY,
    entity   TEXT NOT NULL COLLATE BINARY,
    row_id   TEXT NOT NULL COLLATE BINARY,
    blob     TEXT NOT NULL,
    stale    INTEGER NOT NULL,
    PRIMARY KEY (scope, entity, row_id)
);

CREATE TABLE IF NOT EXISTS cache_versions (
    scope    TEXT NOT NULL COLLATE BINARY,
    entity   TEXT NOT NULL COLLATE BINARY,
    version  TEXT COLLATE BINARY,
    stale    INTEGER NOT NULL,
    PRIMARY KEY (scope, entity)
);
"#;

/// The schema generation this build writes.
///
/// Stored in `PRAGMA user_version`, which SQLite keeps in the database header and never interprets.
/// Version 1 is the first generation this crate has ever numbered: everything before it was a bare
/// `CREATE TABLE IF NOT EXISTS` batch, which creates tables on a fresh database and silently does
/// nothing to an existing one — so a column added to [`SCHEMA`] alone would never reach a database
/// already on disk, and every statement naming it would fail.
pub(crate) const SCHEMA_VERSION: i64 = 1;

/// Bring an existing database up to [`SCHEMA_VERSION`].
///
/// Runs after [`SCHEMA`], which has already created anything missing wholesale.
///
/// # Why the column check rather than the version alone
///
/// A fresh database gets `transport_started` from [`SCHEMA`] and still reports `user_version = 0`,
/// because nothing has stamped it yet. Driving the `ALTER` off the version alone would therefore
/// try to add a column that is already there and fail on the very first open of a new database.
/// `PRAGMA table_info` answers the question actually being asked — *does this column exist* — and
/// makes the step idempotent whatever route the database took to get here. The version is still
/// stamped, because the next migration will want a cheap answer that does not depend on inspecting
/// every table.
///
/// # This migration writes no user data
///
/// One `ALTER TABLE ... ADD COLUMN`, which SQLite records in the header without rewriting rows. The
/// pre-existing rows keep `NULL`, and the read maps that to the conservative value. Rewriting them
/// would have meant an `UPDATE` across every queued mutation on every user's device to store a
/// value the absence already implies.
pub(crate) fn migrate(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    if !has_column(connection, "outbox", "transport_started")? {
        connection.execute_batch("ALTER TABLE outbox ADD COLUMN transport_started INTEGER")?;
    }

    // **Only ever forward.** An unconditional stamp would let an older binary opening a *newer*
    // database write the marker backwards — and the next new-binary open would then re-run
    // migrations it had already applied, against a schema that already has their effects. Two
    // installed versions of one application is not exotic: a desktop build alongside a browser tab,
    // or a rollback.
    //
    // Reading a future database is left possible rather than refused. This crate's migrations add
    // columns, so a newer schema is a superset and every statement here still resolves; refusing
    // would strand a user's queued work behind a version downgrade, which is the more expensive
    // failure. A migration that ever *removed* or *retyped* a column would make that trade wrong
    // and would need a version check on the read path, not just here.
    let current: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current < SCHEMA_VERSION {
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

/// Whether `table` has a column named `column`.
fn has_column(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut names = statement.query_map([], |row| row.get::<_, String>(1))?;
    names.try_fold(false, |found, name| Ok(found || name? == column))
}
