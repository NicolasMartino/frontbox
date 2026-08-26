//! Cases 13, 14, 17: dead-letter timestamps, retention, and payloads.

use super::prelude::*;

/// `rejected_at` comes from the injected clock, not from wall-clock time inside the store.
pub async fn case_13_rejected_at_comes_from_the_clock<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let clock = factory.clock();
    clock.set(1_700_000_000_000);

    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    runner.sync_once().await?;

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].rejected_at, 1_700_000_000_000);
    Ok(())
}

/// Purge drops exactly the dead letters older than the supplied cutoff.
pub async fn case_14_purge_uses_the_supplied_cutoff<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let clock = factory.clock();
    let (store, _) = open(factory).await?;

    clock.set(1_000);
    store.enqueue(intent(1, 1)).await?;
    store
        .apply_outcomes(&[Outcome::new(id(1), Disposition::DeadLetter { error: None })])
        .await?;

    clock.set(2_000);
    store.enqueue(intent(2, 2)).await?;
    store
        .apply_outcomes(&[Outcome::new(id(2), Disposition::DeadLetter { error: None })])
        .await?;

    assert_eq!(DeadLetterStore::count(&store).await?, 2);
    assert_eq!(DeadLetterStore::purge_older_than(&store, 1_500).await?, 1);

    let remaining = DeadLetterStore::list(&store, 10).await?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].mutation_id, id(2));
    Ok(())
}

/// A rejection payload survives the trip through storage intact.
pub async fn case_17_rejection_payload_round_trips<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(
        store,
        Reply::Exact(vec![
            MutationResult::new(id(1), MutationStatus::Rejected).with_error(full_rejection())
        ]),
    );
    runner.sync_once().await?;

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].error.as_ref(), Some(&full_rejection()));
    Ok(())
}
