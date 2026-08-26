//! Cases 35-43: cache versions, invalidation, and pull conflict.

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

/// An event naming an entity the registry does not model changes nothing, and says so.
///
/// The source gets the disposition right and the signal wrong: it logs and drops. A log line is not
/// something an application can branch on, so a client whose registry has fallen behind the server
/// cannot tell that from a quiet period.
///
/// Ignoring is still correct. The registry defines what this application models, so a name outside
/// it names data the client does not hold — there is nothing to mark stale and nothing to refetch.
/// What the report adds is the ability to notice.
pub async fn case_35_an_unknown_entity_is_reported_not_applied<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    let report = runner
        .apply(&[
            InvalidationEvent::new("exercises", 3),
            InvalidationEvent::new("nonesuch", 9),
        ])
        .await?;

    assert_eq!(
        report.marked_stale,
        vec!["exercises"],
        "the known event still applies"
    );
    assert_eq!(
        report.unknown,
        vec!["nonesuch".to_string()],
        "the unknown one is reported in the server's own spelling"
    );

    // Nothing was recorded for it. An unmodelled entity has no local state to make stale.
    assert_eq!(
        runner.store().state("nonesuch").await?,
        EntityState::unknown()
    );
    assert_eq!(
        runner.store().state("exercises").await?,
        EntityState::stale_at(3)
    );
    Ok(())
}

/// Reconnect reconciliation reconciles what it understands and reports the rest.
///
/// This is the sharper half of case 35. The source's per-event path at least logs; its reconnect
/// path is a `filter_map` that drops unknown names from the server's version map silently, so a
/// drifted client reconciles against a truncated view and reports success.
pub async fn case_36_reconnect_reports_unknown_names<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    let report = runner
        .reconcile(&[
            ("exercises".to_string(), 4),
            ("retired_entity".to_string(), 2),
            ("sessions".to_string(), 1),
        ])
        .await?;

    let mut stale = report.marked_stale.clone();
    stale.sort_unstable();
    assert_eq!(
        stale,
        vec!["exercises", "sessions"],
        "everything the registry knows is still reconciled"
    );
    assert_eq!(report.unknown, vec!["retired_entity".to_string()]);
    assert_eq!(
        runner.store().state("exercises").await?.version,
        4,
        "an unknown name in the map must not abort the rest of it"
    );
    Ok(())
}

/// A server version below the local one resets to zero rather than adopting the smaller number.
///
/// The server being behind means its counter restarted — a restore, a re-provisioned user, a
/// migration. Adopting its number would leave the client unable to distinguish "I was reset to 3"
/// from "I legitimately reached 3" on the next comparison. Zero is the one value both ends can
/// agree on.
pub async fn case_37_a_server_behind_local_needs_reset<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    runner
        .apply(&[InvalidationEvent::new("exercises", 10)])
        .await?;
    runner.mark_fresh(&"exercises").await?;

    let report = runner
        .apply(&[InvalidationEvent::new("exercises", 5)])
        .await?;

    assert_eq!(report.needs_reset, vec!["exercises"]);
    assert!(
        report.marked_stale.is_empty(),
        "a reset is not routine invalidation and is reported separately"
    );

    let state = runner.store().state("exercises").await?;
    assert_eq!(
        state.version, 0,
        "reset to zero, not down to the server's 5"
    );
    assert!(state.stale, "a reset owes a refetch");
    Ok(())
}

/// `mark_fresh` clears staleness and leaves the version where it is.
///
/// The two fields move independently, which is the whole reason staleness has to be stored rather
/// than derived. After an invalidation the local version already equals the server's while the data
/// is still unfetched; only the flag remembers that.
pub async fn case_38_mark_fresh_clears_staleness_only<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    runner
        .apply(&[InvalidationEvent::new("sessions", 6)])
        .await?;
    assert_eq!(
        runner.store().state("sessions").await?,
        EntityState::stale_at(6)
    );

    runner.mark_fresh(&"sessions").await?;

    let state = runner.store().state("sessions").await?;
    assert_eq!(state.version, 6, "the version does not move on refetch");
    assert!(!state.stale);

    // And a repeat of the same version is now genuinely nothing to do.
    let report = runner
        .apply(&[InvalidationEvent::new("sessions", 6)])
        .await?;
    assert_eq!(report.unchanged, vec!["sessions"]);
    assert!(!runner.store().state("sessions").await?.stale);
    Ok(())
}

/// Staleness survives reopening the store.
///
/// The case a version-only implementation fails. Persist the version but not the flag and this
/// reopens with local equal to the server, compares `NoChange`, and reports fresh — serving data
/// the server explicitly invalidated, permanently, with nothing to detect it. Losing *both* would
/// be safe by comparison, which is why decision 015 makes the pair one unit rather than a
/// preference.
pub async fn case_39_staleness_survives_a_reopen<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let key = scope("user:dana@tenant:acme@schema:1");
    let first = factory.open_versions(key.clone()).await?;
    InvalidationRunner::new(first, registry())
        .apply(&[InvalidationEvent::new("exercises", 8)])
        .await?;

    // Reopening is how a test says "the process died and came back" to a store with no process.
    let reopened = factory.open_versions(key).await?;
    let state = reopened.state("exercises").await?;

    assert_eq!(state.version, 8, "the version survived");
    assert!(
        state.stale,
        "and so did the staleness — without it the client believes invalidated data is fresh"
    );
    Ok(())
}

/// Two scopes sharing one physical store cannot see each other's versions.
///
/// Cache versions are per-user server state. A store that leaked them across a user switch would
/// leave the new user believing the previous user's data is fresh — which is a data-disclosure
/// shape, not just a staleness bug.
pub async fn case_40_versions_are_scoped<F: VersionStoreFactory>(factory: &F) -> Result<(), Error> {
    let alice = factory
        .open_versions(scope("user:alice@tenant:acme@schema:1"))
        .await?;
    let bob = factory
        .open_versions(scope("user:bob@tenant:acme@schema:1"))
        .await?;

    InvalidationRunner::new(alice, registry())
        .apply(&[InvalidationEvent::new("exercises", 12)])
        .await?;

    assert_eq!(
        bob.state("exercises").await?,
        EntityState::unknown(),
        "another scope's version must not be readable"
    );
    assert!(
        bob.all_states().await?.is_empty(),
        "and must not appear in an enumeration either"
    );
    Ok(())
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
        .apply(&[InvalidationEvent::new("exercises", 2)])
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
            InvalidationEvent::new("exercises", 1),
            InvalidationEvent::new("sessions", 1),
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
        .apply(&[InvalidationEvent::new("exercises", 5)])
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
