//! Cases 1-6: how each server verdict lands locally.

use super::prelude::*;

/// `Applied` removes the record from the outbox.
pub async fn case_01_applied_is_deleted<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Applied));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.counts.applied, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Duplicate` removes the record too: the server already has it, so replaying is pointless.
pub async fn case_02_duplicate_is_deleted<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Duplicate));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.duplicate, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Rejected` is the only status that produces a dead letter.
///
/// The record leaves the outbox and is preserved for inspection rather than discarded. Every peer
/// system surveyed discards or rolls back the terminal case; keeping it is deliberate.
pub async fn case_03_rejected_becomes_a_dead_letter<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 1);

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].mutation_id, id(1));
    Ok(())
}

/// `Blocked` stays queued.
///
/// This diverges from the source, which dead-letters `Blocked` alongside `Rejected`. A blocked
/// mutation was skipped because an earlier ordered mutation failed terminally — it was never
/// evaluated on its own merits, so dead-lettering it discards valid work for the sole reason that
/// it followed a failed record.
pub async fn case_04_blocked_stays_queued<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Blocked));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.blocked, 1);
    assert_eq!(report.retained, 1);
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Pending` stays queued: the server accepted it but has not finished.
pub async fn case_05_pending_stays_queued<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Pending));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.pending, 1);
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// One batch carrying all five statuses applies each correctly, in one atomic call.
pub async fn case_06_mixed_batch_applies_every_status<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 5).await?;

    let runner = runner(
        store,
        Reply::ByPosition(vec![
            MutationStatus::Applied,
            MutationStatus::Duplicate,
            MutationStatus::Rejected,
            MutationStatus::Blocked,
            MutationStatus::Pending,
        ]),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.applied, 1);
    assert_eq!(report.counts.duplicate, 1);
    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(report.counts.blocked, 1);
    assert_eq!(report.counts.pending, 1);

    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 1);

    let remaining = runner.store().pending_batch(10).await?;
    let remaining_ids: Vec<_> = remaining.iter().map(|r| r.mutation_id).collect();
    assert_eq!(remaining_ids, vec![id(4), id(5)]);
    Ok(())
}
