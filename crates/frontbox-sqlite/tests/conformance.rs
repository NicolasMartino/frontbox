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
