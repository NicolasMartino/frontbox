# RepForge Cache Source Corpus

Document Class: Reference
Status: Sourced
Date: 2026-08-25
Category: Source Corpus
Scope: Inventory of the RepForge source files copied into `raw/` to seed this project, and what each one is good for.
Sources: `raw/initial/2026-08-25T083750Z/manifest.md`, `raw/initial/2026-08-25T083750Z/sources`
Related: `wiki/references/source-test-inventory.reference.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`

## Scope

This page records the RepForge source material used to seed the `frontbox` LLM Wiki. The project
was named `dioxus-cache` when the bundle was copied. The copied raw bundle lives under
`raw/initial/2026-08-25T083750Z`.

## Source Repository

- Path: `/Users/nicolasmartino/Documents/workout/cqrs-fullstack`
- Product: RepForge
- Frontend framework: Dioxus
- Relevant frontend crate path: `code/frontend/app`

## Copied Sources

- `docs/current/spec/08-offline-sync.spec.md`
- `docs/current/spec/18-ui-state-and-flows.spec.md`
- `code/frontend/app/src/infrastructure/persistence/`
- `code/frontend/app/src/infrastructure/transport/listener.rs`
- `code/frontend/app/src/application/services/exercises.rs`
- `code/frontend/app/src/application/services/preferences.rs`
- `code/frontend/app/src/server/cache_versions.rs`
- `code/frontend/app/src/server/invalidation/`
- `code/shared/contracts/src/frontend/`

## High-Value Files

| Copied file | Lines | Tests | Extraction value |
| --- | ---: | ---: | --- |
| `persistence/mutations.rs` | 1,371 | 12 | Mutation outbox API, sync batching, status handling, dead-letter transitions, and periodic sync loop. Highest-priority D1 source. |
| `persistence/types.rs` | 334 | 0 | Durable record shapes for outbox, dead letters, and RepForge local read models. |
| `frontend/dto.rs` | 248 | 5 | Mutation protocol DTOs, `MutationId`, and status model. |
| `persistence/native.rs` | 1,585 | 10 | SQLite schema and backend behavior; D5 source for native persistence. |
| `persistence/web.rs` | 766 | 0 | IndexedDB backend behavior and ordering; D5 source for web persistence. |
| `persistence/cache.rs` | 258 | 6 | Entity version cache and stale/reset semantics; D2 source. |
| `listener.rs` | 845 | 9 | Dioxus-specific SSE listener tied to `Signal<EntityCache>`; adapter and invalidation source. |
| `exercises.rs` | 2,426 | 27 | Largest application-flow oracle. Exercise writes always enqueue; reads are network-first with cache fallback. |
| `preferences.rs` | 1,520 | 17 | Preferences service flow; direct dispatch with fallback enqueue and immediate sync. |
| `cache_versions.rs` | 607 | 4 | Server-side per-user entity version tracking; D2 reference, not core source. |

The full copied source test inventory is tracked in
`wiki/references/source-test-inventory.reference.md`.

## Scope Notes

- `cache_versions.rs` and `server/invalidation/` are server-side. They are useful evidence for
  invalidation semantics but are not direct core frontend code.
- `listener.rs` is Dioxus adapter evidence, not core evidence.
- Domain read-model stores such as preferences, exercise drafts, templates, and sessions are useful
  example material but should not shape D1 core outbox APIs.

## Interpretation

The source corpus shows a full offline-first CQRS cache runtime, not only UI request caching.
Durable outbox semantics, HTTP-envelope mutation intents, local read models, cache invalidation
versions, SSE reconnect checks, and Dioxus signal wiring are currently colocated in the
application.

The extraction is viable because the durable mutation queue is already domain-neutral. The risky
parts are not the envelope shape; they are the source defects and policy gaps: `Blocked`
dead-lettering, unbounded batches, non-atomic outcome application, corrupt-record handling,
generic entity keys, cache validity predicates, and pull gating.
