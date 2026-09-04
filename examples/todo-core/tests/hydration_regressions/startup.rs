//! Starting an application up: what a fresh client shows before anything is typed.
//!
//! Part of the `hydration_regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE};
use frontbox::OutboxStore;

/// **Finding 6.** A freshly built application shows what the server already holds, without the
/// caller performing the sequence itself.
///
/// This is the test whose absence was the defect. `refresh_from_server` had six callers and every
/// one of them was a test that called it explicitly as a setup step — so the method was thoroughly
/// proven while nothing checked that an application *starts* by calling it. `examples/todo-app`
/// did not, and the browser UI was write-only against the server: writes drained and showed up in
/// the API, and a reload rendered an empty list.
///
/// The second `app` is the whole point. It is a new client on the same scope with an empty
/// projection, exactly like a reloaded browser tab, and it is handed nothing but `start`.
#[tokio::test]
async fn a_fresh_application_starts_with_the_server_rows() {
    let (base, _state) = server().await;
    let seeder = app(&base, ALICE).await;
    seeder
        .create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");
    seeder
        .create(common::ALICE_USER, "write the trial")
        .await
        .expect("create");
    let report = seeder.sync().await.expect("sync");
    assert_eq!(report.counts.applied, 2, "the seeds reached the server");

    let reloaded = app(&base, ALICE).await;
    assert_eq!(reloaded.store().len(), 0, "a new client starts empty");

    let startup = reloaded.start().await.expect("start");

    assert!(startup.hydrated, "the server was reachable, so it was read");
    assert!(
        startup.is_complete(),
        "and nothing queued went unaccounted for"
    );
    assert_eq!(
        reloaded.store().rows().len(),
        2,
        "start is the whole startup sequence: no explicit refresh_from_server needed"
    );
    assert_eq!(reloaded.store().saving_rows(), 0, "nothing is queued here");
}

/// **Finding 6, the other half.** Starting with no network is a normal start, not a failure.
///
/// The runner treats `Offline` as a clean `Ok` and every other transport error as an `Err`
/// (`src/error.rs`, and `SyncPass::Offline`). `start` takes the same position, because refusing to
/// start when the network is down would be refusing to do the one thing an offline-first cache
/// promises. A wrong base URL is a different error and still propagates.
#[tokio::test]
async fn starting_offline_is_not_a_failure() {
    let (base, _state) = server().await;
    let seeder = app(&base, ALICE).await;
    seeder
        .create(common::ALICE_USER, "already on the server")
        .await
        .expect("create");
    seeder.sync().await.expect("sync");

    let reloaded = app(&base, ALICE).await;
    reloaded.transport().set_offline(true);

    let startup = reloaded
        .start()
        .await
        .expect("an offline start still succeeds");

    assert!(!startup.hydrated, "there was nothing to read it from");
    assert!(
        !startup.is_complete(),
        "and start says so rather than reporting a clean bill"
    );
    assert_eq!(
        reloaded.store().len(),
        0,
        "the projection is untouched, not cleared"
    );
}

/// **Decision 032, end to end.** A reload with no network shows the user's own rows.
///
/// This is the observation D4a could not make and D4b exists for, reachable early because the row
/// store landed in core rather than in each application. The second `TodoApp` shares the backend —
/// the same thing a durable store gives a reloaded tab — and is handed no server it can reach.
#[tokio::test]
async fn an_offline_reload_restores_rows_from_storage() {
    let (base, _state) = server().await;
    let first = app(&base, ALICE).await;
    first
        .create(common::ALICE_USER, "survives a reload")
        .await
        .expect("create");
    first.sync().await.expect("sync");

    // The reload. Same scope, same durable storage, and the network is gone.
    let reloaded = first.reopen();
    reloaded.transport().set_offline(true);
    let startup = reloaded
        .start()
        .await
        .expect("an offline start still succeeds");

    assert!(!startup.hydrated, "there was no server to read");
    assert_eq!(startup.restored, 1, "and the row came back from storage");
    assert!(startup.restored_anything());
    assert_eq!(
        reloaded.store().rows().len(),
        1,
        "the projection is populated with no network at all"
    );
    assert_eq!(
        reloaded.store().rows()[0].title,
        "survives a reload",
        "and it is the user's own row, not a placeholder"
    );
}

/// A hydration does not overwrite a row the user has edited but not yet sent.
///
/// The library enforces this, not the application: `merge_rows` skips rows a pending mutation is
/// bound to. `Startup::protected` is how the application finds out its screen deliberately
/// disagrees with the server.
#[tokio::test]
async fn a_reload_keeps_unsent_edits_over_the_servers_copy() {
    let (base, _state) = server().await;
    let first = app(&base, ALICE).await;
    let id = first
        .create(common::ALICE_USER, "original")
        .await
        .expect("create");
    first.sync().await.expect("sync");

    // An edit that never reaches the server.
    first.transport().set_offline(true);
    first.rename(&id, "edited offline").await.expect("rename");
    assert_eq!(first.outbox().pending_count().await.expect("count"), 1);

    // The reload comes back online and hydrates from a server that never saw the rename.
    let reloaded = first.reopen();
    let startup = reloaded.start().await.expect("start");

    assert!(startup.hydrated, "the server was read");
    assert_eq!(
        startup.protected,
        vec![id.clone()],
        "and startup says which row it refused to overwrite"
    );
    assert_eq!(
        reloaded.store().get(&id).expect("row").title,
        "edited offline",
        "the user's unsent edit survives the server's older copy"
    );
    assert!(
        reloaded.store().is_saving(&id),
        "and the row still shows as saving, because it is"
    );
}
