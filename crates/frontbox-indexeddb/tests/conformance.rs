//! The shared conformance suite, run against IndexedDB in a real browser.
//!
//! These are the identical functions `tests/in_memory.rs` and the SQLite backend's suite run —
//! through the **async** macro variants, because a browser has no `block_on` an IndexedDB future
//! can make progress under. Its event loop is what completes a request, so blocking the only
//! thread deadlocks rather than waits.
//!
//! # Running these
//!
//! ```text
//! wasm-pack test --headless --chrome crates/frontbox-indexeddb -- --features testing
//! ```
//!
//! `frontbox_blocking_only_tests` is absent, and that gap is deliberate: case 30 cancels a pass
//! mid-flight by polling it by hand, which requires every step before the transport to resolve on
//! its first poll. This backend suspends on a browser event before then. See that macro's own
//! documentation. It covers `SyncRunner` logic identical on every backend, so nothing about
//! IndexedDB goes untested as a result.
//!
//! They need a browser. `cargo test` cannot run them, and Node has no IndexedDB, so the gate in
//! `scripts/verify.sh` compiles them and does not execute them — a limitation stated in the gate's
//! own comment rather than left for someone to discover.
#![cfg(all(target_arch = "wasm32", feature = "testing"))]

use frontbox_indexeddb::IdbFactory;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

/// Each suite gets its own database, because IndexedDB is origin-scoped and outlives the test run.
/// A shared name would carry one suite's rows into another's assertions.
async fn conformance_factory() -> IdbFactory {
    IdbFactory::open("frontbox-conformance")
        .await
        .expect("open")
}

async fn single_flight_factory() -> IdbFactory {
    IdbFactory::open("frontbox-single-flight")
        .await
        .expect("open")
}

async fn row_factory() -> IdbFactory {
    IdbFactory::open("frontbox-rows").await.expect("open")
}

async fn cache_factory() -> IdbFactory {
    IdbFactory::open("frontbox-cache").await.expect("open")
}

async fn fault_factory() -> IdbFactory {
    IdbFactory::open("frontbox-faults").await.expect("open")
}

frontbox::frontbox_conformance_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: conformance_factory().await,
}

frontbox::frontbox_single_flight_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: single_flight_factory().await,
}

frontbox::frontbox_row_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: row_factory().await,
}

frontbox::frontbox_cache_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: cache_factory().await,
}

frontbox::frontbox_fault_injection_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: fault_factory().await,
}

frontbox::frontbox_coalescing_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: conformance_factory().await,
}

/// **Decision 031's cross-realm half, as far as one realm can check it.**
///
/// The obligation — two browser tabs must not drain one scope together — cannot be asserted from
/// inside a single page, and this test does not pretend to. What it does check is the half that
/// *can* fail here and would be catastrophic if it did: that the Web Lock is really taken, and
/// really released.
///
/// A lock that is never taken makes the whole mechanism a no-op that no in-realm test would
/// notice, because core's scope registry would still be passing every case. A lock that is taken
/// and never released is worse: the first drain succeeds and **every later drain in the process,
/// and in every other tab, stands down forever** — a queue that silently stops syncing, which is
/// exactly the failure `case_30` guards against for the in-process claim.
///
/// So: drain twice, and require the second to run. That fails against a leaked lease and against
/// a lock this backend forgot to take.
#[wasm_bindgen_test::wasm_bindgen_test]
async fn the_drain_lock_is_taken_and_released() {
    use frontbox::testing::{scope, Reply, ScriptedTransport, StoreFactory};
    use frontbox::{MutationIntent, MutationStatus, OutboxStore, SyncPass, SyncRunner};

    let factory = IdbFactory::open("frontbox-locks").await.expect("open");
    let key = scope("user:locks@tenant:acme");
    let store = factory.open(key.clone()).await.expect("scope");

    for n in 1..=2u128 {
        store
            .enqueue(MutationIntent::new(
                frontbox::testing::id(n),
                "POST",
                "/api/v1/things",
                serde_json::json!({ "n": n }),
                1_700_000_000_000 + n as i64,
            ))
            .await
            .expect("enqueue");
    }

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    )
    .with_batch_limit(1);

    let first = runner.sync_once().await.expect("first pass");
    assert_eq!(first.pass, SyncPass::Completed, "the lock was available");
    assert_eq!(first.counts.applied, 1);

    // The one that matters. If the lease leaked, the scope is still locked and this reports
    // `AlreadyRunning` — from a lock nobody holds any more.
    let second = runner.sync_once().await.expect("second pass");
    assert_eq!(
        second.pass,
        SyncPass::Completed,
        "the lease released on drop, so the next pass can claim the scope again"
    );
    assert_eq!(second.counts.applied, 1);
    assert_eq!(runner.store().pending_count().await.expect("count"), 0);
}

frontbox::frontbox_migration_tests_async! {
    #[wasm_bindgen_test::wasm_bindgen_test]
    factory: conformance_factory().await,
}
