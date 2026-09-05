//! Cases 19-21, 47 and 48: what unblocks, what stalls, what gives up, and how a caller can tell.

use super::prelude::*;

/// `Blocked` clears on the sync after its blocker is dead-lettered.
///
/// This is the liveness argument for retaining `Blocked` rather than dead-lettering it, and it is
/// the reason retention is safe here. `Blocked` is emitted only after an earlier mutation failed
/// *terminally*; the only terminal status is `Rejected`; and `Rejected` is dead-lettered and removed
/// in the same atomic call. So the blocker is gone before the next batch is built and the blocked
/// record is evaluated on its own merits, with no user action.
///
/// An earlier draft of the decision claimed the opposite — that `Blocked` would repeat until a user
/// intervened. This case is what settles it.
pub async fn case_19_blocked_clears_on_the_next_sync<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let transport =
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).then(Reply::ByPosition(vec![
            MutationStatus::Rejected,
            MutationStatus::Blocked,
        ]));
    let runner = SyncRunner::new(store, transport);

    let first = runner.sync_once().await?;
    assert_eq!(first.counts.dead_lettered, 1);
    assert_eq!(first.counts.blocked, 1);
    assert_eq!(runner.store().pending_count().await?, 1);

    // The blocker is gone, so nothing has to be resolved by hand.
    let second = runner.sync_once().await?;
    assert_eq!(second.counts.applied, 1);
    assert!(second.made_progress());
    assert_eq!(runner.store().pending_count().await?, 0);
    Ok(())
}

/// A sync that drains nothing says so.
///
/// No retention bound is configured here, and that is the default: decision 017 ships the bound
/// opt-in, so an unconfigured queue retains indefinitely exactly as D1 did. The minimum obligation
/// is therefore unchanged — a stalled queue must be distinguishable from an idle one — and case 47
/// covers what happens once a caller does set one.
pub async fn case_20_fully_retained_batch_is_no_progress<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::All(MutationStatus::Pending));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert!(!report.made_progress());
    assert!(report.is_stalled());
    assert_eq!(report.retained, 2);
    assert_eq!(runner.store().pending_count().await?, 2);

    // An empty queue is idle, not stalled. The two must not look the same to a caller.
    let (empty, _) = open(factory).await?;
    empty
        .apply_outcomes(&[
            Outcome::new(id(1), Disposition::Delete),
            Outcome::new(id(2), Disposition::Delete),
        ])
        .await?;
    let idle_runner = SyncRunner::new(
        empty,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    );
    let idle = idle_runner.sync_once().await?;
    assert_eq!(idle.pass, SyncPass::Idle);
    assert!(!idle.is_stalled());
    Ok(())
}

/// A `Pending` prefix as long as the batch limit starves the tail, observably.
///
/// This is head-of-line blocking, not deadlock: it resolves when the server settles those jobs.
/// Documenting it as a test is the point — the behaviour is real, and pretending it cannot happen
/// would be worse than surfacing it.
pub async fn case_21_pending_prefix_starves_the_tail_observably<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 3).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_batch_limit(2);
    let report = runner.sync_once().await?;

    assert_eq!(report.sent, 2);
    assert!(report.is_stalled());

    let sent = runner.transport().sent();
    let sent_ids: Vec<_> = sent[0].mutations.iter().map(|m| m.mutation_id).collect();
    assert_eq!(sent_ids, vec![id(1), id(2)]);
    assert!(
        !sent_ids.contains(&id(3)),
        "the tail never reaches the server while the prefix stays pending"
    );
    assert_eq!(runner.store().pending_count().await?, 3);
    Ok(())
}

/// A record retained to the bound is dead-lettered, with no rejection and its count intact.
///
/// The liveness mechanism decision 016 made necessary. Skipping past a stuck head is the obvious
/// cheap fix and 016 forbids it — a skip is a reorder chosen by the queue — so terminating the
/// record is the only remaining way out. It is a dead letter rather than a discard because decision
/// 005's prior-art section promised that in advance: every surveyed peer discards or rolls back,
/// and preserving the record for inspection is this crate's improvement on the cohort.
///
/// The absent `RemoteRejection` is the point, not an omission. **No server refused this
/// mutation** — it answered `Pending` every time — so synthesising a refusal would put words in the
/// server's mouth. `error: None` beside a non-zero `attempts` is what says "the client gave up"
/// rather than "the server said no".
pub async fn case_47_retention_bound_dead_letters_without_a_rejection<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_retention_bound(2);

    // Two passes record two attempts. The record is still queued: the bound is what it takes to
    // reach, not to exceed.
    for pass in 1..=2 {
        let report = runner.sync_once().await?;
        assert_eq!(report.counts.pending, 1, "pass {pass} is a plain retention");
        assert_eq!(report.counts.dead_lettered, 0);
        assert_eq!(runner.store().pending_count().await?, 1);
    }
    assert_eq!(
        runner.store().pending_batch(10).await?[0].attempts,
        2,
        "each retaining pass records one attempt"
    );

    // The third pass finds the count at the bound and terminates the record.
    let report = runner.sync_once().await?;
    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(
        runner.store().pending_count().await?,
        0,
        "the queue is free"
    );

    let parked = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(parked.len(), 1);
    assert_eq!(
        parked[0].reason,
        DeadLetterReason::RetentionBound,
        "the reason is stated, not left to be inferred from an absence"
    );
    assert_eq!(
        parked[0].rejection(),
        None,
        "no server refused this, so no rejection may be invented"
    );
    assert_eq!(
        parked[0].attempts, 2,
        "the count is what makes the reason legible"
    );
    Ok(())
}

/// A pass that got no usable verdict does not count as an attempt.
///
/// The axis matters. Workbox bounds by age, and for an offline-first queue that is wrong: a record
/// that aged a week because the user was offline was never evaluated, and dead-lettering it would
/// punish the operating mode this crate exists to support. An attempt count measures what actually
/// happened — the server has now told us the same unusable thing N times.
///
/// The two passes here are not the same event and the distinction is worth keeping straight.
/// `Offline` is the transport's claim that the network was unavailable; a transport failure is its
/// report that a request was attempted and failed. Neither is proof about what the server received —
/// `Error::Offline` is documented as a claim precisely because a browser `fetch` rejects the same way
/// whether the request never left or its response was lost, which is why coalescing eligibility is a
/// durable mark written *before* the request rather than an inference from either of these
/// (`wiki/decisions/044-transport-started-before-the-request.decision.md`).
///
/// What the two share is that neither produced a verdict this client can read, and the count
/// measures verdicts received rather than requests made.
pub async fn case_48_no_verdict_is_not_an_attempt<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    // Never sent.
    let offline = runner(store, Reply::Offline);
    let report = offline.sync_once().await?;
    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(
        offline.store().pending_batch(10).await?[0].attempts,
        0,
        "an offline pass evaluates nothing"
    );

    // Sent, but no readable answer came back. A fresh scope, so the two halves cannot interfere.
    let store = factory
        .open(scope("user:frank@tenant:acme@schema:1"))
        .await?;
    seed(&store, 1).await?;
    let failing = runner(store, Reply::TransportFailure);
    assert!(failing.sync_once().await.is_err());
    assert_eq!(
        failing.store().pending_batch(10).await?[0].attempts,
        0,
        "a lost response is not a verdict, however far the request got"
    );

    // And a pass that does produce a verdict counts, so the case is not vacuous.
    let store = factory
        .open(scope("user:grace@tenant:acme@schema:1"))
        .await?;
    seed(&store, 1).await?;
    let answering = runner(store, Reply::All(MutationStatus::Pending));
    answering.sync_once().await?;
    assert_eq!(answering.store().pending_batch(10).await?[0].attempts, 1);
    Ok(())
}
