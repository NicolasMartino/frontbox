//! Cases 75-80: what counts as a match, and the two facts that spend a record's eligibility.
//!
//! Case 78 is the one that decides whether the feature works at all in the scenario it was built
//! for. Cases 79 and 80 are its opposites — the two ways a record stops being replaceable, one per
//! writer of the `transport_started` mark. The rest bound the match rule, so a replacement cannot
//! reach a record the caller did not mean.

use super::prelude::*;
use crate::record::RowRef;
use crate::store::{CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal};

fn row() -> RowRef {
    RowRef::new("profile", "user-1")
}

fn edit(n: u128, body: &str, created_at: i64) -> MutationIntent {
    MutationIntent::new(
        id(n),
        "PUT",
        "/api/v1/profile/user-1",
        serde_json::json!({ "name": body }),
        created_at,
    )
    .with_row(row())
}

/// A different row, method, or path is a different write.
///
/// The match key is all three together. A store matching on the row alone would coalesce a `PUT` of
/// the profile into a `DELETE` of it; one matching on the path alone would coalesce across rows
/// whose paths happen to collide under a caller's routing.
pub async fn case_75_matching_needs_row_method_and_path<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    // Same row and path, different method.
    let other_method = MutationIntent::new(
        id(2),
        "PATCH",
        "/api/v1/profile/user-1",
        serde_json::json!({ "name": "patched" }),
        200,
    )
    .with_row(row());

    // Same row and method, different path.
    let other_path = MutationIntent::new(
        id(3),
        "PUT",
        "/api/v1/profile/user-1/avatar",
        serde_json::json!({ "name": "avatar" }),
        300,
    )
    .with_row(row());

    // Same method and path, different row.
    let other_row = MutationIntent::new(
        id(4),
        "PUT",
        "/api/v1/profile/user-1",
        serde_json::json!({ "name": "someone else" }),
        400,
    )
    .with_row(RowRef::new("profile", "user-2"));

    for intent in [other_method, other_path, other_row] {
        let mutation_id = intent.mutation_id;
        assert_eq!(
            store
                .enqueue_coalescing(intent, CoalescingPolicy::RequireExisting)
                .await?,
            CoalescingEnqueue::NotQueued {
                mutation_id,
                reason: CoalescingRefusal::MissingMatch,
            },
            "a differing key must not match"
        );
    }

    assert_eq!(store.pending_count().await?, 1);
    assert_eq!(
        store.pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "first" }),
        "the queued body was never touched"
    );
    Ok(())
}

/// Another scope's pending record is not a match.
///
/// Scope enforcement is on every read for the reason decision 009 gives, and a coalescing match is a
/// read. A store that matched across scopes would let one principal rewrite another's queued body,
/// which is the *authorized wrong write* the scope stamp exists to prevent.
pub async fn case_76_coalescing_cannot_reach_another_scope<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let mine = scope("user:coalesce-a@tenant:acme");
    let theirs = scope("user:coalesce-b@tenant:acme");
    let ours = factory.open(mine).await?;
    let other = factory.open(theirs).await?;

    other.enqueue(edit(1, "theirs", 100)).await?;

    assert_eq!(
        ours.enqueue_coalescing(edit(2, "mine", 200), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::MissingMatch,
        }
    );

    assert_eq!(other.pending_count().await?, 1);
    assert_eq!(
        other.pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "theirs" }),
        "the other scope's body is untouched"
    );
    assert_eq!(ours.pending_count().await?, 0);
    Ok(())
}

/// A replaced record drains once, carrying the newer body.
///
/// The end of the story: coalescing is only worth anything if the surviving record behaves like any
/// other queued write. One send, one verdict, one deletion — and the payload the server sees is the
/// second edit's.
pub async fn case_77_a_replaced_record_drains_once_with_the_newer_body<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store
        .enqueue(edit(1, "first", 100).with_precondition("W/\"server-v1\""))
        .await?;
    store
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;

    let runner = runner(store, Reply::All(MutationStatus::Applied));
    let report = runner.sync_once().await?;

    assert_eq!(report.sent, 1, "one record, not two");
    assert_eq!(report.counts.applied, 1);
    assert_eq!(runner.store().pending_count().await?, 0);

    let sent = runner.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].mutations.len(), 1);
    let mutation = &sent[0].mutations[0];
    assert_eq!(mutation.mutation_id, id(1), "the surviving identifier");
    assert_eq!(mutation.body, serde_json::json!({ "name": "second" }));
    assert_eq!(
        mutation.precondition.as_deref(),
        Some("W/\"server-v1\""),
        "the guard still names the last state the server confirmed"
    );
    Ok(())
}

/// A pass that knows it is offline reads nothing, so the queue stays coalescible.
///
/// **The case the feature depends on.** `read_for_send` marks what it returns and never clears the
/// mark, so without this probe a cadence loop polling while offline would spend the head batch's
/// eligibility on its first tick — and coalescing exists precisely to collapse edits made while
/// offline.
///
/// Distinct from `case_07`, which is offline discovered *during* a send. That one has already handed
/// the batch over and must mark; this one never reads.
pub async fn case_78_an_offline_probe_reads_nothing_and_preserves_coalescing<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store
        .enqueue(edit(1, "first", 100).with_precondition("W/\"server-v1\""))
        .await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).reporting_offline(),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(report.sent, 0, "nothing was read, so nothing was sent");
    assert_eq!(runner.transport().send_count(), 0);
    assert_eq!(runner.transport().probe_count(), 1);
    assert_eq!(runner.store().pending_count().await?, 1);

    // The point: the record is still coalescible, because the pass never marked it.
    let outcome = runner
        .store()
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;
    assert_eq!(
        outcome,
        CoalescingEnqueue::Replaced {
            kept: id(1),
            discarded: id(2),
        }
    );
    assert_eq!(
        runner.store().pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "second" })
    );
    Ok(())
}

/// Offline discovered during a send still marks the batch, so a later edit cannot rewrite it.
///
/// The counterpart to case 78 and the reason the mark is written before the request rather than
/// derived from its outcome. `Error::Offline` is documented to cover a browser `fetch` that failed
/// for lack of connectivity — indistinguishable from one the server received and answered into a
/// lost response. Treating it as proof that nothing was sent would let the next edit rewrite a body
/// the server may already hold, which it would then dedupe away and never apply.
pub async fn case_79_offline_during_a_send_still_spends_coalescibility<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    let runner = runner(store, Reply::Offline);
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Offline);
    assert_eq!(report.sent, 1, "the batch was read and handed over");
    assert_eq!(runner.transport().send_count(), 1);
    assert_eq!(runner.store().pending_count().await?, 1);

    let outcome = runner
        .store()
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;
    assert_eq!(
        outcome,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::TransportStarted,
        },
        "a request that was attempted may have been seen"
    );
    assert_eq!(
        runner.store().pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "first" })
    );
    Ok(())
}

/// A verdict received through `apply_outcomes` spends coalescibility, whoever applied it.
///
/// **`apply_outcomes` is public, and `OutboxStore` says a backend "cannot assume the runner is its
/// only caller".** A direct caller can read with `pending_batch`, send the batch itself, and apply a
/// `Retain` — at which point the server has certainly seen the record, because a `Retain` *is* a
/// verdict, and a verdict cannot exist without a request.
///
/// If only `read_for_send` set the mark, that record would still read as coalescible and the next
/// edit would rewrite its body under an identifier the server already holds. The store would then
/// dedupe the resend and delete the newer body without ever applying it.
///
/// So `Retain` sets the mark too. It is the only disposition that needs to: every other one removes
/// the record from the outbox.
pub async fn case_80_a_retained_verdict_spends_coalescibility<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    // The direct-caller path, deliberately not the runner's: read without marking, then apply a
    // verdict as if this caller had done its own sending.
    let pending = store.pending_batch(10).await?;
    assert_eq!(pending.len(), 1);
    store
        .apply_outcomes(&[Outcome::new(
            pending[0].mutation_id,
            Disposition::Retain {
                reason: Some("server said try again".to_owned()),
            },
        )])
        .await?;

    let after = store.pending_batch(10).await?;
    assert_eq!(after[0].attempts, 1, "the verdict was recorded");

    let outcome = store
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;
    assert_eq!(
        outcome,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::TransportStarted,
        },
        "a record that has received a verdict has certainly been sent"
    );
    assert_eq!(
        store.pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "first" }),
        "and its body was not rewritten"
    );
    Ok(())
}
