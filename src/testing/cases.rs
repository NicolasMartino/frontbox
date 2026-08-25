//! The conformance cases.
//!
//! Each function is a single behavioural assertion that every backend must satisfy. They are
//! public and individually callable so a backend can run each as its own test — a wasm backend, for
//! example, needs one `#[wasm_bindgen_test]` per case rather than one giant one.
//!
//! Several cases exist to prove a *decision* rather than to guard a line of code. Those are called
//! out in their own doc comments, because a failure there means the design is wrong, not that a
//! refactor slipped.

use crate::error::Error;
use crate::protocol::{MutationResult, MutationStatus};
use crate::record::{MutationIntent, OperationMeta};
use crate::runner::{SyncPass, SyncRunner};
use crate::scope::ScopeKey;
use crate::store::{DeadLetterStore, Disposition, OutboxStore, Outcome, QuarantineStore};

use super::{
    full_rejection, id, intent, scope, CorruptKind, FaultInjection, Reply, ScriptedTransport,
    StoreFactory,
};

/// Open a store on a fresh default scope.
async fn open<F: StoreFactory>(factory: &F) -> Result<(F::Store, ScopeKey), Error> {
    let key = scope("user:alice@tenant:acme@schema:1");
    let store = factory.open(key.clone()).await?;
    Ok((store, key))
}

/// Enqueue `n` intents with `created_at` running 1..=n.
async fn seed<S: OutboxStore>(store: &S, n: u128) -> Result<(), Error> {
    for i in 1..=n {
        store.enqueue(intent(i, i as i64)).await?;
    }
    Ok(())
}

fn runner<S: OutboxStore>(store: S, reply: Reply) -> SyncRunner<S, ScriptedTransport> {
    SyncRunner::new(store, ScriptedTransport::new(reply))
}

// ---------------------------------------------------------------------------
// Status application
// ---------------------------------------------------------------------------

/// `Applied` removes the record from the outbox.
pub async fn case_01_applied_is_deleted<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Applied));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.counts.applied, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Duplicate` removes the record too: the server already has it, so replaying is pointless.
pub async fn case_02_duplicate_is_deleted<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Duplicate));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.duplicate, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Rejected` is the only status that produces a dead letter.
///
/// The record leaves the outbox and is preserved for inspection rather than discarded. Every peer
/// system surveyed discards or rolls back the terminal case; keeping it is deliberate.
pub async fn case_03_rejected_becomes_a_dead_letter<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 1);

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].mutation_id, id(1));
    Ok(())
}

/// `Blocked` stays queued.
///
/// This diverges from the source, which dead-letters `Blocked` alongside `Rejected`. A blocked
/// mutation was skipped because an earlier ordered mutation failed terminally — it was never
/// evaluated on its own merits, so dead-lettering it discards valid work for the sole reason that
/// it followed a failed record.
pub async fn case_04_blocked_stays_queued<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Blocked));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.blocked, 1);
    assert_eq!(report.retained, 1);
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// `Pending` stays queued: the server accepted it but has not finished.
pub async fn case_05_pending_stays_queued<F: StoreFactory>(factory: &F) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(store, Reply::All(MutationStatus::Pending));
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.pending, 1);
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// One batch carrying all five statuses applies each correctly, in one atomic call.
pub async fn case_06_mixed_batch_applies_every_status<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 5).await?;

    let runner = runner(
        store,
        Reply::ByPosition(vec![
            MutationStatus::Applied,
            MutationStatus::Duplicate,
            MutationStatus::Rejected,
            MutationStatus::Blocked,
            MutationStatus::Pending,
        ]),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.counts.applied, 1);
    assert_eq!(report.counts.duplicate, 1);
    assert_eq!(report.counts.dead_lettered, 1);
    assert_eq!(report.counts.blocked, 1);
    assert_eq!(report.counts.pending, 1);

    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 1);

    let remaining = runner.store().pending_batch(10).await?;
    let remaining_ids: Vec<_> = remaining.iter().map(|r| r.mutation_id).collect();
    assert_eq!(remaining_ids, vec![id(4), id(5)]);
    Ok(())
}

// ---------------------------------------------------------------------------
// Transport classification
// ---------------------------------------------------------------------------

/// Offline leaves pending work untouched and is not an error.
///
/// Being offline is the expected operating mode for an offline-first queue. Collapsing it into a
/// generic transport failure would produce misleading status and drive an error-backoff path that
/// should not run.
pub async fn case_07_offline_leaves_work_untouched<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::Offline);
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(report.sent, 2);
    assert_eq!(report.retained, 2);
    assert!(!report.made_progress());
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// An attempted request that failed is an error, and still leaves pending work untouched.
pub async fn case_08_transport_failure_is_an_attempted_send<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::TransportFailure);
    let result = runner.sync_once().await;

    match result {
        Err(Error::Transport { .. }) => {}
        other => panic!("expected a transport failure, got {other:?}"),
    }
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

/// A failing `apply_outcomes` leaves no partial state.
///
/// The source inserts the dead letter and deletes the outbox row in two disconnected steps,
/// discarding the second's result. When the delete fails, the record is both pending and
/// dead-lettered: it is re-sent on every later sync, the dead letter is rewritten, and
/// `rejected_at` keeps moving forward so retention never purges it.
pub async fn case_09_failed_apply_leaves_no_partial_state<F: FaultInjection>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    factory.fail_next_apply_outcomes().await;

    let runner = runner(
        store,
        Reply::ByPosition(vec![MutationStatus::Applied, MutationStatus::Rejected]),
    );
    let result = runner.sync_once().await;
    assert!(result.is_err(), "a failing apply must surface as an error");

    // Neither half of the transition happened: nothing was deleted, nothing was dead-lettered.
    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Batching and ordering
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Dead letters
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Corrupt records
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Protocol anomalies
// ---------------------------------------------------------------------------

/// A verdict for a mutation that was never sent changes nothing.
///
/// Acting on it would mean guessing which record the server meant, and guessing wrong mutates an
/// unrelated mutation. It is reported instead.
pub async fn case_16_unknown_result_is_an_anomaly<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = runner(
        store,
        Reply::Exact(vec![MutationResult::new(id(99), MutationStatus::Applied)]),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.anomalies, vec![id(99)]);
    assert_eq!(report.counts.applied, 0);
    assert_eq!(
        runner.store().pending_count().await?,
        1,
        "the unrelated record must be untouched"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

/// `Blocked` clears on the sync after its blocker is dead-lettered.
///
/// This is the liveness argument for retaining `Blocked` rather than dead-lettering it, and it is
/// the reason retention is safe here. `Blocked` is emitted only after an earlier mutation failed
/// *terminally*; the only terminal status is `Rejected`; and `Rejected` is dead-lettered and removed
/// in the same atomic call. So the blocker is gone before the next batch is built and the blocked
/// record is evaluated on its own merits, with no user action.
///
/// An earlier draft of the decision claimed the opposite — that `Blocked` would repeat until a user
/// intervened. This case is what settles it.
pub async fn case_19_blocked_clears_on_the_next_sync<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let transport =
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).then(Reply::ByPosition(vec![
            MutationStatus::Rejected,
            MutationStatus::Blocked,
        ]));
    let runner = SyncRunner::new(store, transport);

    let first = runner.sync_once().await?;
    assert_eq!(first.counts.dead_lettered, 1);
    assert_eq!(first.counts.blocked, 1);
    assert_eq!(runner.store().pending_count().await?, 1);

    // The blocker is gone, so nothing has to be resolved by hand.
    let second = runner.sync_once().await?;
    assert_eq!(second.counts.applied, 1);
    assert!(second.made_progress());
    assert_eq!(runner.store().pending_count().await?, 0);
    Ok(())
}

/// A sync that drains nothing says so.
///
/// Retention has no bound here — nothing ages, counts, or escalates a record retained repeatedly —
/// so the minimum obligation is that a stalled queue is distinguishable from an idle one.
pub async fn case_20_fully_retained_batch_is_no_progress<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(store, Reply::All(MutationStatus::Pending));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert!(!report.made_progress());
    assert!(report.is_stalled());
    assert_eq!(report.retained, 2);
    assert_eq!(runner.store().pending_count().await?, 2);

    // An empty queue is idle, not stalled. The two must not look the same to a caller.
    let (empty, _) = open(factory).await?;
    empty
        .apply_outcomes(&[
            Outcome::new(id(1), Disposition::Delete),
            Outcome::new(id(2), Disposition::Delete),
        ])
        .await?;
    let idle_runner = SyncRunner::new(
        empty,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)),
    );
    let idle = idle_runner.sync_once().await?;
    assert_eq!(idle.pass, SyncPass::Idle);
    assert!(!idle.is_stalled());
    Ok(())
}

/// A `Pending` prefix as long as the batch limit starves the tail, observably.
///
/// This is head-of-line blocking, not deadlock: it resolves when the server settles those jobs.
/// Documenting it as a test is the point — the behaviour is real, and pretending it cannot happen
/// would be worse than surfacing it.
pub async fn case_21_pending_prefix_starves_the_tail_observably<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 3).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Pending)),
    )
    .with_batch_limit(2);
    let report = runner.sync_once().await?;

    assert_eq!(report.sent, 2);
    assert!(report.is_stalled());

    let sent = runner.transport().sent();
    let sent_ids: Vec<_> = sent[0].mutations.iter().map(|m| m.mutation_id).collect();
    assert_eq!(sent_ids, vec![id(1), id(2)]);
    assert!(
        !sent_ids.contains(&id(3)),
        "the tail never reaches the server while the prefix stays pending"
    );
    assert_eq!(runner.store().pending_count().await?, 3);
    Ok(())
}

// ---------------------------------------------------------------------------
// Scope isolation
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

/// Operation metadata survives every transition, and core never reads it.
pub async fn case_24_operation_meta_survives_every_transition<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let op = OperationMeta::new("start_session").with_version("1.2.0");

    // Into a dead letter, on a server refusal.
    let (store, _) = open(factory).await?;
    store.enqueue(intent(1, 1).with_op(op.clone())).await?;

    let runner = runner(store, Reply::All(MutationStatus::Rejected));
    runner.sync_once().await?;

    let dead = DeadLetterStore::list(runner.store(), 10).await?;
    assert_eq!(dead[0].op.as_ref(), Some(&op));

    // And into quarantine, on a local integrity failure.
    let (other, _) = open(factory).await?;
    other.enqueue(intent(2, 2).with_op(op.clone())).await?;
    other
        .apply_outcomes(&[Outcome::new(
            id(2),
            Disposition::Quarantine {
                reason: "manufactured".to_string(),
            },
        )])
        .await?;

    let quarantined = QuarantineStore::list(&other, 10).await?;
    let carried = quarantined
        .iter()
        .find(|r| r.mutation_id == Some(id(2)))
        .expect("quarantined record");
    assert_eq!(carried.op.as_ref(), Some(&op));
    Ok(())
}

/// A body that is not valid JSON cannot enter the outbox at all.
///
/// Not "is rejected at enqueue" — there is no code path to reject, because
/// [`MutationIntent`] holds a parsed [`serde_json::Value`] and a malformed body is not a value that
/// exists. The source stores pre-serialized text, so a bad body is only discovered at replay, after
/// a reconnect, as a corrupt-record case.
///
/// This narrows the corrupt-record surface; it does not remove it. Durable corruption still happens
/// after a successful write, which is what the sweep is for.
pub async fn case_25_invalid_json_cannot_enter_the_outbox<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    let parsed: Result<serde_json::Value, _> = serde_json::from_str("{ not json");
    assert!(parsed.is_err(), "the malformed body must fail to parse");

    // The failure happens before an intent exists, so nothing reached storage.
    assert_eq!(store.pending_count().await?, 0);

    // The well-formed path is the only one available.
    store
        .enqueue(MutationIntent::new(
            id(1),
            "POST",
            "/api/v1/things",
            parsed.unwrap_or(serde_json::json!({ "ok": true })),
            1,
        ))
        .await?;
    assert_eq!(store.pending_count().await?, 1);
    Ok(())
}

/// The wire payload is exactly the shape the existing server already accepts.
///
/// Field-for-field: an intent with no operation metadata serializes to the five keys the source
/// protocol defines, with the client timestamp as an RFC 3339 instant. A server that rejects
/// unknown keys must see nothing new until a caller opts into `op`.
pub async fn case_27_wire_payload_matches_the_source_protocol<F: StoreFactory>(
    _factory: &F,
) -> Result<(), Error> {
    let bare = MutationIntent::new(
        id(1),
        "PATCH",
        "/api/v1/exercises/123",
        serde_json::json!({ "name": "Bench Press" }),
        1_700_000_000_123,
    );

    let value = serde_json::to_value(&bare).map_err(Error::serialization)?;
    let object = value.as_object().expect("intent serializes to an object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["body", "client_datetime", "method", "mutation_id", "path"],
        "an intent without operation metadata must carry no extra keys"
    );
    assert_eq!(
        object["client_datetime"],
        serde_json::json!("2023-11-14T22:13:20.123Z")
    );

    // And it round-trips.
    let back: MutationIntent = serde_json::from_value(value).map_err(Error::serialization)?;
    assert_eq!(back, bare);

    // Opting in adds the key, and only then.
    let labelled = bare.with_op(OperationMeta::new("rename_exercise"));
    let value = serde_json::to_value(&labelled).map_err(Error::serialization)?;
    assert!(value.as_object().expect("object").contains_key("op"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Re-entrancy
// ---------------------------------------------------------------------------

/// A sync entered while one is already in flight does nothing.
///
/// Without the guard, a periodic loop overlapping a manual trigger sends the same batch twice.
/// Idempotency on the server would absorb it, but only because `mutation_id` is the idempotency
/// key — the client should not be leaning on that for an avoidable double send.
pub async fn case_28_reentrant_sync_does_not_double_send<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).yielding(),
    );

    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);

    let mut first = Box::pin(runner.sync_once());
    assert!(
        first.as_mut().poll(&mut cx).is_pending(),
        "the transport must suspend so a second pass can be attempted mid-flight"
    );

    let second = Box::pin(runner.sync_once()).as_mut().poll(&mut cx);
    match second {
        Poll::Ready(Ok(report)) => assert_eq!(report.pass, SyncPass::AlreadyRunning),
        other => panic!("a re-entered sync must return immediately, got {other:?}"),
    }

    loop {
        if let Poll::Ready(result) = first.as_mut().poll(&mut cx) {
            result?;
            break;
        }
    }

    assert_eq!(runner.transport().send_count(), 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    Ok(())
}
