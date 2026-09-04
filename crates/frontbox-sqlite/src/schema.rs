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
    last_error    TEXT
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
