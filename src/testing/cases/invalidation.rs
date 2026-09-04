//! Cases 35-40 and 44: cache version identity, invalidation, and scoping.

use super::prelude::*;

/// Open an outbox and a version store on one scope.
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
            InvalidationEvent::new("exercises", "3"),
            InvalidationEvent::new("nonesuch", "9"),
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
        EntityState::stale_at("3")
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
            ("exercises".to_string(), CacheVersion::new("4")),
            ("retired_entity".to_string(), CacheVersion::new("2")),
            ("sessions".to_string(), CacheVersion::new("1")),
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
        Some(CacheVersion::new("4")),
        "an unknown name in the map must not abort the rest of it"
    );
    Ok(())
}

/// A server identity that differs from the local one is an update, whatever its magnitude.
///
/// D2 shipped this case as `case_37_a_server_behind_local_needs_reset`: a `u64` below the local one
/// meant the server's counter had restarted, and the client reset to zero rather than adopting the
/// smaller number. `wiki/decisions/021-cache-version-identity.decision.md` retired both the
/// ordering and the reset — a [`CacheVersion`] is compared by equality, so "below" is not a
/// relation this crate can observe and a differing identity is simply a differing identity.
///
/// The case is kept at its number, testing the behaviour that replaced it, because a numerically
/// smaller identity is exactly where the old rule and the new one disagree.
pub async fn case_37_a_differing_identity_is_an_update_not_a_reset<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    runner
        .apply(&[InvalidationEvent::new("exercises", "10")])
        .await?;
    runner.mark_fresh(&"exercises").await?;

    let report = runner
        .apply(&[InvalidationEvent::new("exercises", "5")])
        .await?;

    assert_eq!(
        report.marked_stale,
        vec!["exercises"],
        "a smaller identity is routine invalidation, not an anomaly"
    );

    let state = runner.store().state("exercises").await?;
    assert_eq!(
        state.version,
        Some(CacheVersion::new("5")),
        "the server's identity is adopted as given, not replaced with a sentinel"
    );
    assert!(state.stale, "and a refetch is owed");

    // Non-numeric identities are the point of the type, and behave identically.
    runner
        .apply(&[InvalidationEvent::new("exercises", "a3f8")])
        .await?;
    assert_eq!(
        runner.store().state("exercises").await?.version,
        Some(CacheVersion::new("a3f8"))
    );
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
        .apply(&[InvalidationEvent::new("sessions", "6")])
        .await?;
    assert_eq!(
        runner.store().state("sessions").await?,
        EntityState::stale_at("6")
    );

    runner.mark_fresh(&"sessions").await?;

    let state = runner.store().state("sessions").await?;
    assert_eq!(
        state.version,
        Some(CacheVersion::new("6")),
        "the version does not move on refetch"
    );
    assert!(!state.stale);

    // And a repeat of the same version is now genuinely nothing to do.
    let report = runner
        .apply(&[InvalidationEvent::new("sessions", "6")])
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
        .apply(&[InvalidationEvent::new("exercises", "8")])
        .await?;

    // Reopening is how a test says "the process died and came back" to a store with no process.
    let reopened = factory.open_versions(key).await?;
    let state = reopened.state("exercises").await?;

    assert_eq!(
        state.version,
        Some(CacheVersion::new("8")),
        "the version survived"
    );
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
        .apply(&[InvalidationEvent::new("exercises", "12")])
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

/// A zero-valued identity is a version; never having heard one is not.
///
/// The regression this whole amendment turns on. D2 stored the version as a `u64` with `0` meaning
/// "nothing known", which is sound for a counter and wrong for anything content-addressed: XOR's
/// identity element is zero, so a collection the server has legitimately emptied hashes to zero
/// too. Under the old model those two states were one value, `compare` answered `NoChange`, and a
/// client that had never synced would never refetch an empty collection — silently, forever
/// (`wiki/decisions/021-cache-version-identity.decision.md`).
pub async fn case_44_a_zero_identity_is_distinct_from_no_identity<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (_, versions, _) = open_both(factory).await?;
    let runner = cache(versions);

    assert_eq!(
        runner.store().state("exercises").await?,
        EntityState::unknown(),
        "nothing is known yet, which is not a version"
    );

    // The server says this collection is empty. That is news, not agreement.
    let report = runner
        .apply(&[InvalidationEvent::new("exercises", "0")])
        .await?;
    assert_eq!(
        report.marked_stale,
        vec!["exercises"],
        "an empty collection must still be fetched once"
    );
    assert!(report.unchanged.is_empty());
    assert_eq!(
        runner.store().state("exercises").await?,
        EntityState::stale_at("0")
    );

    // Having been told once, being told again is genuinely nothing to do.
    runner.mark_fresh(&"exercises").await?;
    let second = runner
        .apply(&[InvalidationEvent::new("exercises", "0")])
        .await?;
    assert_eq!(second.unchanged, vec!["exercises"]);
    assert!(
        !runner.store().state("exercises").await?.stale,
        "a repeat of a known identity must not re-dirty the entity"
    );
    Ok(())
}

/// **Stale with no version survives a reopen, and is not silently rewritten to fresh.**
///
/// The fourth combination of `EntityState`'s two fields, and the one no constructor covered until
/// D5's second backend needed to rebuild it from columns. `unknown` is `(None, false)`, `stale_at`
/// is `(Some, true)`, `fresh_at` is `(Some, false)`; `(None, true)` had nowhere to come from
/// outside this crate, so a durable backend reading a null version would have had to answer
/// `unknown()` and drop the staleness on the floor.
///
/// **It is reached by the most ordinary path there is.**
/// [`InvalidationRunner::mark_all_stale`](crate::cache::InvalidationRunner::mark_all_stale) flags
/// every registered entity, and for one no invalidation has ever named the read-modify-write is
/// `(None, false)` → `(None, true)`. A client that has just installed and hit an error is exactly
/// where that hammer gets used, so the state is common rather than exotic.
///
/// **What dropping it costs:** the entity reads back not-stale, [`stale`] omits it, and the refetch
/// the recovery was asking for never happens — quietly, and only after a restart, so it would
/// present as "the recovery button works until you close the app".
///
/// The reopen is the point. Held in memory the field is just a `bool`; the failure only exists in
/// the round trip through storage, which is why this is a conformance case rather than a unit test.
///
/// [`stale`]: crate::cache::InvalidationRunner::stale
pub async fn case_67_stale_with_no_version_survives_a_reopen<F: VersionStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let key = scope("user:erin@tenant:acme@schema:1");
    let first = factory.open_versions(key.clone()).await?;

    // Nothing has ever been heard about any of these — no `apply` ran first, deliberately.
    let runner = InvalidationRunner::new(first, registry());
    runner.mark_all_stale().await?;
    let before = runner.store().state("exercises").await?;
    assert_eq!(
        before,
        EntityState::from_parts(None, true),
        "stale, and with no version, because none was ever heard"
    );

    let reopened = factory.open_versions(key).await?;
    let after = reopened.state("exercises").await?;
    assert_eq!(
        after.version, None,
        "a null version must not come back as some sentinel"
    );
    assert!(
        after.stale,
        "and the staleness must survive — a backend that answers `unknown()` for a null version \
         silently cancels the refetch `mark_all_stale` was asking for"
    );

    // And it must be enumerable, not merely readable by name. `stale()` walks `all_states`, so a
    // backend that filtered null versions out of the enumeration would pass the assertion above
    // and still never refetch anything.
    let states = reopened.all_states().await?;
    assert!(
        states
            .iter()
            .any(|(entity, state)| entity == "exercises" && state.stale),
        "the versionless-but-stale entity has to appear in an enumeration too"
    );
    Ok(())
}
