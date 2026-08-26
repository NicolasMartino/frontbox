//! Cases 16, 32, 33, 34: verdicts the runner refuses to act on.

use super::prelude::*;

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

    assert_eq!(
        report.anomalies,
        vec![Anomaly::new(id(99), AnomalyKind::UnknownMutation)]
    );
    assert_eq!(report.counts.applied, 0);
    assert_eq!(
        runner.store().pending_count().await?,
        1,
        "the unrelated record must be untouched"
    );
    Ok(())
}

/// A verdict repeated inside one server response applies *neither* of them.
///
/// This is the case that decides what "no basis for preferring either" costs. Applying the first
/// and reporting the rest is the cheaper implementation, and it is wrong for a reason that does not
/// show up until a server misbehaves: it makes arrival order the tie-break. A server that answered
/// `Applied` then `Rejected` would have its record deleted; one that answered `Rejected` then
/// `Applied` would have the same record dead-lettered. Same disagreement, opposite outcome, decided
/// by JSON array position.
///
/// So every verdict for a repeated id is reported and none is applied. The record keeps its place
/// in the queue and is ruled on again next pass, which is safe because `mutation_id` is the
/// idempotency key: the server either repeats itself coherently or answers `Duplicate`.
pub async fn case_32_a_repeated_verdict_applies_neither<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 2).await?;

    let runner = runner(
        store,
        Reply::Exact(vec![
            MutationResult::new(id(1), MutationStatus::Applied),
            MutationResult::new(id(1), MutationStatus::Rejected),
            MutationResult::new(id(2), MutationStatus::Applied),
        ]),
    );
    let report = runner.sync_once().await?;

    // The contested record is untouched: not applied, not dead-lettered, still queued.
    assert_eq!(
        report.counts.applied, 1,
        "only the uncontested record drains"
    );
    assert_eq!(report.counts.dead_lettered, 0);
    assert_eq!(report.retained, 1);
    assert_eq!(runner.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);

    // Both verdicts are reported, not just the second. Reporting one would imply the other had
    // been honoured.
    assert_eq!(
        report.anomalies,
        vec![
            Anomaly::new(id(1), AnomalyKind::RepeatedVerdict),
            Anomaly::new(id(1), AnomalyKind::RepeatedVerdict),
        ]
    );

    // Order does not change the outcome, which is the whole point. A separate scope, because one
    // factory shares durable state across every store it opens.
    let store = factory
        .open(scope("user:carol@tenant:acme@schema:1"))
        .await?;
    seed(&store, 1).await?;
    let reversed = super::runner(
        store,
        Reply::Exact(vec![
            MutationResult::new(id(1), MutationStatus::Rejected),
            MutationResult::new(id(1), MutationStatus::Applied),
        ]),
    );
    let report = reversed.sync_once().await?;

    assert_eq!(report.counts.applied, 0);
    assert_eq!(report.counts.dead_lettered, 0);
    assert_eq!(reversed.store().pending_count().await?, 1);
    assert_eq!(DeadLetterStore::count(reversed.store()).await?, 0);
    Ok(())
}

/// A mutation the server returned no verdict for stays queued and is counted as retained.
///
/// Silence is not a verdict. Treating an omission as success drops the work; treating it as failure
/// invents a refusal the server never made. Neither is recoverable, so the record keeps its place
/// and is sent again.
///
/// The distinction this case guards is between *retained* and *unsent*: case 21 also ends with
/// records still queued, but those were never in the batch at all. Here the record was sent, the
/// server answered the batch, and the answer simply did not mention it.
pub async fn case_33_an_omitted_verdict_retains_the_record<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 3).await?;

    // A verdict for the first record only. `ByPosition` gives the rest no result at all, which is
    // exactly a server declining to rule on them.
    let runner = runner(store, Reply::ByPosition(vec![MutationStatus::Applied]));
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.sent, 3, "all three were sent");
    assert_eq!(report.counts.applied, 1);

    // Nothing classified the two silent ones — they are not blocked and not pending.
    assert_eq!(report.counts.blocked, 0);
    assert_eq!(report.counts.pending, 0);
    assert_eq!(
        report.retained, 2,
        "retained counts records the server said nothing about, not just the ones it deferred"
    );

    // Silence is not an anomaly either. The server is allowed to answer only part of a batch.
    assert!(report.anomalies.is_empty());

    assert_eq!(runner.store().pending_count().await?, 2);
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 0);
    assert!(report.made_progress(), "one record did drain");

    // They are still there to be sent again, unmodified.
    let next = runner.store().pending_batch(10).await?;
    let ids: Vec<_> = next.iter().map(|record| record.mutation_id).collect();
    assert_eq!(ids, vec![id(2), id(3)]);
    Ok(())
}

/// A status this crate has never heard of retains its record and does not spoil the batch.
///
/// The failure this guards is total, not partial. Before decision 012 the enum derived
/// `Deserialize`, so one unrecognised word failed the *whole* `MutationBatchResponse` — every
/// verdict in it, on every pass, forever, because the next pass sends the same records and gets the
/// same answer. A server rolling out a sixth status would have wedged every client at once.
///
/// So the assertion is in two halves: the known verdicts around it still apply, and the unknown one
/// is retained with the server's own spelling intact. The spelling matters — a bare "something was
/// unrecognised" is a stall with no diagnosis, which is barely better than the silence.
pub async fn case_34_an_unknown_status_retains_and_reports<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let (store, _) = open(factory).await?;
    seed(&store, 3).await?;

    let runner = runner(
        store,
        Reply::ByPosition(vec![
            MutationStatus::Applied,
            MutationStatus::Unknown("Throttled".into()),
            MutationStatus::Rejected,
        ]),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.pass, SyncPass::Completed);
    assert_eq!(report.sent, 3);

    // The verdicts either side of the unknown one were applied normally.
    assert_eq!(report.counts.applied, 1, "a known verdict still drains");
    assert_eq!(
        report.counts.dead_lettered, 1,
        "an unrecognised status must not spoil the verdicts around it"
    );
    assert_eq!(DeadLetterStore::count(runner.store()).await?, 1);

    // The unknown one is retained, counted, and named.
    assert_eq!(report.counts.unknown_status, 1);
    assert_eq!(report.retained, 1);
    assert_eq!(
        report.anomalies,
        vec![Anomaly::new(
            id(2),
            AnomalyKind::UnknownStatus("Throttled".into())
        )],
        "the server's own spelling survives to the report"
    );

    // Retained means still queued, and still queued means sent again.
    assert_eq!(runner.store().pending_count().await?, 1);
    let next = runner.store().pending_batch(10).await?;
    assert_eq!(
        next.iter().map(|r| r.mutation_id).collect::<Vec<_>>(),
        vec![id(2)]
    );

    assert!(report.made_progress(), "two of the three drained");
    Ok(())
}
