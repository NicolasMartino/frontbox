# Extraction Roadmap

Document Class: Roadmap
Status: Active
Date: 2026-08-25
Category: Extraction
Scope: Deliverable sequence for extracting the cache runtime, ordered so the core API is validated by a real migration before the persistence backends are ported.
Sources: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/prior-art-survey.reference.md`
Related: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/references/prior-art-survey.reference.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`

## Goal

Extract RepForge's frontend cache runtime into `frontbox`: a framework-neutral client outbox and
cache invalidation core, with storage and Dioxus adapters layered around it.

## Sequencing Principle

The migration trial is the only real test of whether the extracted API is right, so it comes
before the full SQLite and IndexedDB backend ports. Porting both durable backends first would move
roughly 2,300 lines of backend code before any real application flow validates the API.

The roadmap also keeps implementation behind explicit authorization. D0, D0a, and D1 are complete
as of 2026-08-26 and D2 as of 2026-08-27; the crate exists at the repository root. D3 onward remain
unauthorized.

### D0 - Project Knowledge

Status: Completed
Promise: The source system is copied, indexed, and summarized well enough to plan extraction.
Depends On: None
Execution Plan: Completed bootstrap and ingest work recorded in `wiki/log.md`.

Included:
- Rename the extraction project to `frontbox` after deciding the core must not be framework-named.
- Initialize the LLM Wiki project with the `library-sdk` blueprint.
- Copy RepForge source specs and cache implementation files into `raw/initial`.
- Capture source architecture, extraction boundary, roadmap, D1 plan, and decisions.
- Correct the first source-verification pass after adversarial review.

Excluded:
- Crate implementation.
- Public API stabilization.

Proof:
- `wiki/index.md` catalogs active pages.
- `wiki/specs/source-frontend-cache-architecture.spec.md` records source-backed behavior and
  known divergences.
- `wiki/references/source-test-inventory.reference.md` records the source test inventory.

Promotion Target:
- Keep the spec current as future source verification finds corrections.

Unlocks:
- D0a prior-art survey.
- D1 core runtime implementation.

### D0a - Prior-Art Survey

Status: Completed
Promise: The frontbox design is checked against established offline-first systems before any public
API is written, so known traps are caught on paper rather than after implementation.
Depends On: D0
Execution Plan: `wiki/plans/prior-art-survey.plan.md`

Included:
- Review Replicache/Zero, PowerSync, Electric/ElectricSQL, RxDB, WatermelonDB, PouchDB/CouchDB,
  and Automerge/Yjs, plus the HTTP command-queue cohort that frontbox actually belongs to:
  Workbox Background Sync, Redux Offline, TanStack Query offline mutations, and Amplify DataStore.
- Answer the eight design questions in the plan, in particular how mature systems handle malformed
  local durable data, blocked/pending outcome vocabularies, and cache fallback that must not leak
  data across users, tenants, or languages.
- Record any frontbox API change the survey implies.

Excluded:
- Benchmarking and prototypes.
- Replacing accepted decisions without direct contradictory evidence.

Proof:
- `wiki/references/prior-art-survey.reference.md` cites at least one primary source per system,
  with every URL verified reachable and dead links replaced by archived snapshots.
- The comparison table covers write model, conflict model, invalidation/scope,
  batching/retry/stall, storage, and framework integration.
- `wiki/proposals/extraction-boundary.proposal.md` records the outcome and frontbox follow-up,
  including "no changes required" where that is the finding.
- Systems deliberately not surveyed are named, so the cohort boundary is explicit.

Promotion Target:
- `wiki/proposals/extraction-boundary.proposal.md`
- Any decision page the survey contradicts or independently supports. Decisions 003, 004, 005, and
  006 carry `## Prior-Art Support` sections from this survey; none was contradicted.
- Decisions 008 and 009, written 2026-08-26, are new pages the survey produced rather than pages it
  annotated. Both settle public-API shapes before D1 can freeze them.

Unlocks:
- Decisions 008 and 009, the two API-shape questions the survey raised.
- D1 implementation authorization, subject to explicit user approval.

### D1 - Core Runtime Skeleton

Status: Completed
Promise: A framework-neutral in-memory outbox runtime can enqueue, sync, classify outcomes, retain
blocked work, dead-letter rejected work, quarantine corrupt records, and pass conformance tests.
Depends On: D0, completed D0a, and decisions 008 and 009, which settle the two public-API shapes a
published D1 could otherwise foreclose. Implementation was authorized and completed on 2026-08-26;
decision 010 was written during it.
Execution Plan: `wiki/plans/d1-core-cache-runtime.plan.md`

Included:
- Core mutation id, envelope, batch request/response, and status types. The envelope is
  non-exhaustive and constructor-built, carries a parsed JSON body and optional
  `OperationMeta`, and is stamped with the store's `ScopeKey` (decisions 008 and 009).
- Core `RemoteRejection` payload instead of RepForge `ApiError`.
- Non-exhaustive core `Error`.
- Clock abstraction.
- Outbox, dead-letter, quarantine, and transport traits.
- Required `ScopeKey` on every store constructor, enforced on every read.
- Bounded pending batches with total `(created_at, mutation_id)` ordering.
- Atomic outcome application.
- Backend-owned corrupt-record sweep for rows with no usable `MutationId`.
- No-progress reporting when a sync retains every record.
- In-memory backend with failure injection.
- Ported `persistence/mutations.rs` source tests, adjusted for intentional divergences.

Excluded:
- SQLite and IndexedDB backends.
- Dioxus adapter.
- Cache versioning.
- Retry/backoff policy beyond status classification.

Proof (all met 2026-08-26, extended 2026-08-27 after review, via `scripts/verify.sh`):
- Native build passes.
- `wasm32-unknown-unknown` build passes, with and without default features.
- In-memory conformance tests pass: 32 cases plus 1 fault-injection case, alongside 29 unit tests
  (identifier ordering, clock, error, scope, RFC 3339 rendering), 2 `!Send` proofs, 6 source-oracle
  ports from `persistence/mutations.rs`, 5 wire-format oracles from `frontend/dto.rs`, and 2
  doctests. 77 in total.
- Coverage is 90% regions / 96% lines / 93% functions, against a gated floor of 80%.
- No date library in the runtime dependency graph, asked of `cargo tree --edges normal` rather than
  of the manifest. The `client_datetime` rendering is the crate's own and is held to `chrono`'s
  bytes by a dev-dependency oracle (decision 011).
- No Dioxus or RepForge entity names in core, checked by `grep` rather than by intent.
- `Blocked` clears on the sync after its blocker is dead-lettered, proving decision 005's liveness
  argument rather than assuming it. Conformance case 19.
- Two scopes cannot observe each other's records, and a scope-mismatched record is retained rather
  than dropped, proving decision 009 rather than documenting it. Conformance case 22, run against a
  backend where both scopes share physical storage.
- The wire payload matches the source protocol byte for byte. Conformance case 27 plus the
  `frontend/dto.rs` ports in `tests/dto_oracle.rs`.

Promotion Target:
- `wiki/specs/frontbox-runtime.spec.md`, created during implementation to hold what the library
  actually does, so the source spec stays a record of the source alone.
- `wiki/decisions/010-batch-wire-format.decision.md`, written during implementation.
- `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, amended during implementation.
- `wiki/compatibility/public-dependencies.compat.md`, written 2026-08-27. The public dependencies
  are `serde_json`, `uuid`, and `serde`. Decision 010 had named `chrono` too; decision 011 removed
  it the same day by owning the rendering and keeping chrono as a dev-dependency oracle.

Unlocks:
- D2 cache versioning.
- D4 migration trial preparation.
- D5, which inherits the `StoreFactory` and `FaultInjection` conformance seams rather than having to
  invent a cross-backend test strategy.

### D2 - Cache Version And Invalidation Runtime

Status: Completed
Promise: Core can model stale entities and version reconciliation without RepForge entity names or
Dioxus signals.
Depends On: D1, and decisions 012-015, which settle the four questions D2 would otherwise have to
answer mid-implementation. Planned and implemented 2026-08-27 under explicit user authorization.
Execution Plan: `wiki/plans/d2-cache-invalidation.plan.md`

Included:
- Generic entity key and caller-supplied registry model.
- Cache version comparison and stale/reset semantics.
- Invalidation event handling.
- Reconnect version reconciliation.
- Explicit unknown-entity policy.
- Pull gating policy while local outbox mutations are pending.

Excluded:
- Server Kafka invalidation implementation.
- Dioxus listener hooks.
- RepForge entity models.

Proof (all met 2026-08-27 via `scripts/verify.sh`):
- Tests adapted from `persistence/cache.rs` and selected listener scenarios pass. Conformance cases
  35-43, plus unit tests on `compare` and the registry.
- Unknown entity behaviour is documented and tested, on both the per-event path (case 35) and the
  reconnect path (case 36). The second is the divergence that matters: the source drops unknown
  names from the server version map with no log at all.
- Pull gating behaviour has a test that fails against the copied listener behaviour. Case 41 asserts
  that asking what is stale also reports what refetching would discard; it fails against the source
  not because the source decides differently but because `handle_invalidation_event` returns `()`
  and consults no outbox. Case 43 covers the liveness half — a permanently retained record does not
  suppress staleness reporting.
- Staleness survives a reopen (case 39), which is the case a version-only store fails, and versions
  are scope-isolated against a backend where both scopes share storage (case 40).
- 96 tests, coverage 89% regions / 95% lines / 93% functions against an 80% floor.

Promotion Target:
- `wiki/specs/frontbox-runtime.spec.md`, which holds what the library actually does.
- `wiki/decisions/007-generic-entity-key-registry.decision.md`, amended by decision 013 and refined
  by implementation: the registry uses an associated `Key` type rather than a generic parameter.
- `wiki/decisions/013-unknown-entity-name.decision.md`,
  `wiki/decisions/014-pull-gating.decision.md`,
  `wiki/decisions/015-cache-version-persistence.decision.md`.

Unlocks:
- D3 Dioxus adapter.
- D4 migration trial.

### D3 - Dioxus Adapter

Status: Draft
Promise: Dioxus applications can consume core sync/cache state without core depending on Dioxus.
Depends On: D1 and D2
Execution Plan: Not created yet.

Included:
- Dioxus hooks or context providers for sync status.
- Outbox, dead-letter, and quarantine counts.
- Stale entity signals.
- SSE or websocket invalidation stream adaptation.
- Optional loop runner integration.

Excluded:
- Core storage logic.
- RepForge service code.
- Server invalidation producer code.

Proof:
- Adapter tests or example compile against Dioxus.
- Core crate remains Dioxus-free.

Promotion Target:
- Adapter API docs under `wiki/apis/` after public surface exists.

Unlocks:
- D4 RepForge migration trial.

### D4 - RepForge Migration Trial

Status: Draft
Promise: One real RepForge-style flow proves the frontbox API before durable backend ports are
completed.
Depends On: D1, D2, and D3.
Execution Plan: Not created yet.

Included:
- Migrate one thin application flow against frontbox abstractions.
- Use exercise flow for always-enqueue mutation writes, optimistic favorite projection,
  network-first reads with cache fallback, and cache validity predicates.
- Reproduce the preferences direct-dispatch-then-fallback pattern in application code, using a
  caller-supplied `MutationId`, to confirm the proposal's ruling that this stays out of core.
- Run against an in-memory or thin adapter backend before full backend ports.

Excluded:
- Full RepForge migration.
- Full SQLite/IndexedDB backend parity.
- Public release.

Proof:
- Selected source tests from `exercises.rs` and/or `preferences.rs` pass or have documented,
  intentional divergences.
- Offline behavior regresses neither pending counts nor dead-letter handling.
- API adjustments from the trial are recorded before D5.

Promotion Target:
- `wiki/proposals/extraction-boundary.proposal.md`
- API specs once created

Unlocks:
- D5 persistence backend port with less API churn.

### D5 - Persistence Backends

Status: Draft
Promise: Durable SQLite and IndexedDB storage preserve the D1/D2 semantics across native and web.
Depends On: D4
Execution Plan: Not created yet.

Included:
- SQLite backend for native targets.
- IndexedDB backend for web targets.
- Injective encoding from `ScopeKey` to a physical storage name, and storage of `mutation_id` in one
  consistent textual form — exactly `MutationId::to_string` output, under a binary collation — so
  backend ordering matches core's. Mixed forms invert pairs; see decision 009 and the D1 plan's
  Ordering Policy.
- The D1 conformance suite, run through `StoreFactory` and `FaultInjection` on both backends.
- Atomic `apply_outcomes` spanning outbox, dead-letter, and quarantine transitions.
- Bounded pending queries with deterministic ordering policy.
- Corrupt-record visibility instead of silent row loss.
- Cross-backend conformance tests.

Excluded:
- App-specific read-model stores unless they are examples.
- Backend-specific public APIs unless compatibility notes justify them.

Proof:
- Native backend tests pass.
- Browser/wasm IndexedDB tests pass.
- Shared conformance tests pass on in-memory, SQLite, and IndexedDB backends.

Promotion Target:
- Backend API docs under `wiki/apis/`
- Compatibility notes under `wiki/compatibility/`

Unlocks:
- D6 examples and release preparation.

### D6 - Documentation And Examples

Status: Draft
Promise: Developers can understand and try frontbox on native, web, and Dioxus targets.
Depends On: D5
Execution Plan: Not created yet.

Included:
- Examples showing web IndexedDB, native SQLite, mutation sync, invalidation, and Dioxus
  integration.
- README expansion with install and quickstart once a crate exists.
- API docs for core and adapters.
- Compatibility and semver notes before any public release.

Excluded:
- Marketing site.
- Unsupported framework adapters.

Proof:
- Examples build.
- README quickstart compiles.
- API and compatibility wiki folders are populated before release.

Promotion Target:
- `README.md`
- `wiki/apis/`
- `wiki/compatibility/`

Unlocks:
- First release candidate.
