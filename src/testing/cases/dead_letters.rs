//! Cases 13, 14, 17, 56, 68: dead-letter timestamps, retention, payloads, why one exists, and
//! the order the two terminal stores hand records back in.

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
        .apply_outcomes(&[Outcome::new(
            id(1),
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("test fixture".into()),
            },
        )])
        .await?;

    clock.set(2_000);
    store.enqueue(intent(2, 2)).await?;
    store
        .apply_outcomes(&[Outcome::new(
            id(2),
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("test fixture".into()),
            },
        )])
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
    assert_eq!(
        dead[0].rejection(),
        Some(&full_rejection()),
        "the payload survives the transition"
    );
    assert_eq!(
        dead[0].reason,
        DeadLetterReason::Rejected {
            error: Some(full_rejection())
        },
        "and the reason names a server refusal rather than leaving it to be inferred"
    );
    Ok(())
}

/// The three ways a record gets parked are distinguishable from the record alone.
///
/// **The case the old encoding could not pass.** `error: Option<RemoteRejection>` answered two
/// questions with one field — *did a server refuse this* and *did it explain itself* — and loaded
/// the first onto the absence of an answer to the second. That worked while a server verdict and a
/// retention bound were the only two ways a record could be parked.
///
/// They never were. `apply_outcomes` is public, so an application has always been able to park a
/// record for a reason of its own, and it produced the same `None` as the bound. `attempts` narrows
/// that and does not settle it: a caller-driven dead letter can carry any count, including one
/// equal to the bound (`wiki/decisions/027-dead-letter-reason.decision.md`).
///
/// The three here are deliberately arranged so that no other field separates them: the two
/// unexplained ones both have an absent rejection, and the caller-driven one is parked at an
/// attempt count that matches the bound the other reached.
pub async fn case_56_the_three_parking_reasons_are_distinguishable<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    // A server refusal that explains itself, and one that does not.
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;
    let refusing = runner(
        store,
        Reply::Exact(vec![
            MutationResult::new(id(1), MutationStatus::Rejected).with_error(full_rejection()),
            MutationResult::new(id(2), MutationStatus::Rejected),
        ]),
    );
    refusing.sync_once().await?;

    // The client giving up, at a bound of one so the count is 1.
    let store = factory.open(scope("user:bound@tenant:acme")).await?;
    seed(&store, 1).await?;
    let bounded = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_retention_bound(1);
    bounded.sync_once().await?;
    bounded.sync_once().await?;

    // The application parking one for its own reason, at the same attempt count.
    let store = factory.open(scope("user:caller@tenant:acme")).await?;
    seed(&store, 1).await?;
    let watched = runner(store, Reply::All(MutationStatus::Pending));
    watched.sync_once().await?;
    watched
        .store()
        .apply_outcomes(&[Outcome::new(
            id(1),
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("watchdog".into()),
            },
        )])
        .await?;

    let mut refused = DeadLetterStore::list(refusing.store(), 10).await?;
    refused.sort_by_key(|record| record.mutation_id);
    let by_bound = DeadLetterStore::list(bounded.store(), 10).await?;
    let by_caller = DeadLetterStore::list(watched.store(), 10).await?;

    assert_eq!(
        refused[0].reason,
        DeadLetterReason::Rejected {
            error: Some(full_rejection())
        }
    );
    assert_eq!(
        refused[1].reason,
        DeadLetterReason::Rejected { error: None },
        "a refusal with no payload is still a refusal, and says so"
    );
    assert_eq!(by_bound[0].reason, DeadLetterReason::RetentionBound);
    assert_eq!(
        by_caller[0].reason,
        DeadLetterReason::Caller("watchdog".into()),
        "core stores the application's own word and never reads it"
    );

    // Nothing else separates the last three: all have no rejection payload, and the two the old
    // encoding conflated carry the same attempt count.
    for record in [&refused[1], &by_bound[0], &by_caller[0]] {
        assert_eq!(record.rejection(), None);
    }
    assert_eq!(
        by_bound[0].attempts, by_caller[0].attempts,
        "the attempt count cannot be the discriminator"
    );
    Ok(())
}

/// The terminal stores order by timestamp, not by the order records happened to arrive.
///
/// # The divergence this exists to catch
///
/// [`DeadLetterStore::list`] and [`QuarantineStore::list`] used to take a `limit` and specify no
/// order, and three backends filled the silence three different ways: the in-memory store sorted by
/// timestamp, SQLite ordered by its insertion rowid, and IndexedDB returned whatever its scope index
/// produced. Every existing case passed on all three, because every existing case parks its records
/// in timestamp order and the three answers coincide there.
///
/// So this case makes them disagree. Two records are parked with the clock moved *backwards*
/// between them — which an injected [`ManualClock`](crate::ManualClock) makes trivial and a real
/// deployment makes possible, since nothing synchronises the clock behind a retention sweep with
/// the clock behind a server rejection.
///
/// The `list(1)` assertions are the ones that matter. Under a truncating limit an insertion-ordered
/// backend does not merely return the two records in a different sequence — it returns a **different
/// record**, and a caller triaging the queue sees a different failure depending on which backend it
/// happens to be running against.
pub async fn case_68_terminal_stores_are_ordered_by_time_not_arrival<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let clock = factory.clock();
    let (store, _) = open(factory).await?;
    seed(&store, 4).await?;

    // Parked second, but stamped first. An insertion-ordered backend puts this last.
    clock.set(2_000);
    store
        .apply_outcomes(&[Outcome::new(
            id(1),
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("later".into()),
            },
        )])
        .await?;
    clock.set(1_000);
    store
        .apply_outcomes(&[Outcome::new(
            id(2),
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("earlier".into()),
            },
        )])
        .await?;

    let all = DeadLetterStore::list(&store, 10).await?;
    assert_eq!(
        all.iter()
            .map(|record| record.rejected_at)
            .collect::<Vec<_>>(),
        vec![1_000, 2_000],
        "dead letters come back oldest first, whatever order they were written in"
    );
    let truncated = DeadLetterStore::list(&store, 1).await?;
    assert_eq!(
        truncated[0].mutation_id,
        id(2),
        "a truncating read must return the oldest, not the first written"
    );

    // The same rule on the other terminal store, which had the same silence in its contract.
    clock.set(4_000);
    store
        .apply_outcomes(&[Outcome::new(
            id(3),
            Disposition::Quarantine {
                reason: "later".into(),
            },
        )])
        .await?;
    clock.set(3_000);
    store
        .apply_outcomes(&[Outcome::new(
            id(4),
            Disposition::Quarantine {
                reason: "earlier".into(),
            },
        )])
        .await?;

    let all = QuarantineStore::list(&store, 10).await?;
    assert_eq!(
        all.iter()
            .map(|record| record.quarantined_at)
            .collect::<Vec<_>>(),
        vec![3_000, 4_000],
    );
    let truncated = QuarantineStore::list(&store, 1).await?;
    assert_eq!(truncated[0].mutation_id, Some(id(4)));
    Ok(())
}
