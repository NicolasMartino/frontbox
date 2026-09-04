//! Cases 15, 18, 26, 50, 69: rows that will not decode.

use super::prelude::*;

/// A corrupt record stops counting as pending and becomes visible as quarantined.
///
/// The source hides malformed rows behind `filter_map(|r| r.ok())`. Such a row stays in storage,
/// keeps occupying space, is never sent, is never deleted, and never reaches a user-visible view —
/// it just quietly disappears from every read while the pending count says work remains.
pub async fn case_15_corrupt_record_leaves_the_pending_total<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, key) = open(factory).await?;
    store.enqueue(intent(1, 1)).await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::InvalidBody { id: id(2) })
        .await?;

    // Undecodable rows were never counted as pending...
    assert_eq!(store.pending_count().await?, 1);
    assert_eq!(QuarantineStore::count(&store).await?, 0);

    // ...and the sweep is what makes them visible instead of merely absent.
    assert_eq!(store.sweep_corrupt().await?, 1);
    assert_eq!(store.pending_count().await?, 1);
    assert_eq!(QuarantineStore::count(&store).await?, 1);

    let quarantined = QuarantineStore::list(&store, 10).await?;
    assert_eq!(quarantined[0].mutation_id, Some(id(2)));
    assert!(!quarantined[0].reason.is_empty());
    Ok(())
}

/// The sweep reaches rows no `Outcome` can name.
///
/// A row whose identifier will not parse cannot be addressed through the outcome API at all, so
/// without a backend-owned scan there is no way to reach it from outside.
pub async fn case_18_sweep_finds_rows_outcomes_cannot_name<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, key) = open(factory).await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::UnparseableId)
        .await?;

    assert_eq!(store.pending_count().await?, 0);
    assert_eq!(store.sweep_corrupt().await?, 1);

    let quarantined = QuarantineStore::list(&store, 10).await?;
    assert_eq!(quarantined.len(), 1);
    assert_eq!(
        quarantined[0].mutation_id, None,
        "an unparseable id must stay unparsed, not be invented"
    );
    assert!(!quarantined[0].raw_mutation_id.is_empty());
    Ok(())
}

/// A timestamp the wire format cannot express is corruption, not a panic.
pub async fn case_26_unrepresentable_timestamp_is_corruption_not_panic<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, key) = open(factory).await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::UnrepresentableCreatedAt { id: id(7) })
        .await?;

    assert_eq!(store.pending_count().await?, 0);
    assert_eq!(store.sweep_corrupt().await?, 1);

    let quarantined = QuarantineStore::list(&store, 10).await?;
    assert_eq!(quarantined[0].mutation_id, Some(id(7)));
    Ok(())
}

/// A quarantine transition rolls back with the batch it committed in.
///
/// The third leg of decision 003's atomicity requirement, which spans the outbox, the dead-letter
/// store **and** quarantine. Case 9 covers the first two and says nothing about this one, so a
/// backend could implement quarantine as a separate table, move the row outside the transaction,
/// and pass every existing case.
///
/// Whichever shape a backend picks — a separate table or a state on the row — the transition has to
/// commit with everything around it. A separate table makes that a cross-store write, which on
/// IndexedDB means naming both object stores in one transaction, declared upfront: the fiddliest
/// place in that backend to get atomicity subtly wrong
/// (`wiki/decisions/025-quarantine-storage-shape.decision.md`).
pub async fn case_50_a_failed_apply_rolls_back_quarantine<F: FaultInjection>(
    factory: &F,
) -> Result<(), Error> {
    let (store, key) = open(factory).await?;
    store.enqueue(intent(1, 1)).await?;
    factory
        .insert_corrupt_row(&key, CorruptKind::UnparseableId)
        .await?;

    let before = QuarantineStore::count(&store).await?;
    factory.fail_next_apply_outcomes().await;

    let result = store
        .apply_outcomes(&[Outcome::new(
            id(1),
            Disposition::Quarantine {
                reason: "deliberate".to_string(),
            },
        )])
        .await;
    assert!(result.is_err(), "a failing apply must surface as an error");

    assert_eq!(
        QuarantineStore::count(&store).await?,
        before,
        "nothing may reach quarantine when the transition failed"
    );
    assert_eq!(
        store.pending_count().await?,
        1,
        "and the record must still be queued, not lost between the two"
    );
    Ok(())
}

/// A batch read looks past corruption instead of guessing how much of it there is.
///
/// # The divergence this exists to catch
///
/// A backend that filters undecodable rows *after* asking storage for a bounded window has to
/// decide how wide to make the window. SQLite asked for `limit * 4` and hoped; the in-memory and
/// IndexedDB backends read the whole scope and truncated afterwards, so they were never exposed.
/// With more than `3 * limit` corrupt rows ahead of the good ones, every row in SQLite's window was
/// discarded and `pending_batch` returned **nothing** while `pending_count` correctly reported work
/// outstanding — a store insisting it holds records and refusing to name one.
///
/// No existing case reached it, because none seeds more corruption than the multiplier absorbed.
/// The number here is deliberately larger than any plausible fixed multiplier: the point is not
/// that four was too small, it is that **there is no correct constant**, and a backend that answers
/// this case with a bigger one has only moved the failure further out.
///
/// `SyncRunner` masks it — a pass sweeps before it reads, so the corrupt rows are in quarantine by
/// the time the batch is taken. Direct callers do not sweep, and `pending_batch` is public.
pub async fn case_69_a_batch_is_not_shortened_by_corruption_ahead_of_it<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, key) = open(factory).await?;
    for i in 0..20u128 {
        factory
            .insert_corrupt_row(&key, CorruptKind::InvalidBody { id: id(100 + i) })
            .await?;
    }
    store.enqueue(intent(1, 1)).await?;
    store.enqueue(intent(2, 2)).await?;

    assert_eq!(
        store.pending_count().await?,
        2,
        "the count sees past the corruption"
    );
    let batch = store.pending_batch(2).await?;
    assert_eq!(
        batch.len(),
        2,
        "and so must the batch — a store that reports work must be able to name it"
    );
    assert_eq!(batch[0].mutation_id, id(1));
    assert_eq!(batch[1].mutation_id, id(2));

    // The window has to keep pace with the request, not just clear a fixed hurdle once.
    let single = store.pending_batch(1).await?;
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].mutation_id, id(1));
    Ok(())
}
