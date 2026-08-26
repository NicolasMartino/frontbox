//! Cases 10-12: bounded batches and the total pending order.

use super::prelude::*;

/// `pending_batch` never returns more than `limit`.
///
/// The source loads the entire outbox in one batch, so a single terminal rejection can block every
/// later correlated mutation at once. The bound is what limits that blast radius.
pub async fn case_10_pending_batch_respects_limit<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 5).await?;

    assert_eq!(store.pending_batch(2).await?.len(), 2);
    assert_eq!(store.pending_batch(5).await?.len(), 5);
    assert_eq!(store.pending_batch(50).await?.len(), 5);

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_batch_limit(2);
    let report = runner.sync_once().await?;
    assert_eq!(report.sent, 2);
    Ok(())
}

/// Distinct timestamps come back oldest first, regardless of insertion order.
pub async fn case_11_ordering_is_oldest_first<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(intent(1, 30)).await?;
    store.enqueue(intent(2, 10)).await?;
    store.enqueue(intent(3, 20)).await?;

    let batch = store.pending_batch(10).await?;
    let timestamps: Vec<_> = batch.iter().map(|r| r.created_at).collect();
    assert_eq!(timestamps, vec![10, 20, 30]);
    Ok(())
}

/// Same-millisecond records order stably by `mutation_id`, and repeated reads agree.
///
/// This asserts determinism, never causality. `created_at` is a client clock reading and
/// `mutation_id` is random, so the tie-break makes the order total and reproducible across backends
/// without claiming anything about what actually happened first. Real causal ordering would need a
/// monotonic sequence assigned at enqueue, which this does not provide.
pub async fn case_12_same_timestamp_orders_stably_by_id<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(intent(3, 100)).await?;
    store.enqueue(intent(1, 100)).await?;
    store.enqueue(intent(2, 100)).await?;

    let first: Vec<_> = store
        .pending_batch(10)
        .await?
        .iter()
        .map(|r| r.mutation_id)
        .collect();
    assert_eq!(first, vec![id(1), id(2), id(3)]);

    let second: Vec<_> = store
        .pending_batch(10)
        .await?
        .iter()
        .map(|r| r.mutation_id)
        .collect();
    assert_eq!(
        first, second,
        "repeated reads must return the same sequence"
    );
    Ok(())
}
