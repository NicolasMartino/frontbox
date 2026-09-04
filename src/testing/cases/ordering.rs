//! Cases 10-12 and 46: bounded batches and the enqueue order.

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

/// Records come back in enqueue order, and a client clock that runs backwards does not reorder them.
///
/// D2 shipped this as `case_11_ordering_is_oldest_first`, asserting the *timestamp* order — which is
/// the defect `wiki/decisions/016-monotonic-enqueue-sequence.decision.md` removes. `created_at` is a
/// client clock reading, and a clock that steps backwards mid-session, or a caller that stamps its
/// own times, would reorder a queue that was enqueued correctly.
///
/// The case keeps its number because descending timestamps are exactly where the old rule and the
/// new one disagree.
pub async fn case_11_ordering_is_enqueue_order<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(intent(1, 30)).await?;
    store.enqueue(intent(2, 10)).await?;
    store.enqueue(intent(3, 20)).await?;

    let batch = store.pending_batch(10).await?;
    assert_eq!(
        batch.iter().map(|r| r.mutation_id).collect::<Vec<_>>(),
        vec![id(1), id(2), id(3)],
        "the drain replays what the caller wrote, not what its clock said"
    );
    assert_eq!(
        batch.iter().map(|r| r.created_at).collect::<Vec<_>>(),
        vec![30, 10, 20],
        "timestamps are carried, not consulted"
    );

    // The sequence is strictly increasing and is what the order is read from.
    let seqs: Vec<_> = batch.iter().map(|r| r.seq).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "enqueue sequence must strictly increase, got {seqs:?}"
    );
    Ok(())
}

/// Same-millisecond records come back in enqueue order, and repeated reads agree.
///
/// **The case `(created_at, mutation_id)` got wrong.** Three writes in one millisecond — a session,
/// an exercise in it, a set in that — tie on the timestamp and broke on a random v4 UUID, so the
/// set could be drained before the session it belongs to. Under batching the whole batch went out
/// together and the server evaluated it in the same wrong order; under `batch_limit = 1` a wrong
/// order is a wrong write.
///
/// D2 shipped this as `case_12_same_timestamp_orders_stably_by_id`, asserting the UUID tie-break.
/// It kept its number: same setup, opposite expectation.
pub async fn case_12_same_timestamp_keeps_enqueue_order<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    // Enqueued deliberately against UUID order, so sorting by id would be visible.
    store.enqueue(intent(3, 100)).await?;
    store.enqueue(intent(1, 100)).await?;
    store.enqueue(intent(2, 100)).await?;

    let first: Vec<_> = store
        .pending_batch(10)
        .await?
        .iter()
        .map(|r| r.mutation_id)
        .collect();
    assert_eq!(
        first,
        vec![id(3), id(1), id(2)],
        "enqueue order, not UUID order"
    );

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

/// Enqueue order survives reopening the store.
///
/// The property that makes the sequence durable rather than a process-lifetime convenience. A
/// counter that restarts at zero re-orders the queue: records written after the restart sort ahead
/// of everything already queued, which is the exact failure the sequence exists to prevent, and it
/// only appears after a crash.
pub async fn case_46_enqueue_order_survives_a_reopen<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let key = scope("user:erin@tenant:acme@schema:1");

    let first = factory.open(key.clone()).await?;
    first.enqueue(intent(3, 100)).await?;
    first.enqueue(intent(1, 100)).await?;
    let before: Vec<_> = first
        .pending_batch(10)
        .await?
        .iter()
        .map(|r| r.seq)
        .collect();

    // Reopening is how a test says "the process died and came back" to a store with no process.
    let reopened = factory.open(key).await?;
    reopened.enqueue(intent(2, 100)).await?;

    let batch = reopened.pending_batch(10).await?;
    assert_eq!(
        batch.iter().map(|r| r.mutation_id).collect::<Vec<_>>(),
        vec![id(3), id(1), id(2)],
        "a record enqueued after the reopen must sort last, not first"
    );

    let after: Vec<_> = batch.iter().map(|r| r.seq).collect();
    assert_eq!(
        after[..2],
        before[..],
        "sequences already issued must survive unchanged"
    );
    assert!(
        after[2] > after[1],
        "the counter must resume above what it issued, got {after:?}"
    );
    Ok(())
}
