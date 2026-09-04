//! User records as rows: round trip, delete, and the cascade that follows.
//!
//! Part of the `multi_domain` observations; see `main.rs` for what they are evidence of.

use crate::common::{self, ALICE, ALICE_USER};
use crate::{read_todos, read_users};
use frontbox::{DrainEnd, OutboxStore};
/// A pass that finds nothing to do still says so.
#[tokio::test]
async fn observation_10_an_empty_queue_is_idle_across_both_services() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    let report = app.sync().await.expect("drain");
    assert_eq!(report.passes, 1, "one pass, and it found nothing to do");
    assert_eq!(report.sent, 0);
    assert_eq!(report.ended, DrainEnd::Drained);
}

/// **11. A user record is created, renamed and deleted through the queue.**
///
/// The CRUD the UI needs, asserted against the user service rather than the projection. Every step
/// is an ordinary queued mutation — there is no handshake anywhere in this flow.
#[tokio::test]
async fn observation_11_a_user_record_round_trips_through_the_queue() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.sign_up("Alice").await.expect("sign up");
    let bob = app.create_user("Bob").await.expect("create user");
    app.sync().await.expect("drain");

    let users = read_users(&servers).await;
    assert_eq!(
        users.len(),
        2,
        "both records reached the service: {users:?}"
    );
    assert!(users.iter().any(|u| u["name"] == "Bob"));

    app.rename_user(&bob, "Robert").await.expect("rename");
    app.sync().await.expect("drain");
    assert!(
        read_users(&servers)
            .await
            .iter()
            .any(|u| u["name"] == "Robert"),
        "the rename landed"
    );

    app.delete_user(&bob).await.expect("delete");
    app.sync().await.expect("drain");
    let users = read_users(&servers).await;
    assert_eq!(users.len(), 1, "Bob is gone, Alice is not: {users:?}");
    assert_eq!(users[0]["id"], ALICE_USER);
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
}

/// **12. Deleting a user empties them of todos, on the todo service.**
///
/// **The cascade is the user service's, not this client's.** One mutation leaves here —
/// `DELETE /api/v1/users/{id}` — and the user service fetches that user's todos from the todo
/// service and deletes them before deleting the record
/// (`examples/user-server/src/todos.rs`). So the assertion has to be on the *todo* service's state,
/// which nothing in this client wrote to during the delete.
#[tokio::test]
async fn observation_12_deleting_a_user_cascades_to_their_todos() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.sign_up("Alice").await.expect("sign up");
    let bob = app.create_user("Bob").await.expect("create user");
    app.create(ALICE_USER, "alice keeps this")
        .await
        .expect("create");
    app.create(&bob, "bob loses this").await.expect("create");
    app.create(&bob, "and this").await.expect("create");
    app.sync().await.expect("drain");
    assert_eq!(read_todos(&servers).await.len(), 3);

    let before = app.outbox().pending_count().await.expect("count");
    assert_eq!(before, 0, "nothing queued before the delete");

    app.delete_user(&bob).await.expect("delete");
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "exactly one mutation — the cascade is not enqueued here"
    );
    app.sync().await.expect("drain");

    let todos = read_todos(&servers).await;
    assert_eq!(todos.len(), 1, "only Alice's survived: {todos:?}");
    assert_eq!(todos[0]["user_id"], ALICE_USER);
    assert_eq!(read_users(&servers).await.len(), 1);
}

/// **13. A cascade that cannot run does not delete the user.**
///
/// The half that stops the cascade being worse than no cascade. With the todo service refusing
/// reads, the user service cannot learn what to delete — so it refuses the whole batch, and the
/// record it was asked to remove is still there when the client retries.
///
/// Without this, a cascade that failed at its first step would delete the user anyway and leave
/// exactly the orphans it exists to prevent.
#[tokio::test]
async fn observation_13_a_failed_cascade_leaves_the_user_alone() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.sign_up("Alice").await.expect("sign up");
    let bob = app.create_user("Bob").await.expect("create user");
    app.create(&bob, "bob's errand").await.expect("create");
    app.sync().await.expect("drain");

    servers.todo.down(true);
    app.delete_user(&bob).await.expect("delete");
    let report = app.sync().await.expect("a refused cascade is not an error");

    assert_eq!(
        report.counts.dead_lettered, 0,
        "a cascade this service could not run is transient, never terminal: {report:?}"
    );
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "the delete is still queued"
    );
    assert_eq!(
        read_users(&servers).await.len(),
        2,
        "and Bob is still there — a half-cascade would have removed him and kept his todo"
    );

    // And it completes once the todo service answers again.
    servers.todo.down(false);
    app.sync().await.expect("retry");
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
    assert_eq!(read_users(&servers).await.len(), 1);
    assert!(
        read_todos(&servers).await.is_empty(),
        "the todo went with him"
    );
}
/// **14. A user-service outage holds a todo instead of refusing it.**
///
/// The mirror of observation 13, on the other service and through the other door: 13 fails the
/// cascade's read, this fails the *reference check's* read.
///
/// # Why this observation could not exist before
///
/// `todo-server` asks `user-server` whether a todo's owner exists, and `Missing` splits the two
/// ways that can go badly — `NoSuchUser` and `Unavailable` — precisely so an outage cannot present
/// to a client as a durable ordering failure (`examples/todo-server/src/users.rs`). The second
/// branch was **unreachable**: `AppState::down` documented itself as failing every read, and
/// `GET /api/v1/users/{id}` — the only read another service makes — did not consult it. So the
/// fixture could not produce the condition its own error type exists to name, and nothing observed
/// the difference between the two.
///
/// The distinction is the whole point. A `404` means the user has not landed yet and the transport
/// maps it to `MutationStatus::Blocked` (observation 3). A `503` means this server could not find
/// out, and the honest answer is to leave the batch alone — retained, no verdict, no dead letter —
/// and say so where a person can see it.
#[tokio::test]
async fn observation_14_a_user_service_outage_holds_a_todo_rather_than_refusing_it() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER).await;

    app.sign_up("Alice").await.expect("sign up");
    app.sync().await.expect("drain the user record");

    // Only the user service. The todo service is up and will be *asked* to accept the todo — the
    // refusal has to come from the check it cannot complete, not from the endpoint being gone.
    servers.user.down(true);
    app.create(ALICE_USER, "buy milk").await.expect("create");
    let report = app
        .sync()
        .await
        .expect("an outage is not an error the pass raises");

    assert_eq!(
        report.counts.dead_lettered, 0,
        "an outage is transient by construction, never terminal: {report:?}"
    );
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "the todo is retained, because nothing about it was refused on the merits"
    );
    assert!(
        read_todos(&servers).await.is_empty(),
        "and it has not landed — the check that could not run did not wave it through"
    );
    let failure = app
        .transport()
        .last_send_failure()
        .expect("a destination that answered a status is a condition worth showing");
    assert!(
        failure.contains("503"),
        "the outage is named rather than presenting as an ordinary quiet pass: {failure}"
    );

    // And it lands the moment the user service can answer again, with nothing re-enqueued by hand.
    servers.user.down(false);
    app.sync().await.expect("retry");
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
    assert_eq!(read_todos(&servers).await.len(), 1);
    assert_eq!(
        app.transport().last_send_failure(),
        None,
        "the condition clears on the pass that succeeds, because it describes the last batch \
         rather than everything that ever went wrong"
    );
}
