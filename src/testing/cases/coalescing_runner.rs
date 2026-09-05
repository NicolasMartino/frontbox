//! Cases 81-82: the runner paths that mark a batch and then leave the record queued.
//!
//! Cases 78 and 79 cover the two offline shapes. These are the other two ways a pass ends with the
//! record still in the outbox — an attempted transport failure, and a server response that omits it.
//!
//! The pair is deliberate, because the two differ on `attempts` and agree on the mark. A transport
//! failure produced no response at all, so decision 017's count stays at zero; an omitted verdict
//! arrived inside a response the server did send, which decision 019 synthesizes into a `Retain`, so
//! the count moves. **Neither difference reaches coalescing**: the subject here is that a pass which
//! read a batch has spent it, whatever happened next.
//!
//! Separate from cases 75-80 because that file is at its length.

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

/// Assert the record is still queued, still carries `first`, and refuses replacement.
async fn assert_spent<S: OutboxStore>(store: &S) -> Result<(), Error> {
    assert_eq!(store.pending_count().await?, 1);
    assert_eq!(
        store
            .enqueue_coalescing(edit(2, "second", 200), CoalescingPolicy::RequireExisting)
            .await?,
        CoalescingEnqueue::NotQueued {
            mutation_id: id(2),
            reason: CoalescingRefusal::TransportStarted,
        },
        "the batch was read, so the server may hold this identifier"
    );
    assert_eq!(
        store.pending_batch(10).await?[0].body,
        serde_json::json!({ "name": "first" }),
        "and the queued body was not rewritten"
    );
    Ok(())
}

/// An attempted transport failure spends coalescibility, and does not count as an attempt.
///
/// The pairing that shows the mark is about the *request* rather than the verdict.
/// [`Error::Transport`](crate::Error::Transport) is the honest case — the transport says outright
/// that it tried and failed, so the request may have reached the server and lost only its response.
/// Nothing about `attempts` changes: decision 017 counts verdicts received, and there was none.
///
/// A design that tied eligibility to `attempts == 0` would call this record coalescible. That is the
/// proposal `wiki/decisions/044-transport-started-before-the-request.decision.md` rejects, and this
/// case is what would fail if it were ever adopted.
pub async fn case_81_a_failed_transport_spends_coalescibility<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    let runner = runner(store, Reply::TransportFailure);
    let failure = runner.sync_once().await.expect_err("the transport failed");
    assert!(
        !failure.is_offline(),
        "a transport failure is not offline; the two reach the mark by different routes"
    );

    let record = &runner.store().pending_batch(10).await?[0];
    assert_eq!(record.attempts, 0, "no verdict was received, so no attempt");
    assert_spent(runner.store()).await
}

/// A record the server's response omitted stays queued and stays spent.
///
/// The quietest path, and the one most easily mistaken for "nothing happened". The batch was sent,
/// the server answered, and the answer said nothing about this record — case 33 fixes what that
/// means for the queue: silence is not a verdict, so the record is retained rather than dropped or
/// refused.
///
/// Unlike case 81 this *does* move `attempts`, and the difference is worth stating rather than
/// smoothing over. A response arrived. Decision 019 synthesizes the silence into a `Retain`, which
/// is a ruling this client can act on — the server had its chance at this record and used it to say
/// nothing — so decision 017's bound advances and a permanently ignored record eventually
/// dead-letters instead of cycling forever.
///
/// What the two share is the mark, which is the point. The response is proof the server saw the
/// record, so rewriting its body under the same identifier would be deduped away and never applied,
/// exactly as if a verdict had come back naming it.
pub async fn case_82_an_omitted_verdict_leaves_the_record_spent<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    store.enqueue(edit(1, "first", 100)).await?;

    // A response with no results at all: the server ruled on nothing it was sent.
    let runner = runner(store, Reply::Exact(Vec::new()));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed, "the server did answer");
    assert_eq!(report.sent, 1);
    assert_eq!(report.retained, 1, "silence retains rather than drops");
    assert!(report.anomalies.is_empty(), "a partial answer is allowed");
    assert_eq!(
        runner.store().pending_batch(10).await?[0].attempts,
        1,
        "a response did arrive, so the retention bound advances — unlike case 81"
    );
    assert_spent(runner.store()).await
}
