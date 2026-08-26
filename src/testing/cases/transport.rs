//! Cases 7-9: offline, attempted failure, and a failing apply.

use super::prelude::*;

/// Offline leaves pending work untouched and is not an error.
///
/// Being offline is the expected operating mode for an offline-first queue. Collapsing it into a
/// generic transport failure would produce misleading status and drive an error-backoff path that
/// should not run.
pub async fn case_07_offline_leaves_work_untouched<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::Offline);
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(report.sent, 2);
    assert_eq!(report.retained, 2);
    assert!(!report.made_progress());
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// An attempted request that failed is an error, and still leaves pending work untouched.
pub async fn case_08_transport_failure_is_an_attempted_send<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::TransportFailure);
    let result = runner.sync_once().await;

    match result {
        Err(Error::Transport { .. }) => {}
        other => panic!("expected a transport failure, got {other:?}"),
    }
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// A failing `apply_outcomes` leaves no partial state.
///
/// The source inserts the dead letter and deletes the outbox row in two disconnected steps,
/// discarding the second's result. When the delete fails, the record is both pending and
/// dead-lettered: it is re-sent on every later sync, the dead letter is rewritten, and
/// `rejected_at` keeps moving forward so retention never purges it.
pub async fn case_09_failed_apply_leaves_no_partial_state<F: FaultInjection>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    factory.fail_next_apply_outcomes().await;

    let runner = runner(
        store,
        Reply::ByPosition(vec![MutationStatus::Applied, MutationStatus::Rejected]),
    );
    let result = runner.sync_once().await;
    assert!(result.is_err(), "a failing apply must surface as an error");

    // Neither half of the transition happened: nothing was deleted, nothing was dead-lettered.
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}
