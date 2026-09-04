//! Cases 62-66: the read-model row store, and what a report says drained.
//!
//! Decision 032 brought the rows inside so the merge rule could be enforced rather than
//! rediscovered by every application; decision 035 made the reports name what left the queue so a
//! caller stops rebuilding an index to find out. These cases pin both, and case 63 is the one that
//! justifies the whole boundary move.

use super::prelude::*;
use crate::record::{RowRef, StoredRow};
use crate::runner::DrainedAs;
use crate::store::RowStore;
use crate::testing::RowStoreFactory;

/// A row survives a write and a reopen, and one scope cannot see another's.
///
/// The reopen is how a test says "the process died and came back" to a backend with no process to
/// kill — the same device `case_46` uses for enqueue order.
pub async fn case_62_rows_round_trip_and_stay_scoped<F: RowStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let mine = scope("user:rows-a@tenant:acme");
    let theirs = scope("user:rows-b@tenant:acme");
    let rows = factory.open_rows(mine.clone()).await?;

    let key = RowRef::new("todo", "row-1");
    rows.put_rows(&[StoredRow::new(
        key.clone(),
        // Nested and mixed-typed on purpose. The blob is the only thing a caller can put anything
        // in — decision 032's stamps are gone, so anything an application wants to say about shape
        // or content version it says in here — and a backend that reshaped a nested object or
        // narrowed a number would pass a flat fixture and fail a real one.
        serde_json::json!({
            "title": "buy milk",
            "shape": 3,
            "tags": ["errand", "food"],
            "meta": { "revision": "v1", "pinned": false },
        }),
    )])
    .await?;

    // Reopened rather than reused: a backend that answered from an in-process cache would pass a
    // same-handle read and fail this.
    let reopened = factory.open_rows(mine).await?;
    let found = reopened
        .get_row(&key)
        .await?
        .expect("row survives a reopen");
    assert_eq!(
        found.blob,
        serde_json::json!({
            "title": "buy milk",
            "shape": 3,
            "tags": ["errand", "food"],
            "meta": { "revision": "v1", "pinned": false },
        }),
        "the blob comes back exactly as it went in, nesting and types included; core never parses it"
    );
    assert!(!found.stale);

    let other = factory.open_rows(theirs).await?;
    assert!(
        other.get_row(&key).await?.is_none(),
        "a row written under one scope is invisible to another, exactly as the outbox is"
    );
    assert!(other.list_rows("todo", 10).await?.is_empty());
    Ok(())
}

/// **The case decision 032 exists for.** A hydration write does not clobber a row with queued work.
///
/// This is the failure every offline-first application hits on its first durable reload: the queue
/// survives, the projection does not, so startup fetches from a server that has never seen the
/// unsent writes and overwrites the user's own edits with staler data. The work is not lost — it
/// drains later and the row comes back — but the screen lies in the meantime, and for a client
/// that is still offline "the meantime" has no end.
///
/// Under `put_rows` the clobber happens, which is correct: the caller is stating what it wants
/// stored. `merge_rows` is where the server's word is subordinate to unsent local work.
pub async fn case_63_a_merge_skips_rows_with_queued_work<F: RowStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let key = scope("user:merge@tenant:acme");
    let store = factory.open(key.clone()).await?;
    let rows = factory.open_rows(key).await?;

    let queued = RowRef::new("todo", "row-queued");
    let quiet = RowRef::new("todo", "row-quiet");

    // The optimistic projection: what the user sees, ahead of the server.
    rows.put_rows(&[
        StoredRow::new(
            queued.clone(),
            serde_json::json!({"title": "edited offline"}),
        ),
        StoredRow::new(quiet.clone(), serde_json::json!({"title": "untouched"})),
    ])
    .await?;

    // One unsent write, bound to the first row. This is the only thing that distinguishes the two.
    store
        .enqueue(intent(1, 1_700_000_000_000).with_row(queued.clone()))
        .await?;

    // The server's list, which predates the queued edit and disagrees about both rows.
    let skipped = rows
        .merge_rows(&[
            StoredRow::new(
                queued.clone(),
                serde_json::json!({"title": "server's older copy"}),
            ),
            StoredRow::new(
                quiet.clone(),
                serde_json::json!({"title": "server wins here"}),
            ),
        ])
        .await?;

    assert_eq!(
        skipped,
        vec![queued.clone()],
        "the merge says what it refused"
    );
    assert_eq!(
        rows.get_row(&queued).await?.expect("still there").blob,
        serde_json::json!({"title": "edited offline"}),
        "the row with unsent work keeps the user's version"
    );
    assert_eq!(
        rows.get_row(&quiet).await?.expect("still there").blob,
        serde_json::json!({"title": "server wins here"}),
        "the row with nothing queued takes the server's"
    );
    Ok(())
}

/// An unbound mutation protects nothing, and deleting a row takes its marker with it.
///
/// Two halves of the same promise: the binding is opt-in, and the marker is a field on the row
/// rather than a separate thing that can outlive it. Decision 023 owed an unbounded-growth policy
/// only because the two were separable; here the deletion is the policy.
pub async fn case_64_unbound_work_protects_nothing_and_markers_die_with_rows<F: RowStoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let key = scope("user:unbound@tenant:acme");
    let store = factory.open(key.clone()).await?;
    let rows = factory.open_rows(key).await?;

    let row = RowRef::new("todo", "row-1");
    rows.put_rows(&[StoredRow::new(
        row.clone(),
        serde_json::json!({"v": "local"}),
    )])
    .await?;
    // Queued, but bound to nothing. A caller that does not opt in gets a plain write.
    store.enqueue(intent(1, 1_700_000_000_000)).await?;

    let skipped = rows
        .merge_rows(&[StoredRow::new(
            row.clone(),
            serde_json::json!({"v": "server"}),
        )])
        .await?;
    assert!(
        skipped.is_empty(),
        "an unbound mutation names no row, so the merge has nothing to protect"
    );
    assert_eq!(
        rows.get_row(&row).await?.expect("row").blob,
        serde_json::json!({"v": "server"})
    );

    assert_eq!(rows.set_stale(std::slice::from_ref(&row), true).await?, 1);
    assert!(rows.get_row(&row).await?.expect("row").stale);
    assert_eq!(
        rows.set_stale(std::slice::from_ref(&row), true).await?,
        0,
        "setting a flag to what it already is changes nothing and says so"
    );

    assert_eq!(rows.delete_rows(std::slice::from_ref(&row)).await?, 1);
    assert!(
        rows.get_row(&row).await?.is_none(),
        "the row is gone, and so is the staleness marker that was a field on it"
    );
    assert_eq!(
        rows.delete_rows(&[row]).await?,
        0,
        "deleting what is not there is not an error"
    );
    Ok(())
}

/// A report names every record that left the queue, with what became of it and the row it touched.
///
/// Before decision 035 the reports named a mutation only when something went *wrong*, so a UI
/// tracking per-row progress had no success event and rebuilt its whole index after every drain.
/// The dead-lettered entry is the one that matters most: it left the queue and its write did not
/// happen, and a caller clearing a "saving…" marker without noticing the difference would be
/// telling the user their edit landed.
pub async fn case_65_a_report_names_what_drained<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let store = factory.open(scope("user:drained@tenant:acme")).await?;
    let bound = RowRef::new("todo", "row-1");

    store
        .enqueue(intent(1, 1_700_000_000_000).with_row(bound.clone()))
        .await?;
    store.enqueue(intent(2, 1_700_000_000_001)).await?;
    store.enqueue(intent(3, 1_700_000_000_002)).await?;

    let runner = runner(
        store,
        Reply::Exact(vec![
            MutationResult::new(id(1), MutationStatus::Applied),
            MutationResult::new(id(2), MutationStatus::Duplicate),
            MutationResult::new(id(3), MutationStatus::Rejected).with_error(full_rejection()),
        ]),
    );
    let report = runner.sync_once().await?;

    assert_eq!(report.drained.len(), 3, "all three left the queue");
    assert_eq!(
        report.drained.len(),
        report.counts.applied + report.counts.duplicate + report.counts.dead_lettered,
        "the itemization and the counts describe the same records"
    );

    let applied = report
        .drained
        .iter()
        .find(|d| d.mutation_id == id(1))
        .expect("the applied record is named");
    assert_eq!(applied.outcome, DrainedAs::Applied);
    assert_eq!(
        applied.row.as_ref(),
        Some(&bound),
        "a bound mutation reports the row it was about, so the caller need not parse a path"
    );

    let duplicate = report
        .drained
        .iter()
        .find(|d| d.mutation_id == id(2))
        .expect("the duplicate is named");
    assert_eq!(
        duplicate.outcome,
        DrainedAs::Duplicate,
        "`Applied` and `Duplicate` both delete, and the report still tells them apart"
    );
    assert!(
        duplicate.row.is_none(),
        "an unbound mutation reports no row"
    );

    let refused = report
        .drained
        .iter()
        .find(|d| d.mutation_id == id(3))
        .expect("the dead letter is named");
    assert_eq!(
        refused.outcome,
        DrainedAs::DeadLettered,
        "it left the queue; its write did not happen, and the report must not blur the two"
    );
    Ok(())
}

/// A retained record does not appear in `drained`, and it records why it is still queued.
///
/// The two halves of what a retention pass owes: it is not progress, and it is not silent.
/// Decision 017 made the record give up at a bound; decision 033 is what tells whoever reads the
/// dead letter afterwards what it was up against.
pub async fn case_66_a_retained_record_is_not_drained_and_says_why<F: StoreFactory>(
    factory: &F,
) -> Result<(), Error> {
    let store = factory.open(scope("user:retained@tenant:acme")).await?;
    store.enqueue(intent(1, 1_700_000_000_000)).await?;

    let runner = runner(store, Reply::All(MutationStatus::Pending));
    let report = runner.sync_once().await?;

    assert!(
        report.drained.is_empty(),
        "a retained record has not left the queue, so nothing drained"
    );
    assert!(!report.made_progress());
    assert_eq!(report.retained, 1);

    let pending = runner.store().pending_batch(10).await?;
    let record = pending.first().expect("still queued");
    assert_eq!(record.attempts, 1, "the verdict was an attempt");
    assert_eq!(
        record.last_error.as_deref(),
        Some("pending"),
        "and the record says which verdict left it queued"
    );

    // An over-long reason is cut rather than stored whole: core states the bound, the store keeps
    // it. Applied directly, because no server in this suite produces 512 bytes of status.
    let long = "x".repeat(crate::record::LAST_ERROR_MAX * 2);
    runner
        .store()
        .apply_outcomes(&[Outcome::new(
            id(1),
            Disposition::Retain { reason: Some(long) },
        )])
        .await?;
    let pending = runner.store().pending_batch(10).await?;
    let record = pending.first().expect("still queued");
    assert_eq!(
        record.last_error.as_deref().map(str::len),
        Some(crate::record::LAST_ERROR_MAX),
        "the store truncates to the bound core states"
    );
    assert_eq!(record.attempts, 2, "and it was still an attempt");
    Ok(())
}
