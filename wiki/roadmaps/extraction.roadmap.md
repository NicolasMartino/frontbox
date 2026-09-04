# Extraction Roadmap

Document Class: Roadmap
Status: Active
Date: 2026-08-25 (D4c added 2026-08-30; D4d added 2026-08-31; sequencing principle corrected and D5's audit findings recorded 2026-08-31)
Category: Extraction
Scope: Deliverable sequence for extracting the cache runtime, ordered so the core API is validated by a real migration before the persistence backends are ported.
Sources: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/prior-art-survey.reference.md`
Related: `wiki/proposals/extraction-boundary.proposal.md`, `wiki/references/prior-art-survey.reference.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/plans/d4c-multi-platform-trial.plan.md`, `wiki/plans/d4d-multi-domain-trial.plan.md`, `wiki/proposals/invalidation-delivery.proposal.md`, `wiki/decisions/037-multi-service-routing.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`

## Goal

Extract RepForge's frontend cache runtime into `frontbox`: a framework-neutral client outbox and
cache invalidation core, with storage and Dioxus adapters layered around it.

## Sequencing Principle

The migration trial is the only real test of whether the extracted API is right, so it comes
before the full SQLite and IndexedDB backend ports. Porting both durable backends first would move
roughly 2,300 lines of backend code before any real application flow validates the API.

The roadmap also keeps implementation behind explicit authorization, and **that is a statement
about what has been authorized, not a permanent ranking**. It said "D4b and D5 onward remain
unauthorized" for two days after both were built, which is the failure mode of writing a gate into
prose: the sentence outlives the state it described. The per-deliverable Status lines below are the
authority, and this paragraph defers to them.

As of 2026-08-31: D0 through D5 are built, **D5's last owed proof line was met the same day** — the
two-realm drain is observed rather than argued — and **D4d is built too**. D6 has no plan. Every
deliverable with a plan is now built, and none is carrying an unwitnessed promise. The crate is the root package of a workspace whose other members
are `crates/frontbox-dioxus`, `crates/frontbox-sqlite`, `crates/frontbox-indexeddb` and the four
trial crates under `examples/`; `src/` deliberately did not move.

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

### D3a - Drain Loop

Status: Completed 2026-08-29
Promise: A caller can drain the outbox to completion in one call, with one aggregated report,
without a framework, a timer, or a fifth dependency.
Depends On: D1 and D2
Execution Plan: `wiki/plans/d3-drain-and-dioxus-adapter.plan.md`

Split out of D3 during planning. The loop two decisions independently asked for turned out to need
nothing a framework supplies, and putting it in an adapter would have made it unreachable by the
conformance suite and unavailable to a native consumer
(`wiki/decisions/028-drain-loop-boundary.decision.md`).

Included:
- `SyncRunner::drain`, running passes back to back and returning one `DrainReport`.
- A termination rule that stops on the first pass making no progress rather than on `Idle`, because
  a fruitless pass sends the identical request and, worse, spends a retention bound in milliseconds
  (`wiki/decisions/029-drain-termination.decision.md`).
- `Serialize` on the seven report types, one way only
  (`wiki/decisions/030-serializable-reports.decision.md`).
- Four conformance cases, 57-59 in the outbox suite and 60 in the single-flight profile.

Excluded:
- Any cadence, sleep, or clock. Those are D3b's.
- Emission. Decision 020 keeps the returned value primary; `tracing` packaging is still open.

Proof (all met 2026-08-29 via `scripts/verify.sh`):
- 59 conformance cases pass; coverage 89.03% regions / 96.18% lines / 93.63% functions on
  `-p frontbox` against an 80% floor.
- Case 58 asserts a send count of **one** against a wedged head behind a retention bound of three.
  It fails against the reading decision 018 originally implied, which is why it exists.
- Case 60 drains five records at `batch_limit = 1` in one call, which is decision 018's ~20 s
  against ~17 min gap made observable rather than argued.
- The `--no-default-features` wasm build stays green, proving the loop added no randomness and no
  timer.

Promotion Target:
- `wiki/specs/frontbox-runtime.spec.md`, which gains a divergence row and a no-source-position row.
- Decisions 028, 029, 030. Decisions 018 and 020 amended — 018 because it credited decision 017 with
  preventing a spin that 017 does not prevent.

Unlocks:
- D3b, and D4's example, which needs something an application can hold.

### D3b - Dioxus Adapter

Status: Completed 2026-08-29; reworked 2026-08-30 for register entries 14, 15 and 16; SSE glue
deliberately unbuilt
Promise: Dioxus applications can consume core sync/cache state without core depending on Dioxus.
Depends On: D3a
Execution Plan: `wiki/plans/d3-drain-and-dioxus-adapter.plan.md`

Included:
- `crates/frontbox-dioxus`: a context provider over `Rc<SyncRunner>`, outbox/dead-letter/quarantine
  counts as signals, and an `InvalidationState` carrying stale and unknown entity names.
  **The provider and `InvalidationState` were removed 2026-09-01**; what remains is the cadence, the
  counts and the wait. Neither ever acquired a consumer: entry 14 replaced the provider for the
  reason it records, and D4d built its own inbound seam in the trial because decision 038 directed
  it to — so the adapter's invalidation surface was superseded in practice on the day the first
  application needed one.
- `SyncCadence` and an injected `Sleeper`. **This is the adapter's real content**: a drain returns as
  soon as it stops draining, so how often a caller drains is the only thing left deciding how hard a
  client pushes, and that is a policy about a wall clock core does not have.
- Added 2026-08-30, as one change because the register's note 6 argued three releases for one
  audience was the wrong shape:
  - **`use_sync_loop` takes closures** — `SyncStep` and `CountsStep` — rather than a
    `FrontboxHandle` to call `drain` on. `use_sync_loop_on` kept the old behaviour for an
    application that lets Dioxus own its runner; **that class acquired no member and both were
    removed 2026-09-01**. Entry 14.
  - **A `web` feature, default off**, carrying `WebClock` over `Date.now()` and `use_bfcache_wake`.
    Desktop Dioxus takes no `js-sys` and no `web-sys`, and the feature-less gates prove it. Entries
    15 and 16.
  - **`Wake` and `Sleeper::wakeable`**, public without the feature: a frozen page's timers do not
    fire, so a restored tab would otherwise serve out a wait chosen before it was frozen.

Excluded:
- Core storage logic and the drain loop itself, both D3a's.
- RepForge service code and server invalidation producer code.
- **SSE or websocket stream adaptation, deliberately.** It needs an HTTP client and a reconnect
  policy, both the application's, and the stream *type* would be a dependency choice this crate has
  no business making for a caller. The seam was a method — an application's own handler calling
  `InvalidationState::apply` — and D4d, which is the example this line hoped would shape it, built
  the seam in `examples/todo-core` instead and left the adapter untouched, per decision 038.

Proof:
- Compiles for native and `wasm32-unknown-unknown`, clippy-clean at `-D warnings`, with and without
  the `web` feature. That was the whole gate until 2026-08-30 and the roadmap should not have
  claimed more: the conformance suite cannot reach this crate, because there is no headless Dioxus
  runtime here.
- **Five unit tests on the wake**, which is the first thing in this crate that could have any: a
  latch and a hand-written race are plain Rust, and a counting waker proves the wake *reschedules*
  rather than merely setting a flag. The `pageshow` listener still needs a browser.
- **`examples/todo-app` uses `use_sync_loop`**, which is the only proof entry 14 would accept: a
  hook that compiles proves its signature is well-formed and nothing else.
- **The bfcache wake measured in Chromium**: with a write queued and the app back online, the queue
  drained 38 ms after a `pageshow` restore event against 15 015 ms without one — `offline_ms` served
  out in full. The event is synthesised because an automated Chromium will not bfcache a page;
  everything downstream of it is the real path.
- `cargo tree -p frontbox --edges normal --all-features` mentions no Dioxus, with the adapter in the
  same workspace. That is what keeps the core dependency claim honest.

Promotion Target:
- `wiki/compatibility/dioxus-adapter.compat.md`, which records the first `0.x` crate in any frontbox
  public surface.

Unlocks:
- D4 RepForge migration trial.

### D4a - Migration Trial, API Half

Status: Completed 2026-08-29
Promise: One real application flow proves the frontbox API expresses real work without churn, before
the durable backend ports begin.
Depends On: D1, D2, D3a, D3b
Execution Plan: `wiki/plans/d4a-offline-todo-trial.plan.md`

Split out of D4 by `wiki/proposals/offline-todo-trial.proposal.md`. D4a is D4 as already written —
the roadmap's own D4 includes "Run against an in-memory or thin adapter backend before full backend
ports". What the split adds is the refusal to present that run as the offline demonstration, since
against an in-memory store it is not one.

Included:
- `examples/todo-server`: axum over sqlx/SQLite, speaking the batch wire format. **It does not
  depend on `frontbox`.** Sharing the client's types would make the two ends unable to disagree, so
  the wire shapes are hand-written from the spec and the trial's tests are the oracle.
- `examples/todo-core`: the application. Always-enqueue writes, an optimistic projection, an
  application-owned read model, and the direct-dispatch-then-fallback pattern under a
  caller-supplied `MutationId` — the pattern `wiki/proposals/extraction-boundary.proposal.md` bet
  would stay out of core. **No Dioxus**, so that an adapter leak would be visible.
- `examples/todo-app`: the Dioxus web UI over `todo-core`. Built, and it composed with neither of
  D3b's hooks — register entries 14 and 15.

Excluded:
- Durability. Storage is in memory on both sides of the seam, which is D4b's subject.
- Cache invalidation and row-level staleness. Not exercised; recorded as untested rather than fine.
- Preconditions against a batch. The todo flow sets none, so the tension did not arise.

Proof (all met 2026-08-29 via `scripts/verify.sh`):
- **No line of `src/` or `crates/` changed.** That is the roadmap's question, answered as a fact
  about a commit rather than as a judgment.
- Ten observation tests pass over real HTTP against an in-process server, gated as a **primary**
  gate because they are the only end-to-end evidence the project has.
- Observation 3 is asserted in the *negative*: an in-memory queue does not survive a restart. The
  gap is proven rather than claimed, and D4b's deliverable is that assertion inverting.

Promotion Target:
- `wiki/plans/d4a-offline-todo-trial.plan.md`, which records the trial findings and the
  post-review regressions.
- `wiki/references/open-decisions.reference.md` entries **12** through **15**, opened by the trial.
  That is what a migration trial is for: they are the first questions raised by an application
  holding the API rather than by a conformance case that already knows the answer.

Unlocks:
- D5, with D4a findings as input rather than a blank schema.

### D4b - Migration Trial, Durability Half

Status: **Built 2026-08-30**
Promise: The same application, unchanged above the storage seam, demonstrates that the queue
survives a restart on both native and web.
Depends On: D4a and D5
Execution Plan: Not created yet.

Included:
- Repoint `examples/todo-core` at SQLite on desktop and IndexedDB on web, at the single construction
  site where the backend is named.
- Invert observation 3: same scope, new process, work still queued.
- Re-run the other nine observations against both backends.
- **A reload shows the user's todos with no network** (user's choice, 2026-08-30) — via decision
  032's opaque row store, which supersedes 023 in part. `todo-core` writes through frontbox's row
  store rather than building its own; see the D5 plan's Track D as rewritten.
- **Merge rather than replace on hydration** — settled by decision 032 as the row store's skip
  rule, built and exercised here. Startup's three sources — local rows, durable queue, server —
  stop being the application's to reconcile by hand.

Excluded:
- Any change to the application above the storage seam that the backend swap *forces*. If one is
  needed, that is the finding.

Proof Note (2026-08-30) — the read model complicates "the diff is the result":
- D4b's proof was that the diff touches only the construction site. Adding application-side
  persistence is a deliberate change above the seam, so **the diff must now be read in two parts**:
  what the backend swap forced, and what was chosen. Only the first half is evidence about the
  seam.
- Keep them in separate commits, or the proof this deliverable exists to produce is unreadable.

Proof:
- **The diff is the result.** Either it touches only the construction site, or the seam is in the
  wrong place — and the second outcome is worth more than the demo, since it is cheap to fix while
  the crate is `publish = false`.
- Observation 3 inverted, on both backends.

Promotion Target:
- Whatever the diff forces. If it forces nothing, that is the compatibility claim D5 exists to make.

Unlocks:
- D6 examples and release preparation.

### D4c - The Same Application On Four Platforms

Status: **Built 2026-08-30**
Promise: The trial application runs unchanged on web, macOS desktop, iOS and Android; whatever the
platform seam has to contain is the finding.
Depends On: D4b and D5
Execution Plan: `wiki/plans/d4c-multi-platform-trial.plan.md`

Included:
- One UI crate targeting four platforms, with **nothing below `main.rs` conditionally compiled**.
- A single `platform` module holding everything a platform decides: clock, timer, storage location,
  server address.
- Android's per-application data directory over JNI, because no environment variable answers it.
- Mobile-friendly layout: 16px inputs, safe-area insets, coarse-pointer touch targets, single column.
- A desktop build gate in `scripts/verify.sh`, so the native half of the platform seam cannot rot.

Excluded:
- Any change to `frontbox` or `frontbox-dioxus`. **Nothing in either crate was touched**, which is
  the result: `Clock` (decision 002) and `Sleeper` already covered two platforms neither was written
  for.
- A fix for finding 3. It argues for an adapter change and does not take it; register entry 20 is
  where the evidence sits.

Proof: All four platforms *run*, evidenced by storage each one wrote — see the plan. `rusqlite`
cross-compiled to `aarch64-linux-android` and `aarch64-apple-ios-sim` with no configuration beyond
`ANDROID_NDK_HOME`, which was supposed to be this exercise's main risk and was not.

Finding: **The drain loop dies on Dioxus desktop.** `use_sync_loop` runs on the VirtualDom's task
executor, and on desktop that executor stops being polled after a few seconds — 3 ticks against a
bare `tokio::spawn`'s 18 in the same process over the same 95 seconds. Web, iOS and Android are
unaffected and were tested against the idle window specifically. The workaround the measurement
suggests is closed by decision 001: `tokio::spawn` needs `Send` and no store here is. Register
entry 20.

### D4d - Two Domains, One Queue

Status: **Built 2026-08-31**
Promise: A second domain server makes two paid-for guarantees testable — cross-service enqueue order
(decision 036) and multi-source invalidation (decision 038) — and the invalidation runtime gets its
first consumer.
Depends On: D4c and D5
Execution Plan: `wiki/plans/d4d-multi-domain-trial.plan.md`

Included:
- `examples/user-server`, CRUD plus a versions endpoint, **not** depending on `frontbox` for the
  same oracle reason `todo-server` does not.
- A `user_id` on todos, validated across services — a fixture that makes ordering observable, and
  labelled as one.
- A routing `SyncTransport` per decision 037: one outbox, one scope, batch split by destination.
- An `InvalidationSource` local to `todo-core`, with two implementations (manual, polling) and a
  third later (SSE). Decision 038 keeps all of it out of core until the bar there is met.
- Focus gating and a per-entity **staleness budget**, both in the trial.

Excluded:
- Any change to `frontbox` or `frontbox-dioxus`. If the trial forces one, that is the finding.
- SSE, a server-side invalidation outbox, auth, the duplicate operation, and mobile lifecycle
  gating. Each is named in the plan with the reason, so none reads as an oversight.

Proof (all met 2026-08-31; **ten observations** in `examples/todo-core/tests/multi_domain/main.rs`):
- **Cross-service order holds under one scope, and the sabotage shows it being lost.** Observation
  2 asserts on the *servers'* state: every todo landed owned by the user record that preceded it.
  Observation 3 runs the same flow under one scope per service, and the todo server answers `404`
  for a user it cannot find. **The classification of that is the transport's, not core's** —
  decision 019 opens by saying core does not classify HTTP and places on a transport the obligation
  that a missing prerequisite is transient — so the trial's transport maps it to
  `MutationStatus::Blocked`, and the record is retained, counted in `counts.blocked`, raises no
  anomaly, and **heals on the retry after the user record lands**. Without the sabotage half, the
  good path would prove a drain worked rather than that order held.
- **`InvalidationRunner::apply` is called by an application for the first time**, and the
  per-source half is proven with it: observation 8 stops the user service, marks its entity stale,
  and leaves the todo entity alone. That observation is unwritable with one entity.
- **No `src/` or `crates/*/src/` file changed.** A fact about the diff: every change is under
  `examples/`, plus the workspace member list, `scripts/verify.sh` and `docker-compose.yml`.

Findings (six; see the plan's `## Outcome`). The two that reach beyond the trial:
- **A partial batch burns decision 017's retention bound twice per drain.** A drain stops on the
  first pass making no progress (029); one service draining *is* progress, so the loop runs again
  and re-sends to the still-silent one. **Decisions 037 and 029 interact**, and neither anticipated
  it — a client whose services fail independently reaches the bound in half the drains a
  single-service client would.
- **`InvalidationSource` cannot be `dyn`.** An `async fn` in a trait is not dyn-compatible, and this
  is the first seam where one application wants several implementations *at once*. Core has the
  same constraint on `SyncTransport`, `OutboxStore` and `CacheVersionStore` and has never paid for
  it, because an application has one of each.

Decision 038's promotion bar: **two of four criteria closed, the other two answered.** No cursor is
needed, which removes the strongest argument for core owning the inbound path; and `SyncCadence` did
not generalise to polling. The recommendation is **not to promote yet**, on those grounds rather
than on waiting for SSE.

### D5 - Persistence Backends

Status: **Complete 2026-08-31.** Both backends, the row store, decision 031's cross-realm exclusion
*and* `CacheVersionStore` are done and green, and the fourth Proof line — the one this field spent a
day owing — is now met.

**The two-realm drain is observed.** `crates/frontbox-indexeddb/tests/cross_realm.rs` puts a second
realm on the origin's lock manager and asserts both directions: a realm that finds the scope locked
reports `AlreadyRunning`, sends nothing, and **leaves its queue intact**; and a pass in flight
refuses the other realm that same lock. Composed, those two give the guarantee for two frontbox
realms without assuming either half. Three sabotage runs — a no-op claim, a drifted lock name, a
leaked lease — each turned a test red, so the fixture is known to be able to fail. See decision
031's `## The Two-Realm Observation`.

**This status field overstated itself twice, and what fixes that is not prose.** Both times the
claim was checkable and unchecked. What closes the loop is that `scripts/verify.sh` now *runs* the
IndexedDB browser suite when `CHROMEDRIVER` names a driver, and prints `1 GATE(S) SKIPPED` when it
does not — so the evidence is a gate, and its absence is announced rather than assumed.

This line said "Built" once before the cache half existed, and that was wrong. The sequence is
worth keeping because it is the argument for how the factory traits are split: **the SQLite test
file carried a comment saying the cache suite was absent**, so the overstatement was discoverable
by reading the test file rather than by trusting the roadmap. A backend that skipped the suite at
runtime instead would have reported a full pass and left nothing to find.

**A whole-worktree review on 2026-08-31 then produced three more**, all of the same kind — an API
complete for the backend inside the crate and not for one outside it, or a rule stated in prose that
two implementors read differently. `pending_batch` filtered corrupt rows out of a fixed window and
could come back empty while work was queued; `DeadLetterStore::list` stated no order and three
backends chose three; and a dropped `IdbTransaction` commits where a dropped `rusqlite::Transaction`
rolls back. Cases 68 and 69 close the first two and the browser suite covers the third. **None of
them was reachable by any gate that existed**, which is why two more now do. See `wiki/log.md`,
2026-08-31.

**The cache half also produced a finding of its own**, in the shape this project keeps meeting: an
API that is complete for the backend inside the crate and not for one outside it. `EntityState` is
`#[non_exhaustive]` with three constructors covering three of its four states; the fourth — *stale
with no version* — is what `InvalidationRunner::mark_all_stale` writes for every registered entity
no invalidation has ever named, and nothing outside the crate could reconstruct it. Both durable
backends had to. `EntityState::from_parts` and case 67 close it (see
`wiki/decisions/015-cache-version-persistence.decision.md` and the case's own comment).
Promise: Durable SQLite and IndexedDB storage preserve the D1/D2 semantics across native and web.
Depends On: D4a
Execution Plan: `wiki/plans/d5-persistence-backends.plan.md` (2026-08-30). Scope confirmed with the
user the same day: **both backends together**, not split web-first, so cross-backend conformance
catches divergence rather than deferring it.

Included:
- SQLite backend for native targets.
- IndexedDB backend for web targets.
- Injective encoding from `ScopeKey` to a physical storage name, and storage of `mutation_id` in one
  consistent textual form — exactly `MutationId::to_string` output, under a binary collation — so
  backend ordering matches core's. Mixed forms invert pairs; see decision 009 and the D1 plan's
  Ordering Policy.
- The D1 conformance suite, run through `StoreFactory` and `FaultInjection` on both backends.
- Atomic `apply_outcomes` spanning outbox, dead-letter, and quarantine transitions.
- Bounded pending queries with a **faithful** ordering policy, not merely a deterministic one: a
  durable, globally monotonic `seq` assigned inside the enqueue transaction and used as the primary
  sort key (decision 016). `(created_at, mutation_id)` survived as a tiebreak for rows written
  before the column existed; **removed 2026-09-01**, since `seq` is unique in all three backends and
  no store predating the column has ever existed.
- A durable `attempts` count on the outbox record, incremented when a sent record stays queued and
  carried onto the dead letter at the bound (decision 017). The in-memory backend needs both columns
  first, so conformance can assert them before a durable backend exists.
- Corrupt-record visibility instead of silent row loss.
- **The opaque row store** (decision 032): `(scope, entity, row_id) → (blob, version, stale,
  schema)` on all three backends, the enqueue-time row binding on `MutationIntent`, and the
  library-enforced hydration merge. Blobs are never parsed; the application keeps serialization,
  meaning, and its in-memory query layer.
- Cross-backend conformance tests, including the single-flight profile — an *adapted* run of the
  outbox cases at `batch_limit = 1`, where a wedged head and a violated order are observable at all
  (decision 018). Adapted, not repeated: four cases assert multi-record batch semantics that limit 1
  removes, and a profile that only parameterizes the shared helper leaves the cases with their own
  runner inert. Budget it as new fixtures, not as a third macro emission.
- Cross-realm single-flight, **the half core cannot hold**. At most one drain in flight per scope
  across every handle on it (decision 031); core now claims the scope in-realm and case 61 asserts
  it, but two tabs are two wasm instances and no process-local claim reaches the second one. The
  mechanism is the backend's — Web Locks for IndexedDB, and a native backend with one process may
  need nothing. **The conformance suite cannot ask for this**: it drives a backend through one
  process, so case 61 passes for a durable backend that has done nothing. What D5 owes instead is a
  two-realm fixture — two tabs, or a page and a worker, over one database.
- A **legible** durable schema, which is now two arguments for one rule rather than one. Storing
  `mutation_id` as its `MutationId::to_string` text under a binary collation is already required for
  ordering (decision 009, above); it is also what makes the outbox readable in a browser's storage
  inspector, which is the whole observability story on web and costs nothing to preserve.

Excluded:
- App-specific read-model stores unless they are examples.
- Backend-specific public APIs unless compatibility notes justify them.

Scope Note (2026-08-29) — the D5 plan is now writable:
- The two obligations that blocked it are settled, and both resolved to the same shape: **core
  states an obligation, the backend picks a mechanism, and a conformance case is the enforcement.**
  Decision 024 covers storage naming (injectivity; no encoder in core, no hash dependency);
  decision 025 covers quarantine (four properties; table-or-state is the backend's).
- ~~**Two conformance cases are owed and neither can fail today**~~ — **both landed 2026-08-29**
  as cases 49 and 50: colliding scope keys, and a quarantine transition that rolls back with its
  batch. They pass vacuously on the in-memory backend, which is expected; they exist so a durable
  backend cannot ship the source's scope-collision bug and still show a green suite.
- ~~The plan must fix the outbox column set in one change~~ — **settled 2026-08-29 and 2026-08-30**
  and no longer the D5 plan's problem. `seq` (016), `attempts` (017), `traceparent` (022) and
  `precondition` (026) are all built in core and the in-memory backend, so D5 inherits a fixed
  record shape. Only last-error (register entry 3) is still open, and register entry 12 — whether
  a store can answer *which ids are pending* — was added by the D4a trial and is a D5 question
  as much as a publication one.
- Three durable schemas, not two: outbox, cache versions, and row-level staleness markers (023).

Scope Note (2026-08-30) — durability makes a second drainer the normal case:
- **Built the same day, and this note is kept for what it argued rather than what is left.** The
  in-realm half is done: `src/runner/exclusion.rs` claims the scope, `sync_once` releases it on
  drop, and case 61 fails against the old guard. Register entry 17 closed on a fourth option —
  neither candidate it listed could report `AlreadyRunning` without a signature change. What
  reaches D5 is the cross-realm half and the fixture that can observe it.
- **Found before the first durable row was written, which is the only reason it is cheap.** The
  single-flight guard was a `Cell<bool>` on `SyncRunner`, so it excluded a second drain *by that
  runner* and nothing else. Two handles on one scope are two views of one queue
  (`src/memory/mod.rs:142`, `:113`), and each runner observed its own flag false.
- The 2026-08-27 note below is accurate that the crate "was already single-flight at the pass level"
  — and that is precisely the gap. **Pass level means per runner.** Nothing needed it to mean more
  while the queue was in-process and died with the tab.
- With a durable store it needs no mistake to reach: **two browser tabs on one origin are two realms
  over one IndexedDB database.** Idempotency survives it, ordering and the retention bound do not.
  Decision 031 states the obligation; register entry 17 holds the API question of where the flag
  lives, because moving it off `SyncRunner` changes a built deliverable.
- Surfaced by the Chrome DevTools background-services question, written up in
  `wiki/proposals/browser-background-services.proposal.md`. Service-worker Background Sync is a
  consumer of this guarantee, not an independent feature, and is deferred until after D4b.

Scope Note (2026-08-27):
- RepForge's single-flight proposal asked D5 to shed scope that only served larger batches. The
  answer is that D5 **gains two columns and sheds bounded-concurrency concerns it never had** — the
  crate was already single-flight at the pass level and never grew claim-leases. See
  `wiki/proposals/single-flight-drain.proposal.md` and
  `wiki/references/repforge-single-flight-proposal.reference.md`.
- ~~Decisions 016-019 are recorded and **unimplemented**.~~ **All four were built 2026-08-29**, in
  the order this note proposed: 017, then 016, then 022, then 018's profile and 019's transport
  documentation. The ordering argument held — 018's cases assert a bound that terminates a wedged
  head and an order that limit 1 makes observable, and neither existed before 017 and 016 landed.

Proof:
- Native backend tests pass.
- Browser/wasm IndexedDB tests pass.
- Shared conformance tests pass on in-memory, SQLite, and IndexedDB backends.
- ~~**A two-realm drain does not double-send.**~~ **Met 2026-08-31.** Case 61 covers one realm and
  passes on any backend, so it proved nothing here. The evidence is `tests/cross_realm.rs`: a second
  realm on one origin's lock manager, with the realm that finds the scope locked observing
  `AlreadyRunning` and sending nothing, and a pass in flight refusing that realm the lock. A browser
  fixture rather than a conformance case, because a suite that drives a backend through one process
  cannot open a second realm.

  **The second realm is a dedicated worker, not a second tab**, and the difference is stated rather
  than glossed: the harness drives a single page, and a worker is the separate agent reachable from
  inside it. Web Locks are managed per origin across every agent on it, which is the property under
  test; what a worker does not reproduce is a second wasm instance, which the two tests are shaped
  so as not to need.

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

Adoption Note (2026-08-30) — half of this deliverable moved forward:
- The user's direction is to build toward **RepForge starting on frontbox for real**. The half of
  D6 that adoption needs — compatibility notes, a consumption path (git dependency while
  `publish = false` holds), and an adoption guide mapping RepForge's `persistence/*.rs` to
  frontbox APIs — rides D5's adoption lane instead of waiting here. What stays in D6: crates.io
  publication mechanics, README quickstart, and the general-audience examples.
- The first developer this promise serves is RepForge, and the seam proof is their integration.

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
