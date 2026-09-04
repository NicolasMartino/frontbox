//! A stored object that is not an outbox row at all, and what becomes of it.
//!
//! # Why this is not in the shared conformance suite
//!
//! Every [`CorruptKind`](frontbox::testing::CorruptKind) the suite injects produces a row that
//! parses into the backend's row type and then fails a semantic check. This is the failure only a
//! store of whole objects can have: the stored value's **shape** does not match the row type, which
//! is what an older schema version or a foreign writer leaves behind in a shared origin. SQLite
//! reads its rows column by column and cannot reach the same state through the same door, so this
//! is an adapter case rather than a fourth variant every backend would have to imitate.
//!
//! # What it is guarding
//!
//! `all_in_scope` filtered decode failures out with `filter_map`, which made such a row
//! **unreachable**: absent from `pending_batch` and `pending_count`, which is the documented
//! contract, and absent from `sweep_corrupt`'s scan, which is the one path that exists to make a
//! corrupt row visible. It stayed in storage indefinitely, counted by nothing and named by nothing
//! — the exact outcome `wiki/decisions/006-corrupt-record-policy.decision.md` rejects.
//!
//! These need a browser, like the rest of this crate's suites. `scripts/verify.sh` compiles them
//! and does not run them; `just browser` runs them.
#![cfg(all(target_arch = "wasm32", feature = "testing"))]

use frontbox::{OutboxStore, QuarantineStore, ScopeKey};
use frontbox_indexeddb::IdbFactory;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// Its own database, for the reason the conformance suites each have one: IndexedDB is
/// origin-scoped and outlives the test run, so a shared name carries rows between suites.
async fn factory() -> IdbFactory {
    IdbFactory::open("frontbox-schema-mismatch")
        .await
        .expect("open")
}

fn scope() -> ScopeKey {
    ScopeKey::new("user:mismatch@tenant:test").expect("scope")
}

/// A row whose shape does not match the schema is swept into quarantine, not silently dropped.
#[wasm_bindgen_test]
async fn an_unreadable_row_reaches_quarantine() {
    let factory = factory().await;
    let key = scope();
    let store = frontbox::testing::StoreFactory::open(&factory, key.clone())
        .await
        .expect("open scope");

    factory
        .insert_unreadable_row(&key)
        .await
        .expect("insert unreadable row");

    // Unchanged, and that half was never the defect: a row that will not decode is specified not
    // to count as pending (`OutboxStore::pending_batch`).
    assert_eq!(
        store.pending_count().await.expect("pending"),
        0,
        "an unreadable row is not pending"
    );
    assert_eq!(
        QuarantineStore::count(&store).await.expect("quarantined"),
        0,
        "and it is not quarantined until something sweeps it"
    );

    // The half that was: the sweep could not see it, so nothing ever could.
    assert_eq!(
        store.sweep_corrupt().await.expect("sweep"),
        1,
        "the sweep has to find a row it cannot parse, or the row is unreachable forever"
    );
    assert_eq!(
        QuarantineStore::count(&store).await.expect("quarantined"),
        1,
        "and moving it is what makes it visible"
    );

    let quarantined = QuarantineStore::list(&store, 10).await.expect("list");
    assert_eq!(quarantined.len(), 1);
    assert!(
        quarantined[0].mutation_id.is_none(),
        "the identifier lived in the object that would not parse, so there is none to report — \
         this is the id-less path only a sweep can reach"
    );
    assert!(
        !quarantined[0].reason.is_empty(),
        "and the reason says what happened rather than leaving an empty record"
    );
    assert!(
        quarantined[0]
            .raw_body
            .contains("a schema this build does not know"),
        "the stored value is kept verbatim, so what was actually in there is still inspectable"
    );
}

/// The sweep is not confused by a readable row sitting next to an unreadable one.
///
/// Both kinds of corruption end in the same place through one pass, and the healthy record beside
/// them is untouched — the failure this guards against is a scan that gives up at the first entry
/// it cannot parse and leaves the rest of the queue unswept.
#[wasm_bindgen_test]
async fn a_sweep_handles_both_kinds_of_corruption_at_once() {
    let factory = factory().await;
    let key = ScopeKey::new("user:mixed@tenant:test").expect("scope");
    let store = frontbox::testing::StoreFactory::open(&factory, key.clone())
        .await
        .expect("open scope");

    store
        .enqueue(frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/things".to_owned(),
            serde_json::json!({ "ok": true }),
            1_700_000_000_000,
        ))
        .await
        .expect("enqueue a healthy record");
    frontbox::testing::StoreFactory::insert_corrupt_row(
        &factory,
        &key,
        frontbox::testing::CorruptKind::UnparseableId,
    )
    .await
    .expect("insert a row that parses but will not decode");
    factory
        .insert_unreadable_row(&key)
        .await
        .expect("insert a row that will not parse");

    assert_eq!(
        store.sweep_corrupt().await.expect("sweep"),
        2,
        "both kinds move, in one pass"
    );
    assert_eq!(
        store.pending_count().await.expect("pending"),
        1,
        "and the healthy record is still queued"
    );
}
