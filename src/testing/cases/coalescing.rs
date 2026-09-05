//! Cases 70-74: replacing a queued body in place, and what a replacement keeps.
//!
//! These pin `wiki/proposals/queued-write-coalescing.proposal.md`'s central claim: a queued write
//! may have its body replaced only while frontbox can still prove the transport has never seen it,
//! and a replacement keeps the queue slot and the guard while taking everything that describes the
//! body from the newer intent.

use super::prelude::*;
use crate::record::RowRef;
use crate::store::{CoalescingEnqueue, CoalescingPolicy, CoalescingRefusal};

/// The row every case here binds to.
fn row() -> RowRef {
    RowRef::new("profile", "user-1")
}

/// A row-bound `PUT` carrying `body`, so a case can say which edit it is looking at.
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

/// Two offline edits to one row leave one queued send carrying the later body.
///
/// The whole feature, in the shape RepForge asked for it. The precondition is the earlier edit's,
/// because that is the last state the server confirmed — a later one computed from local state
/// would compare the client against itself and detect nothing.
pub async fn case_70_a_second_edit_replaces_the_first_body<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    store
        .enqueue(edit(1, "first", 100).with_precondition("W/\"server-v1\""))
        .await?;

    let outcome = store
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;

    assert_eq!(
        outcome,
        CoalescingEnqueue::Replaced {
            kept: id(1),
            discarded: id(2),
        }
    );

    // One record, not two. That is the user-visible effect: one eventual send.
    assert_eq!(store.pending_count().await?, 1);
    let pending = store.pending_batch(10).await?;
    assert_eq!(pending.len(), 1);

    let record = &pending[0];
    assert_eq!(record.mutation_id, id(1), "the queued identifier survives");
    assert_eq!(
        record.precondition.as_deref(),
        Some("W/\"server-v1\""),
        "the guard is the first edit's, which is the point of the feature"
    );
    assert_eq!(
        record.body,
        serde_json::json!({ "name": "second" }),
        "the body is the second edit's"
    );
    Ok(())
}

/// A replacement keeps the slot and the guard, and takes everything describing the body.
///
/// The split is the rule: `seq`, `mutation_id`, and `precondition` belong to the queue; `body`,
/// `op`, `traceparent`, and `created_at` all describe content that is being replaced, so keeping the
/// older ones would leave the record asserting things about a body it no longer holds — an operation
/// name a human reads off a dead letter, a `version` documented as "what wrote the body", a trace
/// pointing at the discarded user action, and a `client_datetime` the server is told is when this
/// write happened.
pub async fn case_71_a_replacement_keeps_the_slot_and_takes_the_content<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    // An unrelated record first, so `seq` preservation has something to be preserved *relative to*.
    store.enqueue(intent(9, 1)).await?;
    store
        .enqueue(
            edit(1, "first", 100)
                .with_precondition("W/\"server-v1\"")
                .with_traceparent("00-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbb-01")
                .with_op(OperationMeta::new("save_profile").with_version("1.0.0")),
        )
        .await?;
    store.enqueue(intent(8, 2)).await?;

    let before = store.pending_batch(10).await?;
    let seq_before = before
        .iter()
        .find(|record| record.mutation_id == id(1))
        .expect("the first edit is queued")
        .order_key();

    store
        .enqueue_coalescing(
            edit(2, "second", 999)
                .with_precondition("W/\"guessed-v2\"")
                .with_traceparent("00-cccccccccccccccccccccccccccccccc-dddddddddddddddd-01")
                .with_op(OperationMeta::new("save_profile_again").with_version("2.0.0")),
            CoalescingPolicy::RequireExisting,
        )
        .await?;

    let pending = store.pending_batch(10).await?;
    let record = pending
        .iter()
        .find(|record| record.mutation_id == id(1))
        .expect("the replaced record keeps its identifier");

    // Kept: the queue slot and the guard.
    assert_eq!(
        record.order_key(),
        seq_before,
        "the queue position survives"
    );
    assert_eq!(record.precondition.as_deref(), Some("W/\"server-v1\""));

    // Taken: everything that describes the body.
    assert_eq!(record.body, serde_json::json!({ "name": "second" }));
    assert_eq!(record.created_at, 999);
    assert_eq!(
        record.traceparent.as_deref(),
        Some("00-cccccccccccccccccccccccccccccccc-dddddddddddddddd-01")
    );
    let op = record.op.as_ref().expect("the newer operation label");
    assert_eq!(op.name, "save_profile_again");
    assert_eq!(op.version.as_deref(), Some("2.0.0"));

    // A record that is not transport-started has received no verdict.
    assert_eq!(record.attempts, 0);
    assert_eq!(record.last_error, None);

    // The neighbours did not move.
    let order: Vec<_> = pending.iter().map(|record| record.mutation_id).collect();
    assert_eq!(order, vec![id(9), id(1), id(8)]);
    Ok(())
}

/// A record that has been read for sending is never coalescible again.
///
/// **The safety rule, through the first of its two paths.** `read_for_send` marks the batch before
/// the caller can send it, so once a record has been handed out its identifier may be one the server
/// already holds — and rewriting its body would hand the server new content under an id it dedupes
/// against, deleting the new body without ever applying it.
///
/// Case 80 is the other path: a `Retain` applied through `apply_outcomes`, which reaches the same
/// state without this method being called at all.
///
/// Deliberately *not* `attempts == 0`: nothing here ever reached a server, so `attempts` is still
/// zero and a rule written against it would have permitted the rewrite.
pub async fn case_72_a_record_read_for_sending_is_not_coalescible<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    let read = store.read_for_send(10).await?;
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].attempts, 0, "no verdict has been received");

    let outcome = store
        .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
        .await?;
    assert_eq!(
        outcome,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::TransportStarted,
        }
    );

    // Refused, and nothing written: the first body is still what would be sent.
    assert_eq!(store.pending_count().await?, 1);
    assert_eq!(
        store.pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "first" })
    );

    // The same state under the other policy queues a second record rather than refusing.
    let appended = store
        .enqueue_coalescing(edit(3, "third", 300), CoalescingPolicy::AppendIfMissing)
        .await?;
    assert_eq!(appended, CoalescingEnqueue::Appended { mutation_id: id(3) });
    assert_eq!(store.pending_count().await?, 2);
    Ok(())
}

/// `AppendIfMissing` queues the write in every case that is not a replacement.
///
/// The policy exists so a caller holding a valid precondition never has to reason about why a
/// replacement was impossible. Returning "not queued" for a forgotten `with_row` would hand back an
/// `Ok` for a write that was silently dropped, which is the failure decision 006 refuses.
pub async fn case_73_append_if_missing_never_drops_a_write<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    // No match at all.
    let first = store
        .enqueue_coalescing(edit(1, "first", 100), CoalescingPolicy::AppendIfMissing)
        .await?;
    assert_eq!(first, CoalescingEnqueue::Appended { mutation_id: id(1) });

    // No row binding: nothing to match on, so it cannot coalesce — and is queued anyway.
    let unbound = MutationIntent::new(
        id(2),
        "PUT",
        "/api/v1/profile/user-1",
        serde_json::json!({ "name": "unbound" }),
        200,
    );
    let second = store
        .enqueue_coalescing(unbound, CoalescingPolicy::AppendIfMissing)
        .await?;
    assert_eq!(second, CoalescingEnqueue::Appended { mutation_id: id(2) });

    // Two matching records now exist, because the unbound one did not coalesce into the first.
    // Ambiguity is refused as a *replacement* and still queued.
    store.enqueue(edit(3, "third", 300)).await?;
    let third = store
        .enqueue_coalescing(edit(4, "fourth", 400), CoalescingPolicy::AppendIfMissing)
        .await?;
    assert_eq!(third, CoalescingEnqueue::Appended { mutation_id: id(4) });

    assert_eq!(
        store.pending_count().await?,
        4,
        "every write reached the queue"
    );
    Ok(())
}

/// `RequireExisting` names why it queued nothing, and writes nothing in each case.
///
/// The caller has said it cannot compute a precondition for an appended write, so a refusal must be
/// legible enough to act on: `Unbound` is a bug in the call, `MissingMatch` means there was nothing
/// pending, `AmbiguousMatch` means the store declined to choose which write to discard.
pub async fn case_74_require_existing_names_its_refusals<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;

    let unbound = MutationIntent::new(
        id(1),
        "PUT",
        "/api/v1/profile/user-1",
        serde_json::json!({ "name": "unbound" }),
        100,
    );
    assert_eq!(
        store
            .enqueue_coalescing(unbound, CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(1),
            reason: CoalescingRefusal::Unbound,
        }
    );

    assert_eq!(
        store
            .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::MissingMatch,
        }
    );

    store.enqueue(edit(3, "third", 300)).await?;
    store.enqueue(edit(4, "fourth", 400)).await?;
    assert_eq!(
        store
            .enqueue_coalescing(edit(5, "fifth", 500), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(5),
            reason: CoalescingRefusal::AmbiguousMatch,
        }
    );

    // Two records, both from the plain enqueues. No refusal wrote anything.
    assert_eq!(store.pending_count().await?, 2);
    Ok(())
}
