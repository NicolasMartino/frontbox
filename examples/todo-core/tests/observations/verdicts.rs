//! Observations 4-6 and their two companions: what happens once a server does answer.
//!
//! Part of the `observations` suite; see `main.rs` for what these are evidence of.

use crate::common::{self, app, server, ALICE};
use frontbox::{DeadLetterReason, DrainEnd, OutboxStore};
use todo_core::Direct;

/// **Observation 4.** A refusal lands in dead letters carrying the server's own words.
#[tokio::test]
async fn observation_4_a_refusal_becomes_a_readable_dead_letter() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;

    app.create(common::ALICE_USER, "keeps its title")
        .await
        .expect("create");
    app.create(common::ALICE_USER, "   ").await.expect("create"); // the server refuses a blank title
    app.create(common::ALICE_USER, "also fine")
        .await
        .expect("create");

    let report = app.sync().await.expect("sync");
    assert_eq!(report.counts.applied, 2);
    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        0,
        "a refusal drains the queue as surely as an acceptance; it just drains elsewhere"
    );

    let parked = app.dead_letters(100).await.expect("dead letters");
    assert_eq!(parked.len(), 1);
    let DeadLetterReason::Rejected { error: Some(error) } = &parked[0].reason else {
        panic!(
            "expected a server refusal carrying its payload, got {:?}",
            parked[0].reason
        );
    };
    assert_eq!(error.code.as_deref(), Some("invalid_input"));
    assert_eq!(error.message, "title must not be empty");
    assert!(
        error.details.is_some(),
        "the UI has something to render beyond a status word"
    );
}

/// **Observation 5.** The client dies between the server's commit and its own apply. The resend is
/// safe, and `Duplicate` is how the server says so.
///
/// The window is not exotic. `apply_outcomes` is atomic on the *client* (decision 003), so nothing
/// spans the server's commit and the client's; on the web this gap is a page reload. Here it is
/// simulated exactly: the batch is put on the wire by hand and the response thrown away, which
/// leaves the record queued and the server's effect committed.
#[tokio::test]
async fn observation_5_a_resend_after_a_lost_verdict_is_a_duplicate() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;
    app.create(common::ALICE_USER, "written once")
        .await
        .expect("create");

    // The send that the client never got to apply.
    let queued = app.outbox().pending_batch(10).await.expect("batch");
    let request = frontbox::MutationBatchRequest::new(
        queued
            .iter()
            .map(frontbox::OutboxRecord::to_intent)
            .collect(),
    );
    let answered = frontbox::SyncTransport::send_batch(app.transport(), request)
        .await
        .expect("send");
    assert_eq!(answered.results.len(), 1, "the server did rule on it");
    assert_eq!(state.applied_count().await.expect("applied"), 1);
    drop(answered); // the verdict is lost, exactly as a reload would lose it

    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "still queued, because nothing applied the verdict"
    );

    let report = app.sync().await.expect("sync");
    assert_eq!(
        report.counts.duplicate, 1,
        "the id is the idempotency key, and the server recognised it"
    );
    assert_eq!(report.counts.applied, 0);
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);

    app.refresh_from_server().await.expect("refresh");
    assert_eq!(
        app.store().rows().len(),
        1,
        "written once, sent twice, one row"
    );
}

/// **Observation 6.** A record the server never resolves is terminated by the bound, and says so.
///
/// A positive assertion on the variant rather than on an absent payload, which is what decision 027
/// exists to make possible: before it, "no rejection" was how a caller had to infer this, and an
/// application parking a record for its own reason produced the identical silence.
#[tokio::test]
async fn observation_6_a_wedged_record_terminates_at_the_bound() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await.with_retention_bound(2);

    state.hold(true); // every mutation comes back Pending, forever
    app.create(common::ALICE_USER, "never resolved")
        .await
        .expect("create");

    let first = app.sync().await.expect("sync");
    assert_eq!(first.ended, DrainEnd::Stalled, "sent, and drained nothing");
    assert_eq!(
        app.transport().request_attempts(),
        1,
        "one request: a drain that makes no progress stops instead of spending the bound"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 1);

    assert_eq!(app.sync().await.expect("sync").ended, DrainEnd::Stalled);
    assert_eq!(app.outbox().pending_count().await.expect("count"), 1);

    let freed = app.sync().await.expect("sync");
    assert_eq!(freed.counts.dead_lettered, 1);
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);
    let parked = app.dead_letters(100).await.expect("dead letters");
    assert_eq!(
        parked[0].reason,
        DeadLetterReason::RetentionBound,
        "no server refused it; it ran out of attempts, and the record says which"
    );
    assert_eq!(state.applied_count().await.expect("applied"), 0);
}

/// **The direct-dispatch bet**, tested on the write it was predicted to be tested on.
///
/// Three paths, and the third is the one `wiki/proposals/extraction-boundary.proposal.md` bet a
/// caller-supplied `MutationId` would cover.
#[tokio::test]
async fn direct_dispatch_falls_back_under_the_same_mutation_id() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "toggle me")
        .await
        .expect("create");
    app.sync().await.expect("sync");

    // Reachable: the write lands directly and nothing is queued.
    assert!(matches!(
        app.set_done(&id, true).await.expect("toggle"),
        Direct::Applied
    ));
    assert_eq!(app.outbox().pending_count().await.expect("count"), 0);

    // Unreachable: undecided, so it falls back to the queue.
    state.hold(true);
    assert!(matches!(
        app.set_done(&id, false).await.expect("toggle"),
        Direct::Queued
    ));
    assert_eq!(
        app.outbox().pending_count().await.expect("count"),
        1,
        "queued, not lost"
    );

    // The queued replay carries the *same* id the direct attempt used — that is what the pattern
    // is for. It comes back `Applied` rather than `Duplicate` because the held attempt was
    // refused before it reached the idempotency table, so the server never recorded that id.
    // Observation 5 is the other half: when the write does land and only the response is lost,
    // the same id comes back `Duplicate`.
    state.hold(false);
    let report = app.sync().await.expect("sync");
    assert_eq!(
        report.counts.applied, 1,
        "the replay was the write that landed"
    );
    assert_eq!(
        report.counts.duplicate, 0,
        "the held attempt never reached the table"
    );
    app.refresh_from_server().await.expect("refresh");
    assert!(!app.store().get(&id).expect("row").done);
}

/// A server that declines to rule leaves the record queued and reports no anomaly.
///
/// Silence is not a verdict (conformance case 33), and this is that case against a real server
/// rather than a scripted one.
#[tokio::test]
async fn a_silent_server_retains_without_complaining() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;
    app.create(common::ALICE_USER, "unanswered")
        .await
        .expect("create");

    state.silent(true);
    let report = app.sync().await.expect("sync");
    assert_eq!(report.ended, DrainEnd::Stalled);
    assert_eq!(report.retained, 1);
    assert!(
        report.anomalies.is_empty(),
        "declining to rule is not a protocol violation"
    );
    assert_eq!(app.outbox().pending_count().await.expect("count"), 1);
}
