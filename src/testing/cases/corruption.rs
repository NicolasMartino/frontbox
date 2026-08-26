//! Cases 15, 18, 26: rows that will not decode.

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
