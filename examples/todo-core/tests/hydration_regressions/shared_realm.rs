//! Two windows on one durable store, and what each has to hydrate for itself.
//!
//! Part of the `hydration_regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE, ALICE_USER};
use frontbox::OutboxStore;

/// Two browser tabs share IndexedDB, but each has its own polling source state.
///
/// The first tab can observe a server version change and write that version into the shared cache
/// before the second tab polls. Core then quite properly reports "no durable version change" in
/// the second tab. The UI still needs the second tab to hydrate, because its in-memory projection
/// has not seen the user list that belongs to that version.
#[tokio::test]
async fn polling_hydrates_a_tab_when_the_shared_version_store_already_advanced() {
    let servers = common::servers().await;
    let tab_a = common::app_on(&servers, ALICE, ALICE_USER).await;
    tab_a.sign_up("Alice").await.expect("sign up");
    tab_a.sync().await.expect("drain Alice");

    let tab_b = tab_a.reopen();
    let baseline = tab_b.poll_invalidation().await.expect("baseline poll");
    assert!(
        baseline.changed.iter().any(|entity| entity == "user"),
        "the second tab observed the initial user-service version: {baseline:?}"
    );
    tab_b
        .refresh_users_from_server()
        .await
        .expect("hydrate baseline users");

    let bob = tab_a.create_user("Bob").await.expect("create Bob");
    tab_a.sync().await.expect("drain Bob");

    let tab_a_tick = tab_a.refresh_now().await.expect("tab A poll");
    assert!(
        tab_a_tick.changed.iter().any(|entity| entity == "user"),
        "tab A observed Bob and advanced the shared version row: {tab_a_tick:?}"
    );
    tab_a
        .refresh_users_from_server()
        .await
        .expect("tab A hydrate");

    let tab_b_tick = tab_b.refresh_now().await.expect("tab B poll");
    assert!(
        tab_b_tick.changed.iter().any(|entity| entity == "user"),
        "tab B must still hydrate even though tab A already wrote the shared version: \
         {tab_b_tick:?}"
    );
    tab_b
        .refresh_users_from_server()
        .await
        .expect("tab B hydrate");
    assert_eq!(tab_b.user_name(&bob).as_deref(), Some("Bob"));
}

/// **The "saving…" marker outlives the record when another window drains it.**
///
/// On web the outbox is one origin-scoped IndexedDB database, so two browser windows are two
/// `TodoStore`s over one queue — [`reopen`](todo_core::TodoApp::reopen) is that arrangement, a
/// second application on the same backend and scope with its own projection and its own pending
/// index. Decision 035 moves the index by exactly what a pass's `DrainReport::drained` names, which
/// is correct for work *this* client drained and says nothing about work another realm drained out
/// from under it.
///
/// Reproduced in a browser before it was written here: with two windows open and the first switched
/// offline, a todo enqueued in the first reached the server through the second, and the two windows
/// then disagreed about the same row — `saving` in one and not in the other, permanently.
#[tokio::test]
async fn a_record_drained_by_another_window_clears_this_window_s_saving_marker() {
    let (base, _state) = server().await;
    let tab_a = app(&base, ALICE).await;
    let id = tab_a
        .create(ALICE_USER, "queued in tab A")
        .await
        .expect("create");
    assert!(tab_a.store().is_saving(&id), "tab A marked its own enqueue");

    let tab_b = tab_a.reopen();
    let drained_by_b = tab_b.sync().await.expect("tab B drains the shared queue");
    assert_eq!(
        drained_by_b.drained.len(),
        1,
        "the second window drained the first window's record: {drained_by_b:?}"
    );

    let drained_by_a = tab_a.sync().await.expect("tab A syncs");
    assert!(
        drained_by_a.drained.is_empty(),
        "there was nothing left in the shared queue for tab A to drain: {drained_by_a:?}"
    );
    assert!(
        !tab_a.store().is_saving(&id),
        "the row must stop saying 'saving…'. The record is gone from the queue, and only the \
         window that happened to win the drain would ever hear about it through `drained`"
    );
    assert_eq!(tab_a.store().saving_rows(), 0);
}

/// A queued record belonging to another entity is not an *unplaceable* row.
///
/// The rebuild sorted every non-todo record into `unrecoverable`, which the status bar renders as
/// "index incomplete: N unplaceable". Signing up while offline and reloading therefore reported a
/// perfectly healthy queue as broken. The pending index is the todo list's "saving…" marker, and a
/// queued `user` mutation is simply not its business; the only thing it genuinely cannot place is a
/// record that names no row at all.
#[tokio::test]
async fn a_queued_user_record_is_not_an_unplaceable_row() {
    let servers = common::servers().await;
    let client = common::app_on(&servers, ALICE, ALICE_USER).await;
    client.transport().set_offline(true);
    client.sign_up("Alice").await.expect("sign up offline");

    assert_eq!(
        client.outbox().pending_count().await.expect("count"),
        1,
        "the user record is queued, which is the whole point of signing up offline"
    );

    let gap = client.refresh_pending().await.expect("rebuild the index");
    assert_eq!(
        gap.unrecoverable, 0,
        "a queued user record is placed, not unplaceable"
    );
    assert!(
        gap.is_empty(),
        "and the index is therefore complete, so nothing tells the user it is not: {gap:?}"
    );
}
