//! Behaviour ported from the source system's own tests.
//!
//! These exist to catch accidental drift while semantics are being changed deliberately. They are
//! not a statement that the source was right — one of them asserts the *opposite* of what the
//! source asserts, and says so.
//!
//! # What did not transfer
//!
//! The source's `persistence/mutations.rs` has twelve tests. Five assert RepForge route
//! construction — `typed_request_intents_map_supported_workout_routes`,
//! `typed_request_intents_map_preferences_and_exercise_routes`,
//! `typed_request_intents_map_translation_routes`,
//! `remove_pending_exercise_draft_mutations_only_removes_matching_create_update`, and
//! `enqueue_additional_variants_and_clear_pending_cover_route_shapes` — and have no counterpart in
//! a domain-neutral library, because typed enqueue helpers live in the application. The other seven
//! ported into the six tests below; `test_sync_status_default` and `sync_status_reports_priority_order`
//! collapse into one.
//!
//! The more valuable oracle turned out to be `frontend/dto.rs`, whose round-trip tests pin the wire
//! format this crate has to keep speaking. Those live in `tests/dto_oracle.rs`.
//!
//! # Two kinds of test live here
//!
//! **Genuine oracles**, which would fail if this crate drifted: the dead-letter purge and
//! caller-supplied id preservation here, and the whole of `tests/dto_oracle.rs`. These assert
//! behaviour the source and this crate share.
//!
//! **Divergence records**, which assert the *opposite* of the source and exist so the divergence
//! cannot be undone silently: `blocked_is_retained_where_the_source_dead_letters_it`,
//! `pending_count_needs_no_refresh`, `each_pass_reports_what_it_did`, and
//! `the_client_timestamp_is_supplied_not_captured`. Their value is the doc comment as much as the
//! assertion — each records what the source did and why this crate does otherwise. That is a
//! legitimate use of a test, but it is not an oracle, and the distinction is worth keeping visible.

use frontbox::record::DeadLetterReason;
use frontbox::{
    Clock, DeadLetterStore, Disposition, InMemoryBackend, ManualClock, MutationBatchRequest,
    MutationBatchResponse, MutationId, MutationIntent, MutationResult, MutationStatus, OutboxStore,
    Outcome, ScopeKey, SyncPass, SyncRunner,
};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn id(n: u128) -> MutationId {
    MutationId::from_uuid(uuid::Uuid::from_u128(n))
}

fn fixture() -> (ManualClock, InMemoryBackend, frontbox::InMemoryStore) {
    let clock = ManualClock::new(1_700_000_000_000);
    let backend = InMemoryBackend::new(clock.clone());
    let store = backend.open(ScopeKey::new("user:alice").expect("scope"));
    (clock, backend, store)
}

// ---------------------------------------------------------------------------
// Ported from persistence/mutations.rs
// ---------------------------------------------------------------------------

/// Source: `mutation_is_pending_tracks_outbox_membership`.
///
/// The source exposes `mutation_is_pending(&id)`. This crate does not: membership is read from the
/// batch, which keeps the query surface to one scoped read rather than two.
#[test]
fn pending_membership_tracks_the_outbox() {
    pollster::block_on(async {
        let (clock, _backend, store) = fixture();
        store
            .enqueue(MutationIntent::new(
                id(1),
                "PUT",
                "/api/v1/me/preferences",
                serde_json::json!({ "default_rest_seconds": 120 }),
                clock.now_ms(),
            ))
            .await
            .expect("enqueue");

        let pending = store.pending_batch(10).await.expect("batch");
        assert!(pending.iter().any(|r| r.mutation_id == id(1)));

        store
            .apply_outcomes(&[Outcome::new(id(1), Disposition::Delete)])
            .await
            .expect("apply");

        let pending = store.pending_batch(10).await.expect("batch");
        assert!(!pending.iter().any(|r| r.mutation_id == id(1)));
    });
}

/// Source: `enqueue_put_user_preferences_with_id_preserves_caller_supplied_mutation_id`.
///
/// The source has two enqueue functions per route, one that mints an id and one that accepts the
/// caller's. Here there is only the second shape, because the caller-supplied id is what lets an
/// application try a direct write first and fall back to the queue under the same id.
#[test]
fn a_caller_supplied_mutation_id_is_preserved() {
    pollster::block_on(async {
        let (clock, _backend, store) = fixture();
        let chosen = id(0xDEAD_BEEF);

        store
            .enqueue(MutationIntent::new(
                chosen,
                "PUT",
                "/api/v1/me/preferences",
                serde_json::json!({ "units": "Kg" }),
                clock.now_ms(),
            ))
            .await
            .expect("enqueue");

        let pending = store.pending_batch(10).await.expect("batch");
        assert_eq!(pending[0].mutation_id, chosen);
    });
}

/// Source: `purge_dead_letters_removes_only_expired_records`.
///
/// Same scenario — a record rejected 40 days ago and one rejected 2 days ago, purged at a 30-day
/// retention — with the cutoff supplied instead of computed from `Utc::now()` inside the store.
/// The source's version depends on when it runs; this one does not.
#[test]
fn purge_removes_only_expired_dead_letters() {
    pollster::block_on(async {
        let (clock, _backend, store) = fixture();
        let now = clock.now_ms();

        clock.set(now - 40 * DAY_MS);
        store
            .enqueue(MutationIntent::new(
                id(1),
                "PATCH",
                "/api/v1/things/old",
                serde_json::json!({ "name": "Old" }),
                now - 45 * DAY_MS,
            ))
            .await
            .expect("enqueue old");
        store
            .apply_outcomes(&[Outcome::new(
                id(1),
                Disposition::DeadLetter {
                    reason: DeadLetterReason::Caller("oracle fixture".into()),
                },
            )])
            .await
            .expect("dead-letter old");

        clock.set(now - 2 * DAY_MS);
        store
            .enqueue(MutationIntent::new(
                id(2),
                "PATCH",
                "/api/v1/things/recent",
                serde_json::json!({ "name": "Recent" }),
                now - 5 * DAY_MS,
            ))
            .await
            .expect("enqueue recent");
        store
            .apply_outcomes(&[Outcome::new(
                id(2),
                Disposition::DeadLetter {
                    reason: DeadLetterReason::Caller("oracle fixture".into()),
                },
            )])
            .await
            .expect("dead-letter recent");

        let removed = DeadLetterStore::purge_older_than(&store, now - 30 * DAY_MS)
            .await
            .expect("purge");
        assert_eq!(removed, 1, "only one expired record should be removed");

        let remaining = DeadLetterStore::list(&store, 10).await.expect("list");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].mutation_id, id(2));
    });
}

/// Source: `apply_sync_results_moves_rejected_and_blocked_mutations_to_dead_letter`.
///
/// **This one is ported inverted, and that is the point.**
///
/// The source asserts `rejected_count == 2` and three records deleted: it dead-letters `Blocked`
/// alongside `Rejected`. Here `Blocked` is retained, so the same batch produces one dead letter and
/// two deletions, and the blocked record is still queued afterwards.
///
/// The source's own product spec defines `Blocked` as "skipped because an earlier mutation in the
/// same ordered sequence failed terminally". A record that was never evaluated on its own merits is
/// not a record the server refused.
#[test]
fn blocked_is_retained_where_the_source_dead_letters_it() {
    pollster::block_on(async {
        let (clock, _backend, store) = fixture();
        let now = clock.now_ms();

        for (n, path) in [
            (1u128, "/api/v1/me/preferences"),
            (2, "/api/v1/favorites"),
            (3, "/api/v1/things"),
        ] {
            store
                .enqueue(MutationIntent::new(
                    id(n),
                    "PUT",
                    path,
                    serde_json::json!({ "n": n }),
                    now + n as i64,
                ))
                .await
                .expect("enqueue");
        }

        let runner = SyncRunner::new(
            store,
            ScriptedOnce::new(MutationBatchResponse::new(vec![
                MutationResult::new(id(1), MutationStatus::Rejected).with_error(
                    frontbox::RemoteRejection::new("bad request").with_code("mutation_rejected"),
                ),
                MutationResult::new(id(2), MutationStatus::Blocked).with_error(
                    frontbox::RemoteRejection::new("blocked behind failure")
                        .with_code("mutation_blocked"),
                ),
                MutationResult::new(id(3), MutationStatus::Duplicate),
            ])),
        );

        let report = runner.sync_once().await.expect("sync");

        assert_eq!(report.counts.applied, 0);
        assert_eq!(report.counts.duplicate, 1);
        // The source counts 2 here, because it dead-letters `Blocked` too.
        assert_eq!(report.counts.dead_lettered, 1);
        assert_eq!(report.counts.blocked, 1);

        let dead = DeadLetterStore::list(runner.store(), 10)
            .await
            .expect("list");
        let dead_ids: Vec<_> = dead.iter().map(|r| r.mutation_id).collect();
        assert_eq!(dead_ids, vec![id(1)]);

        let pending = runner.store().pending_batch(10).await.expect("batch");
        let pending_ids: Vec<_> = pending.iter().map(|r| r.mutation_id).collect();
        assert_eq!(
            pending_ids,
            vec![id(2)],
            "the blocked record keeps its place in the queue"
        );
    });
}

/// Source: `sync_status_reports_priority_order` and `test_sync_status_default`.
///
/// The source holds four sticky booleans — `is_syncing`, `is_error`, `is_offline`, plus an implicit
/// idle — and reads them back in priority order. Its test has to reach into private atomics to set
/// them up, which is the tell: the status is not reachable through the public API.
///
/// This crate reports per pass instead of holding sticky state, so the equivalent assertion is that
/// each pass names what it did.
#[test]
fn each_pass_reports_what_it_did() {
    pollster::block_on(async {
        let (clock, backend, store) = fixture();

        // Nothing queued.
        let idle = SyncRunner::new(store.clone(), ScriptedOnce::offline());
        assert_eq!(idle.sync_once().await.expect("idle").pass, SyncPass::Idle);

        store
            .enqueue(MutationIntent::new(
                id(1),
                "POST",
                "/api/v1/things",
                serde_json::json!({}),
                clock.now_ms(),
            ))
            .await
            .expect("enqueue");

        // Queued, but no request could be attempted.
        let offline = SyncRunner::new(backend.open(store.scope().clone()), ScriptedOnce::offline());
        let report = offline.sync_once().await.expect("offline");
        assert_eq!(report.pass, SyncPass::Offline);
        assert_eq!(report.sent, 1);
        assert_eq!(report.retained, 1);

        // Queued, sent, and ruled on.
        let completed = SyncRunner::new(
            backend.open(store.scope().clone()),
            ScriptedOnce::new(MutationBatchResponse::new(vec![MutationResult::new(
                id(1),
                MutationStatus::Applied,
            )])),
        );
        let report = completed.sync_once().await.expect("completed");
        assert_eq!(report.pass, SyncPass::Completed);
        assert!(report.made_progress());
    });
}

/// Source: `refresh_pending_count_reloads_external_outbox_changes`.
///
/// The source caches the pending count in an `AtomicUsize` and needs an explicit
/// `refresh_pending_count()` after anything else writes to the outbox — a cache that can silently
/// disagree with storage. Here the count is read from storage on every call, so the ported
/// assertion is that no refresh exists to forget.
#[test]
fn pending_count_needs_no_refresh() {
    pollster::block_on(async {
        let (clock, backend, store) = fixture();
        assert_eq!(store.pending_count().await.expect("count"), 0);

        // A different handle on the same scope, standing in for the source's "external" writer.
        let other = backend.open(store.scope().clone());
        other
            .enqueue(MutationIntent::new(
                id(1),
                "POST",
                "/api/v1/things",
                serde_json::json!({}),
                clock.now_ms(),
            ))
            .await
            .expect("enqueue");

        assert_eq!(
            store.pending_count().await.expect("count"),
            1,
            "the write is visible immediately, with nothing to refresh"
        );
    });
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A transport that answers with one canned response, or refuses to send.
struct ScriptedOnce {
    response: Option<MutationBatchResponse>,
}

impl ScriptedOnce {
    fn new(response: MutationBatchResponse) -> Self {
        Self {
            response: Some(response),
        }
    }

    fn offline() -> Self {
        Self { response: None }
    }
}

impl frontbox::SyncTransport for ScriptedOnce {
    async fn send_batch(
        &self,
        _request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, frontbox::Error> {
        match &self.response {
            Some(response) => Ok(response.clone()),
            None => Err(frontbox::Error::Offline),
        }
    }
}
