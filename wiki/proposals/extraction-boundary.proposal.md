# Extraction Boundary Proposal

Document Class: Proposal
Status: Proposed
Date: 2026-08-25
Category: Architecture
Scope: Where to draw the line between a reusable cache/sync core, persistence backends, a Dioxus adapter, and RepForge application code.
Sources: `raw/initial/2026-08-25T083750Z/sources`, `wiki/specs/source-frontend-cache-architecture.spec.md`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/plans/prior-art-survey.plan.md`

## Problem

RepForge's frontend cache combines reusable offline-first CQRS runtime logic with app-specific
Dioxus and domain code. Extracting it as a library can reduce duplication and make the pattern
reusable, but only if the boundary avoids baking RepForge concepts, source defects, and
application-only policies into the core crate.

## Proposed Package Shape

The project is named `frontbox`, which carries no framework name, so the core can stay
framework-neutral without fighting its own title. It was originally named `dioxus-cache`; that name
implied exactly the Dioxus coupling this proposal avoids.

- `frontbox-core`: mutation protocol types, outbox runtime, cache version model, invalidation
  handling, dead-letter policy, quarantine policy, and persistence traits.
- `frontbox-sqlite`: native SQLite persistence backend.
- `frontbox-indexeddb`: web IndexedDB persistence backend.
- `frontbox-dioxus`: Dioxus signals, hooks, context providers, and SSE glue.
- `examples/repforge-style`: minimal CQRS app example modeled on RepForge flows without
  RepForge-only domain ownership.

If the first implementation milestone needs less workspace overhead, start as one crate with
modules matching those boundaries and split crates after the APIs settle.

## Extract To Core

The boundary is split by whether a behavior exists in the copied source and whether it is safe to
port unchanged.

### Extracted Source Shapes

- Mutation identifiers and the generic mutation intent envelope. The durable queue already stores
  HTTP `method`, `path`, and JSON `body`, so the persisted write model is domain-neutral.
- Outbox records with their real five fields: `mutation_id`, `method`, `path`, `body`,
  `created_at`. `mutation_id` is the idempotency key (`frontend/dto.rs:36`).
- Dead-letter records: the same five fields plus `rejected_at` and serialized remote rejection
  data.
- Server status names: `Applied`, `Duplicate`, `Rejected`, `Blocked`, and `Pending`.
- Dead-letter retention/purge: source default is 30 days, with a per-process 24h cleanup gate.
- Basic cache version comparison and invalidation event shape.
- Compile-time backend split between web IndexedDB and native SQLite.

### Generic, But New Design

- **`Blocked` retention.** Source dead-letters it, but the offline sync spec defines `Blocked` as
  skipped, not evaluated. D1 retains it. See
  `wiki/decisions/005-mutation-outcome-policy.decision.md`.
- **Bounded pending batches.** Source loads the full outbox in one batch. A reusable library needs
  `pending_batch(limit)` to limit blast radius and memory use.
- **Atomic outcome application.** Source inserts dead letters and deletes outbox records in
  separate steps. See `wiki/decisions/003-atomic-outcome-application.decision.md`.
- **Corrupt-record quarantine.** Source hides malformed rows through `filter_map(...ok())` and can
  wedge pending counts. See `wiki/decisions/006-corrupt-record-policy.decision.md`.
- **Structured error model.** Source uses `pub type StoreError = String`.
- **Generic entity registry.** RepForge's `EntityType::all()` does not generalize to arbitrary
  app strings. See `wiki/decisions/007-generic-entity-key-registry.decision.md`.
- **Cache validity predicates.** RepForge can have a cached value present but invalid for a query,
  such as non-English exercise discovery or private exercise leakage. Generic read APIs need a
  hook for that policy.
- **Pull gating policy for pending local outbox entries.** The specs call for it, but the observed
  listener refetches eagerly and never consults the outbox.
- **Deterministic tie-break ordering.** Source sorts by `created_at`, a client clock, with no
  same-millisecond tie-breaker, so the two backends can disagree on the order of simultaneous
  writes. D1 orders by `(created_at, mutation_id)`, which is total and reproducible without a schema
  change. A monotonic sequence number, which is what real causal ordering would require, stays open
  and belongs with the durable backends.
- **Retry/backoff policy.** Source has only fixed loop timing and no per-record attempt tracking.

### Not Source, Not Accepted

- A persisted auth snapshot.
- An operation key distinct from `mutation_id`.
- Priority ordering.
- Per-record retry count or last-error fields.
- Dead-letter recovery actions.

These may become features later, but they are not extraction work.

## Keep Out Of Core

- RepForge entity models such as exercises, templates, sessions, and preferences.
- RepForge command payload builders and API route knowledge.
- Dioxus `Signal` types and UI context wiring.
- Server-specific Kafka invalidation implementation.
- Application metrics names and domain-specific telemetry.
- Product-specific conflict resolution policy beyond generic hooks.
- Direct single-write dispatch and its terminal-vs-retryable error classification. See below.

## Direct Dispatch Stays In The Application

`PreferencesService` writes by trying the real endpoint first, giving up on a terminal refusal, and
falling back to the outbox with the same mutation id on any other failure
(`preferences.rs:460,476,503,533-535`), then forcing an immediate sync (`preferences.rs:557`).
`ExerciseService` does none of this and always enqueues (`exercises.rs:66-122`). The source itself is
inconsistent, which is a good sign the pattern is policy rather than mechanism.

frontbox does not put this in core for v0:

- The terminal-vs-retryable split is domain judgement. `is_terminal_preferences_write_error`
  (`preferences.rs:503`) encodes which server responses are worth queueing for this product; another
  app will draw that line differently, and core has no basis to guess.
- It is a latency optimization, not a durability requirement. Always-enqueue plus an immediate sync
  reaches the same end state, which is exactly what the preferences path does anyway after its
  fallback.
- Adding it would mean a second send path, a third status vocabulary
  (`MutationDispatchStatus`), and a terminal-refusal error variant, all to save one round trip on
  the online happy path.

Core makes the pattern expressible without owning it: enqueue accepts a caller-supplied
`MutationId`, so an application can generate an id, attempt its own direct call, and enqueue under
the same id if that call fails. Idempotency holds because `mutation_id` is the idempotency key
(`frontend/dto.rs:36`). The example crate should demonstrate this rather than the library absorbing
it.

Revisit if the D4 migration trial cannot express a real RepForge flow this way.

## Adapter Responsibilities

The Dioxus adapter should map core events into Dioxus state primitives. It can provide hooks and
providers for:

- opening the selected persistence backend,
- exposing outbox, dead-letter, and quarantine counts,
- connecting an SSE or websocket invalidation stream,
- driving periodic sync,
- applying stale/refetch events to app service callbacks.

## First API Bias

The first extracted API should favor explicit traits and typed envelopes over macros. The source
system shows useful runtime semantics, but review found several source defects. The first API must
therefore port source shapes selectively, not faithfully.

## Prior-Art Gap

Every current conclusion is based on RepForge's implementation and product specs. Before public
API stabilization, the project still needs a focused survey of Replicache/Zero, PowerSync,
ElectricSQL, RxDB, WatermelonDB, PouchDB, and Automerge/Yjs. That work is tracked in
`wiki/plans/prior-art-survey.plan.md`.

## Resolved Decisions

- **Concurrency model** - single-threaded, no `Send` bounds:
  `wiki/decisions/001-single-threaded-core.decision.md`.
- **Error model** - one core error type, no per-trait associated errors:
  `wiki/decisions/002-error-model.decision.md`.
- **Outcome application** - one transactional call, not `delete` plus `insert`:
  `wiki/decisions/003-atomic-outcome-application.decision.md`.
- **Auth representation** - fresh per send, not a persisted per-record snapshot:
  `wiki/decisions/004-transport-auth-and-offline.decision.md`.
- **Mutation status policy** - retain `Blocked`, dead-letter only `Rejected`:
  `wiki/decisions/005-mutation-outcome-policy.decision.md`.
- **Corrupt records** - quarantine instead of silent-drop:
  `wiki/decisions/006-corrupt-record-policy.decision.md`.
- **Entity keys** - caller-supplied registry:
  `wiki/decisions/007-generic-entity-key-registry.decision.md`.
- **Direct dispatch** - application-owned, not a core send path. See the section above.
- **Batch ordering** - `(created_at, mutation_id)` for D1, giving determinism without claiming
  causality: `wiki/plans/d1-core-cache-runtime.plan.md`.

## Open Decisions

- Whether cache version state should be persisted by default. Currently in-memory only.
- Whether mutation ordering needs a monotonic sequence number for genuine causal ordering. D1's
  `(created_at, mutation_id)` tie-break settles determinism only; a sequence column would be a D5
  schema change.
- Whether local read model persistence belongs in core or should be a companion trait implemented
  by each app.
- Whether to build a retry/backoff policy at all, and on what evidence.
- Whether quarantine is a distinct store or a status in a single outbox table.
