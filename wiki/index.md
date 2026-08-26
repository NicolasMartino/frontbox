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
| [roadmaps/extraction.roadmap.md](roadmaps/extraction.roadmap.md) | Active | Deliverable sequence D0, D0a, D1-D6 with proof gates, keeping the prior-art survey before implementation and the migration trial before backend ports. |
| [plans/d1-core-cache-runtime.plan.md](plans/d1-core-cache-runtime.plan.md) | Completed | The first core outbox runtime slice: shapes, conformance cases, and what implementation changed. Built 2026-08-26. |
| [plans/d2-cache-invalidation.plan.md](plans/d2-cache-invalidation.plan.md) | Completed | The second slice: generic entity keys, version reconciliation, invalidation handling, and the pending-write conflict signal. Built 2026-08-27. |
| [plans/prior-art-survey.plan.md](plans/prior-art-survey.plan.md) | Completed | Executed D0a: compared frontbox against offline/local-first prior art before implementation is authorized. |
| [references/prior-art-survey.reference.md](references/prior-art-survey.reference.md) | Sourced | D0a prior-art comparison across two cohorts: state-replication engines (Replicache/Zero, PowerSync, Electric, RxDB, WatermelonDB, PouchDB/CouchDB, Automerge, Yjs) and the HTTP command-queue peers frontbox actually belongs to (Workbox, Redux Offline, TanStack Query, Amplify DataStore). URLs verified 2026-08-26. |
| [references/repforge-cache-source-corpus.reference.md](references/repforge-cache-source-corpus.reference.md) | Sourced | Inventory of copied RepForge source files, line counts, scope notes, and extraction value. |
| [references/source-test-inventory.reference.md](references/source-test-inventory.reference.md) | Sourced | Inventory of 130 source tests and how they map to extraction milestones. |
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
- Decide whether mutation ordering needs a monotonic sequence number before durable backends. D1's
  `(created_at, mutation_id)` tie-break settles determinism only. Prior art raises the priority but
  rests on one precedent, Replicache; CouchDB, RxDB, and PowerSync were withdrawn as evidence on
  verification.
- Decide whether retained work needs aging, attempt tracking, or last-error metadata. D1 only
  reports a no-progress sync; it does not escalate one. **Decision 012 raised the priority of this**:
  an unrecognised status now retains indefinitely, and attempt tracking is the only route out of a
  vocabulary mismatch the server never resolves. Reference designs exist in Redux Offline
  (`retry() -> null`), Workbox (`maxRetentionTime`), and Amplify (`outboxStatus{isEmpty}`).
- Decide whether local read-model persistence belongs in core or app-owned companion traits.
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
