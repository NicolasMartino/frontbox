# Source Test Inventory

Document Class: Reference
Status: Sourced
Date: 2026-08-25
Category: Source Corpus
Scope: Test inventory from the copied RepForge source corpus, used as the conformance oracle for extraction planning.
Sources: `raw/initial/2026-08-25T083750Z/sources`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/references/repforge-cache-source-corpus.reference.md`, `wiki/decisions/010-batch-wire-format.decision.md`

## Summary

The copied source corpus contains 130 Rust test functions, counted by scanning for `#[test]` and
`#[tokio::test]` under `raw/initial/2026-08-25T083750Z/sources`.

The earlier external review reported 125 tests. The local scan found 130. Use 130 as the project
inventory until a more precise semantic categorization is needed.

## Counts By File

| File | Tests | Extraction value |
| --- | ---: | --- |
| `exercises.rs` | 27 | Largest application-flow oracle; verifies network-first cache fallback, optimistic favorite updates, privacy-safe exercise fallback, and enqueue behavior. |
| `preferences.rs` | 17 | Preferences cache/server/fallback behavior; confirms direct dispatch with fallback enqueue. |
| `persistence/mutations.rs` | 12 | Highest-priority D1 conformance source for status application, sync loop behavior, and dead-letter handling. |
| `frontend/workout.rs` | 11 | Domain contract tests; mostly out of core scope. |
| `persistence/native.rs` | 10 | SQLite schema and native backend behavior; useful for D5 compatibility. |
| `listener.rs` | 9 | Dioxus listener and invalidation behavior; adapter and D2/D3 evidence. |
| `invalidation/consumer.rs` | 7 | Server-side invalidation consumer behavior; mostly out of core, useful for event semantics. |
| `persistence/cache.rs` | 6 | Entity cache version comparison and stale/reset semantics; D2 oracle. |
| `frontend/error.rs` | 6 | Domain/shared error behavior; mostly out of core. |
| `frontend/validation.rs` | 5 | Contract validation behavior; out of D1. |
| `frontend/dto.rs` | 5 | Mutation protocol DTO round-trip and idempotency-key shape; D1 type oracle. |
| `cache_versions.rs` | 4 | Server version repository behavior; D2 reference only. |
| `persistence/preferences_cache.rs` | 3 | Local preferences read model behavior; example/read-model evidence. |
| `persistence/preferences.rs` | 3 | Preferences persistence helper behavior; app-layer evidence. |
| `persistence/exercise_drafts.rs` | 2 | Draft persistence behavior; out of core. |
| `frontend/auth/claims.rs` | 2 | Auth claims behavior; out of core. |
| `invalidation/mapper.rs` | 1 | Outcome-to-invalidation mapping; useful model for app-supplied hooks. |

Total: 130.

## D1 Priority

**Corrected 2026-08-26, after D1 was implemented.** The original guidance was to port the 12 tests
from `persistence/mutations.rs` first. In practice seven of the twelve transferred, into six ported
tests — the two `SyncStatus` tests collapse into one:

| Source test | Ported as |
| --- | --- |
| `mutation_is_pending_tracks_outbox_membership` | `pending_membership_tracks_the_outbox` |
| `enqueue_put_user_preferences_with_id_preserves_caller_supplied_mutation_id` | `a_caller_supplied_mutation_id_is_preserved` |
| `purge_dead_letters_removes_only_expired_records` | `purge_removes_only_expired_dead_letters` |
| `apply_sync_results_moves_rejected_and_blocked_mutations_to_dead_letter` | `blocked_is_retained_where_the_source_dead_letters_it` — **inverted** |
| `sync_status_reports_priority_order`, `test_sync_status_default` | `each_pass_reports_what_it_did` |
| `refresh_pending_count_reloads_external_outbox_changes` | `pending_count_needs_no_refresh` |

The remaining five all assert RepForge route construction, which a domain-neutral library has no
counterpart for:

| Not ported | What it asserts |
| --- | --- |
| `typed_request_intents_map_supported_workout_routes` | That session and set helpers build the right method, path, and body |
| `typed_request_intents_map_preferences_and_exercise_routes` | The same for preferences and exercise routes |
| `typed_request_intents_map_translation_routes` | The same for translation-proposal routes |
| `remove_pending_exercise_draft_mutations_only_removes_matching_create_update` | Selective removal of pending mutations by RepForge draft semantics |
| `enqueue_additional_variants_and_clear_pending_cover_route_shapes` | Coverage across the remaining typed enqueue helpers |

All five depend on `PostSessionRequest`, `PutUserPreferencesRequest`, and their siblings. The
extraction boundary puts typed enqueue helpers in the application, so there is nothing in this crate
for them to test — this is the boundary working, not a coverage gap. The behaviour they *indirectly*
exercise, that an enqueued envelope round-trips its method, path, and body intact, is covered
directly by conformance case 27 and by the `dto.rs` ports.

An earlier version of this section said "only six transferred" and "the other six", which is
arithmetically impossible against twelve. Corrected 2026-08-26.

**The higher-value oracle was `frontend/dto.rs`.** Its five round-trip tests pin the wire format,
which `wiki/decisions/010-batch-wire-format.decision.md` commits to preserving byte for byte. Four
of the five ported directly; the fifth (`test_mutation_batch_request_new_sets_client_timestamp`)
became an exact assertion rather than a `>=` one, because the timestamp is now a parameter instead
of an internal `Utc::now()` call.

The `Blocked` divergence stands as originally warned: the source dead-letters it, D1 retains it per
`wiki/decisions/005-mutation-outcome-policy.decision.md`, and the ported test asserts the opposite
of the source's on purpose.

The `persistence/mutations.rs` ports live in `tests/source_oracle.rs`; the `frontend/dto.rs`
round-trips live in `tests/dto_oracle.rs`. Split 2026-08-27 along the seam the original file's
own header already described, when a 400-line file limit was adopted.

## Later Priority

- D2 should use `persistence/cache.rs` and listener tests to verify version and invalidation
  behavior.
- D3 should use `listener.rs` tests for Dioxus adapter behavior.
- D4 should use selected `exercises.rs` and `preferences.rs` tests to validate a RepForge-style
  migration trial.
- D5 should use `persistence/native.rs` plus new IndexedDB tests to verify backend conformance.
