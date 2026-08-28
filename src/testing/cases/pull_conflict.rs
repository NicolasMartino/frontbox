//! Cases 41-43: what refetching would cost, and why it is reported rather than enforced.

use super::prelude::*;

/// Open an outbox and a version store on one scope.
async fn open_both<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(F::Store, F::Versions, ScopeKey), Error> {
    let key = scope("user:alice@tenant:acme@schema:1");
    let outbox = factory.open(key.clone()).await?;
    let versions = factory.open_versions(key.clone()).await?;
    Ok((outbox, versions, key))
}

fn cache<V: CacheVersionStore>(store: V) -> InvalidationRunner<V, SliceRegistry<&'static str>> {
    InvalidationRunner::new(store, registry())
}

/// Asking what is stale also answers what refetching it would cost.
///
/// The source's listener refetches eagerly and never consults the outbox, so nothing in its call
/// path *could* have decided otherwise — and its refetch is a `DELETE` plus re-insert, which
/// destroys the optimistic projection of a queued mutation outright. Reporting the conflict at the
/// moment staleness is read means an application has to actively ignore it to reproduce that.
pub async fn case_41_staleness_reports_pending_conflict<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (outbox, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    runner
        .apply(&[InvalidationEvent::new("exercises", "2")])
        .await?;

    // Nothing queued: replacing local state discards nothing.
    let clean = runner.stale(&outbox).await?;
    assert_eq!(clean.len(), 1);
    assert_eq!(clean[0].key, "exercises");
    assert_eq!(clean[0].conflict, PendingConflict::None);
    assert!(!clean[0].conflict.is_possible());

    // Queue a write, and the same question now carries the cost.
    outbox.enqueue(intent(1, 1)).await?;
    let dirty = runner.stale(&outbox).await?;
    assert_eq!(
        dirty[0].conflict,
        PendingConflict::Unattributed { pending: 1 },
        "without a classifier core cannot say which entity, only that something is queued"
    );
    assert!(dirty[0].conflict.is_possible());
    Ok(())
}

/// A classifier narrows the conflict to the entity it actually affects.
///
/// Core never reads the record itself — the body is uninterpreted JSON by decision 008 — so the
/// caller supplies the attribution and core only counts. An unrelated queued write should not make
/// an unrelated entity look unsafe to refresh.
pub async fn case_42_a_classifier_narrows_the_conflict<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (outbox, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    runner
        .apply(&[
            InvalidationEvent::new("exercises", "1"),
            InvalidationEvent::new("sessions", "1"),
        ])
        .await?;

    // One queued write, for sessions only. `intent` builds `/api/v1/things/{n}`, so the classifier
    // keys off the id to stand in for an application reading its own routes.
    outbox.enqueue(intent(1, 1)).await?;
    let classify = |record: &OutboxRecord| -> Option<&'static str> {
        if record.mutation_id == id(1) {
            Some("sessions")
        } else {
            None
        }
    };

    let stale = runner.stale_classified(&outbox, classify).await?;
    let for_key = |name: &str| {
        stale
            .iter()
            .find(|entry| entry.key == name)
            .expect("stale")
            .conflict
    };

    assert_eq!(
        for_key("sessions"),
        PendingConflict::ForEntity { pending: 1 }
    );
    assert_eq!(
        for_key("exercises"),
        PendingConflict::None,
        "an unrelated queued write must not make this entity look unsafe"
    );

    // A scan too small to see the whole queue refuses to claim anything is clean, because a false
    // "nothing is queued" is the answer that loses data.
    outbox.enqueue(intent(2, 2)).await?;
    let narrow = InvalidationRunner::new(
        factory
            .open_versions(scope("user:alice@tenant:acme@schema:1"))
            .await?,
        registry(),
    )
    .with_conflict_scan(1);
    let partial = narrow.stale_classified(&outbox, classify).await?;
    assert!(
        partial
            .iter()
            .all(|e| matches!(e.conflict, PendingConflict::Unattributed { .. })),
        "an incomplete scan degrades to unattributed rather than reporting a clean partial view"
    );
    Ok(())
}

/// A permanently stuck outbox record does not suppress staleness reporting.
///
/// This is the liveness half of decision 014. Under a hard gate, one record the server never
/// resolves would freeze every entity's cache forever — the data equivalent of a queue that never
/// drains, which decision 005 spent its whole argument avoiding. Core reports the conflict and
/// leaves the choice with the application, so the stale list keeps working.
pub async fn case_43_a_stuck_record_does_not_suppress_staleness<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (outbox, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    outbox.enqueue(intent(1, 1)).await?;

    // A verdict this crate cannot act on: retained forever, by decision 012.
    let sync = SyncRunner::new(
        factory
            .open(scope("user:alice@tenant:acme@schema:1"))
            .await?,
        ScriptedTransport::new(Reply::All(MutationStatus::Unknown("Throttled".into()))),
    );
    let report = sync.sync_once().await?;
    assert_eq!(report.counts.unknown_status, 1);
    assert_eq!(
        outbox.pending_count().await?,
        1,
        "still queued, indefinitely"
    );

    runner
        .apply(&[InvalidationEvent::new("exercises", "5")])
        .await?;

    let stale = runner.stale(&outbox).await?;
    assert_eq!(
        stale.len(),
        1,
        "a stuck record must not hide what needs refreshing"
    );
    assert_eq!(stale[0].key, "exercises");
    assert_eq!(
        stale[0].conflict,
        PendingConflict::Unattributed { pending: 1 }
    );
    Ok(())
}
