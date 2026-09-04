//! What the queue does with a write the server will not take, or cannot be placed.
//!
//! Part of the `regressions` suite; see `main.rs` for what it covers.

use crate::common::{self, app, server, ALICE};
use frontbox::{DeadLetterReason, DrainEnd, OutboxStore};

/// The server refuses malformed toggles instead of silently treating them as `false`.
#[tokio::test]
async fn malformed_done_is_a_dead_letter_not_a_silent_untick() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "typed")
        .await
        .expect("create");
    app.sync().await.expect("sync");

    app.outbox()
        .enqueue(frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "PUT",
            format!("/api/v1/todos/{id}/done"),
            serde_json::json!({ "done": "yes" }),
            1_700_000_000_010,
        ))
        .await
        .expect("enqueue malformed write");

    let report = app.sync().await.expect("sync");
    assert_eq!(report.counts.dead_lettered, 1);
    app.refresh_from_server().await.expect("refresh");
    assert!(
        !app.store().get(&id).expect("row").done,
        "the malformed write must not default to false after the server saw no boolean"
    );
}

/// A create with a colliding row id is refused, not used as a replacement.
#[tokio::test]
async fn duplicate_create_does_not_replace_the_row() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "first title")
        .await
        .expect("create");
    app.sync().await.expect("sync");

    app.outbox()
        .enqueue(frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/todos",
            // `user_id` is required by the todo service since D4d; a create without one is
            // refused for *that* reason and this regression would stop testing collisions.
            serde_json::json!({ "id": id, "title": "second title", "user_id": common::ALICE_USER }),
            1_700_000_000_020,
        ))
        .await
        .expect("enqueue duplicate create");

    let report = app.sync().await.expect("sync");
    assert_eq!(report.counts.dead_lettered, 1);
    let parked = app.dead_letters(100).await.expect("dead letters");
    assert!(parked.iter().any(|record| matches!(
        &record.reason,
        DeadLetterReason::Rejected { error: Some(error) }
            if error.code.as_deref() == Some("already_exists")
    )));

    app.refresh_from_server().await.expect("refresh");
    assert_eq!(app.store().get(&id).expect("row").title, "first title");
}

/// A queued envelope this application cannot place still leaves a usable index and a live report.
///
/// The old behaviour raised on it, which discarded the `DrainReport` for work the server had
/// already committed and left the *previous* index on screen — an older answer, presented as
/// current, because a newer one was one record short of perfect.
#[tokio::test]
async fn an_unplaceable_envelope_is_reported_not_raised() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "placeable")
        .await
        .expect("create");

    // Not a todo path, so nothing binds a row to it — `MutationIntent::with_row` is how a write
    // says which row it is about, and this one cannot. (It used to be a `row_id_of` helper that
    // parsed the path; decision 032 replaced parsing with binding at enqueue.) Enqueued by hand
    // because the application has no write that produces an unbound envelope; only a future
    // version of it, or a corrupted queue, would.
    app.outbox()
        .enqueue(frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "POST",
            "/api/v1/elsewhere",
            serde_json::json!({}),
            1_700_000_000_030,
        ))
        .await
        .expect("enqueue unplaceable write");

    app.transport().set_offline(true);
    let report = app
        .sync()
        .await
        .expect("a capped or unreadable scan is not a sync failure");
    assert_eq!(report.ended, DrainEnd::Offline, "the drain still reported");

    // Reconciliation is what fills the gap, and since decision 035 that is a startup step rather
    // than something every drain redoes — `sync` now decrements from `DrainReport::drained`. The
    // explicit call is what a restart would do.
    app.refresh_pending().await.expect("reconcile");
    let gap = app.pending_index_gap();
    assert_eq!(
        gap.unrecoverable, 1,
        "the envelope with no bound row is counted"
    );
    assert_eq!(gap.beyond_scan_cap, 0);
    assert!(!gap.is_empty());
    assert!(
        app.store().is_saving(&id),
        "the row it could place is still indexed"
    );
}

/// A scan that hits its cap indexes what it read and says how much it missed.
#[tokio::test]
async fn a_capped_index_scan_reports_the_shortfall() {
    let (base, _state) = server().await;
    let app = app(&base, ALICE).await.with_pending_scan_limit(1);

    app.transport().set_offline(true);
    app.create(common::ALICE_USER, "first")
        .await
        .expect("create");
    app.create(common::ALICE_USER, "second")
        .await
        .expect("create");
    app.create(common::ALICE_USER, "third")
        .await
        .expect("create");

    app.sync().await.expect("sync");
    app.refresh_pending().await.expect("reconcile");
    let gap = app.pending_index_gap();
    assert_eq!(
        gap.beyond_scan_cap, 2,
        "three queued, one scanned, two unaccounted for"
    );
    assert_eq!(gap.unrecoverable, 0);
    assert_eq!(
        app.store().saving_rows(),
        1,
        "the index holds what the scan could reach rather than nothing at all"
    );
}

/// Deleting a row another device already deleted is not a refusal.
///
/// The asymmetry is deliberate: `DELETE` names no state to lose, so a replay against an absent row
/// achieved what the user asked for. `PUT` carries a title or a flag that would be silently
/// dropped, so it refuses. Two devices deleting one todo is the ordinary case, and dead-lettering
/// the second would show a user a failure for a write that did exactly what they wanted.
#[tokio::test]
async fn deleting_an_absent_row_is_idempotent() {
    let (base, state) = server().await;
    let app = app(&base, ALICE).await;
    let id = app
        .create(common::ALICE_USER, "delete me twice")
        .await
        .expect("create");
    app.delete(&id).await.expect("delete");
    app.sync().await.expect("sync");

    // The second device's delete: same row, a mutation id this server has never seen.
    app.outbox()
        .enqueue(frontbox::MutationIntent::new(
            frontbox::MutationId::new(),
            "DELETE",
            format!("/api/v1/todos/{id}"),
            serde_json::json!({}),
            1_700_000_000_040,
        ))
        .await
        .expect("enqueue second delete");

    let report = app.sync().await.expect("sync");
    assert_eq!(report.counts.applied, 1, "the repeat delete is applied");
    assert_eq!(report.counts.dead_lettered, 0, "and refuses nothing");
    assert_eq!(state.applied_count().await.expect("applied"), 3);
}
