//! What hydration is not allowed to delete, and the queue scan that decides it.
//!
//! Part of the `hydration_regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE, ALICE_USER};
use frontbox::OutboxStore;

/// A queued local todo create is not deleted by a server list that cannot know it yet.
#[tokio::test]
async fn todo_hydration_keeps_pending_local_creates() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "queued locally")
        .await
        .expect("create");

    app.refresh_from_server().await.expect("hydrate todos");

    assert!(
        app.store().get(&id).is_some(),
        "the pending create is protected even though the server list is empty"
    );
    assert!(
        app.store().is_saving(&id),
        "and the row still renders as saving"
    );
}

/// A user missing from the server is removed from the cached read model.
///
/// The todo hydration path already did this; the user path only merged, so a browser profile could
/// keep showing a user from IndexedDB after the service no longer returned it. A container restart
/// made that especially visible: the server was empty and the browser still had yesterday's users.
#[tokio::test]
async fn user_hydration_deletes_server_absent_users_without_pending_work() {
    let servers = common::servers().await;
    let stale = common::app_on(&servers, ALICE, common::ALICE_USER).await;
    stale.sign_up("Alice").await.expect("sign up");
    let bob = stale.create_user("Bob").await.expect("create Bob");
    stale.sync().await.expect("seed users");

    let remover = common::app_on(&servers, "user:remover@tenant:acme", common::ALICE_USER).await;
    remover.delete_user(&bob).await.expect("delete Bob");
    remover.sync().await.expect("drain delete");

    assert!(
        stale.store().users().iter().any(|user| user.id == bob),
        "the stale client still has Bob before hydration"
    );
    stale
        .refresh_users_from_server()
        .await
        .expect("hydrate users");
    assert!(
        stale.store().users().iter().all(|user| user.id != bob),
        "hydration removes a cached user the server no longer returns"
    );
}

/// A queued local user create is still protected from an empty server list.
///
/// Deleting stale rows must not turn hydration into replacement: a user row with an unsent create
/// is deliberately ahead of the server and must survive until the outbox drains or dead-letters it.
#[tokio::test]
async fn user_hydration_keeps_pending_local_users() {
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, common::ALICE_USER).await;
    let bob = app.create_user("Bob").await.expect("create Bob");

    app.refresh_users_from_server()
        .await
        .expect("hydrate users");

    assert!(
        app.store().users().iter().any(|user| user.id == bob),
        "the pending user is protected even though the server list is empty"
    );
    assert_eq!(
        app.outbox().pending_count().await.expect("pending"),
        1,
        "the protection is because the create is still queued"
    );
}

/// A queue longer than the pending scan limit keeps its tail through a hydration.
///
/// # What this used to do
///
/// Hydration deletes the rows the server did not mention, and protects whatever is still queued
/// from that delete — the server has never seen a queued write, so its absence from the server's
/// list means nothing. The protection asked for `pending_batch(pending_scan_limit)` and stopped
/// there, so on a queue longer than the limit the tail was unprotected: queued, unseen by the
/// server, absent from its list, and therefore deleted. The user watched rows they had just typed
/// disappear while waiting to drain, and nothing said why.
///
/// `with_pending_scan_limit(1)` is the same shape as a real overflow without ten thousand rows.
/// Three creates and a limit of one leaves two rows depending entirely on the protection.
#[tokio::test]
async fn hydration_does_not_delete_queued_rows_past_the_scan_limit() {
    let (base, _state) = server().await;
    let app = common::open(todo_core::Config::single_server(
        &base, ALICE, ALICE_USER, ":memory:",
    ))
    .await
    .with_pending_scan_limit(1);

    app.create(ALICE_USER, "first").await.expect("create");
    app.create(ALICE_USER, "second").await.expect("create");
    app.create(ALICE_USER, "third").await.expect("create");
    assert_eq!(app.store().len(), 3, "three rows projected locally");
    assert_eq!(
        app.outbox().pending_count().await.expect("pending"),
        3,
        "and all three are still queued"
    );

    // The server has none of them: nothing has drained. Every local row is therefore absent from
    // the response, which is precisely the condition the delete acts on.
    app.refresh_from_server().await.expect("hydrate");

    assert_eq!(
        app.store().len(),
        3,
        "a capped queue scan must suppress the delete, not delete what it could not see"
    );
}

/// The same protection for users, where the delete is more expensive.
///
/// A user row deleted while its signup is still queued does not only lose that row: the todos
/// belonging to it are owned by a user the client no longer lists, and a user delete cascades
/// server-side (`wiki/decisions/039-user-delete-cascades-server-side.decision.md`). Both hydrations
/// now call one function, and this is the second half of its evidence.
#[tokio::test]
async fn user_hydration_does_not_delete_queued_rows_past_the_scan_limit() {
    // Both services, not `single_server`: with `user_url: None` the hydration returns before it
    // reaches the delete, and this test would pass without proving anything.
    let servers = common::servers().await;
    let app = common::app_on(&servers, ALICE, ALICE_USER)
        .await
        .with_pending_scan_limit(1);

    app.sign_up("Alice").await.expect("sign up");
    app.create_user("Bob").await.expect("create user");
    app.create_user("Carol").await.expect("create user");
    assert_eq!(
        app.store().users().len(),
        3,
        "three user rows projected locally"
    );
    assert_eq!(
        app.outbox().pending_count().await.expect("pending"),
        3,
        "and all three are still queued, which is what the delete must respect"
    );

    app.refresh_users_from_server()
        .await
        .expect("hydrate users");

    assert_eq!(
        app.store().users().len(),
        3,
        "a capped queue scan must not let a queued signup be deleted"
    );
}
