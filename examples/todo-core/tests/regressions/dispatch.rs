//! Direct dispatch: when a write may skip the queue, and when it must not.
//!
//! Part of the `regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE};
use frontbox::OutboxStore;
use todo_core::Direct;

/// A direct write uses the same transport switch as batch sync.
#[tokio::test]
async fn direct_dispatch_offline_queues_without_an_http_attempt() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "toggle offline")
        .await
        .expect("create");
    app.sync().await.expect("sync");

    let before = app.transport().request_attempts();
    app.transport().set_offline(true);
    assert!(matches!(
        app.set_done(&id, true).await.expect("toggle"),
        Direct::Queued
    ));
    assert_eq!(
        app.transport().request_attempts(),
        before,
        "the offline switch stops the direct request before the transport attempts it"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 1);
}

/// A row with queued local work is ordered through the queue instead of raced directly.
#[tokio::test]
async fn direct_dispatch_waits_behind_pending_work_for_the_same_row() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "brand new")
        .await
        .expect("create");

    let before = app.transport().request_attempts();
    assert!(matches!(
        app.set_done(&id, true).await.expect("toggle"),
        Direct::Queued
    ));
    assert_eq!(
        app.transport().request_attempts(),
        before,
        "the server cannot toggle a row before the queued create reaches it"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 2);

    let report = app.sync().await.expect("sync");
    assert_eq!(report.counts.applied, 2);
    app.refresh_from_server().await.expect("refresh");
    assert!(app.store().get(&id).expect("row").done);
}

/// A terminal direct refusal is visible and not queued for replay.
#[tokio::test]
async fn direct_dispatch_refusal_is_terminal() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;

    let Direct::Refused(refusal) = app.set_done("missing", true).await.expect("toggle") else {
        panic!("expected a terminal refusal");
    };
    assert_eq!(refusal.code.as_deref(), Some("not_found"));
    assert_eq!(
        refusal.details,
        Some(serde_json::json!({ "id": "missing" }))
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
}
