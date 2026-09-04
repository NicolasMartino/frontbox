//! Observations 1-3: what a queue does while nobody can reach the server.
//!
//! Part of the `observations` suite; see `main.rs` for what these are evidence of.

use crate::common::{self, app, server, ALICE};
use frontbox::{DrainEnd, OutboxStore};

/// **Observation 1.** Offline, three writes queue and the UI still shows them.
#[tokio::test]
async fn observation_1_offline_writes_queue_and_still_render() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;

    app.transport().set_offline(true);
    app.create(common::ALICE_USER, "buy milk")
        .await
        .expect("create");
    app.create(common::ALICE_USER, "write the trial")
        .await
        .expect("create");
    app.create(common::ALICE_USER, "read the trial")
        .await
        .expect("create");

    let report = app.sync().await.expect("sync");
    assert_eq!(report.ended, DrainEnd::Offline, "the plug is out");
    assert_eq!(
        report.passes, 1,
        "offline stops at the first pass rather than retrying behind a dropped connection"
    );

    assert_eq!(app.outbox().pending_count().await.expect("count"), 3);
    assert_eq!(
        app.store().rows().len(),
        3,
        "the optimistic projection renders work the server has never seen"
    );
    assert_eq!(
        app.store().saving_rows(),
        3,
        "and every row can say it is still saving"
    );
}

/// **Observation 2.** One drain empties the queue, and `passes` is the number worth asserting.
#[tokio::test]
async fn observation_2_one_drain_empties_the_queue() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;

    app.transport().set_offline(true);
    for title in ["one", "two", "three"] {
        app.create(common::ALICE_USER, title).await.expect("create");
    }
    app.transport().set_offline(false);

    let report = app.sync().await.expect("sync");
    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(report.counts.applied, 3);
    assert_eq!(
        report.passes, 2,
        "one pass that sends and one that finds the queue empty: a drain stops when a pass drains \
         nothing, not when the queue is known empty"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
    assert_eq!(app.store().saving_rows(), 0, "nothing is saving any more");
    assert_eq!(state.applied_count().await.expect("applied"), 3);

    // The server is the authority, and it agrees with the projection that ran ahead of it.
    app.refresh_from_server().await.expect("refresh");
    assert_eq!(app.store().rows().len(), 3);
}

/// **Observation 2, single-flight.** The same queue at `batch_limit = 1` is four passes, not two.
///
/// This is decision 018's estimate made concrete at small scale: one request per record, all of
/// them inside one `drain` call, where a five-second poll loop would spread them over three
/// intervals.
#[tokio::test]
async fn observation_2b_single_flight_is_one_request_per_record() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await.with_batch_limit(1);

    for title in ["one", "two", "three"] {
        app.create(common::ALICE_USER, title).await.expect("create");
    }
    let report = app.sync().await.expect("sync");

    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(
        report.passes, 4,
        "three sending passes and one that confirms"
    );
    assert_eq!(
        app.transport().request_attempts(),
        3,
        "four passes but three requests: the confirming pass reads an empty batch and returns \
         `Idle` without touching the transport"
    );
    assert_eq!(report.counts.applied, 3);
}

/// **Observation 3.** Queued work outlives the process that queued it.
///
/// # This test used to assert the opposite, and that is the point
///
/// Under D4a storage was in memory, so a second application over a second backend was what a
/// restart looked like: the queue was gone. `AGENTS.md` calls this library "a durable local queue
/// of writes that survives restarts", and *durable* was the adjective D4a could not supply — so
/// this test asserted the loss, in the negative, to prove the gap rather than claim it.
///
/// D4b supplied it. Same scope, new process, work still queued
/// (`wiki/proposals/offline-todo-trial.proposal.md` §3). The assertions below were inverted in
/// place rather than replaced, which is what makes the pair legible: the claim did not change, the
/// answer did.
#[tokio::test]
async fn observation_3_queued_work_survives_a_restart() {
    let (base, _state) = server().await;

    // A real file, because `:memory:` is what every other observation uses and it cannot show
    // this: the point of the case is that the storage outlives the process holding it.
    let directory = std::env::temp_dir().join(format!("frontbox-d4b-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("observation-3.db");
    let _ = std::fs::remove_file(&path);
    let storage = path.to_str().expect("utf-8 path").to_owned();

    let id = {
        let first = common::open(todo_core::Config::single_server(
            &base,
            ALICE,
            common::ALICE_USER,
            &storage,
        ))
        .await;
        first.transport().set_offline(true);
        let id = first
            .create(common::ALICE_USER, "survives a restart")
            .await
            .expect("create");
        assert_eq!(first.outbox().pending_count().await.expect("count"), 1);
        id
    }; // Everything the first "process" held drops here.

    let restarted = common::open(todo_core::Config::single_server(
        &base,
        ALICE,
        common::ALICE_USER,
        &storage,
    ))
    .await;

    assert_eq!(
        restarted.outbox().pending_count().await.expect("count"),
        1,
        "the unsent write outlived the process that queued it"
    );
    let queued = restarted.outbox().pending_batch(10).await.expect("batch");
    assert_eq!(
        queued[0].row.as_ref().map(|row| row.row_id.as_str()),
        Some(id.as_str()),
        "and it still names the row it is about, which is what the merge rule needs after a restart"
    );

    // And the projection comes back too, with no network: the row store outlived the process
    // exactly as the queue did.
    restarted.transport().set_offline(true);
    let startup = restarted.start().await.expect("offline start");
    assert!(!startup.hydrated, "there was no server to read");
    assert_eq!(
        startup.restored, 1,
        "the row came back from durable storage"
    );
    assert_eq!(
        restarted.store().get(&id).expect("row").title,
        "survives a restart"
    );

    let _ = std::fs::remove_file(&path);
}
