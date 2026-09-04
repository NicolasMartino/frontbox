//! Cases 28, 30, 31: re-entrancy, cancellation, and outcome hygiene.

use super::prelude::*;

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

    // Finished by awaiting rather than by polling in a loop, and the difference is not cosmetic.
    //
    // A busy `loop { poll() }` with a noop waker never yields to whatever actually completes the
    // work. Every backend was synchronous when this case was written, so each `await` inside
    // resolved on the first poll and the loop terminated immediately. **An IndexedDB backend
    // suspends on a browser event**, so the same loop spins on the only thread the event loop
    // has, the request callback can never fire, and the case hangs the tab rather than failing.
    //
    // The manual polls above are still needed — they are the only way to suspend a pass mid-flight
    // so a second can be re-entered. Once that is proven, handing the future to the real executor
    // is both correct and backend-agnostic.
    first.await?;

    assert_eq!(runner.transport().send_count(), 1);
    assert_eq!(runner.store().pending_count().await?, 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Cancellation and outcome hygiene
// ---------------------------------------------------------------------------

/// A cancelled pass does not wedge the runner.
///
/// Cancellation is routine on the runtimes this crate targets: a Dioxus `use_future` is dropped
/// whenever its component re-renders. A re-entrancy flag released only on the success path would
/// leave every later pass reporting [`SyncPass::AlreadyRunning`] forever — a queue that silently
/// stops syncing, with no error to explain it.
pub async fn case_30_a_cancelled_pass_does_not_wedge_the_runner<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    use std::future::Future;
    use std::task::{Context, Waker};

    let (store, _) = open(factory).await?;
    seed(&store, 1).await?;

    let runner = SyncRunner::new(
        store,
        ScriptedTransport::new(Reply::All(MutationStatus::Applied)).yielding(),
    );

    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);

    // Start a pass, let it suspend inside the transport, then drop it.
    {
        let mut cancelled = Box::pin(runner.sync_once());
        assert!(
            cancelled.as_mut().poll(&mut cx).is_pending(),
            "the pass must be in flight when it is dropped"
        );
    }

    // The work is untouched: the batch went out, but the verdicts were never applied.
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(runner.transport().send_count(), 1);

    // And the next pass runs normally rather than reporting `AlreadyRunning`.
    let report = runner.sync_once().await?;
    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.counts.applied, 1);
    assert_eq!(runner.store().pending_count().await?, 0);

    // The record was sent twice, which is safe only because `mutation_id` is the idempotency key.
    assert_eq!(runner.transport().send_count(), 2);
    Ok(())
}

/// An outcome set naming the same record twice is rejected, and commits nothing.
///
/// Two outcomes resolve to one record. A pair of `DeadLetter` dispositions would write two dead
/// letters for a single row, so the dead-letter count would stop matching reality; a `Delete` paired
/// with a `DeadLetter` has no defensible winner. Rejecting the set is the only answer that cannot
/// silently corrupt a count.
///
/// [`SyncRunner`] already satisfies this — it drops a second verdict for an id the same response
/// already ruled on — but `apply_outcomes` is public and has to defend its own contract.
pub async fn case_31_duplicate_outcomes_are_rejected<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    for pair in [
        [
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("test fixture".into()),
            },
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("test fixture".into()),
            },
        ],
        [
            Disposition::Delete,
            Disposition::DeadLetter {
                reason: DeadLetterReason::Caller("test fixture".into()),
            },
        ],
        [Disposition::Retain { reason: None }, Disposition::Delete],
    ] {
        let [first, second] = pair;
        let result = store
            .apply_outcomes(&[
                Outcome::new(id(1), first.clone()),
                Outcome::new(id(1), second.clone()),
            ])
            .await;

        match result {
            Err(Error::Protocol { .. }) => {}
            other => {
                panic!("a repeated id must be rejected, got {other:?} for {first:?}/{second:?}")
            }
        }
    }

    // Nothing was committed by any of the rejected attempts.
    assert_eq!(store.pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(&store).await?, 0);
    assert_eq!(QuarantineStore::count(&store).await?, 0);

    // The same dispositions applied to distinct records are fine.
    store
        .apply_outcomes(&[
            Outcome::new(id(1), Disposition::Delete),
            Outcome::new(
                id(2),
                Disposition::DeadLetter {
                    reason: DeadLetterReason::Caller("test fixture".into()),
                },
            ),
        ])
        .await?;
    assert_eq!(store.pending_count().await?, 0);
    assert_eq!(DeadLetterStore::count(&store).await?, 1);
    Ok(())
}
