# frontbox Wiki

**frontbox** is an offline-first cache and mutation outbox for Rust frontends.

The name is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. Alongside the queue sit local read models and version-based cache
invalidation.

This wiki is the project knowledge base for extracting that runtime out of RepForge's Dioxus
application and into a reusable library.

## Project Shape

- **Current stage:** D1 implemented 2026-08-26, D2 implemented 2026-08-27. The crate exists at the
  repository root; design continues to live in this wiki. D3 onward are unauthorized.
- **Blueprint:** library-sdk
- **Source system:** `/Users/nicolasmartino/Documents/workout/cqrs-fullstack` (RepForge)
- **Extraction target:** `/Users/nicolasmartino/Documents/rust/frontbox`
- **Primary goal:** Separate the generic client-side cache/runtime from RepForge's domain-specific
  Dioxus app.
- **Naming:** formerly `dioxus-cache`, renamed 2026-08-25. The core is framework-neutral, so
  Dioxus belongs in an adapter.

## Current Thesis

The current cache is not just a local map or request cache. It is an offline-first CQRS client
runtime with a persistent mutation outbox, local read models, dead-letter handling, server version
invalidation, SSE reconnect checks, and optimistic UI flows.

The durable outbox persists a raw HTTP envelope: `method`, `path`, and JSON `body`. That makes the
queue naturally domain-neutral. The extraction risk is not the envelope shape; it is accidentally
porting source defects such as `Blocked` dead-lettering, unbounded batches, non-atomic outcome
application, corrupt-record loss, and closed RepForge entity enums.

Correcting a source defect can introduce a new one. Retaining `Blocked` work instead of
dead-lettering it removes a data-loss bug but makes the queue's liveness a property that has to be
argued and tested rather than assumed. Divergences from the source carry that obligation.

## Catalog

| Path | Status | Summary |
| --- | --- | --- |
| [specs/source-frontend-cache-architecture.spec.md](specs/source-frontend-cache-architecture.spec.md) | Active | Source-backed architecture spec for RepForge's frontend cache: what the source system does, verified against the copied corpus. |
| [specs/frontbox-runtime.spec.md](specs/frontbox-runtime.spec.md) | Active | What the extracted library does, as built and tested: divergences from the source, behaviours the source has no position on, and the constraints these place on D5. |
| [proposals/extraction-boundary.proposal.md](proposals/extraction-boundary.proposal.md) | Accepted | Proposed split between core, storage backends, Dioxus adapter, examples, and RepForge-owned app code. |
| [proposals/single-flight-drain.proposal.md](proposals/single-flight-drain.proposal.md) | Proposed | Response to RepForge's single-flight proposal: the ordering ask is a bug fix rather than a price, `batch_limit = 1` freezes the queue without a retention bound, and the default stays 100. |
| [proposals/repforge-read-model-convergence.proposal.md](proposals/repforge-read-model-convergence.proposal.md) | Proposed | Answers RepForge's 2026-08-28 revision: the read model, trace context, the sink boundary, and all eight questions. |
| [roadmaps/extraction.roadmap.md](roadmaps/extraction.roadmap.md) | Active | Deliverable sequence D0, D0a, D1-D6 with proof gates, keeping the prior-art survey before implementation and the migration trial before backend ports. |
| [plans/d1-core-cache-runtime.plan.md](plans/d1-core-cache-runtime.plan.md) | Completed | The first core outbox runtime slice: shapes, conformance cases, and what implementation changed. Built 2026-08-26. |
| [plans/d2-cache-invalidation.plan.md](plans/d2-cache-invalidation.plan.md) | Completed | The second slice: generic entity keys, version reconciliation, invalidation handling, and the pending-write conflict signal. Built 2026-08-27. |
| [plans/prior-art-survey.plan.md](plans/prior-art-survey.plan.md) | Completed | Executed D0a: compared frontbox against offline/local-first prior art before implementation is authorized. |
| [references/prior-art-survey.reference.md](references/prior-art-survey.reference.md) | Sourced | D0a prior-art comparison across two cohorts: state-replication engines (Replicache/Zero, PowerSync, Electric, RxDB, WatermelonDB, PouchDB/CouchDB, Automerge, Yjs) and the HTTP command-queue peers frontbox actually belongs to (Workbox, Redux Offline, TanStack Query, Amplify DataStore). URLs verified 2026-08-26. |
| [references/repforge-cache-source-corpus.reference.md](references/repforge-cache-source-corpus.reference.md) | Sourced | Inventory of copied RepForge source files, line counts, scope notes, and extraction value. |
| [references/source-test-inventory.reference.md](references/source-test-inventory.reference.md) | Sourced | Inventory of 130 source tests and how they map to extraction milestones. |
| [references/repforge-single-flight-proposal.reference.md](references/repforge-single-flight-proposal.reference.md) | Recorded (not in `raw/`) | The proposal RepForge architecture sent on 2026-08-27, recorded so the response and its decisions have something to cite. Received in conversation, not in `raw/`. |
| [references/open-decisions.reference.md](references/open-decisions.reference.md) | Active | Every decision not yet made: origin, options, consequences, and which of the three clocks each one is on. |
| [references/repforge-section-c-answers.reference.md](references/repforge-section-c-answers.reference.md) | Recorded (not in `raw/`) | RepForge's 2026-08-29 answers to the register's section C: the accumulator seeds at zero, a `retry` terminality field now exists, and there is no production queue-depth data. |
| [decisions/001-single-threaded-core.decision.md](decisions/001-single-threaded-core.decision.md) | Accepted | Core uses single-threaded frontend-friendly traits with no `Send` bounds. |
| [decisions/002-error-model.decision.md](decisions/002-error-model.decision.md) | Accepted | Core uses one non-exhaustive structured error type instead of per-trait associated errors. |
| [decisions/003-atomic-outcome-application.decision.md](decisions/003-atomic-outcome-application.decision.md) | Accepted | Outbox outcome application is one atomic transition, not separate delete and dead-letter writes. |
| [decisions/004-transport-auth-and-offline.decision.md](decisions/004-transport-auth-and-offline.decision.md) | Accepted | Auth is evaluated freshly per send; offline is distinct from attempted transport failure. |
| [decisions/005-mutation-outcome-policy.decision.md](decisions/005-mutation-outcome-policy.decision.md) | Accepted | `Blocked` and `Pending` stay queued; only `Rejected` becomes a dead letter. |
| [decisions/006-corrupt-record-policy.decision.md](decisions/006-corrupt-record-policy.decision.md) | Accepted | Corrupt local records are quarantined or surfaced, not silently dropped from reads. |
| [decisions/007-generic-entity-key-registry.decision.md](decisions/007-generic-entity-key-registry.decision.md) | Accepted | Generic cache versioning needs caller-owned entity keys and a caller-supplied registry. |
| [decisions/008-mutation-envelope-extensibility.decision.md](decisions/008-mutation-envelope-extensibility.decision.md) | Accepted | Records are non-exhaustive and constructor-built, the body is parsed JSON, and optional structured operation metadata rides along uninterpreted. |
| [decisions/009-local-scope-identity.decision.md](decisions/009-local-scope-identity.decision.md) | Accepted | Every store takes a required opaque scope key; records are stamped, reads verify, and a mismatch retains rather than discards. |
| [decisions/010-batch-wire-format.decision.md](decisions/010-batch-wire-format.decision.md) | Accepted (amended) | The batch payload stays byte-compatible with the source server. Amended by decision 011, which kept the bytes and dropped the date dependency. |
| [decisions/011-owned-rfc3339-rendering.decision.md](decisions/011-owned-rfc3339-rendering.decision.md) | Accepted | `client_datetime` is rendered by this crate and compared against `chrono` by a dev-dependency oracle, so byte compatibility costs no public dependency. |
| [decisions/012-unknown-mutation-status.decision.md](decisions/012-unknown-mutation-status.decision.md) | Accepted | An unrecognised server status is retained with its original spelling and reported as an anomaly, never mapped onto a known status. |
| [decisions/013-unknown-entity-name.decision.md](decisions/013-unknown-entity-name.decision.md) | Accepted | An invalidation naming an unregistered entity is ignored for cache purposes but reported to the caller, in one event shape. |
| [decisions/014-pull-gating.decision.md](decisions/014-pull-gating.decision.md) | Accepted | Core reports that a refetch would discard unsent local work; it does not gate a pull it does not perform. |
| [decisions/015-cache-version-persistence.decision.md](decisions/015-cache-version-persistence.decision.md) | Accepted | Cache version and staleness are durable by default and persist as one unit — storing the version alone is a correctness bug. |
| [decisions/016-monotonic-enqueue-sequence.decision.md](decisions/016-monotonic-enqueue-sequence.decision.md) | Accepted (unimplemented) | Enqueue assigns a durable, globally monotonic sequence and `pending_batch` orders by it. `(created_at, mutation_id)` was never causal, and batching never hid that. |
| [decisions/017-bounded-retention.decision.md](decisions/017-bounded-retention.decision.md) | Accepted (unimplemented) | A record retained to a caller-set attempt bound is dead-lettered, never discarded and never skipped past. Without it, `batch_limit = 1` freezes the queue on one stuck record. |
| [decisions/018-single-flight-drain-mode.decision.md](decisions/018-single-flight-drain-mode.decision.md) | Accepted (unimplemented) | `batch_limit = 1` gains a conformance profile and does not become the default. |
| [decisions/019-verdict-synthesis.decision.md](decisions/019-verdict-synthesis.decision.md) | Accepted (unimplemented) | With no server producing a batch response, the transport synthesizes verdicts — and must not read terminality off an HTTP status class. |
| [decisions/020-observability-surface.decision.md](decisions/020-observability-surface.decision.md) | Accepted (built model; additions unimplemented) | Core emits nothing: diagnostics are returned, correlation is `mutation_id`, and the no-clock and `!Send` gates are why in-library emission is not available to a convergence. |
| [decisions/021-cache-version-identity.decision.md](decisions/021-cache-version-identity.decision.md) | **Implemented** 2026-08-28 | A cache version is an opaque identity compared by equality, not an ordered counter. `VersionUpdate::NeedsReset` loses its producer and zero stops being a safe sentinel. |
| [decisions/022-durable-trace-context.decision.md](decisions/022-durable-trace-context.decision.md) | Accepted (unimplemented) | The record carries W3C trace context, stamped at enqueue and caller-supplied — core has no randomness, and a span cannot model enqueue-to-send. |
| [decisions/023-read-model-boundary.decision.md](decisions/023-read-model-boundary.decision.md) | Accepted (unimplemented) | frontbox stores row-level staleness markers, never the rows. Resolves the read-model persistence question. |
| [compatibility/public-dependencies.compat.md](compatibility/public-dependencies.compat.md) | Active | Which crates appear in the public API and therefore in the semver contract — `serde_json`, `uuid`, `serde` — and which server payload the wire format targets. |

## Open Work

- D1 and D2 are implemented and all gates pass (`scripts/verify.sh`: 96 tests, both wasm builds,
  clippy on native and wasm, coverage at 89% regions / 95% lines against an 80% floor). D3 onward
  remain unauthorized.
- The public-dependency note now exists at `compatibility/public-dependencies.compat.md`, which
  discharges two of decision 010's three release obligations. Outstanding: a changelog entry the
  first time a public major moves, and naming a *checkable* server-contract version in place of
  "RepForge as of 2026-08-25" — a D4 question, since D4 is when a second consumer first exists.
- The public dependencies are `serde_json`, `uuid`, and `serde`, all major `1`. `chrono` was on
  that list for one day: writing the note exposed it as public surface *by behaviour* rather than
  by type, and decision 011 removed it by owning the rendering. No 0.x crate is in the semver
  contract, which is what makes the remaining release obligations small.
- D0a prior-art survey is complete. Both shape decisions that a published D1 API could foreclose
  are settled: decision 008 (envelope extensibility and operation metadata) and decision 009
  (local scope identity). Decision 010 was added during D1.
- **D2 is implemented** (2026-08-27) and all gates pass: 96 tests, coverage 89% regions / 95% lines
  against an 80% floor. D3 onward remain unauthorized.
- ~~Decide the policy for a server status this crate has never heard of~~ — settled by decision 012
  and shipped: `MutationStatus::Unknown(String)` retains the record and reports an
  `AnomalyKind::UnknownStatus` carrying the server's spelling. `SyncReport::anomalies` is now
  `Vec<Anomaly>` rather than `Vec<MutationId>`, because three reasons in one bare list of
  identifiers told a caller nothing.
- ~~Decide whether cache version state is persisted by default~~ — settled by decision 015: durable,
  with version and staleness as one unit. Persisting the version alone would let a client believe an
  invalidated entity is fresh.
- **RepForge is removing its BFF**, which expires the premise several D1 decisions were argued
  against. Their proposal is recorded at `references/repforge-single-flight-proposal.reference.md`
  and answered at `proposals/single-flight-drain.proposal.md`. Decisions 016-019 came out of it and
  are **recorded but unimplemented** — they are design changes and need an explicit go-ahead per
  `AGENTS.md`. Implementation order is 017, then 016, then 018's conformance profile, with 019 as
  documentation on `SyncTransport`. The profile is the expensive step and is **not** the existing
  case list re-run: four outbox cases assert multi-record batch semantics that cannot exist at
  `batch_limit = 1`, several build their own runner and would sit inert, and the rest need
  expectations derived from the limit instead of the seed count. It also has to land after 016 and
  017, not alongside them — the profile's new cases are a wedged head and a respected order, and
  neither is assertable until the bound and the sequence exist. See decision 018,
  `## What The Profile Actually Costs`.
- ~~Decide whether mutation ordering needs a monotonic sequence number before durable backends~~ —
  settled by decision 016: yes, globally monotonic and assigned by the store at enqueue. The
  deciding evidence turned out to be in this crate rather than in prior art. `runner.rs:263-264`
  builds the batch from `pending_batch` order, so a mis-sorted queue is a mis-ordered batch and the
  hazard exists today at `batch_limit = 100`.
- ~~Decide whether retained work needs aging, attempt tracking, or last-error metadata~~ — partly
  settled by decision 017: attempt tracking is adopted, aging is rejected (a record that aged while
  the user was offline was never evaluated), and dead-lettering at the bound was already promised by
  decision 005's prior-art section. **Last-error metadata stays open** — the attempt count plus the
  anomaly report covers the diagnosis 017 needed, and a durable last-error field on the record is a
  separate question.
- Decide whether retention bounds should be per-status. `Pending` waits on a server job that will
  probably settle; `Unknown` waits on a client rebuild that will not. One bound either buries
  `Pending` work early or leaves `Unknown` work queued long (decision 017, Revisit If).
- Decide whether D5's ordering should be a total order or a partial one. Decision 016 takes the
  total order because it is the version that can be reasoned about, but with four per-service
  origins it serializes a `billing` write behind an unrelated `workout` write. Per-origin sequences
  or explicit dependency edges are the alternatives, and both are larger designs.
- D3 owes a drain-until-idle loop, and at `batch_limit = 1` it stops being a performance nicety: at
  the source's 5 s cadence a 200-record backlog takes ~17 minutes, not the ~20 s RepForge estimated
  (decision 018).
- Ask RepForge for a structured terminality signal in service error bodies. It is the one thing the
  redesign could add that would collapse decision 019 into a mapping table, and it is cheap while
  the services are still being designed.
- **RepForge revised the proposal on 2026-08-28** and withdrew "not asking D2 to change". Recorded
  at `references/repforge-single-flight-proposal.reference.md` (`## The 2026-08-28 Revision`) and
  answered at `proposals/repforge-read-model-convergence.proposal.md`. Decisions 021-023 came out of
  it. **Decision 021 changes shipped D2 code** — `EntityState`, `InvalidationEvent`, `StaleEntity`,
  `VersionUpdate`, and `compare` all assume an ordered `u64` — which no decision since D1 has done,
  and it is affordable only because `publish = false`.
- ~~Decide what becomes of **`VersionUpdate::NeedsReset`**~~ — removed when decision 021 was
  implemented. The 005 precedent did not transfer: `Blocked` is kept because a *server* can still
  send it, whereas `VersionUpdate` is produced only by `compare`, so keeping the variant would have
  meant an unreachable arm and a permanently empty `InvalidationReport::needs_reset`. The enum is
  `#[non_exhaustive]`, so restoring it is additive.
- ~~Decide whether local read-model persistence belongs in core or app-owned companion traits~~ —
  settled by decision 023: app-owned, with core holding `(entity, row_id, stale)` markers and never
  the rows. Row markers make D5's third durable schema.
- **Observability convergence across RepForge, frontbox, and kafkaman.** The §5b.6 boundary is
  accepted (decision 020, amended) — emit `tracing`, application owns the sink. Open: whether the
  `tracing` dependency is default-on or feature-gated, and its wasm binary-size cost, which nobody
  has measured. Earlier framing was: frontbox's side is
  documented at `decisions/020-observability-surface.decision.md`; the convergence *specification*
  was not supplied, so four questions there are open — the shared event vocabulary, whether the
  target is OpenTelemetry and what `mutation_id` maps to in it, whether kafkaman pushes or returns,
  and whose clock stamps the shared format. Two frontbox-side items fall out and both are cheap
  now: **derive `Serialize` on the report types** (`SyncReport`, `SyncOutcomeCounts`, `SyncPass`,
  `Anomaly`, `AnomalyKind` are the only public types that lack it, and they are the ones a
  telemetry pipeline most wants), and **settle last-error metadata**, which decision 020 promotes
  from optional to load-bearing because it is the only proposed field that outlives the process.
- Decide whether quarantine is a distinct store or a status in a single durable table.
- Decide how durable backends encode a `ScopeKey` into a storage name. Character replacement is not
  injective, so D5 needs reversible encoding or hashing; the source's approach is safe only for
  UUIDs (decision 009).
- D5 must store `mutation_id` in one consistent textual form — exactly what `MutationId::to_string`
  returns — with a binary collation. `MutationId: Ord` compares the UUID's bytes; any single
  canonical form sorts identically, but *mixed* forms (some rows uppercase, some unhyphenated)
  silently invert pairs and give that backend a different pending order. Found during D1; see the D1
  plan's Ordering Policy and the unit tests in `src/id.rs`.
- Decide whether a cross-scope diagnostic is needed to surface work retained under a scope no store
  currently opens. D1's no-progress signal cannot see it (decision 009).
- Create `wiki/apis/` and `wiki/compatibility/` entries once public APIs exist.

## Crate

The library lives at the repository root as a single crate, `frontbox`, with modules matching the
eventual crate split. `scripts/verify.sh` runs every gate the roadmap names, including the prose
ones — no Dioxus, no RepForge entity names, no `Send` bound — as `grep` checks rather than
intentions.

The conformance suite is a library module behind a `testing` feature, not a test file, so D5's
SQLite and IndexedDB backends run the identical cases through `StoreFactory` and `FaultInjection`.
It emits in two shapes — synchronous cases driven by a caller-supplied `block_on`, and `async` cases
for a harness that drives them itself, which is what a browser needs — from one shared case list, so
the native and wasm backends cannot drift into running different suites.

The runtime dependency graph is `serde`, `serde_json`, `thiserror`, and `uuid`. There is no date
library in it: `src/rfc3339.rs` renders the wire format's `client_datetime`, and `chrono` is a
dev-dependency whose only job is to prove those bytes are still its own (decision 011).

Source files are kept under ~400 lines, and `scripts/verify.sh` gates total coverage at 80%.

## Source Evidence

Initial source material was copied by `llm-wiki init` into
`raw/initial/2026-08-25T083750Z`.

Prior-art source notes for D0a live under `raw/research/2026-08-25-prior-art-survey`. That
directory's `manifest.md` records per-URL retrieval state, a verification pass, corrections to four
claims, and the systems deliberately not surveyed.

## Maintenance

- Add durable architecture findings as typed pages under `wiki/specs`, `wiki/proposals`,
  `wiki/roadmaps`, `wiki/plans`, `wiki/decisions`, or `wiki/references`.
- Keep this index and `wiki/log.md` updated when wiki knowledge changes.
- Do not rely on Git operations for wiki bookkeeping; Git commits remain user-owned.
