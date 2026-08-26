//! Cases 22, 23, 29: required scope keys, and no normalization of them.

use super::prelude::*;

/// Two scopes cannot observe each other's records, and a mismatch retains rather than discards.
///
/// This is the case that matters most for safety. The source ships a scoped *and* an unscoped
/// constructor on both of its backends, so its isolation holds only where every call site
/// remembered to pick the right one. Log out with pending mutations, log in as someone else, and
/// any code path that opened the shared database replays the first user's writes under the second
/// user's token.
///
/// Fresh per-send authentication does not help: a valid token for the second user applied to the
/// first user's record produces an *authorized* wrong write. Enforcement has to happen where
/// records are read.
///
/// The retain half matters equally. Discarding on mismatch would trade a leak for data loss, which
/// contradicts the premise that queued writes survive.
pub async fn case_22_scopes_cannot_observe_each_other<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let alice = scope("user:alice");
    let bob = scope("user:bob");

    let a = factory.open(alice.clone()).await?;
    a.enqueue(intent(1, 1)).await?;
    a.apply_outcomes(&[Outcome::new(id(1), Disposition::DeadLetter { error: None })])
        .await?;
    a.enqueue(intent(2, 2)).await?;

    let b = factory.open(bob.clone()).await?;
    assert_eq!(b.pending_count().await?, 0);
    assert!(b.pending_batch(10).await?.is_empty());
    assert_eq!(DeadLetterStore::count(&b).await?, 0);
    assert_eq!(QuarantineStore::count(&b).await?, 0);

    // Bob syncing must not touch Alice's work, and must not send it either.
    let b_runner = runner(b, Reply::All(MutationStatus::Applied));
    let report = b_runner.sync_once().await?;
    assert_eq!(report.pass, SyncPass::Idle);
    assert_eq!(report.sent, 0);

    // Reopening Alice's scope finds the work exactly as she left it, and it still syncs.
    let a_again = factory.open(alice).await?;
    assert_eq!(a_again.pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(&a_again).await?, 1);

    let a_runner = runner(a_again, Reply::All(MutationStatus::Applied));
    let report = a_runner.sync_once().await?;
    assert_eq!(report.counts.applied, 1);
    assert_eq!(a_runner.store().pending_count().await?, 0);
    Ok(())
}

/// An empty scope key is rejected at construction.
pub async fn case_23_empty_scope_key_is_rejected<F: StoreFactory>(
    _factory: &F,
) -> Result<(), Error> {
    match ScopeKey::new("") {
        Err(Error::InvalidScopeKey { .. }) => Ok(()),
        other => panic!("an empty scope key must be rejected, got {other:?}"),
    }
}

/// Scope keys are compared exactly, never normalized.
///
/// Any normalization is non-injective, and collapsing two distinct keys onto one identity is the
/// precise failure scoping exists to prevent. Backends face the same hazard when they turn a key
/// into a storage name: the source replaces characters, which maps `tenant/1` and `tenant_1` onto
/// the same database.
pub async fn case_29_scope_keys_are_not_normalized<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let plain = factory.open(scope("user:a")).await?;
    plain.enqueue(intent(1, 1)).await?;

    for variant in ["user:a ", " user:a", "User:A", "user:a\n"] {
        let other = factory.open(scope(variant)).await?;
        assert_eq!(
            other.pending_count().await?,
            0,
            "{variant:?} must not be treated as the same scope as \"user:a\""
        );
    }

    assert_eq!(plain.pending_count().await?, 1);
    Ok(())
}
