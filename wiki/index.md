# frontbox Wiki

**frontbox** is an offline-first cache and mutation outbox for Rust frontends.

The name is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. Alongside the queue sit local read models and version-based cache
invalidation.

This wiki is the project knowledge base for extracting that runtime out of RepForge's Dioxus
application and into a reusable library.

## Project Shape

- **Current stage:** Research and planning. No crate exists yet; design lives in this wiki.
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
| [specs/source-frontend-cache-architecture.spec.md](specs/source-frontend-cache-architecture.spec.md) | Active | Source-backed architecture spec for RepForge's frontend cache, including corrected divergences and extraction implications. |
| [proposals/extraction-boundary.proposal.md](proposals/extraction-boundary.proposal.md) | Proposed | Proposed split between core, storage backends, Dioxus adapter, examples, and RepForge-owned app code. |
| [roadmaps/extraction.roadmap.md](roadmaps/extraction.roadmap.md) | Active | Deliverable sequence D0, D0a, D1-D6 with proof gates, keeping the prior-art survey before implementation and the migration trial before backend ports. |
| [plans/d1-core-cache-runtime.plan.md](plans/d1-core-cache-runtime.plan.md) | Active | Implementation-prep plan for the first core outbox runtime slice; no code authorized yet. |
| [plans/prior-art-survey.plan.md](plans/prior-art-survey.plan.md) | Active | Executes D0a: compares frontbox against offline/local-first prior art before implementation is authorized. |
| [references/repforge-cache-source-corpus.reference.md](references/repforge-cache-source-corpus.reference.md) | Sourced | Inventory of copied RepForge source files, line counts, scope notes, and extraction value. |
| [references/source-test-inventory.reference.md](references/source-test-inventory.reference.md) | Sourced | Inventory of 130 source tests and how they map to extraction milestones. |
| [decisions/001-single-threaded-core.decision.md](decisions/001-single-threaded-core.decision.md) | Accepted | Core uses single-threaded frontend-friendly traits with no `Send` bounds. |
| [decisions/002-error-model.decision.md](decisions/002-error-model.decision.md) | Accepted | Core uses one non-exhaustive structured error type instead of per-trait associated errors. |
| [decisions/003-atomic-outcome-application.decision.md](decisions/003-atomic-outcome-application.decision.md) | Accepted | Outbox outcome application is one atomic transition, not separate delete and dead-letter writes. |
| [decisions/004-transport-auth-and-offline.decision.md](decisions/004-transport-auth-and-offline.decision.md) | Accepted | Auth is evaluated freshly per send; offline is distinct from attempted transport failure. |
| [decisions/005-mutation-outcome-policy.decision.md](decisions/005-mutation-outcome-policy.decision.md) | Accepted | `Blocked` and `Pending` stay queued; only `Rejected` becomes a dead letter. |
| [decisions/006-corrupt-record-policy.decision.md](decisions/006-corrupt-record-policy.decision.md) | Accepted | Corrupt local records are quarantined or surfaced, not silently dropped from reads. |
| [decisions/007-generic-entity-key-registry.decision.md](decisions/007-generic-entity-key-registry.decision.md) | Accepted | Generic cache versioning needs caller-owned entity keys and a caller-supplied registry. |

## Open Work

- Run the prior-art survey (D0a). D1 implementation is gated on it; D1 planning is not.
- Decide whether cache version state is persisted by default.
- Decide whether mutation ordering needs a monotonic sequence number for causal ordering. D1's
  `(created_at, mutation_id)` tie-break settles determinism only.
- Decide whether retained work needs aging or attempt tracking. D1 only reports a no-progress sync;
  it does not escalate one.
- Decide whether local read-model persistence belongs in core or app-owned companion traits.
- Decide whether quarantine is a distinct store or a status in a single durable table.
- Create `wiki/apis/` and `wiki/compatibility/` entries once public APIs exist.

## Source Evidence

Initial source material was copied by `llm-wiki init` into
`raw/initial/2026-08-25T083750Z`.

## Maintenance

- Add durable architecture findings as typed pages under `wiki/specs`, `wiki/proposals`,
  `wiki/roadmaps`, `wiki/plans`, `wiki/decisions`, or `wiki/references`.
- Keep this index and `wiki/log.md` updated when wiki knowledge changes.
- Do not rely on Git operations for wiki bookkeeping; Git commits remain user-owned.
