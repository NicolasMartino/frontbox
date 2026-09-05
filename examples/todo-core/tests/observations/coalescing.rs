//! Observations 15-17: what queued-write coalescing changes, against a real server.
//!
//! Part of the `observations` suite; see `main.rs` for what these are evidence of. The server does
//! not depend on `frontbox`, so what it counts is an independent measurement of what actually
//! crossed the wire rather than a restatement of what the client believed it sent.

use crate::common::{self, app, server, ALICE};
use frontbox::{CoalescingEnqueue, DrainEnd, OutboxStore};

/// **Observation 15.** Two offline renames of one row reach the server as one write.
///
/// The user-visible claim, measured at the far end. `applied_count` is the server's own idempotency
/// table, so "one" here means one mutation id was ever recorded — not that the client chose to send
/// one and the server happened to dedupe two.
#[tokio::test]
async fn observation_15_two_offline_renames_reach_the_server_once() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;

    let id = app
        .create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");
    app.sync().await.expect("sync the create");
    let after_create = state.applied_count().await.expect("applied");

    app.transport().set_offline(true);
    let first = app
        .rename_coalescing(&id, "buy oat milk")
        .await
        .expect("first rename");
    assert!(
        matches!(first, CoalescingEnqueue::Appended { .. }),
        "nothing was queued for this row yet, so the first rename appends: {first:?}"
    );

    let second = app
        .rename_coalescing(&id, "buy oat milk and bread")
        .await
        .expect("second rename");
    assert!(
        matches!(second, CoalescingEnqueue::Replaced { .. }),
        "the first rename is still unsent, so the second replaces its body: {second:?}"
    );

    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "one queued write, not two"
    );

    app.transport().set_offline(false);
    let report = app.sync().await.expect("sync");
    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(report.counts.applied, 1);

    assert_eq!(
        state.applied_count().await.expect("applied") - after_create,
        1,
        "the server recorded one rename, not two"
    );

    // And it is the *later* title. Read back from the server rather than from the projection, so
    // this cannot pass on a client that merely rendered the right thing locally.
    app.refresh_from_server().await.expect("refresh");
    let rows = app.store().rows();
    let row = rows.iter().find(|todo| todo.id == id).expect("the todo");
    assert_eq!(row.title, "buy oat milk and bread");
}

/// **Observation 16.** The plain rename path still queues both, which is what makes 15 a difference.
///
/// The same sequence through [`TodoApp::rename`](todo_core::TodoApp::rename). Without this,
/// observation 15 shows a feature working and not what it changed — and a trial that cannot show the
/// before is asserting the after against nothing.
#[tokio::test]
async fn observation_16_the_plain_rename_path_still_queues_both() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;

    let id = app
        .create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");
    app.sync().await.expect("sync the create");
    let after_create = state.applied_count().await.expect("applied");

    app.transport().set_offline(true);
    app.rename(&id, "buy oat milk").await.expect("first rename");
    app.rename(&id, "buy oat milk and bread")
        .await
        .expect("second rename");

    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        2,
        "appending is still what `rename` does"
    );

    app.transport().set_offline(false);
    app.sync().await.expect("sync");

    assert_eq!(
        state.applied_count().await.expect("applied") - after_create,
        2,
        "both renames crossed the wire"
    );

    // The end state agrees either way — the later write still wins — so what coalescing buys is the
    // request that never happened, not a different answer.
    app.refresh_from_server().await.expect("refresh");
    let rows = app.store().rows();
    let row = rows.iter().find(|todo| todo.id == id).expect("the todo");
    assert_eq!(row.title, "buy oat milk and bread");
}

/// **Observation 17.** Polling while offline does not spend the queue's coalescibility.
///
/// This is the one that justifies `SyncTransport::offline_now` existing at all.
///
/// A real application polls on a timer, so a drain attempt lands *between* the two renames as a
/// matter of course. `read_for_send` marks every record it returns as transport-started and never
/// unmarks it — so without the probe, that poll would make the queued rename ineligible and the
/// second rename would append. The feature would be sound and almost never fire.
///
/// The assertion is the outcome of the second rename, not a count: `Replaced` is only reachable if
/// the intervening drain read nothing.
#[tokio::test]
async fn observation_17_an_offline_poll_does_not_spend_coalescibility() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;

    let id = app
        .create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");
    app.sync().await.expect("sync the create");

    app.transport().set_offline(true);
    app.rename_coalescing(&id, "buy oat milk")
        .await
        .expect("first rename");

    let attempts_before = app.transport().request_attempts();
    let report = app.sync().await.expect("the poll a timer would fire");
    assert_eq!(report.ended, DrainEnd::Offline);
    assert_eq!(
        app.transport().request_attempts(),
        attempts_before,
        "the probe answered before anything was read, so no request was attempted"
    );

    let second = app
        .rename_coalescing(&id, "buy oat milk and bread")
        .await
        .expect("second rename");
    assert!(
        matches!(second, CoalescingEnqueue::Replaced { .. }),
        "the offline poll left the queued rename coalescible: {second:?}"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 1);
}

/// **Observation 17b.** A drain that actually read the batch does spend it, and that is correct.
///
/// The counterpart, and the reason the mark is written before the request rather than derived from
/// how the request failed. Here the server is reachable but refuses every batch, so the records were
/// genuinely handed to transport — after which frontbox cannot prove the server has not seen them,
/// and a replacement under the same mutation id could be deduped away and never applied.
///
/// So the second rename appends. Nothing is lost; the later write still wins.
#[tokio::test]
async fn observation_17b_a_real_send_attempt_spends_coalescibility() {
    let base = common::broken_sync_server().await;
    let app = app(&base, ALICE).await;

    app.rename_coalescing("todo-1", "buy oat milk")
        .await
        .expect("first rename");

    // Reachable and refusing, which is an attempted request rather than an offline pass.
    let attempts_before = app.transport().request_attempts();
    let _ = app.sync().await;
    assert!(
        app.transport().request_attempts() > attempts_before,
        "the batch was handed to transport"
    );

    let second = app
        .rename_coalescing("todo-1", "buy oat milk and bread")
        .await
        .expect("second rename");
    assert!(
        matches!(second, CoalescingEnqueue::Appended { .. }),
        "a record the server may have seen must not be rewritten: {second:?}"
    );
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        2,
        "so the newer title is queued behind it rather than replacing it"
    );
}
