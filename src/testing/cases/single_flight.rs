//! Cases 52-54, 60 and 61: the profile that runs at `batch_limit = 1`.
//!
//! Two behaviours are unobservable at the default limit of 100 and obvious at 1: a head that never
//! clears, and whether ordering is respected rather than accidentally correct because everything
//! went out in one request. A defect in either is invisible to the rest of the suite, which is why
//! this profile exists as coverage frontbox owes itself rather than as a favour to one consumer
//! (`wiki/decisions/018-single-flight-drain-mode.decision.md`).

use super::prelude::*;
use crate::runner::DrainEnd;

/// One pass sends exactly one record, whatever the queue holds and however the pass ends.
///
/// The adaptation the rest of the suite cannot make. Cases 07, 20 and 33 all assert `sent` or
/// `retained` equal to the number seeded — 2, 2 and 3 — which is the right assertion at the default
/// limit and arithmetic that cannot hold here. This states the property those cases were reaching
/// for, expressed against the limit instead of the seed.
pub async fn case_52_one_record_per_pass_whatever_the_outcome<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    // Each block opens its own scope: the shared helper reuses one, and three seeds into it would
    // stack rather than start clean.
    // Offline: the batch is read and nothing is sent, but the width is still one.
    let store = factory.open(scope("user:sf-offline@tenant:acme")).await?;
    seed(&store, 3).await?;
    let offline =
        SyncRunner::new(store, ScriptedTransport::new(Reply::Offline)).with_batch_limit(1);
    let report = offline.sync_once().await?;
    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(report.sent, 1, "the window is one even when nothing leaves");
    assert_eq!(report.retained, 1);
    assert_eq!(offline.store().pending_count().await?, 3);

    // Retained: a pass that drains nothing reports no progress, and is stalled.
    let store = factory.open(scope("user:sf-retained@tenant:acme")).await?;
    seed(&store, 3).await?;
    let retaining = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_batch_limit(1);
    let report = retaining.sync_once().await?;
    assert_eq!(report.sent, 1);
    assert_eq!(report.retained, 1, "not the three that are queued");
    assert!(!report.made_progress());
    assert!(report.is_stalled());

    // Omitted: the server answers the batch and says nothing about the one record in it.
    let store = factory.open(scope("user:sf-silent@tenant:acme")).await?;
    seed(&store, 3).await?;
    let silent = SyncRunner::new(store, ScriptedTransport::new(Reply::ByPosition(vec![])))
        .with_batch_limit(1);
    let report = silent.sync_once().await?;
    assert_eq!(report.sent, 1);
    assert_eq!(report.retained, 1, "silence retains the one record sent");
    assert!(report.anomalies.is_empty(), "silence is not an anomaly");
    assert_eq!(silent.store().pending_count().await?, 3);
    Ok(())
}

/// A wedged head freezes the entire queue, and a retention bound is the only thing that frees it.
///
/// **The case single-flight exists to make observable.** At the default limit a retained head
/// starves its window while ninety-nine other records still go out; at 1 the window *is* the head,
/// so one record the server never resolves stops all sync indefinitely.
///
/// `pending_batch` is oldest-first with no cursor, so the next pass reads the same record. Skipping
/// past it is the obvious cheap fix and decision 016 forbids it — a skip is a reorder chosen by the
/// queue rather than by the caller — which leaves terminating the record as the only remaining
/// liveness mechanism. That is the whole argument for decision 017 in one case.
pub async fn case_53_a_wedged_head_freezes_the_queue_until_the_bound<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    // An `Unknown` status never resolves from a running client's point of view: it clears only when
    // the client is rebuilt with a vocabulary it did not have (decision 012).
    let store = factory.open(scope("user:sf-wedged@tenant:acme")).await?;
    seed(&store, 3).await?;
    let wedged = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Unknown("Throttled".into()))),
    )
    .with_batch_limit(1);

    for pass in 1..=3 {
        let report = wedged.sync_once().await?;
        assert!(report.is_stalled(), "pass {pass} makes no progress");
        assert_eq!(
            wedged.store().pending_count().await?,
            3,
            "nothing behind the head has moved after {pass} passes"
        );
    }
    let head = wedged.store().pending_batch(10).await?;
    assert_eq!(
        head[0].attempts, 3,
        "the same record was sent every time, and the count is what proves it"
    );
    assert_eq!(
        wedged.transport().send_count(),
        3,
        "three requests, all carrying the same record"
    );

    // The same queue with a bound terminates the head and the tail starts moving.
    let store = factory.open(scope("user:sf-bounded@tenant:acme")).await?;
    seed(&store, 3).await?;
    let bounded = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Unknown("Throttled".into()))),
    )
    .with_batch_limit(1)
    .with_retention_bound(2);

    for _ in 0..2 {
        bounded.sync_once().await?;
    }
    assert_eq!(bounded.store().pending_count().await?, 3, "still wedged");

    let freed = bounded.sync_once().await?;
    assert_eq!(freed.counts.dead_lettered, 1);
    assert_eq!(
        bounded.store().pending_count().await?,
        2,
        "the head is gone and the queue can advance"
    );
    assert_eq!(
        DeadLetterStore::list(bounded.store(), 10).await?[0].reason,
        DeadLetterReason::RetentionBound,
        "no server refused it; it ran out of attempts, and the record says so"
    );
    Ok(())
}

/// The server sees one record at a time, in enqueue order.
///
/// At the default limit the whole batch goes out in one request and the server evaluates it in
/// whatever order the payload happens to carry, so an ordering defect is invisible: the records
/// arrive together either way. At 1 each request is independently committed, and **a wrong order is
/// a wrong write** — a set arriving against a session that does not exist yet.
///
/// The records are enqueued deliberately against both of the orders the old key would have
/// produced: descending `created_at`, and ids that do not sort in enqueue order.
pub async fn case_54_records_reach_the_server_one_at_a_time_in_order<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let store = factory.open(scope("user:sf-order@tenant:acme")).await?;
    store.enqueue(intent(3, 30)).await?;
    store.enqueue(intent(1, 20)).await?;
    store.enqueue(intent(2, 10)).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    )
    .with_batch_limit(1);

    for _ in 0..3 {
        runner.sync_once().await?;
    }
    assert_eq!(
        runner.store().pending_count().await?,
        0,
        "the queue drained"
    );

    let requests = runner.transport().sent();
    assert_eq!(requests.len(), 3, "one request per record");
    assert!(
        requests.iter().all(|request| request.mutations.len() == 1),
        "each request carries exactly one mutation"
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.mutations[0].mutation_id)
            .collect::<Vec<_>>(),
        vec![id(3), id(1), id(2)],
        "enqueue order, not timestamp order and not id order"
    );
    Ok(())
}

/// One `drain` clears a single-flight backlog that the source's loop would spread over minutes.
///
/// **The case that recovers decision 018's estimate.** At `batch_limit = 1` a backlog needs one
/// request per record, and the source drives them from a loop that sleeps five seconds between
/// passes — so the drain rate is set by the cadence, not by the round trip. Five records is five
/// poll intervals there and one call here; the same arithmetic at 200 records is the difference
/// between roughly seventeen minutes and roughly twenty seconds.
///
/// The ordering assertion rides along deliberately: a drain must not reorder what
/// [`case_54`](case_54_records_reach_the_server_one_at_a_time_in_order) established for individual
/// passes, and it reads the same queue five times to do it.
pub async fn case_60_a_drain_clears_a_single_flight_backlog<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let store = factory.open(scope("user:sf-drain@tenant:acme")).await?;
    seed(&store, 5).await?;
    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    )
    .with_batch_limit(1);

    let report = runner.drain().await?;

    assert_eq!(report.ended, DrainEnd::Drained);
    assert_eq!(
        runner.store().pending_count().await?,
        0,
        "one call drained it"
    );
    assert_eq!(report.counts.applied, 5);
    assert_eq!(
        report.passes, 6,
        "five sending passes and one that finds the queue empty"
    );

    let requests = runner.transport().sent();
    assert_eq!(
        requests.len(),
        5,
        "one request per record, as the limit says"
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.mutations[0].mutation_id)
            .collect::<Vec<_>>(),
        (1..=5).map(id).collect::<Vec<_>>(),
        "enqueue order survives the loop, not just the pass"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-realm single-flight
// ---------------------------------------------------------------------------

/// Two handles on one scope do not drain together.
///
/// **The case that fails if the guard sits on the runner.** A `Cell<bool>` on `SyncRunner` excludes
/// a second pass by that runner and nothing else, while two handles opened on one scope are two
/// views of one queue — so two runners over them each find their own flag false and both send the
/// head. Idempotency absorbs the double send, because the server dedupes on `mutation_id`. Ordering
/// does not: at a limit of one the second drainer ships record 2 while record 1 is still in flight,
/// which is exactly the guarantee decision 016's monotonic `seq` exists to provide. The retention
/// bound does not either, since two drainers spend one record's `attempts` budget in parallel
/// (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
///
/// **What it does not prove.** Both runners live in one realm, which is all a conformance suite can
/// build: it drives a backend through one process, and a second realm is not something a case can
/// open. A durable backend therefore passes this without holding a lock that outlives a realm, and
/// two browser tabs over one IndexedDB database are exactly that second realm. Extending the
/// exclusion across realms is stated on [`OutboxStore`] as prose, for the same reason decision 024's
/// injectivity obligation is: the case can pin the half it can reach, and saying so is what keeps
/// the other half from looking covered.
pub async fn case_61_two_handles_on_one_scope_do_not_drain_together<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    let key = scope("user:sf-two-handles@tenant:acme");
    let first_handle = factory.open(key.clone()).await?;
    let second_handle = factory.open(key.clone()).await?;
    seed(&first_handle, 2).await?;
    assert_eq!(
        second_handle.pending_count().await?,
        2,
        "the two handles must be two views of one queue, or this case tests nothing"
    );

    let first = SyncRunner::new(
        first_handle,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).yielding(),
    )
    .with_batch_limit(1);
    let second = SyncRunner::new(
        second_handle,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    )
    .with_batch_limit(1);

    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);

    let mut in_flight = Box::pin(first.sync_once());
    assert!(
        in_flight.as_mut().poll(&mut cx).is_pending(),
        "the transport must suspend so the second handle can be tried mid-flight"
    );

    match Box::pin(second.sync_once()).as_mut().poll(&mut cx) {
        Poll::Ready(Ok(report)) => assert_eq!(
            report.pass,
            SyncPass::AlreadyRunning,
            "a second handle on a draining scope must stand down"
        ),
        other => panic!("a drain on a busy scope must return immediately, got {other:?}"),
    }
    assert_eq!(
        second.transport().send_count(),
        0,
        "standing down means sending nothing, not sending and discarding"
    );

    // Awaited rather than polled in a loop, for the reason case 28 carries in full: a busy
    // `loop { poll() }` with a noop waker never yields to whatever completes the work, so on a
    // backend that suspends on a browser event it spins the only thread the event loop has and
    // freezes the tab. The manual polls above are still needed — they are what suspends a pass
    // mid-flight so the second handle can be tried — but finishing it belongs to the executor.
    in_flight.await?;

    assert_eq!(first.transport().send_count(), 1, "one pass, one record");
    assert_eq!(
        first.store().pending_count().await?,
        1,
        "the head was applied and the tail is untouched"
    );
    let remaining = first.store().pending_batch(10).await?;
    assert_eq!(
        remaining[0].mutation_id,
        id(2),
        "record 2 is still the tail, so nothing was drained out of order"
    );

    // And the scope is free again: the claim is released when the pass ends, not when the runner is
    // dropped.
    let after = second.sync_once().await?;
    assert_eq!(after.pass, SyncPass::Completed);
    assert_eq!(first.store().pending_count().await?, 0);
    Ok(())
}
