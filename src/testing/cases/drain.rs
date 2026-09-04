//! Cases 57-59: draining until the queue stops draining.
//!
//! A drain is the loop `sync_once` always needed. These cases pin down the two halves that are easy
//! to get wrong: that it keeps going while it is working, and that it stops the moment it is not
//! (`wiki/decisions/029-drain-termination.decision.md`).

use super::prelude::*;
use crate::runner::DrainEnd;

/// One drain empties a queue that one pass cannot.
///
/// The contrast is the point, so the case makes it twice with the same seed and the same limit: a
/// single `sync_once` leaves three of five records queued, and a single `drain` leaves none.
pub async fn case_57_a_drain_empties_what_one_pass_cannot<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    // One pass, for the baseline. Its own scope: the blocks below seed again.
    let store = factory
        .open(scope("user:drain-one-pass@tenant:acme"))
        .await?;
    seed(&store, 5).await?;
    let single = runner(store, Reply::All(MutationStatus::Applied)).with_batch_limit(2);
    single.sync_once().await?;
    assert_eq!(
        single.store().pending_count().await?,
        3,
        "one pass drains one batch and stops"
    );

    let store = factory.open(scope("user:drain-many@tenant:acme")).await?;
    seed(&store, 5).await?;
    let drained = runner(store, Reply::All(MutationStatus::Applied)).with_batch_limit(2);
    let report = drained.drain().await?;

    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(drained.store().pending_count().await?, 0);
    assert_eq!(
        drained.transport().send_count(),
        3,
        "five records at a limit of two is three requests"
    );
    assert_eq!(
        report.passes, 4,
        "three sending passes, then one that confirms the queue is empty rather than assuming it"
    );
    assert_eq!(report.sent, 5, "summed over passes");
    assert_eq!(report.counts.applied, 5);
    assert_eq!(report.retained, 0);
    assert!(report.made_progress());
    Ok(())
}

/// A drain stops at the first pass that drains nothing, and does not burn the retention bound.
///
/// **The case that separates "drain until idle" from "drain until it stops draining".** A loop that
/// stopped only on `Idle` would send this wedged head over and over as fast as the network allowed;
/// with a bound of three it would spend all three attempts inside a few hundred milliseconds and
/// dead-letter the record almost immediately. The bound means *give the server three chances*, and
/// the chances are spread by the caller's cadence, not consumed by one drain
/// (`wiki/decisions/017-bounded-retention.decision.md`).
///
/// So the assertion that matters is a send count of one.
pub async fn case_58_a_drain_stops_when_a_pass_drains_nothing<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    // `Unknown` never resolves from a running client's point of view (decision 012), which is what
    // makes it a head that stays wedged for as long as the case needs.
    let store = factory.open(scope("user:drain-wedged@tenant:acme")).await?;
    seed(&store, 3).await?;
    let wedged = runner(
        store,
        Reply::All(MutationStatus::Unknown("Throttled".into())),
    )
    .with_retention_bound(3);

    let report = wedged.drain().await?;
    assert_eq!(report.ended, DrainEnd::Stalled);
    assert_eq!(report.passes, 1, "it stopped on the first fruitless pass");
    assert_eq!(
        wedged.transport().send_count(),
        1,
        "one request, not one per remaining attempt: the drain is not a spin"
    );
    assert!(!report.made_progress());
    assert_eq!(report.sent, 3);
    assert_eq!(report.retained, 3);
    assert_eq!(report.counts.unknown_status, 3);
    assert_eq!(wedged.store().pending_count().await?, 3);
    assert!(
        wedged
            .store()
            .pending_batch(10)
            .await?
            .iter()
            .all(|record| record.attempts == 1),
        "one drain is one attempt, however many passes it could have run"
    );

    // The bound is still reached — across drains, one attempt each, which is the behaviour the
    // caller's cadence is supposed to control.
    for _ in 0..2 {
        assert_eq!(wedged.drain().await?.ended, DrainEnd::Stalled);
    }
    assert_eq!(wedged.store().pending_count().await?, 3, "still wedged");

    let freed = wedged.drain().await?;
    assert_eq!(
        freed.counts.dead_lettered, 3,
        "the bound fired on the fourth"
    );
    assert_eq!(
        freed.ended,
        DrainEnd::Drained,
        "dead-lettering is progress, so the drain went round again and found the queue empty"
    );
    assert_eq!(wedged.store().pending_count().await?, 0);
    assert_eq!(
        DeadLetterStore::list(wedged.store(), 10).await?[0].reason,
        DeadLetterReason::RetentionBound
    );
    Ok(())
}

/// An offline drain stops at the first pass and leaves the queue untouched.
///
/// Offline is not a failed attempt and not a reason to retry harder. One request is attempted, it
/// reports offline, and the drain ends — a caller waiting for connectivity is not helped by a loop
/// asking again immediately.
pub async fn case_59_an_offline_drain_stops_at_the_first_pass<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let store = factory
        .open(scope("user:drain-offline@tenant:acme"))
        .await?;
    seed(&store, 3).await?;
    let offline = runner(store, Reply::Offline);

    let report = offline.drain().await?;
    assert_eq!(report.ended, DrainEnd::Offline);
    assert_eq!(report.passes, 1);
    assert_eq!(
        offline.transport().send_count(),
        1,
        "one attempt, not a retry storm behind a dropped connection"
    );
    assert_eq!(report.sent, 3);
    assert_eq!(report.retained, 3);
    assert_eq!(offline.store().pending_count().await?, 3, "untouched");
    assert!(report.anomalies.is_empty());

    // An empty queue is the other single-pass ending, and it costs no request at all.
    let store = factory.open(scope("user:drain-empty@tenant:acme")).await?;
    let idle = runner(store, Reply::All(MutationStatus::Applied));
    let report = idle.drain().await?;
    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(report.passes, 1, "a drain always runs at least one pass");
    assert_eq!(idle.transport().send_count(), 0);
    assert!(
        !report.made_progress(),
        "nothing was drained, because there was nothing to drain"
    );
    Ok(())
}
