# Source Test Inventory

Document Class: Reference
Status: Sourced
Date: 2026-08-25
Category: Source Corpus
Scope: Test inventory from the copied RepForge source corpus, used as the conformance oracle for extraction planning.
Sources: `raw/initial/2026-08-25T083750Z/sources`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/references/repforge-cache-source-corpus.reference.md`

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

Port the 12 tests from `persistence/mutations.rs` first. They sit closest to the reusable outbox
runtime and should become the initial conformance suite.

Do not port them blindly. At least one source behavior is now considered wrong for extraction:
`Blocked` currently follows the `Rejected` dead-letter path, but D1 retains it according to
`wiki/decisions/005-mutation-outcome-policy.decision.md`.

## Later Priority

- D2 should use `persistence/cache.rs` and listener tests to verify version and invalidation
  behavior.
- D3 should use `listener.rs` tests for Dioxus adapter behavior.
- D4 should use selected `exercises.rs` and `preferences.rs` tests to validate a RepForge-style
  migration trial.
- D5 should use `persistence/native.rs` plus new IndexedDB tests to verify backend conformance.
