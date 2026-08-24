# Knowledge Log

## [2026-08-25] ingest | bootstrap

Created LLM Wiki project `dioxus-cache` with the `library-sdk` blueprint and copied initial
RepForge cache sources into `raw/initial/2026-08-25T083750Z`.

Pages affected: `.llm_wiki/init.toml`, `.llm_wiki/runtime.toml`, `.llm_wiki/search.toml`,
`raw/initial/2026-08-25T083750Z/manifest.md`, `raw/initial/2026-08-25T083750Z/sources`

## [2026-08-25] ingest | source cache architecture

Summarized RepForge's frontend cache as an offline-first CQRS client runtime. Captured the
extraction boundary between framework-neutral cache core, Dioxus adapter, persistence backends,
and RepForge-specific app code. User clarified that Git state is user-owned; no Git staging or
commits should be performed by Codex.

Pages affected: `wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`,
`wiki/references/repforge-cache-source-corpus.reference.md`, `wiki/index.md`, `wiki/log.md`

## [2026-08-25] lint | source verification and correction

Audited wiki claims against the copied sources. Corrected the `OutboxRecord` model to the real
five fields: `mutation_id`, `method`, `path`, `body`, and `created_at`. Moved invented fields to
the desired-additions section. Corrected `DeadLetterRecord`, DTO names, the HTTP-envelope replay
finding, absence of per-record retry/backoff, dead-letter retention, and the non-atomic
dead-letter transition. Fixed raw input paths, roadmap sequencing, invalid statuses, and missing
metadata.

Pages affected: `wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`,
`wiki/references/repforge-cache-source-corpus.reference.md`, `wiki/index.md`, `wiki/log.md`

## [2026-08-25] create | API decisions 001-004

Added decision pages for the initial public API constraints: single-threaded core with no `Send`
bounds, one core error type, atomic outcome application, and fresh per-send auth with offline as a
distinct outcome. The user later confirmed these should remain `Accepted`, not downgraded.

Pages affected: `wiki/decisions/001-single-threaded-core.decision.md`,
`wiki/decisions/002-error-model.decision.md`,
`wiki/decisions/003-atomic-outcome-application.decision.md`,
`wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/index.md`, `wiki/log.md`

## [2026-08-25] update | rename to frontbox

Renamed the project from `dioxus-cache` to `frontbox` and moved the folder to
`/Users/nicolasmartino/Documents/rust/frontbox`. Rationale: the core is framework-neutral by
design, so it should not carry a framework name. Historical mentions of `dioxus-cache` are
retained where they record project history.

Pages affected: `.llm_wiki/init.toml`, `AGENTS.md`, `project_guidelines.md`, `wiki/index.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/references/repforge-cache-source-corpus.reference.md`, `wiki/log.md`

## [2026-08-25] update | implementation reverted

A crate scaffold and D1 core runtime were written, then reverted at the user's direction. The
project is in a research and planning phase, and implementation has not been authorized. No
`Cargo.toml` or `src/` code exists. `src/`, `tests/`, and `examples/` are empty or absent. The
wiki corrections and decision pages were kept as planning artifacts.

Pages affected: `wiki/log.md`

## [2026-08-25] lint | review corrections before implementation

Applied the in-depth review findings that survived source verification. Corrected `Blocked`
semantics to retain rather than dead-letter. Rewrote the atomicity decision with accurate upsert
evidence. Added corrupt-record quarantine, generic entity registry, and source test inventory
pages. Fixed exercise/preference flow descriptions, cache validity predicates, roadmap deliverable
format, index catalog entries, and metadata-template drift. Promoted the prior-art gap into its
own active plan.

Pages affected: `project_guidelines.md`, `wiki/index.md`, `wiki/log.md`,
`wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`,
`wiki/references/repforge-cache-source-corpus.reference.md`,
`wiki/references/source-test-inventory.reference.md`,
`wiki/decisions/001-single-threaded-core.decision.md`,
`wiki/decisions/002-error-model.decision.md`,
`wiki/decisions/003-atomic-outcome-application.decision.md`,
`wiki/decisions/004-transport-auth-and-offline.decision.md`,
`wiki/decisions/005-mutation-outcome-policy.decision.md`,
`wiki/decisions/006-corrupt-record-policy.decision.md`,
`wiki/decisions/007-generic-entity-key-registry.decision.md`, `.mcp.json`, `.gitignore`,
`README.md`

## [2026-08-25] lint | second review pass: liveness, quarantine API, direct dispatch

Applied the second round of in-depth review findings.

Corrected the liveness reasoning in decision 005. The page previously claimed `Blocked` would
repeat until the user removed the earlier rejected mutation. That was wrong in the pessimistic
direction: `Blocked` follows only a terminal failure, the terminal status is `Rejected`, and
`Rejected` is dead-lettered and deleted in the same `apply_outcomes` call, so `Blocked` self-clears
on the next sync with no user action. The review that prompted this change had also claimed a
`Pending`-head livelock; that does not follow either, because `Pending` is explicitly not terminal
(`08-offline-sync.spec.md:89,90,98`). The real residual risk is narrower and now recorded: `Pending`
is retained with no bound, and a `Pending` prefix as long as `pending_batch(limit)` starves the tail.
D1 answers this with a no-progress signal and tests, not with attempt counters.

Gave decision 006 an actual API. `QuarantineStore` could only list and count, so the id-less path
the decision requires had no implementation route. Added `OutboxStore::sweep_corrupt`, scoped
`pending_count` to decodable records only, and added tests 15 and 18.

Resolved batch ordering: `(created_at, mutation_id)`, total and reproducible across backends
without a schema change. Recorded that this buys determinism, not causality, and that a monotonic
sequence number remains open for D5.

Ruled that direct single-write dispatch stays in application code, with rationale: the
terminal-vs-retryable split is domain judgement, the pattern is a latency optimization rather than a
durability requirement, and core already makes it expressible through caller-supplied mutation ids.

Promoted the prior-art survey from an orphan plan to roadmap deliverable D0a, and gated D1
implementation on it. Two active documents previously disagreed about whether D1 was blocked.

Also: synced `Disposition` between decision 003 and the D1 plan, documented why no `Error` variant
carries `#[from]`, recorded the third status enum `MutationDispatchStatus`, recorded the N+1
dead-letter purge, corrected the `persistence/mod.rs` theme-helper export claim, fixed D4's vague
`Depends On`, and added `cargo clippy` to the verification gates.

Pages affected: `wiki/index.md`, `wiki/log.md`,
`wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`,
`wiki/decisions/002-error-model.decision.md`,
`wiki/decisions/003-atomic-outcome-application.decision.md`,
`wiki/decisions/005-mutation-outcome-policy.decision.md`
