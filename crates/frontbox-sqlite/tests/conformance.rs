//! The shared conformance suite, run against the SQLite backend.
//!
//! Nothing here is specific to SQLite. These are the identical functions `tests/in_memory.rs` runs,
//! through the identical macros, which is what turns "all backends behave identically" from a claim
//! into something that fails a build.
//!
//! **The cache suite runs here now.** It was absent until the cache half of D5 landed, and the
//! absence was doing its job: this file said so in prose while the roadmap said D5 was built, which
//! is how the overstatement was caught. That is the whole argument for splitting the factory traits
//! — a missing capability leaves a visible hole in a test file instead of a runtime skip that
//! reports a full pass.
#![cfg(feature = "testing")]

use frontbox_sqlite::SqliteFactory;

frontbox::frontbox_conformance_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_single_flight_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_blocking_only_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_row_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_cache_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_fault_injection_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

frontbox::frontbox_coalescing_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}

/// **The observation D4a could not make.** A queue written by one process is still there for the
/// next one.
///
/// Every other case in this file runs against one open database, which is the same shape the
/// in-memory backend has. This one closes the connection and opens the file again — one of the two
/// places in the project where "durable" means what the word means. The other is `observation_3` in
/// `examples/todo-core/tests/observations/main.rs`, which asserted the loss until this backend existed
/// and now asserts the survival; that one goes through the application, this one through the trait.
#[test]
fn a_queue_survives_a_reopen_of_the_file() {
    use frontbox::{
        MutationId, MutationIntent, OutboxStore, RowRef, RowStore, ScopeKey, StoredRow,
    };
    use frontbox_sqlite::SqliteBackend;

    let directory = std::env::temp_dir().join(format!("frontbox-sqlite-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("survives.db");
    let _ = std::fs::remove_file(&path);

    let scope = ScopeKey::new("user:restart@tenant:acme").expect("scope");
    let id = MutationId::new();

    {
        let backend = SqliteBackend::open(&path, frontbox::ManualClock::new(1_700_000_000_000))
            .expect("open");
        let store = backend.open_scope(scope.clone());
        pollster::block_on(
            store.enqueue(
                MutationIntent::new(id, "POST", "/api/v1/todos", serde_json::json!({"a": 1}), 1)
                    .with_row(RowRef::new("todo", "row-1")),
            ),
        )
        .expect("enqueue");
        pollster::block_on(store.put_rows(&[StoredRow::new(
            RowRef::new("todo", "row-1"),
            serde_json::json!({"title": "written once"}),
        )]))
        .expect("put");
    } // The connection drops here. Whatever survives is on disk.

    let backend =
        SqliteBackend::open(&path, frontbox::ManualClock::new(1_700_000_000_000)).expect("reopen");
    let store = backend.open_scope(scope);

    assert_eq!(
        pollster::block_on(store.pending_count()).expect("count"),
        1,
        "the queued mutation outlived the process that wrote it"
    );
    let pending = pollster::block_on(store.pending_batch(10)).expect("batch");
    assert_eq!(pending[0].mutation_id, id);
    assert_eq!(
        pending[0].row.as_ref().map(|row| row.row_id.as_str()),
        Some("row-1"),
        "and so did the row it is bound to, which is what the merge rule needs after a restart"
    );

    let row = pollster::block_on(store.get_row(&RowRef::new("todo", "row-1")))
        .expect("get")
        .expect("row survives");
    assert_eq!(row.blob, serde_json::json!({"title": "written once"}));

    let _ = std::fs::remove_file(&path);
}

/// A database written before `transport_started` existed reopens, and its rows are not coalescible.
///
/// **The migration case.** Every other test here starts from a schema this build created, so none
/// of them can reach the state that actually matters: a queue already on a user's device, written
/// by a version of this crate that had no such column.
///
/// The old shape is built by hand rather than by checking out an old commit, which is the only way a
/// test can hold two schema generations at once. What it asserts is the conservative reading: the
/// migrated rows still send, and they refuse to be rewritten, because nothing in the database can
/// say whether the server has seen them.
#[test]
fn rows_written_before_the_column_existed_migrate_as_already_sent() {
    use frontbox::{
        CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal, MutationId, MutationIntent,
        OutboxStore, RowRef, ScopeKey,
    };
    use frontbox_sqlite::SqliteBackend;

    let directory =
        std::env::temp_dir().join(format!("frontbox-sqlite-mig-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("legacy.db");
    let _ = std::fs::remove_file(&path);

    let scope = ScopeKey::new("user:legacy@tenant:acme").expect("scope");
    let old = MutationId::from_uuid(uuid::Uuid::from_u128(1));

    // The pre-migration schema, verbatim: the outbox as it was before this feature, with no
    // `transport_started` and no `user_version`.
    {
        let connection = rusqlite::Connection::open(&path).expect("open raw");
        connection
            .execute_batch(
                "CREATE TABLE outbox (
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
                 );",
            )
            .expect("legacy schema");
        connection
            .execute(
                "INSERT INTO outbox (scope, mutation_id, method, path, raw_body, created_at, \
                 row_entity, row_id, attempts) \
                 VALUES (?1, ?2, 'PUT', '/api/v1/profile/user-1', '{\"name\":\"old\"}', 100, \
                 'profile', 'user-1', 0)",
                rusqlite::params![scope.as_str(), old.to_string()],
            )
            .expect("legacy row");
    }

    let backend =
        SqliteBackend::open(&path, frontbox::ManualClock::new(1_700_000_000_000)).expect("migrate");
    let store = backend.open_scope(scope);

    // It still reads, and it still sends. The migration must not strand queued work.
    assert_eq!(
        pollster::block_on(store.pending_count()).expect("count"),
        1,
        "a pre-migration row is still queued"
    );
    let pending = pollster::block_on(store.pending_batch(10)).expect("batch");
    assert_eq!(pending[0].mutation_id, old);
    assert_eq!(pending[0].body, serde_json::json!({ "name": "old" }));

    // And it is not coalescible, because `NULL` means "written before we tracked this" and the only
    // safe reading of that is that the server may already hold the identifier.
    let newer = MutationId::from_uuid(uuid::Uuid::from_u128(2));
    let outcome = pollster::block_on(
        store.enqueue_coalescing(
            MutationIntent::new(
                newer,
                "PUT",
                "/api/v1/profile/user-1",
                serde_json::json!({ "name": "new" }),
                200,
            )
            .with_row(RowRef::new("profile", "user-1")),
            CoalescingPolicy::RequireExisting,
        ),
    )
    .expect("coalescing");

    assert_eq!(
        outcome,
        CoalescingEnqueue::NotQueued {
            mutation_id: newer,
            reason: CoalescingRefusal::TransportStarted,
        },
        "a migrated row must not be rewritten"
    );
    assert_eq!(
        pollster::block_on(store.pending_batch(10)).expect("batch")[0].body,
        serde_json::json!({ "name": "old" })
    );

    // Rows written *after* the migration are coalescible as normal, so the conservatism is scoped
    // to the rows that earned it rather than disabling the feature for the database.
    let fresh = MutationId::from_uuid(uuid::Uuid::from_u128(3));
    pollster::block_on(
        store.enqueue(
            MutationIntent::new(
                fresh,
                "PUT",
                "/api/v1/other/row-9",
                serde_json::json!({ "name": "fresh" }),
                300,
            )
            .with_row(RowRef::new("other", "row-9")),
        ),
    )
    .expect("enqueue");

    let replaced = pollster::block_on(
        store.enqueue_coalescing(
            MutationIntent::new(
                MutationId::from_uuid(uuid::Uuid::from_u128(4)),
                "PUT",
                "/api/v1/other/row-9",
                serde_json::json!({ "name": "fresher" }),
                400,
            )
            .with_row(RowRef::new("other", "row-9")),
            CoalescingPolicy::RequireExisting,
        ),
    )
    .expect("coalescing");
    assert!(
        matches!(replaced, CoalescingEnqueue::Replaced { kept, .. } if kept == fresh),
        "a post-migration row coalesces normally, got {replaced:?}"
    );

    let _ = std::fs::remove_file(&path);
}

/// An older binary opening a newer database does not write the schema marker backwards.
///
/// Two installed versions of one application is not exotic — a desktop build alongside a browser
/// tab, or a rollback after a bad release. If `migrate` stamped `user_version` unconditionally, the
/// older one would mark a v2 database as v1, and the next new-binary open would re-run migrations it
/// had already applied against a schema that already has their effects.
///
/// Reading the future database is deliberately still allowed: this crate's migrations add columns,
/// so a newer schema is a superset and every statement still resolves. Refusing would strand queued
/// work behind a version downgrade, which is the more expensive failure.
#[test]
fn opening_a_newer_database_does_not_move_the_version_backwards() {
    use frontbox::{MutationId, MutationIntent, OutboxStore, ScopeKey};
    use frontbox_sqlite::SqliteBackend;

    let directory =
        std::env::temp_dir().join(format!("frontbox-sqlite-ver-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("future.db");
    let _ = std::fs::remove_file(&path);

    let scope = ScopeKey::new("user:future@tenant:acme").expect("scope");

    // Build the database with this binary, then stamp it as if a later version had migrated it and
    // added a column of its own.
    {
        let backend = SqliteBackend::open(&path, frontbox::ManualClock::new(1_700_000_000_000))
            .expect("open");
        let store = backend.open_scope(scope.clone());
        pollster::block_on(store.enqueue(MutationIntent::new(
            MutationId::from_uuid(uuid::Uuid::from_u128(1)),
            "PUT",
            "/api/v1/todos/1",
            serde_json::json!({ "title": "queued under a future schema" }),
            100,
        )))
        .expect("enqueue");
    }
    {
        let connection = rusqlite::Connection::open(&path).expect("open raw");
        connection
            .execute_batch(
                "ALTER TABLE outbox ADD COLUMN invented_by_a_later_version TEXT; \
                 PRAGMA user_version = 7;",
            )
            .expect("fake a newer schema");
    }

    // This binary opens it again. It must not stamp the marker down to its own version.
    let backend =
        SqliteBackend::open(&path, frontbox::ManualClock::new(1_700_000_000_000)).expect("reopen");
    let store = backend.open_scope(scope);

    let connection = rusqlite::Connection::open(&path).expect("open raw");
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(
        version, 7,
        "an older binary must not rewrite a newer database's schema marker"
    );

    // And the queued work is still readable through the superset schema, which is why reading a
    // future database is permitted rather than refused.
    assert_eq!(
        pollster::block_on(store.pending_count()).expect("count"),
        1,
        "queued work is not stranded by a version downgrade"
    );

    let _ = std::fs::remove_file(&path);
}

frontbox::frontbox_migration_tests! {
    #[test]
    factory: SqliteFactory::new(),
    block_on: pollster::block_on,
}
