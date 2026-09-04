//! Batches that reach some destinations, all of them, or none.
//!
//! Part of the `regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE};
use frontbox::OutboxStore;

/// A batch whose every destination failed retains its records, counts the attempt, and says so.
///
/// # The finding this answers, and why it was wrong
///
/// A worktree review read `send_batch`'s all-destinations-failed path — `Ok` carrying no verdicts —
/// as "a bad sync endpoint becomes retained records instead of a transport failure", and proposed
/// returning the first failure instead. Building that revealed the opposite: `SyncRunner::run`
/// answers a transport `Err` with `return Err(error)` **before any outcome is applied**, so the
/// attempt is never recorded. A permanently broken endpoint would then retry forever, never
/// reaching decision 017's retention bound — the unbounded behaviour the change was meant to
/// prevent.
///
/// The `Ok` path is the bounded one: every record the response does not name is retained with
/// `reason: Some("no verdict returned")` and `attempts` incremented, so a broken endpoint walks the
/// bound and dead-letters. This pins that, because it is not obvious and will be re-proposed.
#[tokio::test]
async fn every_destination_failing_retains_the_records_and_counts_the_attempt() {
    let broken = common::broken_sync_server().await;
    let app = app(&broken, ALICE).await;
    app.create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");

    let report = app
        .sync()
        .await
        .expect("a failing destination is a failed attempt, not an aborted drain");

    assert_eq!(
        app.outbox().pending_count().await.expect("pending"),
        1,
        "the record is retained rather than lost"
    );
    assert_eq!(
        report.counts.dead_lettered, 0,
        "and not terminated on its first failure"
    );
    let pending = app.outbox().pending_batch(10).await.expect("pending batch");
    assert_eq!(
        pending[0].attempts, 1,
        "the attempt is counted, which is what bounds the retries"
    );
    assert_eq!(
        pending[0].last_error.as_deref(),
        Some("no verdict returned"),
        "and the record says what came back"
    );
}

/// The same failure is legible afterwards instead of living only in a trace line.
///
/// This is the part of the review's finding that was real. A pass where every destination answered
/// with a status produced the same `Ok`, the same empty verdict list and the same report as a
/// healthy pass with nothing to say, so nothing on screen could tell them apart. The condition is
/// now readable — a condition and not an event, which is the status bar's half of decision 042.
#[tokio::test]
async fn a_failing_destination_is_readable_as_a_condition() {
    let broken = common::broken_sync_server().await;
    let app = app(&broken, ALICE).await;
    app.create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");

    assert_eq!(
        app.transport().last_send_failure(),
        None,
        "nothing has been sent yet"
    );

    app.sync().await.expect("sync");

    let failure = app
        .transport()
        .last_send_failure()
        .expect("a batch that reached no destination has a reason");
    assert!(
        failure.contains("500"),
        "and the reason names what the endpoint answered: {failure}"
    );
}

/// A pass that reaches a destination clears the condition rather than leaving it standing.
///
/// The failure above is about the *last* batch, not about whether one ever failed. A condition that
/// never clears is a stale banner, which decision 042 is explicit about.
#[tokio::test]
async fn reaching_a_destination_clears_the_failure_condition() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    app.create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");

    app.sync().await.expect("sync");

    assert_eq!(
        app.transport().last_send_failure(),
        None,
        "a healthy pass leaves no condition behind"
    );
}

/// One failed destination does not discard the verdicts the others returned.
///
/// # Why this calls the transport rather than draining
///
/// A partial reach only exists *within one batch*, and the drain never builds one here: the todo is
/// correlated to a user record that is still pending, so exclusion holds it back and the two leave
/// in separate passes, each touching a single destination. Going through `sync()` would prove the
/// pass semantics and never reach the branch under test. `send_batch` is the unit that decides
/// this, so it is the unit this asks.
#[tokio::test]
async fn a_partial_destination_failure_keeps_the_verdicts_that_arrived() {
    let servers = common::servers().await;
    let broken = common::broken_sync_server().await;
    // Users to the real service, todos to the broken one.
    let app = common::open(todo_core::Config {
        todo_url: broken,
        user_url: Some(servers.user_url.clone()),
        scope: ALICE.to_owned(),
        user_id: common::ALICE_USER.to_owned(),
        storage: ":memory:".to_owned(),
    })
    .await;

    let batch = frontbox::MutationBatchRequest::new(vec![
        frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/users".to_owned(),
            serde_json::json!({ "id": common::ALICE_USER, "name": "Alice" }),
            1_700_000_000_000,
        ),
        frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/todos".to_owned(),
            serde_json::json!({ "id": "todo-1", "user_id": common::ALICE_USER, "title": "buy milk" }),
            1_700_000_000_001,
        ),
    ]);

    let response = frontbox::SyncTransport::send_batch(app.transport(), batch)
        .await
        .expect("a partial reach is not an error");

    assert_eq!(
        response.results.len(),
        1,
        "the destination that answered contributed its verdict, and the broken one contributed \
         nothing rather than failing the whole batch"
    );
    assert_eq!(
        app.transport().last_send_failure(),
        None,
        "a partial reach is not a failing pass: something did get through"
    );
}

/// A batch that reaches nothing still names the destination that *answered*, rather than only
/// reporting offline.
///
/// # The case the condition used to be blank for
///
/// One destination unreachable and another answering with a status is the mixed batch: nothing was
/// reached, so `send_batch` returns [`frontbox::Error::Offline`] — correct, because the queue must
/// not burn decision 017's retention bound on an aeroplane. But the failure that *was* observed used
/// to be discarded by that early return, so the pass presented as an ordinary offline one while a
/// genuinely broken endpoint was the thing holding the queue up. That is the worst case for the
/// condition to be empty in.
#[tokio::test]
async fn a_mixed_batch_records_the_destination_that_answered() {
    let broken = common::broken_sync_server().await;
    let gone = common::unreachable_server().await;
    // Todos to the port nothing is listening on, users to the server that answers 500.
    let app = common::open(todo_core::Config {
        todo_url: gone,
        user_url: Some(broken),
        scope: ALICE.to_owned(),
        user_id: common::ALICE_USER.to_owned(),
        storage: ":memory:".to_owned(),
    })
    .await;

    let batch = frontbox::MutationBatchRequest::new(vec![
        frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/users".to_owned(),
            serde_json::json!({ "id": common::ALICE_USER, "name": "Alice" }),
            1_700_000_000_000,
        ),
        frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/todos".to_owned(),
            serde_json::json!({ "id": "todo-1", "user_id": common::ALICE_USER, "title": "buy milk" }),
            1_700_000_000_001,
        ),
    ]);

    let failure = frontbox::SyncTransport::send_batch(app.transport(), batch)
        .await
        .expect_err("nothing was reached, so the pass is offline");
    assert!(
        failure.is_offline(),
        "a batch that reached no destination reports offline, which is what leaves the queue \
         untouched and the attempt uncounted: {failure}"
    );

    let condition = app
        .transport()
        .last_send_failure()
        .expect("the destination that answered is named even though the pass reports offline");
    assert!(
        condition.contains("500"),
        "and it says what that destination answered: {condition}"
    );
}

/// A pass that attempted nothing leaves the condition alone rather than clearing it.
///
/// Silence is not evidence of recovery: switching the transport offline asks no destination
/// anything, so a condition raised by the last real attempt is still the most recent thing known
/// about that endpoint.
#[tokio::test]
async fn an_offline_pass_does_not_clear_a_standing_failure() {
    let broken = common::broken_sync_server().await;
    let app = app(&broken, ALICE).await;
    app.create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");

    app.sync().await.expect("sync");
    let raised = app
        .transport()
        .last_send_failure()
        .expect("the broken endpoint raised the condition");

    app.transport().set_offline(true);
    app.sync().await.expect("an offline pass is a clean Ok");

    assert_eq!(
        app.transport().last_send_failure(),
        Some(raised),
        "the endpoint was not asked, so nothing was learned and nothing is cleared"
    );
}
