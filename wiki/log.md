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

## [2026-08-25] ingest | prior-art survey D0a

Completed the D0a prior-art survey. Compared Replicache/Zero, PowerSync, Electric/ElectricSQL,
RxDB, WatermelonDB, PouchDB/CouchDB, Automerge, and Yjs against frontbox's planned HTTP-envelope
outbox and cache runtime. The first draft was produced by parallel read-only agents; those reports
are workflow artifacts and are not retained as sources, so every claim was written against a
primary URL.

The survey did not replace the extraction boundary. It preserved the D1 shape while raising
follow-up questions about optional operation metadata, explicit local namespace/scope identity,
monotonic enqueue sequence, attempt/error aging for retained work, and durable storage format
versioning.

Pages affected: `raw/research/2026-08-25-prior-art-survey/` (manifest, research summary, four
source notes), `wiki/references/prior-art-survey.reference.md`, `wiki/index.md`, `wiki/log.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/plans/prior-art-survey.plan.md`, `wiki/plans/d1-core-cache-runtime.plan.md`

## [2026-08-26] lint | prior-art survey verification pass

Re-fetched every URL cited by D0a and re-checked every claim against its source. The survey's
conclusions survived; several of its citations and one of its headline findings did not.

Citation defects fixed:

- Two cited sources did not exist. `legacy.electric-sql.com` fails TLS with a certificate name
  mismatch over HTTPS and returns 404 over HTTP. Both legacy ElectricSQL citations are now Wayback
  snapshots. Separately, `electric-sql.com` now redirects to `electric.ax`.
- The survey's strongest claim, that permanent failures are acknowledged so later mutation ids can
  advance, was attributed to Replicache's `concepts/how-it-works`, which does not contain it. The
  rule is in `reference/server-push`; that page is now cited and quoted.
- `rocicorp/replicache` is archived, last pushed 2022-05-07. Recorded so it is not weighted as a
  live peer.

Claims corrected:

- **Cohort error.** The survey compared frontbox against nine systems, eight of which replicate
  *state*. frontbox replays *HTTP commands*, so the original set omitted its nearest peers. Added
  `sources/05-http-command-queues.md` covering Workbox Background Sync, Redux Offline, TanStack
  Query offline mutations, and Amplify DataStore. The headline finding that raw HTTP-envelope
  replay is "uncommon in mature prior art" is true of the replication cohort and false of
  frontbox's own; in that cohort the envelope is standard.
- **Ordering evidence narrowed.** The draft cited Replicache, CouchDB, RxDB, and PowerSync for
  monotonic per-client ordering. Only Replicache holds. CouchDB's `update_seq` is "The current
  database Sequence ID" and its checkpoint is a "Recorded Sequence ID used for Replication
  recovery" — database-scoped resumption, not per-client causality. RxDB checkpoints are resume
  tokens. PowerSync documents FIFO but publishes no per-client causal operation ID.
- **PowerSync stalled-queue claim withdrawn.** Its repeated-front-entry warning is a developer-error
  diagnostic for a connector that fails to call `.complete()`, not queue observability. Stalled-queue
  prior art is CouchDB/PouchDB, RxDB, and Amplify — three systems, not four.
- **Electric under-described.** Electric documents four client write patterns, one keeping "a log
  of local writes in a `changes` table", and rates its own rejected-write rollback as "very naive".
  That is independent support for decision 005, and the draft omitted it.
- **Quarantine novelty rescoped.** No surveyed local-first system implements per-record quarantine,
  which stands. But poison-message quarantine is long established in message brokers, so decision
  006 is borrowed prior art that is unusual in local-first, not an invention.

Decision pages cross-linked. The 2026-08-25 entry above claimed the survey "strengthened" the
`Rejected` dead-letter, fresh auth, atomic outcome application, and corrupt-record decisions, but
those pages were never updated. Each now carries a `## Prior-Art Support` section citing the
evidence, and each `Related:` line now points at the reference page: decisions 003, 004, 005, 006.

Housekeeping: removed a trailing blank line at EOF from all six original raw files, which was
failing `git diff --check`. Added retrieval state to every cited URL, a `## Verification Pass` and
`## Corrections` section to the manifest, and a `## Systems Not Surveyed` section naming the
2024-2026 local-first cohort (Triplit, LiveStore, TanStack DB, InstantDB, Jazz) as a known gap.

Pages affected: `raw/research/2026-08-25-prior-art-survey/manifest.md`,
`raw/research/2026-08-25-prior-art-survey/research-summary.md`,
`raw/research/2026-08-25-prior-art-survey/sources/01-replicache-zero.md`,
`raw/research/2026-08-25-prior-art-survey/sources/02-powersync-electric.md`,
`raw/research/2026-08-25-prior-art-survey/sources/03-rxdb-watermelon.md`,
`raw/research/2026-08-25-prior-art-survey/sources/04-pouchdb-crdt.md`,
`raw/research/2026-08-25-prior-art-survey/sources/05-http-command-queues.md`,
`wiki/references/prior-art-survey.reference.md`,
`wiki/decisions/003-atomic-outcome-application.decision.md`,
`wiki/decisions/004-transport-auth-and-offline.decision.md`,
`wiki/decisions/005-mutation-outcome-policy.decision.md`,
`wiki/decisions/006-corrupt-record-policy.decision.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/plans/d1-core-cache-runtime.plan.md`,
`wiki/plans/prior-art-survey.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`,
`wiki/log.md`

## [2026-08-26] decision | D1 public API shape: envelope extensibility and scope identity

Settled the two D0a follow-ups that a published D1 API could foreclose. Both were recorded as
"must be settled before the D1 public API freezes" in `wiki/plans/d1-core-cache-runtime.plan.md`;
both are now decision pages. User chose the structured-but-uninterpreted option in each case.

**Decision 008 - mutation envelope extensibility.** Three parts. Records are `#[non_exhaustive]`
and constructor-built, so later fields are additive rather than breaking. The body is parsed
`serde_json::Value` instead of the source's pre-serialized `String`, which moves malformed-body
detection from replay time to enqueue time and narrows decision 006's surface. An optional
`OperationMeta { name, version }` rides along; core stores, returns, and copies it across
transitions but never branches on it, because behavior derived from an optional field is behavior
a caller cannot opt out of.

The page deliberately narrows the survey's own framing. Named operations dominate the replication
cohort because the operation is a function and functions cannot be persisted - TanStack Query's
constraint. frontbox replays `{method, path, body}`, which is self-describing, so it does not need
a name to replay. The field is adopted on two narrower grounds: dead letters are unreadable without
a label, and schema drift across long offline windows is otherwise undetectable.

**Decision 009 - local scope identity.** Every store constructor takes a required, non-empty,
caller-composed `ScopeKey`. No unscoped constructor exists. Records are stamped, reads verify by
equality, and a scope mismatch retains the record rather than discarding it. Core never parses the
key; composing principal, tenant, schema version, and read scope into it is the caller's job.

Two findings drove this. First, the source ships **both** an isolated and a shared constructor on
each backend (`persistence/native.rs:70` and `:86`, `persistence/web.rs:81` and `:89`), so isolation
holds only where every call site chose the per-user variant - a logout/login cycle with pending
records can replay the previous user's mutations under new credentials. Second, both backends
sanitize the user id by character replacement (`persistence/native.rs:65`, `persistence/web.rs:33`),
which is **not injective**: safe for the UUIDs it was written for, but two distinct non-UUID
identifiers can collapse onto the same database name. Neither behavior was recorded anywhere in the
wiki; `wiki/specs/source-frontend-cache-architecture.spec.md` now has a
`### Store Identity And User Isolation (as observed)` section covering both.

Prior art supports the boundary but not the enforcement: Replicache warns that "each user of your
application uses a different Replicache `name`", PowerSync warns client parameters are not an
authorization mechanism, and CouchDB treats filters as selection rather than isolation. No surveyed
system enforces it inside the client library. frontbox enforcing equality on read is the same
posture as decision 006 - borrow the established pattern, close the gap where peers rely on caller
discipline.

Ordering between the two: 008 lands first, because stamping a scope key on records is additive only
because the records are already non-exhaustive.

Downstream updates. D1 plan gained the new core shapes, scope-filtered `pending_batch` and
`pending_count` doc comments, and conformance tests 22-25 (cross-scope invisibility, empty-key
rejection, `OperationMeta` transition survival, enqueue-time body rejection). The roadmap's D1
`Depends On`, `Included`, and `Proof` now name both decisions, and D0a's `Unlocks` records that it
produced them. The proposal's two follow-ups are struck through as settled with their acceptance
criteria marked met.

Two new open questions arose and are recorded in `wiki/index.md` and the proposal: how durable
backends encode a `ScopeKey` injectively into a storage name (D5), and whether a cross-scope
diagnostic is needed, since work retained under a scope no store currently opens is invisible to
D1's no-progress signal.

Pages affected: `wiki/decisions/008-mutation-envelope-extensibility.decision.md`,
`wiki/decisions/009-local-scope-identity.decision.md`,
`wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/proposals/extraction-boundary.proposal.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`, `wiki/log.md`

## [2026-08-26] implement | D1 core runtime built, verified, and promoted

D1 was authorized and implemented. The crate exists at the repository root as a single package,
`frontbox`, with modules matching the eventual crate split rather than a workspace — the route
`wiki/proposals/extraction-boundary.proposal.md` already endorsed for the first milestone.

Built: `MutationId`, `ScopeKey`, `Error`, `Clock`/`SystemClock`/`ManualClock`, `MutationIntent`,
`OutboxRecord`, `DeadLetterRecord`, `QuarantinedRecord`, `OperationMeta`, the batch protocol types,
`OutboxStore`/`DeadLetterStore`/`QuarantineStore`, `SyncTransport`, `SyncRunner`, a complete
in-memory backend with failure injection, and a backend-agnostic conformance suite behind a
`testing` feature.

All gates pass via `scripts/verify.sh`: `cargo fmt --check`, clippy with `-D warnings` on native and
`wasm32-unknown-unknown`, 43 tests, and wasm builds with and without default features. The prose
gates the D1 plan listed — no Dioxus, no RepForge entity names, no `Send` bound — are now `grep`
checks in that script rather than intentions. The `Send` gate is textual by necessity, since a
type-level assertion can prove a bound satisfied but never absent; `tests/not_send.rs` backs it by
having `InMemoryStore`, which holds `Rc`s and is therefore `!Send`, implement all three storage
traits.

Three things implementation changed in the design.

**Decision 008 needed an amendment.** Its `OutboxRecord::new(...)` takes no scope, decision 009 rule
2 requires the store to stamp one, and the D1 plan shows `OutboxRecord` carrying a `scope` field.
All three cannot hold for one type. Resolved by splitting into `MutationIntent` (caller-built, this
decision's constructor shape exactly) and `OutboxRecord` (store-built via
`OutboxRecord::stamp(intent, scope)`). This is the line the source itself draws between
`MutationIntentDto` and its `OutboxRecord`, and it strengthens the rule: a caller has no type in
which to put a scope, so there is no forgeable path.

**Decision 010 was written**, recording that the batch payload stays byte-compatible with the source
server. The user chose this over a neutral integer-timestamp payload so the D4 migration trial's
transport is a passthrough. Consequence: `chrono` joins `serde_json` as public compatibility
surface. It is depended on without its `clock` feature, which means `Utc::now()` does not compile
anywhere in the crate — decision 002's clock-injection rule is now enforced by the build rather than
by review.

**`MutationId: Ord` constrains the D5 durable schema**, which no page had said. The tie-break
compares the UUID's 16 bytes; only lowercase canonical hyphenated hex sorts identically, so a
backend storing uppercase, unhyphenated, or differently ordered bytes silently produces a different
pending order — the cross-backend disagreement the tie-break exists to remove. Added to the D1
plan's Ordering Policy and to the open work in `wiki/index.md`.

Four things implementation found that the planning pages had wrong or overstated.

- **Conformance case 25 was mis-specified.** It asked for a malformed body to be "rejected at
  `enqueue`". With a parsed `serde_json::Value` body there is nothing to reject: the malformed case
  is not a value that exists. Corrected in the D1 plan; the residual corrupt-body surface is durable
  corruption after a successful write, which cases 15 and 18 cover.
- **"Port the 12 tests in `persistence/mutations.rs`" overstates what is portable.** Six of the
  twelve assert RepForge route construction. `wiki/references/source-test-inventory.reference.md`
  now carries a mapping table of what transferred and what did not.
- **`frontend/dto.rs` was the more valuable oracle**, because its round-trip tests pin the wire
  format decision 010 commits to. Four of its five tests ported directly.
- **The source's `SyncStatus` does not transfer as a shape.** It is four sticky booleans read in
  priority order, and the source's own test arranges them by writing to private atomics — a status
  reachable only past the public API. `SyncReport` reports per pass instead, with an explicit
  no-progress signal.

One design choice worth recording because it looks wrong at a glance: the in-memory backend holds
every scope in one shared store rather than one store per scope. Scope enforcement has to hold when
two scopes share physical storage, because that is exactly the D5 hazard decision 009 names — a
non-injective storage-name encoding collapsing two scopes into one database. A
one-store-per-scope test backend would have made conformance case 22 pass for the wrong reason.

Conformance cases 26-29 were added beyond the plan's original 25: unrepresentable timestamps,
wire-payload shape, sync re-entrancy, and scope-key non-normalization.

New page: `wiki/decisions/010-batch-wire-format.decision.md`.

Pages affected: `wiki/decisions/010-batch-wire-format.decision.md`,
`wiki/decisions/008-mutation-envelope-extensibility.decision.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/references/source-test-inventory.reference.md`,
`wiki/proposals/extraction-boundary.proposal.md`, `wiki/index.md`, `wiki/log.md`
