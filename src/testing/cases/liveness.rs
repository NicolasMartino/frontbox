//! Cases 19-21: what unblocks, what stalls, and how a caller can tell.

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
/// Retention has no bound here — nothing ages, counts, or escalates a record retained repeatedly —
/// so the minimum obligation is that a stalled queue is distinguishable from an idle one.
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
