# Source Frontend Cache Architecture

Document Class: Spec
Status: Active
Date: 2026-08-25
Category: Architecture
Scope: What RepForge's frontend cache actually does today, verified against the copied source corpus, as the factual basis for extraction.
Sources: `raw/initial/2026-08-25T083750Z/sources`
Related: `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/references/source-test-inventory.reference.md`, `wiki/plans/d1-core-cache-runtime.plan.md`

## Summary

RepForge's frontend cache is an offline-first CQRS client runtime. It combines local read models,
a durable mutation outbox, dead-letter storage, server-driven invalidation versions, optimistic UI
updates, and Dioxus-specific state propagation.

This page is source truth, not future intent. When the product specs and copied implementation
disagree, the disagreement is recorded instead of smoothing it over.

## Ownership

The architecture docs define `frontend/app` as the owner of service calls, client state, storage,
API clients, SSE listeners, and sync coordination. `shared-ui` remains presentational and must not
own persistence, routing, auth, or service orchestration.

## Persistence Model

The persistence layer selects a backend at compile time:

- Web uses IndexedDB through `rexie`.
- Native uses SQLite through `rusqlite`.
- The persistence module re-exports `MutationStore`, `OutboxStore`, `DeadLetterStore`,
  `ExerciseStore`, `TemplateStore`, `SessionStore`, `PreferenceCache`, and the exercise-draft
  helpers (`persistence/mod.rs:23-50`). There is no exported `PreferencesStore`. Theme helpers are
  not in that re-export block; they live in `persistence/preferences.rs` and are reached through the
  `preferences` module path (`persistence/mod.rs:11`).

### Store Identity And User Isolation (as observed)

Each backend exposes two constructors, one isolated per user and one shared:

| Backend | Isolated | Legacy shared |
| --- | --- | --- |
| Native | `open_for_user(user_id)` opens `{user_id}.db` (`persistence/native.rs:86`) | `open()` opens `app.db` (`persistence/native.rs:70`) |
| Web | `init_db_for_user(user_id)` opens `repforge_{user_id}` (`persistence/web.rs:89`) | `init_db()` opens `repforge_app` (`persistence/web.rs:81`) |

The isolated constructors carry the stated intent "Each user gets their own database file to ensure
data isolation" and "This ensures data isolation after logout/login cycles"
(`persistence/native.rs:85`, `persistence/native.rs:62`).

Two properties of this design matter for extraction:

- **The shared constructors still exist.** Isolation holds only where every call site chose the
  per-user variant. A logout/login cycle on a device with pending outbox records can replay the
  previous user's mutations under the new user's credentials on any path that opened the shared
  database.
- **Sanitization is not injective.** Both backends map the user id through
  `replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")` (`persistence/native.rs:65`,
  `persistence/web.rs:33`). The comments note this is safe because the ids are UUIDs. For any
  non-UUID identifier it is not: two distinct ids can collapse to the same storage name.

Isolation is keyed on `user_id` alone. Tenant, schema version, and read scope play no part in local
storage identity. See `wiki/decisions/009-local-scope-identity.decision.md` for how frontbox
diverges.

### Durable Record Model (as observed)

`OutboxRecord` has exactly five fields (`persistence/types.rs:19-25`, schema confirmed at
`persistence/native.rs:119-125`):

| Field | Type | Notes |
| --- | --- | --- |
| `mutation_id` | `String` | `PRIMARY KEY`; a UUID, and the idempotency key |
| `method` | `String` | HTTP method |
| `path` | `String` | Public BFF path |
| `body` | `String` | Serialized JSON payload |
| `created_at` | `i64` | Client timestamp, epoch millis |

`DeadLetterRecord` has seven (`persistence/types.rs:68-76`, schema at
`persistence/native.rs:131-140`): the same five, plus `rejected_at: i64` and
`error: Option<String>` (a serialized `ApiError`).

RepForge-specific local read models - exercise, template, session, and preferences records - sit
alongside these in the same backends.

### Desired Additions (not in source)

Earlier drafts of this page listed these as observed fields. They do **not** exist in the source
and are recorded here as candidate requirements, to be designed deliberately rather than
"extracted":

- Retry count and per-record error state - no retry bookkeeping exists at all (see Mutation Protocol).
- Priority ordering - the outbox is strictly `created_at ASC` on native (`persistence/native.rs:253`)
  and sorted by `created_at` in memory on web (`persistence/web.rs:115`). There is no tie-breaker.
- An operation key distinct from `mutation_id`.
- A persisted auth snapshot. Auth is supplied per sync call, not stored per record; see
  `wiki/decisions/004-transport-auth-and-offline.decision.md`.
- A recovery action on dead letters. Recovery today is manual/absent; only retention is automated.
- Bounded pending batches. The source loads the whole outbox in both native and web backends
  (`persistence/mutations.rs:513-514`, `persistence/native.rs:248-269`, `persistence/web.rs:99-115`).
- Corrupt-record quarantine. Malformed records are currently skipped or hidden, not isolated.

Note that `mutation_id` already *is* the client-provided idempotency key (`frontend/dto.rs:36`), so
idempotency is not a missing field.

## Mutation Protocol

The shared frontend contract (`frontend/dto.rs`) defines:

- `MutationId` - a transparent UUID newtype, documented as the "client-provided idempotency key"
- `MutationIntentDto` - `{ mutation_id, method, path, client_datetime, body }`
- `MutationBatchRequest` - `{ mutations: Vec<MutationIntentDto> }`
- `MutationBatchResponse` - `{ results: Vec<MutationResult> }`
- `MutationResult` - `{ mutation_id, status, error: Option<ApiError> }`
- `MutationStatus`: `Applied`, `Duplicate`, `Rejected`, `Blocked`, `Pending`

Server-side invalidation mapping uses a separate `OutcomeStatus` enum with an extra `Failed`
variant (`invalidation/mapper.rs:8-19`). That status is not present in the client sync response
type and should not be conflated with `MutationStatus`.

A third status enum exists on the direct-dispatch path: `MutationDispatchStatus`, with only
`Applied`, `Duplicate`, and `Pending` (`preferences.rs:7,484-488`). It has no `Rejected` variant
because a terminal refusal arrives there as an `Err`, classified by
`is_terminal_preferences_write_error` (`preferences.rs:503`), not as a status value. So the source
carries three overlapping status vocabularies for one protocol: batch sync results, server
invalidation outcomes, and single-write dispatch results. A reusable library should expose one and
document how the others map onto it.

### The outbox is an HTTP-envelope replay queue

This is the single most important shape fact for extraction. What is persisted and replayed is a
**raw HTTP envelope** - `method` + `path` + JSON `body` - not a typed domain mutation. Typing
exists only in the helper constructors that *build* an envelope, e.g. `enqueue_post_exercise`
produces `POST /api/v1/exercises` (`persistence/mutations.rs:416`), and
`patch_translation_proposal_intent` produces `PATCH /api/v1/translation-proposals/{id}`
(`persistence/mutations.rs:221`).

The consequence is favourable: the durable core is already domain-agnostic. RepForge coupling
lives in the typed enqueue helpers, which are a thin layer above the queue and belong in
application code or an example - not in the library.

### Status application semantics (observed, not all accepted for extraction)

`apply_sync_results` (`persistence/mutations.rs:246-305`) currently does the following per result:

| Status | Observed source effect | Extraction policy |
| --- | --- | --- |
| `Applied` | marked for deletion from the outbox | delete |
| `Duplicate` | marked for deletion from the outbox | delete |
| `Rejected` | dead letter inserted, then marked for deletion | dead letter |
| `Blocked` | dead letter inserted, then marked for deletion | retain for D1 |
| `Pending` | no action; record stays queued | retain |

The extraction deliberately diverges for `Blocked`. The product sync spec defines `Blocked` as a
mutation skipped because an earlier mutation in the same ordered sequence failed terminally
(`08-offline-sync.spec.md:89-98`). Since it was not evaluated, D1 keeps it queued instead of
moving it to dead letters. See `wiki/decisions/005-mutation-outcome-policy.decision.md`.

Deletion happens afterwards in `sync`, in a separate unchecked loop -
`let _ = OutboxStore::delete(...)` (`persistence/mutations.rs:658`). See Known Gaps.

### Poison-record behavior

The source has no recovery path for malformed local records:

- `sync` silently drops records that fail `to_intent()` (`persistence/mutations.rs:643`).
- `apply_sync_results` silently omits records whose `mutation_id` cannot parse
  (`persistence/mutations.rs:252-255`).
- A result for an unknown mutation is logged but not repaired (`persistence/mutations.rs:293-298`).
- Native reads hide per-row decode errors with `filter_map(|r| r.ok())` for outbox and dead letters
  (`persistence/native.rs:269,382`).
- Web reads hide deserialization failures with `filter_map(...ok())`
  (`persistence/web.rs:112,229`).

For an offline-first library this is not acceptable background noise. A corrupt record can remain
in storage and keep counts wedged forever. See `wiki/decisions/006-corrupt-record-policy.decision.md`.

### No retry or backoff exists

There is no per-record retry count, no attempt tracking, and no exponential backoff. The only
timing behaviour is two fixed intervals in `start_sync_loop` (`persistence/mutations.rs:731-748`):
5s between syncs normally, 30s when the last sync errored. Any retry policy in the library is new
design, not extraction.

The source loop sleeps before the first sync attempt (`persistence/mutations.rs:741-748`), so
startup sync can wait 5s. That contradicts the product UI spec's "Immediately push outbox" wording
for generic domain mutations (`18-ui-state-and-flows.spec.md:171`), though the preferences fallback
path explicitly calls `sync()` after enqueue (`preferences.rs:549-557`).

### Dead-letter retention

Dead letters are purged on a 30-day retention window (`persistence/mutations.rs:47,570-579`). The
sync loop attempts cleanup when `now_ms - last_dead_letter_cleanup_ms >= 24h`
(`persistence/mutations.rs:48,739,754-772`). Because the in-memory gate starts at `0`, the "at most
once per 24h" behavior is per process and cleanup runs on the first loop after every app launch.

The purge itself is N+1: it loads every dead letter into memory and then issues one delete per
expired record (`persistence/mutations.rs:573-579`), and it derives its cutoff from `Utc::now()`
inside the function (`persistence/mutations.rs:572`). A library should push both the filtering and
the clock to the boundary, which is why the D1 trait is `purge_older_than(cutoff_ms) -> usize`.

### Concurrency model

The extracted core targets single-threaded frontends and must avoid `Send` bounds. The observed
source is mixed:

- `MutationStore` wraps `Rc<MutationStoreInner>` (`persistence/mutations.rs:242`), so the store
  handle is `!Send`.
- Web `rexie`/IndexedDB futures are `!Send`.
- Native `Database` is cloneable and wraps `Arc<tokio::sync::Mutex<rusqlite::Connection>>`
  (`persistence/native.rs:19-21`), so the native backend alone is not evidence for a
  single-threaded-only design.

See `wiki/decisions/001-single-threaded-core.decision.md`.

## Cache Versioning

`EntityCache` tracks per-entity freshness through server versions:

- `Exercise`
- `Session`
- `Template`
- `UserPreferences`

Server-side `CacheVersionRepo` maintains per-user entity versions and invalidates entities after
mutation outcomes (`cache_versions.rs:87`). The frontend listener compares server versions against
local versions after SSE reconnects and marks stale entities for refetch when server versions are
newer.

The entity-key model is not directly extractable as generic code. The client and server each have a
closed RepForge `EntityType` enum (`persistence/cache.rs:14-40`, `cache_versions.rs:17-43`), while
the wire SSE payload uses `entity: String` (`frontend/sse.rs:18`). `EntityCache::mark_all_stale`
depends on `EntityType::all()` (`persistence/cache.rs:183-186`). A generic library needs a caller
supplied key registry. See `wiki/decisions/007-generic-entity-key-registry.decision.md`.

Unknown entity names are ignored by the listener after logging (`listener.rs:465-505`). Reconnect
version checks similarly drop unknown names when parsing the server map (`listener.rs:414`). Missing
versions passed as `None` to `update_version` become `0` (`persistence/cache.rs:91-97`), which can
look like an anomaly for a client with a non-zero local version.

## Dioxus Integration

The SSE listener is currently Dioxus-specific. It receives invalidation events, mutates a
`Signal<EntityCache>`, and triggers eager refetch callbacks for stale entities. The cache core
should not depend on Dioxus signals; this belongs in an adapter.

## Application Flows

`ExerciseService` demonstrates cache-backed reads and optimistic write projection, but not direct
write dispatch:

- Exercise writes enqueue mutation intents directly (`exercises.rs:66-122`, `exercises.rs:136-182`).
- Favorite toggles optimistically update local storage, then enqueue a favorite/unfavorite mutation
  (`exercises.rs:188-229`).
- `load_library` reads local cache first, but returns the remote result on successful refresh and
  uses cached data only when auth is empty or refresh fails (`exercises.rs:246-283`). This is
  network-first with cache fallback, not cache-first.
- Cached exercise fallback is allowed only for safe English `mine` and `favorites` queries
  (`exercises.rs:600-609`). Tests explicitly guard against leaking private cached exercises into
  discover fallback (`exercises.rs:1692`) and against non-English fallback
  (`exercises.rs:1880`). This is a cache-key and validity-predicate requirement for any reusable
  read-model API.

`PreferencesService` demonstrates a different write policy:

- Bootstrap is cache-then-server, with server refresh gated by cache version and stale state.
- Local preference changes attempt direct API dispatch first (`preferences.rs:476`).
- Non-terminal dispatch failure falls back to the local outbox with the same mutation id
  (`preferences.rs:522-535`).
- After fallback enqueue, preferences immediately attempt `mutation_store.sync(&auth_header)`
  (`preferences.rs:549-557`).

## Known Gaps

- The offline sync spec describes `GET /api/v1/sync/state`, but the observed listener performs
  typed endpoint refetches after invalidation rather than a single state endpoint.
- The spec says pull/refetch work should be delayed while the outbox has pending local mutations,
  but the observed listener eagerly refetches after invalidation and never consults the outbox.
- The outbox processes by timestamp only; the spec's stronger FIFO and causality language should be
  revisited during extraction. Same-millisecond ties are not deterministic.
- `EntityCache` version state appears in memory only (a `HashMap`, `persistence/cache.rs:69`);
  persistence should be decided for a reusable library.
- Domain-specific stores and typed enqueue helpers are coupled to RepForge entities.
- Current error values are mostly strings - `pub type StoreError = String`
  (`persistence/types.rs:12`); a library should expose structured errors.
- **The source dead-letter transition is not atomic.** The dead letter is inserted inside
  `apply_sync_results` (`persistence/mutations.rs:281`), but the outbox delete happens later in a
  separate loop that discards its result (`persistence/mutations.rs:658`). Both target backends
  upsert dead letters (`persistence/native.rs:390`, `persistence/web.rs:246`), so the failure mode
  is not a primary-key collision. The real risk is a record that is both pending and dead-lettered,
  is resent on each sync, rewrites the dead letter, and can keep resetting `rejected_at` so the
  retention window never expires. See `wiki/decisions/003-atomic-outcome-application.decision.md`.
- **Auth is a per-call parameter, not stored state** - `sync(&self, auth_header: &str)` forwards
  straight to `push_mutations` (`persistence/mutations.rs:612,653`). Any extracted transport trait
  must carry it.
- **Offline is distinguished from failure** - `SyncStatus::Offline` means "stay queued, no
  backoff" while `SyncStatus::Error` drives the 30s interval (`persistence/mutations.rs:61,334-345,
  731-748`). A transport returning an undifferentiated error would lose this.

## Extraction Implication

The library should separate cache mechanics from application policy.

**Generic, and present in source** - outbox persistence, raw HTTP mutation envelopes, response
status names, dead-letter retention, basic version comparison, invalidation event shape, and the
compile-time backend split.

**Generic, but new design** - bounded batching, deterministic tie-breaks, `Blocked` retention,
corrupt-record quarantine, atomic outcome application, structured errors, cache validity
predicates, generic entity registries, retry/backoff policy, and pull gating while local mutations
are pending.

**RepForge-specific, and out of scope** - entity models, typed enqueue helpers, API route knowledge,
auth context, server Kafka fan-out, application metrics/probes, and Dioxus signals. These move to
adapters or examples.

Because the persisted envelope is already domain-neutral HTTP, the boundary falls naturally: the
queue, the sync loop, and the status machine are reusable after correcting the source defects; only
the layer that constructs envelopes is domain-bound.

## D1 Extraction Outcome

Added 2026-08-26, after D1 was implemented. This page continues to record what the **source** does;
this section records which of those behaviours the extracted library kept, corrected, or dropped,
and which conformance case proves it. Nothing above was rewritten — the source facts stand as
observed.

| Source behaviour | frontbox D1 | Proof |
| --- | --- | --- |
| `Blocked` dead-lettered with `Rejected` (`mutations.rs:270`) | Retained; only `Rejected` dead-letters | Cases 4, 19; `blocked_is_retained_where_the_source_dead_letters_it` |
| Whole outbox loaded as one batch (`mutations.rs:513-514`) | Bounded `pending_batch(limit)` | Cases 10, 21 |
| Dead-letter insert then separate discarded delete (`mutations.rs:281,658`) | One atomic `apply_outcomes` | Case 9 |
| Malformed rows hidden by `filter_map(...ok())` (`native.rs:269`, `web.rs:112`) | Quarantine plus a backend-owned `sweep_corrupt` | Cases 15, 18, 26 |
| `pub type StoreError = String` (`types.rs:12`) | One non-exhaustive structured `Error` | Compile-time |
| `Utc::now()` inside record constructors (`types.rs:88`) | Injected `Clock`; `chrono` without its `clock` feature, so `Utc::now()` does not compile | Case 13 |
| `ORDER BY created_at` with no tie-break (`native.rs:253`, `web.rs:115`) | Total `(created_at, mutation_id)` order | Cases 11, 12 |
| Scoped and unscoped constructors side by side (`native.rs:70`/`:86`, `web.rs:81`/`:89`) | Required `ScopeKey`, no unscoped constructor, enforced on every read, retained on mismatch | Cases 22, 23, 29 |
| Body stored as pre-serialized text (`types.rs:23`) | Parsed `serde_json::Value`, so a malformed body is unrepresentable | Case 25 |
| Pending count cached in an `AtomicUsize` needing explicit refresh | Read from storage per call | `pending_count_needs_no_refresh` |
| Sticky `SyncStatus` booleans read in priority order (`mutations.rs:61,334-345`) | Per-pass `SyncReport`, with an explicit no-progress signal | Cases 7, 20; `each_pass_reports_what_it_did` |
| `MutationIntentDto` wire shape (`dto.rs:96-108`) | Preserved byte for byte | Case 27; the `dto.rs` ports in `tests/source_oracle.rs` |

Two source behaviours were deliberately **not** carried and remain out of scope: typed per-route
enqueue helpers, and direct-dispatch-with-fallback, which stays application-owned.

One observation about the source's own tests belongs here. `sync_status_reports_priority_order`
sets up its scenario by writing directly to `store.inner.is_offline` and `store.inner.is_error` —
private atomics. A status that can only be arranged by reaching past the public API is a status the
public API does not expose, which is why `SyncStatus` did not transfer as a shape.
