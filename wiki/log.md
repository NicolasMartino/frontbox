# Knowledge Log

## Index

Eighty-four entries. **This file is in append order, which is not always date order**: the D3 and D4a
entries carry 2026-08-29 and were written after several 2026-08-30 ones, because those tracks were
logged when they closed rather than when they started. The order is left as written — an
append-only record's sequence is itself information, and re-sorting it would replace *when this was
recorded* with *when it happened*, losing the first to state the second twice.

| Date | Entries |
| --- | --- |
| 2026-08-25 | bootstrap · source cache architecture · source verification · decisions 001-004 · rename to frontbox · implementation reverted · review corrections · second review pass · prior-art survey D0a |
| 2026-08-26 | prior-art verification · D1 public API shape · **D1 built and promoted** · D1 review: four defects |
| 2026-08-27 | review fixes and coverage floor · owned RFC 3339 rendering · D2 prepared (012-015) · **D2 built** · answered the single-flight proposal (016-019) · stale-claim sweep · review fixes to 012, 017-019 |
| 2026-08-28 | observability surface (020) · RepForge revision v2 (021-023) · **D2 amendment: 021 built** · open-decisions register created |
| 2026-08-29 | section C answered · decisions 024-025, D5 writable · **the outbox column set (016, 017, 022)** · open-questions register · 018's profile and 019's docs · 019's status line corrected · **D3: drain loop and Dioxus adapter** · **D4a: migration trial** · stale claims after D3/D4a · D4a's UI |
| 2026-08-30 | all eight answered, 017's liveness claim corrected · **decision 026** · intra-doc links gated · **decision 027** · D4a review fixes (×2) · Docker Compose · Swagger UI · saving-marker reactivity · Chrome background services · **031: the flag moved to the scope** · the adapter's three questions · the trial's read path · D5's plan · **decision 032** · reoriented on RepForge · **D5 core lane** · trial on the row store · **SQLite backend** · **IndexedDB backend** · five IndexedDB defects · **IndexedDB green, 54/54** · **D4b** · **D5 cache half** · **D4c: four platforms** |
| 2026-08-31 | invalidation delivery, and the boundary that outlived its reason (037, 038, D4d) · **whole-worktree review: two cross-backend divergences and stale docs** · **D5 closed: the two-realm drain observed** · **D4d: two domains, one queue, and the first invalidation that ever ran** |
| 2026-09-01 | **legacy sweep: dead schema tolerances, the handle surface, and a gate with a hole in it** · **decision 032 amended: the row loses its two stamps** · **containers rebuilt from scratch; a silent port trap removed** · **nested user→todo CRUD, and decision 039's delete cascade** · **driving the UI in a browser: three defects, one of them in the IndexedDB backend** · **a justfile, and the trial run on all four platforms** · **browser invalidation: stale rows and cross-tab polling** |
| 2026-09-02 | Four-platform lifecycle re-verification · two external reviews · a file-by-file pass · dead-code sweep and stale references |
| 2026-09-06 | **cache versions documented as optional, after an adopter's wrong turn** |
| 2026-09-05 | RepForge queued-write coalescing request recorded · proposed safe frontbox contract · implementation plan drafted · **review of the coalescing commit: the offline release rule, the silent unbound drop, and two migrations that could not run** · **queued-write coalescing built on all three backends (decision 044)** · trial adopts it: observations 15-17b · **external review: a real bypass of the safety mark, fixed with case 80** · **second review: the contract had not caught up with the fix (cases 81, 82)** |

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

## [2026-08-26] fix | Review of the D1 implementation: four defects, two wrong claims

**Summary.** An external review found four real defects — one that could silently stop a queue
syncing forever — plus a set of consistency and documentation gaps. All are fixed. Verifying the
review also turned up two claims I had written that were wrong: a test count that does not add up,
and an ordering constraint stated more narrowly than the truth.

### Defects fixed

- **`SyncRunner::sync_once` could poison itself on cancellation** (`src/runner.rs`). The in-flight
  flag was released only on the success path, so a future dropped mid-`await` left it set and every
  later pass returned `AlreadyRunning` forever — a queue that stops syncing with no error to explain
  it. Cancellation is not exotic here: a Dioxus `use_future` is dropped on every component
  re-render. Now released by a `Drop` guard. Conformance case 30.
- **`apply_outcomes` accepted duplicate ids** (`src/memory.rs`). Two outcomes for one id resolve to
  the same record, so a pair of `DeadLetter` dispositions wrote two dead letters for one row and the
  count stopped matching reality. Now `Error::Protocol`, committing nothing. The runner already
  guarded this internally; the trait is public and had to defend its own contract. Conformance
  cases 31 and 32.
- **Public protocol types were closed while the rest of the API was additive**
  (`src/protocol.rs`, `src/store.rs`). `MutationStatus`, `MutationResult`, both batch types,
  `RemoteRejection`, `Disposition`, and `Outcome` are now `#[non_exhaustive]`, with `new`
  constructors on the batch types since they had none. Recorded on `MutationStatus` is what this
  does *not* buy: the enum still derives `Deserialize`, so an unknown status string still fails the
  whole response. Tolerating unknown wire statuses is a protocol change, not an API one, and
  decision 010 pins the payload to five.
- **Reads silently skipped corrupt rows with no contract saying so** (`src/store.rs`). Documented
  rather than changed, with the two rejected alternatives written down: sweeping inside a read makes
  reads mutate storage, and erroring on a read lets one bad row wedge an otherwise healthy queue —
  the outcome decision 006 exists to prevent. The window is bounded because the runner sweeps every
  pass; a caller that reads without ever syncing must sweep itself.

### Two claims of mine that were wrong

- **The source-test arithmetic did not add up.** `wiki/references/source-test-inventory.reference.md`
  said "only six transferred" *and* "the other six" against twelve tests. The truth is seven
  transferred into six ported tests — the two `SyncStatus` tests collapse into one — and five did
  not, all of them RepForge route construction. The five are now tabulated with what each asserts
  and why the extraction boundary means there is nothing here to test. Corrected in the reference,
  the D1 plan, and `tests/source_oracle.rs`.
- **The `MutationId` ordering constraint was stated too narrowly.** I had written that only
  lowercase canonical hex preserves the byte order and that "any other textual form breaks that
  guarantee". False: ASCII puts digits below both letter cases, so consistently uppercase hex sorts
  correctly too, as does a big-endian `BLOB`. The actual hazard is *mixed* forms — one row uppercase
  and another lowercase inverts any pair straddling the case boundary, and an unhyphenated row sorts
  before a hyphenated one once their first eight characters match, since `-` is `0x2D`. The D5 rule
  is therefore "store exactly what `MutationId::to_string` returns, never transform it, and use a
  binary collation". Four unit tests in `src/id.rs` now demonstrate both inversions instead of
  asserting the rule in prose. Corrected in the D1 plan's Ordering Policy and `wiki/index.md`.

### Structure

- **New page `wiki/specs/frontbox-runtime.spec.md`.** The D1 outcome table had been appended to the
  source spec, which blurred the distinction the extraction depends on: what was observed in
  RepForge versus what was decided for frontbox. Implementation truth now lives on its own page,
  with a pointer from the source spec. It also carries a second table for behaviours the source has
  no position on, which is where the cancellation and duplicate-outcome contracts are recorded.
- **`wiki/proposals/extraction-boundary.proposal.md` moved to `Accepted`**, with a status note
  saying what acceptance does not cover: the crate split, the direct-dispatch ruling, and the
  adapter responsibilities remain untested until D3, D4, and D5.
- **Decision 009 gained an explicit whitespace-only-key stance.** Only the empty key is rejected;
  `" "` is a valid, distinct scope. Rejecting it would be core judging key *contents*, and it would
  catch almost nothing — the realistic bug is `format!("user:{id}")` with an empty `id`, which
  yields `"user:"`.
- **Decision 010 gained a `## Release Follow-Up`** naming three obligations that come due before any
  release: a `wiki/compatibility/` note for `serde_json` and `chrono`, a changelog entry when either
  major moves, and a pinned statement of which server payload version the wire format targets.

### Other

`ManualClock::advance` now saturates rather than wrapping, and `SystemClock` uses a checked
conversion instead of `as`. `AGENTS.md` now bounds what "maintenance" means — public API changes
need the same authorization as a new deliverable. `scripts/verify.sh` locates the repository root
from its own path and labels the three `grep` checks as secondary to the compile gates, with a note
on what each is a proxy for. The `README` and the crate-level docs gained a runnable sync example;
`src/testing/mod.rs` documents the wasm attribute mode the macro already supported.

50 tests pass. All gates green.

### Declined

One review item asked for archived permalinks in
`raw/research/2026-08-25-prior-art-survey/sources/05-http-command-queues.md`. `raw/` is immutable
provenance that the user curates, so this is left for them. The point is a good one.

Pages affected: `wiki/specs/frontbox-runtime.spec.md` (new),
`wiki/specs/source-frontend-cache-architecture.spec.md`,
`wiki/proposals/extraction-boundary.proposal.md`,
`wiki/decisions/008-mutation-envelope-extensibility.decision.md`,
`wiki/decisions/009-local-scope-identity.decision.md`,
`wiki/decisions/010-batch-wire-format.decision.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`,
`wiki/references/source-test-inventory.reference.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`, `wiki/log.md`

## [2026-08-27] lint | review fixes, file splits, coverage floor

Acted on an external code review of the D1 implementation. Five findings, all confirmed against the
code before anything was changed; two of them needed the user's go-ahead because the fix added
public API, and both were granted.

**Duplicate server verdicts no longer resolve by arrival order.** `src/runner.rs` accepted the first
verdict for a repeated `mutation_id` and reported only the later ones as anomalies, while
`src/transport.rs` documented that there is "no basis for preferring either". Those cannot both be
true: first-wins *is* a preference, decided by position in a JSON array. A server answering
`Applied` then `Rejected` deleted the record; the same disagreement in the other order
dead-lettered it. Repeated ids are now found in a pre-scan, every verdict for one is reported, and
none is applied — the record stays queued and is ruled on again next pass, which is safe because
`mutation_id` is the idempotency key. Conformance case 32 was rewritten to assert both orders
produce the same outcome. Verdicts that merely agree are not exempted: deciding two are "the same"
would mean core comparing rejection payloads, which is the judgment it is declining to make.

**The wasm conformance example named a function that does not exist.**
`wasm_bindgen_futures::spawn_local_blocking` is not real, and could not be — a browser cannot block
on an IndexedDB future, because the callback that would resolve it cannot fire until the stack
unwinds. The suite gained `frontbox_conformance_tests_async!` and
`frontbox_fault_injection_tests_async!`, which emit `async fn` cases with no `block_on` at all. Both
emission shapes read one shared case list, so the native and wasm backends cannot drift into running
different suites. The async shape is instantiated in `tests/in_memory.rs` under
`#[allow(dead_code)]`: nothing runs, but the macro is expanded and type-checked on every build,
rather than first being tried halfway through the D5 IndexedDB port.

**Case 33 added, and a wrong proof citation corrected.** `wiki/specs/frontbox-runtime.spec.md` cited
case 21 as the proof that a mutation with no server verdict stays queued. Case 21 is about records
that were never *sent*, which is a different claim. Case 33 sends three records, has the server
answer only one, and asserts the other two are counted in `retained` and are still there to resend.

**Two more public dependencies found.** Decision 010's release follow-up named `serde_json` and
`chrono`. Writing `wiki/compatibility/public-dependencies.compat.md` found `uuid` as well —
`MutationId::from_uuid`/`as_uuid` take and return `uuid::Uuid` — and `serde`, since implementing a
transport means serializing the protocol types. Both arrived through decisions 008 and 009 rather
than 010, which is how a dependency becomes public without any single decision noticing.

**Smaller corrections.** `ManualClock::advance` said "move forward" while taking a signed delta;
documented as a signed move, with backward motion supported deliberately and overflow still
saturating so it cannot happen by accident. `README.md` used `uuid::Uuid::new_v4()`, which forced a
direct `uuid` dependency on a reader — the exact cost `Cargo.toml` argues the default `v4` feature
exists to avoid — and described `made_progress() == false` as meaning stalled, which is also true of
an idle pass. `src/id.rs` now names the binary-collation requirement the wiki already carried.
`ScopeKey` gained a runnable example of the caller-side validation its docs recommend.
`scripts/verify.sh` dropped `grep -q` so a failing textual gate shows what it matched.

**Two standing constraints adopted, at the user's request rather than the review's.** Files stay
under ~400 lines: `src/testing/cases.rs` (984) became a ten-module directory, `src/memory.rs` (562)
a four-module one, `src/testing/mod.rs` split off its scripted transport and its macros, and
`tests/source_oracle.rs` (459) split along the seam its own header already described, with the
`frontend/dto.rs` ports moving to `tests/dto_oracle.rs`. Every public path is unchanged. And total
coverage stays at or above 80%, now gated by `cargo llvm-cov` in `scripts/verify.sh`; the weak
modules the first measurement exposed — `clock` at 27%, `error` at 48%, `scope` at 56% — gained unit
tests and now sit at 94%, 97%, and 98%. Crate total: 89% regions, 96% lines, 93% functions.

Test count moved from 50 to 66. `AGENTS.md` records both constraints and splits its
maintenance-versus-design-change rule into two lists, since the wasm macro question turned on it.

Pages affected: `wiki/specs/frontbox-runtime.spec.md`, `wiki/plans/d1-core-cache-runtime.plan.md`,
`wiki/decisions/009-local-scope-identity.decision.md`,
`wiki/decisions/010-batch-wire-format.decision.md`,
`wiki/compatibility/public-dependencies.compat.md`, `wiki/index.md`

## [2026-08-27] decision | owned RFC 3339 rendering, chrono demoted to a test oracle

**Question asked:** whether a more mature crate could hold the wire format's RFC 3339 compatibility,
given the project is pre-release with no production users and no legacy.

**Checking the candidates changed the answer.** `chrono` is `0.4`, `time` is `0.3.55`, `jiff` is
`0.2.35` — all `0.x`, so all three break on the *minor* position by Cargo's rules. Swapping one for
another would have traded a breaking-on-minor public dependency for a breaking-on-minor public
dependency, leaving decision 010's compatibility problem exactly where it was. chrono was also not
the culprit its reputation suggests: its known troubles live in the `clock` feature, which decision
010 had already excluded, and what remained in use was three functions across two call sites.

**Decision 011** takes the other exit instead. `src/rfc3339.rs` renders and parses `client_datetime`
itself — roughly 130 lines, since the format is one fixed shape with no timezones, no DST, and no
leap seconds — and `chrono` moves to `[dev-dependencies]` as the oracle `src/rfc3339/tests.rs`
compares against. Decision 010's "the bytes match rather than merely resemble" therefore survives as
a test that runs on every build rather than as an argument from delegation.

**The oracle earned its place before it was written.** Probing chrono first showed the obvious
implementation is wrong: a zero millisecond emits *no* fractional part (`…:20Z`), while a non-zero
one emits exactly three padded digits (`…:20.100Z`). Always-three-digits would have round-tripped
perfectly and sent the server bytes it had never seen. The oracle was then checked by breaking the
formatter on purpose — always-three-digits, and truncating division in place of Euclidean — and each
mutation was caught by four separate tests.

**Two behaviours tightened.** The representable range is now RFC 3339's four-digit year
(`0000-01-01` .. `9999-12-31`) rather than chrono's ±262,000; past year 9999 chrono emitted ISO 8601
expanded form (`+10000-01-01T00:00:00Z`), which the RFC 3339 grammar has no production for, so those
payloads were built only to be refused remotely and now quarantine locally instead. And the accepted
parse grammar is written down rather than inherited: no space for `T`, no leap-second `:60`, no
expanded years — each stricter than chrono, each documented with its reason.

`chrono` leaves the compatibility surface, which now holds only `serde_json`, `uuid`, and `serde`,
all major `1`. The runtime dependency graph is `serde`, `serde_json`, `thiserror`, `uuid`; a new
secondary gate in `scripts/verify.sh` asks `cargo tree --edges normal` rather than the manifest,
since the risk it guards against is a date crate arriving transitively. Test count 66 to 77;
coverage 90% regions / 96% lines / 93% functions. All gates pass.

Pages affected: `wiki/decisions/011-owned-rfc3339-rendering.decision.md` (new),
`wiki/decisions/010-batch-wire-format.decision.md` (amended in five places),
`wiki/compatibility/public-dependencies.compat.md`, `wiki/index.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/specs/frontbox-runtime.spec.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/proposals/extraction-boundary.proposal.md`

## [2026-08-27] decision | D2 prepared: decisions 012-015 and the cache-invalidation plan

**Scope.** D2 is the next roadmap deliverable and had no execution plan. This batch writes it plus
the four decisions it forces, and settles one D1 loose end that rhymes with them. **No code.**
`AGENTS.md` gates D2 implementation behind an explicit go-ahead; the point of the order is that the
go-ahead becomes a decision about a known design. D1's precedent argues for it — decisions 008 and
009 were written before D1 was authorized and both constrained the public API, while 010 was written
during D1 and needed an amendment three weeks later.

**Decision 012 — an unknown `MutationStatus` is retained and reported.** Today one unrecognised
status fails the *whole* `MutationBatchResponse`, discarding every verdict in it, on every pass,
forever. Retain is the only disposition that assumes nothing: `Delete` assumes acceptance, and
`DeadLetter` assumes refusal, and both are destructive if wrong. The catch-all keeps the server's
own spelling — `Unknown(String)` — because tolerating a status you cannot name is a stall with no
diagnosis. That costs `Copy` on `MutationStatus`, which is free now and a major version later. It
also forces `SyncReport::anomalies` to carry a reason: the field already means two things, and a
third would leave a caller with a list of identifiers and no way to tell why any of them is there.

**Decision 013 — an unknown entity name is ignored but reported.** Shares 012's principle and
diverges on disposition, deliberately: an unknown status blocks a real record, while an unknown
entity names data this client does not model, so there is nothing to mark stale and ignoring is the
only meaningful action. The source's defect is the silence, not the ignoring — and the reconnect
path is the worse half, since `listener.rs:414` drops unknown names from the server version map with
no log at all, so a drifted client reconciles against a truncated view and reports success. One
event shape, because `LegacyInvalidationEvent` is declared and never used anywhere in the corpus.

**Decision 014 — core reports pending-write conflict; it does not gate the pull.** The harm is
concrete: `eager_refetch` calls `replace_all`, which is `DELETE FROM …` plus re-insert
(`persistence/native.rs:585`), so a queued mutation's optimistic projection is destroyed and the
user watches an offline edit revert. Three findings shaped the answer. Rebase — the survey's only
precedent, Replicache — is unavailable by construction, because it needs replayable named mutators
and frontbox's outbox holds an uninterpreted HTTP envelope by decision 008. A hard gate reintroduces
exactly the failure decision 005 exists to prevent, since one permanently retained record would
freeze every entity's cache forever. And core does not perform the refetch at all; whether read
models belong in core is still open. So core makes the conflict visible at the moment staleness is
read, and the application gates.

**Decision 015 — version and staleness persist together, or not at all.** The trap: the local
version advances at *invalidation*, not at refetch, so immediately afterwards local equals server
while the data is still unfetched, and only the staleness flag remembers the refetch is owed.
Persist the version alone and a restart yields `NoChange` on an entity that was never refreshed —
serving data the server explicitly invalidated. Memory-only fails safe by comparison. Half
persistence turns a safe failure into a silent correctness bug, so the pair is one atomically
written unit, scoped per decision 009.

**What the research changed.** Four things the plan did not have going in: `replace_all` is a
`DELETE`, not a merge, and its only guard is against an empty *server* response; **no caller ever
passes `None` to `update_version`** — all eleven pass `Some(...)` — so the D2 plan deletes the
`Option` rather than documenting its hazard, and decision 007's consequence is superseded;
`LegacyInvalidationEvent` is dead code; and only 3 of the 9 listener tests are D2 oracles, the other
6 being SSE backoff and reconnect delay that belong to D3.

Conformance cases 34-43 are specified in the plan, to be added to the single shared list in
`src/testing/macros.rs`.

Pages affected: `wiki/decisions/012-unknown-mutation-status.decision.md` (new),
`wiki/decisions/013-unknown-entity-name.decision.md` (new),
`wiki/decisions/014-pull-gating.decision.md` (new),
`wiki/decisions/015-cache-version-persistence.decision.md` (new),
`wiki/plans/d2-cache-invalidation.plan.md` (new),
`wiki/decisions/007-generic-entity-key-registry.decision.md` (amended and superseded in part),
`wiki/decisions/010-batch-wire-format.decision.md`, `wiki/index.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/specs/source-frontend-cache-architecture.spec.md`

## [2026-08-27] implementation | D2 built, plus decision 012's D1 change

Authorized immediately after the planning batch above and built the same day. All
`scripts/verify.sh` gates pass: 96 tests (up from 77), coverage 89% regions / 95% lines / 93%
functions against an 80% floor, both wasm builds, clippy clean on both targets.

**Decision 012 shipped first**, since it is a D1 change the D2 work would otherwise have to route
around. `MutationStatus::Unknown(String)` with hand-written serde so the wire form round-trips
transparently, `Retain` as its disposition, and `SyncReport::anomalies` reworked from
`Vec<MutationId>` to `Vec<Anomaly>` carrying an `AnomalyKind`. `SyncOutcomeCounts::unknown_status`
was added beyond the decision so `blocked + pending + unknown` still reconciles against `retained`
without walking the anomaly list. Case 34 asserts that one unrecognised word no longer discards the
verdicts around it.

**D2 itself** is `src/entity.rs` (`EntityKey`, `EntityRegistry`, `SliceRegistry`), `src/cache/`
(`EntityState`, `InvalidationEvent`, `VersionUpdate`, `compare`, `CacheVersionStore`,
`InvalidationRunner` and its report types), and `src/memory/versions.rs`. Cases 35-43 cover it,
behind a new `frontbox_cache_tests!` macro and a `VersionStoreFactory` trait.

**Six things implementation changed from the plan.**

- **`CacheVersionStore` speaks strings, not the application's key type.** The plan had it generic
  over `Self::Key`; that cannot work, because `all_states` would have to reconstruct typed keys from
  storage and a store holds no registry. Worse, a durable store outlives the build that wrote it, so
  it can legitimately hold a name the current registry no longer models. Moving the seam to
  `EntityKey::as_str` keeps that honest and stops D5's backends being generic over an application
  type they never interpret.
- **`EntityRegistry` uses an associated `Key` type**, not decision 007's generic parameter. One
  registry serving several key types is not a thing anyone wants.
- **`VersionStoreFactory` is a separate trait with its own macro**, following the `FaultInjection`
  precedent rather than extending `StoreFactory`. A backend without a version store leaves a visible
  gap instead of a silent pass.
- **An incomplete conflict scan degrades to `Unattributed`.** The plan said the classifier scan is
  bounded but not what a bounded scan should conclude. A false "nothing is queued" is the answer
  that loses data, so a scan that could not see the whole queue declines to give one. Case 42.
- **`InvalidationEvent` drops the source's `user_id`.** Its own comment says the field is
  debugging-only and must not be used for access control; rather than carry a field whose
  documentation is a warning, isolation stays entirely with `ScopeKey`.
- **`src/cache/runner.rs` hit 466 lines** and split into `mod.rs`/`report.rs`/`conflict.rs`. The
  conflict half is the natural seam — it is the only part that reads the outbox rather than the
  version store.

**One gate caught something worth keeping.** `no RepForge entity names in core` fired on
`src/entity.rs`, where prose explaining the source's closed enum still named its variants. Reworded
rather than exempted: the gate is a smell test, and teaching it to ignore comments would blunt
exactly what it is for.

Pages affected: `wiki/plans/d2-cache-invalidation.plan.md` (status and implementation outcome),
`wiki/decisions/012-unknown-mutation-status.decision.md`,
`wiki/decisions/013-unknown-entity-name.decision.md`, `wiki/decisions/014-pull-gating.decision.md`,
`wiki/decisions/015-cache-version-persistence.decision.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`,
`README.md`, `AGENTS.md`

## [2026-08-27] decision | Answered RepForge's single-flight proposal; decisions 016-019

RepForge is rewriting its backend to include frontbox and kafkaman, and sent a proposal asking that
`batch_limit = 1` become frontbox's default, with a durable monotonic sequence added to D5 as the
price. Recorded at `wiki/references/repforge-single-flight-proposal.reference.md` — received in
conversation, not copied into `raw/`, which is human-curated. Answered at
`wiki/proposals/single-flight-drain.proposal.md`.

**Their three claims about frontbox's code all check out**, verified against `src/`: `pending_batch`
already takes a bound and `with_batch_limit` already clamps to 1; `sync_once` is already
single-flight via `in_flight: Cell<bool>` and `InFlightGuard`; `apply_outcomes` is already atomic.
Their reading of decision 010's expiring premise is right, and decision 019 takes it further.

**The ledger was wrong in both directions, and that is what produced four decisions.**

- **The ordering ask is a bug fix, not a price.** They argued a bad sort was survivable under
  batching because the server evaluates a batch in request order. It is not: `runner.rs:263-264`
  builds the request from `pending_batch` order, so a mis-sorted queue is a mis-*ordered batch* and
  the set-before-session hazard exists today at `batch_limit = 100`. Batching never bought causality.
  Accepted as **decision 016** — with one change: make the sequence globally monotonic rather than
  per-scope, since reads are scope-filtered already and per-scope is the version that makes their
  IndexedDB question hard.
- **The cost they missed is the frozen head.** `pending_batch` is oldest-first with no cursor. At
  100 a retained record starves its window; at 1 it freezes the whole queue permanently. Their §4
  argues both that `Retain` is unreachable and that decision 012 matters more during the migration —
  and 012's `Unknown` is a reachable *permanent* retain. Skipping the head is ruled out by decision
  016. **Decision 017** bounds retention with an attempt count and dead-letters at the bound,
  rejecting aging on the merits and closing an open item carried since D1.
- **`Blocked` loses its producer but keeps its hazard.** With no BFF nothing produces a
  `MutationBatchResponse`, so the transport synthesizes verdicts and the *client* now decides
  terminal versus transient. A missing-parent `404` is a 4xx that looks terminal and is not.
  Decision 005's closing caution about Redux Offline now describes frontbox's own transport layer;
  **decision 019** promotes it into a contract on `SyncTransport`.
- **The default is refused.** `with_batch_limit(1)` already works, so the real ask is conformance
  coverage, which is granted as a profile in **decision 018**. The default stays 100, because at 1
  the liveness argument depends on decision 017 being switched on and defaults belong at the safe end
  of a setting.

**Two corrections to their numbers.** Their ~20 s drain estimate for a 200-record backlog assumes
back-to-back requests; `sync_once` sends one batch per call, and at the source's
`SYNC_INTERVAL_MS = 5000` (`persistence/mutations.rs:731`) that is ~17 minutes. And their
pull-gating question was already answered by decision 014 the same day — core does not gate a pull
it does not perform, and D2's per-entity classifier dissolves the starvation their two suggested
directions would both inherit.

Decisions 016-019 are **recorded and unimplemented**; they are design changes gated by `AGENTS.md`.
Implementation order is 017, then 016, then 018's profile, with 019 as documentation.

Pages affected: new `wiki/references/repforge-single-flight-proposal.reference.md`,
`wiki/proposals/single-flight-drain.proposal.md`,
`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`,
`wiki/decisions/017-bounded-retention.decision.md`,
`wiki/decisions/018-single-flight-drain-mode.decision.md`,
`wiki/decisions/019-verdict-synthesis.decision.md`. Amended
`wiki/decisions/005-mutation-outcome-policy.decision.md` (unbounded retention superseded; the
skip-past deferral now rejected on the merits), `wiki/decisions/010-batch-wire-format.decision.md`
(premise note), `wiki/decisions/012-unknown-mutation-status.decision.md` (the permanent stall now
resolved), `wiki/decisions/014-pull-gating.decision.md` (confirmed against a second consumer),
`wiki/roadmaps/extraction.roadmap.md` (D3 drain loop, D5 scope), `wiki/index.md`.

## [2026-08-27] lint | Stale-claim sweep after decisions 016-019

The consistency sweep for the decisions above found four claims elsewhere in the wiki that they
falsify, plus one that predated them.

- `wiki/proposals/extraction-boundary.proposal.md` `## Open Decisions` still listed the monotonic
  sequence as open (settled by 016) and attempt count and aging as unbuilt (partly settled by 017).
  It also still listed **cache version persistence** as open, which decision 015 settled on
  2026-08-27 and which this sweep caught rather than the D2 one. All three struck through with the
  original wording preserved.
- The same page's "Deterministic tie-break ordering" entry claimed the `(created_at, mutation_id)`
  tie-break made a sequence number a durable-backend concern. Annotated: the tie-break bought less
  than the entry says, because the batch is built in `pending_batch` order.
- `wiki/plans/d1-core-cache-runtime.plan.md` said a monotonic sequence "remains open". Marked
  settled with a forward pointer, and the surrounding note left standing as the D1-state record.
- `wiki/specs/frontbox-runtime.spec.md` gained the two pending D5 columns under
  `## Constraints This Places On D5`, flagged explicitly as decided-but-unbuilt so the page keeps
  its as-built-and-tested contract.

**Deliberately not changed:** `src/store.rs:73`, `src/record.rs:190`, `src/runner.rs:152`,
`src/testing/cases/ordering.rs:47`, and `src/testing/cases/invalidation.rs:343` all still describe
determinism-without-causality and unbounded retention. Those doc comments are accurate — decisions
016 and 017 are unimplemented, and editing them would make the code claim behaviour it does not
have.

`scripts/verify.sh` re-run after the sweep: ALL GATES PASSED, 4001 regions at 89.00%, unchanged,
which is the check that no code was touched.

## [2026-08-27] amend | Review fixes to decisions 012, 017, 018, 019 and the single-flight response

External review of the single-flight write-up raised six findings. All six were checked against the
code and all six held, two of them more strongly than the review stated.

- **Decision 018's conformance claim was wrong** and was the review's high finding. It said the
  outbox case list could be re-run unchanged at `batch_limit = 1` as "a third emission of the same
  cases". The suite disagrees. Four cases lose their subject entirely: case 06 applies five statuses
  in one atomic call (`src/testing/cases/status.rs:91`), case 19 needs `Blocked`, which by
  definition names a predecessor *in the same batch* (`cases/liveness.rs:15`), case 32's contested
  verdict needs an uncontested one beside it (`cases/anomalies.rs:46`), and case 34 asserts an
  unknown status "must not spoil the verdicts around it" (`cases/anomalies.rs:160`). Cases 07, 20,
  and 33 assert counts equal to the seed size and need parameterizing. Beyond the review's finding:
  cases 10 and 21 call `.with_batch_limit(2)` and cases 19, 28, 30 call `SyncRunner::new` directly,
  so a profile that parameterizes only the shared `runner()` helper (`cases/mod.rs:86`) leaves them
  running at their own limit — passing, testing nothing, and reporting green. Decision 018 gains
  `## What The Profile Actually Costs`; the correction is propagated to `wiki/index.md`, the roadmap,
  and the response's Implementation Status.
- **Decision 018 also contradicted itself.** Its `Revisit If` already said multi-verdict handling is
  "exercised by cases the single-flight profile cannot reach" — the same cases the Decision section
  claimed would re-run unchanged. Now consistent, and the four are named.
- **Decision 017 said a failed transport means the record was "never evaluated".** False for that
  half of the pairing: decision 004 defines `Error::Transport` as a request that *was* attempted, and
  the runner says so at `src/runner.rs:275`, so the server may have evaluated it and only the
  response was lost. Rewritten to separate offline (never sent) from transport failure (no usable
  verdict), which strengthens the argument — the count measures verdicts received, not requests made.
- **Decision 019 claimed "no server produces one" unscoped.** True of RepForge's redesign, not of
  frontbox's protocol. Scoped, with the note that a transport passing a genuine batch response
  through synthesizes nothing and so owes none of the three obligations. Decision 010's echo of the
  same sentence scoped to match.
- **The RepForge proposal reference claimed `Status: Sourced`**, the label the other three reference
  pages carry on the strength of a `raw/` path. This one has none. Restatused
  `Recorded from conversation; not filed in raw/`, and the provenance note now says plainly that the
  page is a paraphrase and that nothing in it should be quoted as RepForge's own words. Four accepted
  decisions rest on it, so filing the verbatim original under `raw/` is a standing ask on a human.
- **Decision 012's superseded paragraph was interleaved with its replacement.** Split: current policy
  in `## Consequences`, original text preserved verbatim under `## Superseded Text` because 017 and
  018 both cite it as the argument that forced them.

No code changed. `scripts/verify.sh` reports the same figures as before the amendment, which is the
check that confirms it.

## [2026-08-28] decision | Observability surface (020); RepForge proposal re-supplied verbatim

The RepForge single-flight proposal was supplied a second time, framed as carrying observability
changes. It does not: the text is the same document already recorded and answered on 2026-08-27, and
the word "observability" does not appear in it. Recorded here because a reader comparing the two
dates will otherwise look for a delta that is not there.

Two things were still worth doing.

- **Provenance improved.** The second copy is verbatim, so
  `references/repforge-single-flight-proposal.reference.md` gains `## Verbatim Passages` quoting the
  six passages decisions 016-019 turn on — including the §4 contradiction, where `Retain` is called
  "unreachable" four rows above the claim that decision 012 becomes "more valuable, not less". The
  page is still not in `raw/` and its Status still says so; the gap is narrower, not closed.
- **Decision 020 written**, covering frontbox's side of the convergence the request implies.

Decision 020 records the built model — core emits nothing, diagnostics are returned — and why it is
not an oversight. `scripts/verify.sh` gates no date library in the runtime graph, so frontbox cannot
timestamp an event, which for an offline-first queue would date every disconnected write to the
reconnect. Decision 001 and the `!Send` gate rule out handing state to a collector that outlives the
call, which is the useful half of tracing integration.

Two findings from reading the code:

- **The report types are the only public types that do not derive `Serialize`.** `OutboxRecord`,
  `DeadLetterRecord`, `MutationId`, `OperationMeta`, and the cache types all do — storage required
  it. `SyncReport`, `SyncOutcomeCounts`, `SyncPass`, `Anomaly`, `AnomalyKind` do not, because
  nothing needed it until now. So the types a converged pipeline most wants to ship are exactly the
  ones that cannot leave the process unmapped. Additive and free while `publish = false`.
- **Single-flight is an observability regression the proposal does not price.** At `batch_limit = 1`
  the 200-record backlog from its §7 produces 200 reports rather than 2, each with counts of 0 or 1.
  Decision 018 had this as a readability note; it is amended to record that aggregation moves out of
  the library into every caller, and that D3's drain-until-idle loop is the natural aggregation
  boundary. That is a second, independent argument for a loop decision 018 already owed on liveness
  grounds.

Decision 017 amended: last-error metadata is promoted from "a separate question" to load-bearing,
being the only proposed field that survives a restart and explains why a record is still queued.

No code changed.

## [2026-08-28] decision | RepForge revision v2: read model, trace context, sink boundary (021-023)

RepForge revised the 2026-08-27 proposal. The revision withdraws "not asking D2 to change" — which
its own preface calls the most load-bearing claim in the v1 document, correctly — and adds §5.2
(`traceparent` on the record), §5b (the whole read side), and §5b.6 (the observability sink
boundary). Recorded under `## The 2026-08-28 Revision` in the reference; answered at
`proposals/repforge-read-model-convergence.proposal.md`.

**A correction to our own work first.** Decision 020, written earlier the same day, argued that
in-library `tracing` emission was *structurally blocked* by two of the crate's gates. Both claims
were wrong and were checked before withdrawal: `no_send_bound` greps `src/` for `Send` bounds and a
`tracing::info!` adds none, and `tracing` depends on `pin-project-lite`, `tracing-core`, and
`once_cell` — no date crate, so `date_crate_in_runtime_graph` stays quiet. The clock argument was
wrong too: a subscriber stamps at emit time in-process, so an offline pass is stamped during the
offline pass. §5b.6 is accepted. One limit survives and it is the useful one: a span cannot model
enqueue-to-send, because a span is in-process and that interval spans restarts and days — which is
exactly why §5.2's durable trace context is right.

Three decisions.

- **021 — a cache version is an opaque identity, not an ordered counter.** Their Q4 asks whether
  decision 015 assumes ordering. It does, and so does shipped code: `compare` at `src/cache/mod.rs:142`
  reaches `VersionUpdate::NeedsReset` *only* through `<`, so under a set hash that public variant
  loses its producer. Beyond their question: **XOR's identity is zero, so an empty set hashes to
  zero**, and `EntityState::unknown()` is documented as "version zero, not stale" — making *never
  synced* and *legitimately empty* the same stored value, which `compare` answers `NoChange` to.
  That is the same silent-failure class §5b.1 uses to reject `max(updated_at)`, reappearing inside
  the replacement.
- **022 — trace context is durable, caller-supplied, stamped at enqueue.** Storage accepted;
  generation declined. A W3C `traceparent` is mostly randomness, frontbox's only randomness is
  behind the `v4` feature, and `scripts/verify.sh:30` builds `--no-default-features` for wasm to
  prove that path works. The crate already has this rule for `MutationId` and for the same reason.
- **023 — frontbox tracks what is stale, not what the data is.** §5b.3's row shape makes frontbox a
  read-model store, which D5 excludes in one line; the revision does not flag it as a scope change
  and it is the largest one in the document. The justification §5b.4 gives is about the *marker*,
  which a `(scope, entity, row_id, stale)` table grants without the blob. Their permanent-data-hole
  argument is the strongest passage in the revision, and decision 017 widens the hole it closes — a
  record dead-lettered at the attempt bound also terminates with the server state unchanged.

Also answered from built code rather than opinion: Q6 (the registry already takes URL-segment names
— `EntityKey` is implemented for `String` and `&'static str`) and §5b.5 (the two dead-letter causes
already map onto `DeadLetterRecord::error` being `Some` or `None`, which decision 017 chose
deliberately). Decisions 014 and 015 amended with cross-links. 023 resolves the long-standing open
item on read-model persistence.

No code changed.

## [2026-08-28] build | D2 amendment: decision 021 implemented

Authorized and built. The first change to shipped code since D1, and it was affordable only because
`publish = false`.

- **`CacheVersion` is new** (`src/cache/version.rs`, 115 lines): a newtype over `String`, serialized
  transparently, stored byte-exact with no trim or case fold — the reasoning `ScopeKey` uses, since
  every normalization is non-injective. The empty string is accepted, per decision 008's rule about
  fields core never reads.
- **`EntityState::version` is `Option<CacheVersion>`.** `None` means "never heard", which is the
  whole point: under XOR set hashing an empty collection hashes to zero, so the old `0` sentinel
  made *never synced* and *legitimately empty* one value. `EntityState` loses `Copy`, the same trade
  decision 012 made for `MutationStatus`.
- **`compare` is equality only**, and `VersionUpdate::NeedsReset` was removed along with
  `InvalidationReport::needs_reset`.
- **Event collapse is last-wins**, not highest. This was a fifth ordering assumption that decision
  021 had missed — `reconcile_pairs` used `(*current).max(version)`. Last-wins is also more faithful
  independently of hashing: under `max`, an entity that changed and then changed *back* kept the
  intermediate version forever.

**On `NeedsReset`:** decision 021 left this unpicked and leaned to the 005 precedent — keep the
variant unreachable, as `MutationStatus::Blocked` is kept — "absent a reason". Implementing produced
the reason. `Blocked` survives because a *server* can still send it; `VersionUpdate` is produced by
`compare`, which is ours alone, so keeping it would have meant an unreachable arm and a report field
that is always empty. The enum is `#[non_exhaustive]`, so restoring it is additive; a surface that
reports a category which cannot occur is not free. Recorded on the decision page.

**Conformance:** case 37 rewritten in place as
`case_37_a_differing_identity_is_an_update_not_a_reset` — it keeps its number because a numerically
smaller identity is exactly where the old rule and the new one disagree. **Case 44 added**, pinning
the zero-sentinel bug directly: a client that has never synced must still fetch a collection the
server says is empty. `src/testing/cases/invalidation.rs` split at 444 lines, with cases 41-43 moved
to `pull_conflict.rs`.

`ALL GATES PASSED`: 44 conformance cases, coverage 89.06% regions / 95.23% lines / 92.56% functions
against an 80% floor, both wasm builds, clippy clean on native and wasm.

Noted in passing and **not fixed**, being outside this amendment: `cargo doc` reports 17 unresolved
intra-doc links in `src/id.rs`, `src/lib.rs`, `src/memory/mod.rs`, and `src/runner.rs`. All predate
this change and none is in a file it touched. `cargo doc` is not one of `scripts/verify.sh`'s gates,
which is why they accumulated silently.

## [2026-08-28] lint | Open-decisions register

Nine genuinely open decisions were scattered across `wiki/index.md`'s Open Work, six decision pages'
Revisit If sections, and two proposals, mixed in with settled items, work items, and asks addressed
to RepForge. Collected into `references/open-decisions.reference.md` with origin, options,
consequences, and a recommendation for each.

The organizing idea is that urgency here is not importance — it is **which door closes first**.
Three clocks: D5 (schema, closes at the first durable write), publication (public API, closes at
first publish), and RepForge's own design settling. A fourth group has no clock and is listed
separately so it is not confused with the rest.

Two things the collection exposed that the scattered version hid.

- **The index files "total order versus partial order" as a sequence question and it is not one.**
  Decision 016 chose a *globally* monotonic `seq` because global monotonicity gives per-scope for
  free once reads are scope-filtered — and that transfers to origins unchanged. The sequence design
  already serves both orders. What a partial order needs is the ability to partition by origin, and
  core does not know what an origin is. So the real decision is *classifier or column*, and those
  are on different clocks: a classifier is additive and free forever, a column has to land with
  `seq`, `attempts`, and `traceparent` or become a fourth migration.
- **Last-error metadata makes server error bodies durable on the client device.** A rejection
  payload carries whatever the server put in it. That is a data-retention decision and not only a
  schema one, and it is why the field should be transport-shaped rather than a captured raw body.

Also recorded: the argument that settles the `ScopeKey` storage-name question. Reversible encoding
looks valuable and is not needed, because decision 009 already stamps `scope` on every record, so
the mapping back is recoverable from any row's contents. That removes reversibility's only real
advantage and leaves the filesystem's 255-byte component limit, where hashing wins outright.

`wiki/index.md` Open Work now points at the register and its stale gate figures were refreshed to
decision 021's numbers (44 conformance cases, 89.06% / 95.23%).

## [2026-08-29] decision | RepForge answered section C; register now 11 open, decision 019 improved

All four section C questions came back, recorded at
`references/repforge-section-c-answers.reference.md`.
Asking them first was the right sequencing and it paid: **the terminality signal exists because the
question was asked.** Every RepForge service error now carries a required `retry` field of
`terminal` / `transient` / `conflict`, which collapses decision 019's obligation from a judgment
into a lookup for that consumer.

Decision 019 is amended rather than retired, for four reasons. Its rules still bind the fallback
path, since gateway and transport errors never reach a service and carry no envelope. They bind
every *other* consumer's transport, because this is a library. **`transient` gives `Blocked` a
producer back** — RepForge argued it lost one under single-flight, and a transport synthesizing a
retaining status from `retry: transient` is a producer on the client side, which vindicates decision
005's refusal to delete the variant from a direction nobody argued at the time. And `conflict` is a
third end state the type system does not separate: all three land as dead letters,
`Some(rejection)` versus `None` separates decision 017's case, and `RemoteRejection::code` is where
the other two differ — a convention rather than a guarantee, now stated.

**C1 confirmed the accumulator seeds at zero**, so conformance case 44's scenario is live and
decision 021 does not simplify. RepForge volunteered this against their own interest. Their
follow-on — that zero-versus-never-computed *"touches your side as much as ours"* — is wrong in our
favour: `EntityState::version` has been `Option<CacheVersion>` since 2026-08-28 and case 44 asserts
it, and their own suggested remedy is what was built. What remains is theirs alone, since `SetHash`
derives `Default` and a server that never computed an accumulator reports the same 32 zero bytes as
one whose collection is empty. No client representation can reach that.

**A correction to this project's own work.** RepForge read decision 018's ~17 min figure as
RTT-bound and inferred it implied ~10,000 queued mutations. It is cadence-bound: 200 records at one
per pass against the source's `SYNC_INTERVAL_MS = 5000` is 1000 s. The arithmetic in 018 was right
and stated its basis, but `references/open-decisions.reference.md` compressed it to "~20 s versus
~17 min", which reads as two competing estimates. They were never competing — ~20 s is the floor a
back-to-back drain reaches and ~17 min is what the source's loop delivers, and **the gap between
them is the whole value of D3's drain-until-idle loop.** Fixed in both places.

Two new open decisions, taking the register from nine to eleven.

- **10 — does the outbox carry replayable request preconditions.** Their `conflict` is defined as
  `If-Match` failing against the current row hash, and `MutationIntent` has no field for a
  precondition; there is no header concept anywhere in `src/`. An `If-Match` only means anything if
  it carries the version the user was looking at *at enqueue* — computed at drain it is vacuous. So
  it is decision 022's shape exactly, and that is now two instances of "caller-supplied, captured at
  enqueue, replayed as a header", which is where the general case becomes worth considering. It may
  still be app-side: a transport could hold the base hash in the application's read model keyed by
  `mutation_id`. Ask before designing. The two differ in one way that matters — losing trace context
  degrades diagnostics, losing a precondition silently disables conflict detection.
- **11 — what frontbox contributes to the dead-letter report body.** They invited input and made this
  project's own argument back at it. Cheapest item on the register: the field list is already known
  from `DeadLetterRecord`, with the notes that `error: None` is information rather than a gap
  (decision 017) and that `scope` must not be sent, being caller-composed local identity under
  decision 009.

No code changed.

## [2026-08-29] decision | Decisions 024 and 025; the dead-letter reply. D5's plan is writable

Four artifacts, closing three register entries and unblocking D5's plan.

**Decisions 024 and 025 resolved to the same shape, which was not the expected answer for either.**
Both were filed as "frontbox must pick a mechanism". Neither is: core never names a store
(`StoreFactory::open` hands the backend a `ScopeKey` and gets a store back) and core never sees the
quarantine table (`sweep_corrupt` returns a count). So both became **core states an obligation, the
backend picks a mechanism, a conformance case is the enforcement.**

- **024 — storage naming.** The obligation is injectivity, and core supplies no encoder. Writing one
  would have cost a hash dependency in a runtime graph the crate has worked to keep at four crates —
  decision 011 removed `chrono` by owning its rendering rather than depending on it, and adding
  `sha2` so a backend can name a file is the same trade in reverse. Hash-with-readable-prefix is
  recommended, not required. The argument that settles the recommendation: **reversibility is
  unnecessary because decision 009 already stamps `scope` on every record**, so a store's scope is
  recoverable by reading any row in it — which leaves the filesystem's 255-byte component limit,
  where hashing wins outright.
- **025 — quarantine shape.** Decision 006's deferral is upheld, and four properties are named.
  Three are already proven (cases 15, 18, 26). The fourth — atomicity of the quarantine leg under
  decision 003 — is not: case 9 asserts `pending_count` and `DeadLetterStore::count` and says
  nothing about quarantine. A property nobody had written down also surfaced: **a row that fails to
  decode must still be movable**, so the identifier and scope cannot live inside the payload blob.
  That is already forced by ordering and scope filtering, so it costs nothing — but a backend
  storing the whole record as one blob would satisfy every other property and be unable to implement
  `sweep_corrupt` at all.

**Two conformance cases are owed and neither can fail today.** Worth stating plainly because a case
that passes everywhere on the day it lands looks like padding. Every scope key in the suite is clean
— `user:alice`, `user:bob`, `user:a` — so **a backend could ship the source's exact
`tenant/1`/`tenant_1` collision bug and pass all 44 cases.** Decision 009's injectivity obligation
has been unproven since D1.

**The reply to RepForge** answers the dead-letter report body at field level and asks the one
question that decides register entry 10. Three things in it change their schema rather than fill it
in: `error: None` must be a discriminated cause rather than a null, because decision 017 makes
"client gave up at its bound" a different incident from "server refused"; **`scope` must not be
sent** at all, being caller-composed unauthenticated local identity under decision 009; and `body`
should be opt-in rather than default, being the full payload of a user's write. The question is
narrow on purpose — *where does their client get the `If-Match` value at drain time, and is it the
hash from enqueue or the current local one* — because the answer decides a schema column and asking
is cheaper than designing.

**D5's plan is now writable** and is the next artifact. Its job is to fix the outbox column set in
one change rather than four migrations.

No code changed.

## [2026-08-29] build | The outbox column set: decisions 016, 017 and 022 implemented

`OutboxRecord` gains `seq`, `attempts` and `traceparent`; `DeadLetterRecord` carries all three
across the transition. 50 conformance cases, `ALL GATES PASSED`, coverage 89.06% regions / 95.61%
lines / 92.88% functions against an 80% floor.

**The assumption that unblocked this.** The register argued the columns must land as one change or
cost three migrations. That binds D5's *durable* schemas and does not bind core: `InMemoryBackend`'s
state is a `Vec` and a `HashMap`, so a field costs nothing to migrate. So the two column candidates
still unresolved — last-error metadata, and a write precondition pending RepForge's `If-Match`
answer — can join later at no cost, and waiting for them was unnecessary.

Four things the decisions did not anticipate, each recorded on its own page.

- **The runner now emits `Retain` for every sent record still queued** (017). It deliberately did
  not before — "records the server did not rule on get no outcome at all" — and that was right while
  `Retain` was a no-op. Once it means *increment*, silence has to produce one, or a server that
  permanently omits a verdict never advances that record's count and freezes a single-flight queue
  forever. Cases 32 and 33 pass unmodified, which is the check that this changed what the *store*
  receives without changing what the *report* says.
- **A real bug in that change, caught by a test that reused a scope.** The new loop emitted
  duplicate outcomes when the store held two rows under one id, violating `apply_outcomes`'s
  distinctness contract and surfacing as a protocol error blamed on the server. The verdict loop got
  distinctness free from `repeated`; the retain loop now deduplicates explicitly.
- **The bound fires at `attempts >= bound`**, terminating on the pass *after* the count reaches it.
  One extra round trip, and the number on the dead letter is exactly how many attempts were made —
  no off-by-one to explain later.
- **`traceparent` is `#[serde(skip)]`** (022). `MutationIntent` *is* the wire body, so serializing
  the field would have put trace context in the payload and silently changed the shape decision 010
  fixed. It rides the type without riding the wire; the transport reads it off and sets the header.

**Two cases asserted the defect 016 removes and were rewritten in place.** Case 11 asserted
timestamp order, case 12 the UUID tie-break. They keep their numbers — as case 37 did under 021 —
because descending timestamps and a same-millisecond tie are exactly where the old rule and the new
one disagree. Case 12 is now the case 016's Consequences asked for.

New: 46 (order survives a reopen, counter resumes above what it issued), 47 (the bound dead-letters
with no `RemoteRejection` and the count intact), 48 (offline and a failed transport are not
attempts — the count measures verdicts received, not requests made), 51 (trace context is durable,
absent from the payload, carried to the dead letter, and never invented).

**Decisions 024 and 025's owed cases landed the same day**, as 49 and 50. Both pass on the in-memory
backend and exist for D5's — 49 in particular closes decision 009's injectivity obligation, which
had been unproven prose since D1 and which every clean scope key in the suite let a backend evade.

`src/runner.rs` reached 447 lines and was split into `runner/mod.rs` and `runner/report.rs`,
following `cache/runner/`. Stale doc comments removed: `pending_batch`'s "determinism, not
causality" note and case 20's "retention has no bound here".

Still unbuilt: 018's single-flight profile (which had to follow this), 019's transport
documentation, 023's row-level staleness markers.

## [2026-08-29] lint | Open-questions register

Questions waiting on other people were scattered across three proposals' "what we ask in return"
sections, decision 020's open list, and RepForge's own unanswered question to kafkaman. Collected
into `references/open-questions.reference.md`, which is deliberately separate from the decision
register: a decision can be made on any afternoon we choose, a question costs a round trip and can
be ignored.

Collecting them exposed three things the scattered version hid.

- **Three asks have survived one or two rounds without being addressed.** Enqueue order being causal
  order was asked on 2026-08-27 and again on 2026-08-28; the `contracts` crate's wasm constraints
  and early sight of the per-service `PUT` shape were each asked once. RepForge answered section C
  thoroughly and none of these were in section C, so this reads as an artefact of how the
  correspondence was structured rather than neglect — but they have aged, and the first one governs
  whether decision 016 is *sufficient* rather than merely correct.
- **Adding the `retry` field solved the largest problem and created three smaller ones**, all of the
  same kind: turning a three-value enum into something a transport can act on without guessing,
  which is decision 019's failure mode one level down. What stable `code` marks a `conflict`, given
  frontbox's types do not separate it from a plain refusal. Whether envelope-*absence* is really a
  reliable "the gateway failed" signal — a load-bearing invariant stated in passing, where a proxy
  returning a parseable body would make a rate limiter read as a service verdict. And what retention
  bound a RepForge client should set, now that decision 017 is built and ships no default.
- **Nobody from this project has asked kafkaman anything.** RepForge's statement that "kafkaman
  neither pushes nor returns — it emits into a subscriber we installed" is RepForge's description of
  kafkaman, not kafkaman's, and two teams have now built a convergence plan on it. Decision 020 is
  correct about frontbox regardless; what is unconfirmed is the premise that all three libraries
  already agree.

One item sits in both registers and belongs to neither: which `MutationStatus` a transport should
synthesize from `retry: transient`. Decision 019's amendment says this gives `Blocked` a producer
back — but `Blocked` means "skipped because a predecessor *in the same batch* failed", and at
`batch_limit = 1` there is no batch, so `Pending` may be the more honest synthesis. Ours to settle
when 019's documentation is written; recorded because it was found reading their answer, not ours.

## [2026-08-29] build | Decision 018's single-flight profile and 019's transport documentation

53 conformance cases, `ALL GATES PASSED`, coverage 88.96% regions / 95.77% lines.

**018's profile is not the shape its decision page described, and that is the finding.** The page
had already established that re-running the outbox list at `batch_limit = 1` does not work.
Building it showed the *adaptation* does not work either: four cases lose their subject entirely,
and rewriting cases 07, 20 and 33 in place would break them at the default limit, while
parameterizing every case function to know a batch limit threads a value through forty signatures
for the benefit of three.

So the profile is a fifth suite macro beside the four that already exist. That is the pattern
`frontbox_cache_tests!` and `frontbox_fault_injection_tests!` set: a backend that cannot satisfy a
profile does not invoke it, and the gap stays visible in its test file. Three cases, each earning
its place — one record per pass whatever the outcome; a wedged head that freezes the whole queue
until a bound frees it; and the server seeing records one at a time in enqueue order. **The four
exclusions are named in the macro's own documentation**, where a reader looking at the profile will
be.

**019 landed as a `SyncTransport` doc section** carrying the three obligations, a concrete mapping
table for RepForge's `retry` field, and two things the decision did not anticipate.

The status-line fallback rests on **body-absence** being a reliable discriminator between "a service
ruled on this" and "this never got there". The documentation states it and then tells the
implementor to check it holds in their deployment, because a proxy in front of the gateway returning
a parseable error body breaks it — and a rate limiter read as a service verdict dead-letters work
that would have succeeded. That is decision 019's own failure mode relocated one hop upstream, and
it is now open question 6.

And **the vocabulary has no word for "transient"**, which the documentation says rather than forcing
a fit. All three retaining statuses are wrong differently: `Blocked` covers a missing prerequisite
and not a dependency being down; `Pending` asserts the server *accepted* the mutation, which a
transient refusal is not; `Unknown` is honest but reports a routine condition through the channel
that exists for vocabulary mismatches. So the documented obligation is **the disposition, not the
spelling** — a transient condition must retain — and which status carries it is a diagnostic choice.
A variant that means what it says would be the fix; that is a public API change and was not made
here.

Two scope-reuse bugs in new cases, both the same trap the case-48 work hit: the shared `open()`
helper returns one scope, so successive blocks in one case stack rather than start clean. Each block
now opens its own.

## [2026-08-29] lint | Decision 019's status line contradicted its own body

Caught during a state sweep, and worth recording because of *how* it happened rather than what it
was. Decision 019's Status still read "implementation not authorized" while the same page carried
both a `## The Ask Was Granted` section and an `## Implementation Outcome` — a page contradicting
itself in its own metadata.

The cause: two separate edits keyed their Status-line replacement on the wrong date (2026-08-28
rather than 019's actual 2026-08-27), so both silently matched nothing while the body edits, which
anchored on `## Revisit If`, applied normally. A replacement that finds no match is not an error in
any tool involved; it is just a no-op, and the page looked edited because most of it was.

Fixed, and swept: every decision page carrying an `## Implementation Outcome` now has a Status that
says so, and the only page still marked "implementation not authorized" is 023, which genuinely is.


## [2026-08-30] decision | RepForge answered all eight; decision 017's liveness claim corrected

Recorded at `references/repforge-eight-answers.reference.md`, with every claim they make about this
repository checked one by one. Seven checked out. One did not: they state that "RepForge D2 is the
subject of your D4 migration trial", and D4 targets the *exercise* flow — always-enqueue writes and
an optimistic favourite projection — plus reproducing the preferences direct-dispatch pattern in
application code. Their conclusion that D4 will not exercise parent-before-child probably survives,
because a favourite is also flat, but not for the reason given. Also minor: our field is
`traceparent`, not `trace_context`.

**Their finding about decision 017 is correct and 017 is corrected.** `attempts` counts verdicts
received, and both the offline and transport-failure paths decline to increment — so a head that
receives no verdicts never advances its count and the bound never fires. Verified against
`src/runner/mod.rs`, which returns before `apply_outcomes` on both. Under their own transport table
a persistent gateway `502` is a transport error, so the scenario is reachable in their deployment
specifically.

Two qualifications went in with the correction. **The behaviour was right and only the claim was too
broad** — incrementing on a pass that produced no verdict is exactly what 017 refuses for offline
work, and for the identical reason: an outage is not a stuck record, and terminating writes because
a gateway is down is the worse failure. And **the condition is not silent**: `sync_once` returns
`Err(Error::Transport)` rather than a `SyncReport`, so a caller does not even reach `is_stalled()`.
What 017 actually bounds is wedging by *unusable verdicts*, and it says so now.

**Two findings their reply produces and does not notice.**

- **Making `401` transient contradicts their own derivation of the bound.** Q6 argues 8 is safe
  because an outage cannot consume it — outages arrive without an envelope and so are transport
  errors, which do not increment. Q5 then makes `401` transient and says *services* author it, so it
  carries an envelope, so it increments. With a five-minute token lifetime and a bound spanning
  roughly 90 s, a token refresh failing for about that long dead-letters every queued mutation, for
  a condition that clears without the user acting. Both changes are individually right; together
  they need either a bound derived with auth in mind or `401` treated as transport-level.
- **Their third cause kind cannot be reconstructed from a frontbox dead letter.** `watchdog` and
  `retention_exhausted` both arrive as `DeadLetter { error: None }` and produce identical records,
  and `apply_outcomes` is public so an application dead-lettering for its own reason is
  indistinguishable too. `attempts == bound` is a heuristic, not a discriminator. Not a defect in
  their design — their client knows which path fired — but it means frontbox's record carries the
  *fact* of a dead letter without a server verdict and not the *reason*. Whether
  `DeadLetterRecord` should carry a reason is a new open decision, the same shape as the last-error
  question already in the register.

They also re-priced Q7 rather than letting us find out: decision 023 removed the column their ask
reused, so a write precondition is now a fourth durable column and they say so. That is register
entry 10, and it is now answered enough to decide.

No code changed.

## [2026-08-30] build | Decision 026: the write precondition, and why there is no header map

54 conformance cases, `ALL GATES PASSED`, coverage 89.04% regions / 95.87% lines. `OutboxRecord`
gains a fourth durable field — an opaque caller-supplied precondition captured at enqueue, replayed
as a header, never parsed — joining `seq`, `attempts` and `traceparent`.

**The interesting part is what was refused.** Two fields now share one shape: caller-supplied,
stored durably, replayed as a header, never interpreted. That is normally where the general case
gets taken, and a header map was the obvious design. It was refused because **it would quietly
repeal decision 004.** That decision holds that core persists no auth data on outbox records,
because a queued mutation may sit for days and persisting credentials would replay stale or
sensitive state. A generic header map is exactly the mechanism by which an `Authorization` header
ends up durable on disk — not through misuse, but because it is the obvious thing a map is for. A
named field cannot hold a credential without someone deliberately putting it where it plainly does
not belong.

Three supporting reasons, none sufficient alone: the two fields are not the same kind of thing —
one is diagnostic, one is load-bearing for correctness, and a map flattens that into two equally
optional entries; a map is unbounded caller-controlled state in a durable store; and decision 022's
argument against `OperationMeta` weakens but survives, since a transport iterating a map must know
which key it needs, which is a contract in string literals rather than in the type.

The field is opaque because a `PUT` does not reveal whether a mutation creates or updates, so core
cannot know whether the value is a base hash or a wildcard — only the application does. RepForge
asked for exactly that and the reasoning is theirs.

**`SyncTransport` gained a section rather than the line decision 026 predicted**, covering both
header fields together and stating the asymmetry: dropping `traceparent` costs diagnostics, dropping
`precondition` clobbers a concurrent edit with no symptom at all. RepForge's own mitigation is
recorded as the recommended one — a transport that cannot attach the header should refuse to send
and let the record dead-letter, turning a lost update into something a human can see.

`src/record.rs` reached 442 lines and split into `record/mod.rs` and `record/terminal.rs`, on a real
boundary rather than a line count: the pending path against the two terminal states, neither of
which is ever written directly.

This closes register entry 10, the last item that was blocked on someone else.

## [2026-08-30] lint | Intra-doc links fixed and gated

Eighteen unresolved intra-doc links had accumulated, and the reason is the whole point: **nothing
ran `cargo doc`**, so nothing could fail. They were first noticed on 2026-08-29 and left as outside
the amendment's scope, which was the wrong call — by 2026-08-30 there were eighteen rather than
seventeen, and **both of the new ones were added by the work that noticed them**. A defect class
with no gate does not stay the size you found it.

Fifteen were in `src/memory/mod.rs`, where the store traits are referenced by bare name and are not
in scope; those are now fully qualified. Two were `MutationId::to_string`, which comes from
`Display` rather than an inherent method and cannot be linked at all — plain code spans now. One was
a redundant explicit target introduced when `pending_batch`'s ordering note was rewritten for
decision 016.

`scripts/verify.sh` now runs `cargo doc --all-features --no-deps` with `RUSTDOCFLAGS="-D warnings"`
as a primary gate. It sits with the compile gates rather than the three textual ones, because a
broken doc link is a compile-time fact about the crate rather than a grep over its text.

## [2026-08-30] build | Decision 027: a dead letter says why it is one

55 conformance cases, `ALL GATES PASSED`, coverage 89.03% regions / 96.00% lines.
`DeadLetterRecord::error: Option<RemoteRejection>` is replaced by a `DeadLetterReason`, and
`Disposition::DeadLetter` carries one too.

**We sent RepForge this exact argument and had not applied it to ourselves.** Reviewing their
dead-letter report body, this project asked them to model cause as a discriminated field rather than
a nullable error, because serialized as a null it reads as a missing field and the distinction is
lost at the boundary it was built to survive. They accepted, went further, and added a third kind we
did not have — which demonstrated that our own type could not express what they had just been
persuaded to express.

The old field answered two questions at once: *did a server refuse this* and *did it explain
itself*. Decision 017 loaded the first onto the absence of an answer to the second, which held while
those were the only two ways a record could be parked. **They never were** — `apply_outcomes` is
public, so an application has always been able to park a record for a reason of its own and produce
the same `None`. The `Option` now sits inside `Rejected`, where it means the server refused and may
or may not have said why, which is a real distinction decision 005 already recognised.

An enum rather than a second field beside `error`: a `reason` alongside it would permit states that
cannot exist and make every reader learn which combinations are real.

**Case 56 is arranged so no other field separates the three.** Both unexplained kinds have an absent
rejection, and the caller-driven one is parked at an attempt count equal to the bound the other
reached — and the case asserts that equality, so nobody later mistakes `attempts` for the
discriminator. It had quietly become one: its doc comment claimed the count plus an absent rejection
"say which happened", true of two kinds and false of three. Same conflation at one remove.

**The `docs resolve` gate earned itself on the day it was added.** Removing the `error` field left a
doc link pointing at it, which `cargo doc -D warnings` failed on. Without the gate that would have
shipped and joined the eighteen links that accumulated before it.

## [2026-08-29] build | D3: the drain loop, and the Dioxus adapter

Implemented D3, split in two while planning it. **D3a** is `SyncRunner::drain` in core; **D3b** is
`crates/frontbox-dioxus`. `ALL GATES PASSED`, 59 conformance cases, coverage 89.03% regions /
96.18% lines / 93.63% functions on `-p frontbox` against an 80% floor.

Pages affected: `wiki/decisions/028-drain-loop-boundary.decision.md`,
`wiki/decisions/029-drain-termination.decision.md`,
`wiki/decisions/030-serializable-reports.decision.md`,
`wiki/plans/d3-drain-and-dioxus-adapter.plan.md`,
`wiki/proposals/offline-todo-trial.proposal.md`,
`wiki/compatibility/dioxus-adapter.compat.md`,
`wiki/compatibility/public-dependencies.compat.md`,
`wiki/decisions/018-single-flight-drain-mode.decision.md`,
`wiki/decisions/020-observability-surface.decision.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/references/open-decisions.reference.md`, `wiki/index.md`, `AGENTS.md`

**Why this deliverable was next.** Two accepted decisions converged on one unbuilt mechanism from
unrelated directions, which is usually a sign the mechanism is in the right place. Decision 018
wanted it for liveness — at `batch_limit = 1` the poll cadence multiplies the backlog, so 200
records at the source's five-second interval is roughly seventeen minutes against the roughly twenty
seconds a back-to-back drain costs. Decision 020 wanted it as the observability aggregation
boundary, because at limit 1 every count is 0 or 1 and aggregation otherwise moves into every
caller.

**The loop went into core, not the adapter, which is decision 028.** D3 was filed as "the Dioxus
adapter", and that was a filing convenience rather than a judgment. The loop needs no timer (it
waits for nothing), no executor, and no signal. Two things decide where it belongs: an adapter
cannot be decision 020's aggregation boundary without fixing aggregation for Dioxus applications
only and leaving a native SQLite consumer to reinvent it; and the conformance suite cannot reach an
adapter at all, since there is no headless Dioxus runtime here. What genuinely stayed with the
adapter is the **cadence** — how long to wait — which is a policy about a wall clock core does not
have by construction.

**Decision 018's liveness claim was wrong, and building it is what showed that.** 018 said the loop
"must not spin on a retained head, which decision 017 is what makes possible." It does not. 017's
bound counts verdicts *received*, and a back-to-back drain receives them as fast as the network
allows — so a bound of eight against a wedged head becomes eight requests inside a second and then a
dead letter. A policy meaning *give the server eight chances* is spent before the server has had
one, and without a configured bound the same loop is unbounded. So a drain stops on the first pass
that **makes no progress**, not on `Idle` (decision 029). That also supplies a termination argument
018 never had: a continuing pass removed at least one record, so the queue is strictly shorter each
time round, and no pass cap is needed. 018 is amended.

**Case 58 is the assertion that carries it.** A wedged head behind a retention bound of three,
drained, sends exactly **one** request. It fails against the naive reading of 018, which is the
point of writing it. It then drains three more times to show the bound is still reached — across
drains, one attempt each, which is the behaviour a caller's cadence is meant to control. Case 60
measures the gap 018 argued: five records at `batch_limit = 1` drain in one call and five requests.

**`retained` and `sent` on a `DrainReport` are sums over sends, not over records**, and the docs say
so, because a number that looks authoritative and is wrong by the number of retries is worse than no
number. "How much is left" is `pending_count()`.

**Decision 030 closes register entry 5.** Seven report types derive `Serialize` and none derives
`Deserialize`. The entry argued from `#[non_exhaustive]`; the stronger reason is that **nothing
writes a report**, so the reverse derive is semver surface with no caller and a way to fabricate a
diagnostic that code branching on `is_stalled()` would trust. `DrainReport` arriving is what forced
the timing: a new report type either joins the gap or closes it.

**The `no dioxus dependency` gate broke, and the fix was an improvement rather than a workaround.**
It ran `grep -ni dioxus Cargo.toml`, which matches a workspace member named `frontbox-dioxus` — a
`members` entry is not a dependency, so the gate would have failed the one arrangement it existed to
permit. It now asks `cargo tree -p frontbox --edges normal --all-features`, which is the argument
`scripts/verify.sh` already makes in its own comment for the date-library check beside it. Verified
directly: zero Dioxus mentions in core's graph, eight in the adapter's, in one workspace.

**Every gate now names its package.** A bare `cargo clippy` or `cargo llvm-cov` in a workspace
silently changes which crates it covers — the same failure the conformance macros' documentation
exists to prevent, arriving through the build system instead of the case list. The coverage floor
stays on `-p frontbox`, so a regression in core cannot hide behind the adapter.

**Two lint fixes found while amending.** Decision 018 had lost its `## Revisit If` header, leaving
its closing paragraph as orphaned prose that only parses under that heading. And `wiki/index.md`'s
catalog contradicted its own Open Work section on six rows — 016, 017, 019 and 022 were marked
unimplemented while the same file said all were built, and 024 and 025 said "case owed" for cases 49
and 50, which exist.

**The SSE stream adaptation stayed unbuilt, deliberately.** The roadmap lists it under D3. What it
needs is an HTTP client and a reconnect policy, both the application's, and the stream *type* would
be a dependency choice this crate has no business making for a caller. The seam is a method instead:
an application's own handler calls `InvalidationState::apply` and the signals update. What is left
is glue, not a decision, and D4's example should shape it.

**`frontbox-dioxus` is the first frontbox surface with a `0.x` crate in it.** `wiki/index.md` records
that no `0.x` crate is in the semver contract; that is still true of core and is now proven by a
gate rather than asserted, and it is not true of the adapter, which re-exports Dioxus 0.7 types.

## [2026-08-29] build | D4a: the migration trial, API half

Built the trial's API half against the in-memory backend: an axum/sqlx server, a framework-free
application, and ten observations running as tests over real HTTP. `ALL GATES PASSED`.

Pages affected: `wiki/plans/d4a-offline-todo-trial.plan.md`,
`wiki/proposals/offline-todo-trial.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/references/open-decisions.reference.md`, `wiki/index.md`, `AGENTS.md`, `README.md`

**No line of `src/` or `crates/` changed.** The roadmap puts the migration trial before the durable
backends because it "is the only real test of whether the extracted API is right", and the answer is
that four writes, an optimistic projection, a dead-letter surface, a retention bound and direct
dispatch all expressed against the published API with no core change at all. That is stated as a
fact about the core boundary rather than as a judgment: the implementation changed examples and
package/verification wiring, while the same worktree also updated wiki and README bookkeeping.

**The server deliberately does not depend on `frontbox`.** Importing `MutationBatchRequest` would
have been one line and would have deleted the evidence: two ends sharing a type cannot disagree
about it, so the end-to-end run would have proved that serde round-trips. The wire shapes are
hand-written in `examples/todo-server/src/wire.rs` from the source spec and decision 010, which makes
**the ten observation tests the wire-format oracle** — a disagreement of one field name fails all of
them. `AGENTS.md` now records that constraint so nobody removes it as a simplification.

**The client half does not depend on Dioxus, for the mirror-image reason.** If the application logic
needed the adapter, the adapter would be leaking; keeping `examples/todo-core` framework-free is what
makes that falsifiable. It is also what lets the observations run as ordinary tests rather than as
"start it and look", and they are gated as a **primary** gate rather than an example build.

**Observation 3 is asserted in the negative, and that is the point.** The trial cannot show a queue
surviving a reload, so it proves that this one does not: a second application over a second
in-memory backend starts empty. `AGENTS.md` calls this library "a durable local queue of writes that
survives restarts", and *durable* is the adjective D5 supplies. When D5 lands the test inverts, and
that inversion is D4b's deliverable. Owing the observation openly beats quietly not testing it.

**Finding 1, and it was not predicted: nothing names the mutations that drained.** A row rendering
"saving…" needs a row-to-pending index, because `pending_count` and `OutboxCounts` are cardinalities
and `pending_batch` is a queue scan. The proposal predicted the index. What it missed is that the
index **cannot be maintained**: `SyncReport` and `DrainReport` name mutation ids only when something
went *wrong* — `UnknownMutation`, `RepeatedVerdict`, `UnknownStatus` — and nothing names an id that
was applied, deduplicated, or dead-lettered. There is no event to decrement on, so the index is
rebuilt from `pending_batch` after every drain. Affordable per drain; wrong for a queue of ten
thousand records on a phone. Nobody noticed before because every previous consumer was a conformance
case, and a case already knows which ids it seeded. Register entry 12.

**Finding 2: the direct-dispatch bet holds and is incomplete.** One `MutationId`, generated once,
covers the direct `PUT` and the queued replay — no second send path, no third status vocabulary, no
terminal-refusal error variant, which are the three things `extraction-boundary` priced and refused.
What the bet does not cover is deciding *whether* to fall back, which means deciding whether the
direct failure was terminal: decision 019's problem sitting in an application. Fifteen lines, and
every caller writes them differently; the ones who get it wrong queue a permanent refusal forever or
drop a write that would have succeeded. Register entry 13.

**Finding 3: the envelope's routing cost falls on both ends and was priced on neither.** The server
hand-routes `(method, path segments)` because a batch of envelopes defeats an axum `Router`, and the
client hand-parses the path back to recover which row a queued write is about. Neither is a defect
and neither argues against the envelope. Both are worth writing down, because "the envelope is
domain-neutral" reads as free and is not.

**Finding 4: two predicted tensions were not reached**, recorded so that "no problem found" is not
mistaken for "tested and fine". The todo flow sets no preconditions, so the per-record-header
versus batch-request tension never arose; and cache invalidation is not exercised at all, because
the trial refetches the whole list — which is exactly the "refetch everything or refetch nothing"
the proposal predicted, invisible on three rows.

**Two corrections found by re-reading the tests rather than by a gate.** A comment claimed the
direct-dispatch fallback used a fresh mutation id when the code uses the same one, contradicting the
test's own name; and `observation_2b` asserted four requests under a comment saying the confirming
pass sends nothing. The comment was right and the number was wrong: four passes, three requests,
because a pass that reads an empty batch returns `Idle` without touching the transport.

## [2026-08-29] lint | stale claims after D3 and D4a

Six present-tense statements had become false and were corrected. All were true when written; none
was found by a gate, because a gate cannot read prose.

Pages affected: `wiki/roadmaps/extraction.roadmap.md`,
`wiki/decisions/020-observability-surface.decision.md`,
`wiki/decisions/024-scope-storage-encoding.decision.md`,
`wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/index.md`,
`wiki/decisions/018-single-flight-drain-mode.decision.md`

- **D5's scope note said "Decisions 016-019 are recorded and unimplemented".** All four were built
  2026-08-29, in the order that note itself proposed — which is worth recording, because the
  ordering argument it made turned out to hold.
- **D5's scope note said two conformance cases are owed "and neither can fail today".** Cases 49
  and 50 landed the same day decisions 024 and 025 were accepted.
- **D5's scope note said the plan must fix the outbox column set in one change.** No longer D5's
  problem: `seq`, `attempts`, `traceparent` and `precondition` are all built, so D5 inherits a fixed
  record shape rather than a decision. Register entry 12, opened by D4a, is the new one in that area.
- **Decision 020 called decision 017's attempt counter "accepted and unimplemented".** Built.
- **Decision 024 said "the suite does not currently catch it".** Case 49 closed that gap on the day
  024 was accepted. The paragraph is kept under an explicit correction note rather than rewritten,
  because it is the argument that produced the case and paraphrasing it would make the case look
  like padding.
- **The D1 plan's constraint list said authorization "still binds D3 onward".** D3 and D4a are
  authorized and built; it binds D4b onward.

Two more were found earlier in the session and are recorded with the D3 build entry: decision 018
had lost its `## Revisit If` header, leaving its closing paragraph as orphaned prose, and
`wiki/index.md`'s catalog contradicted its own Open Work section on six rows.

One statement was deliberately **not** changed. Decision 021's implementation note says "The suite
is 44 cases", which was true of the change it describes. Decision pages record what an
implementation produced at the time; updating that number would falsify the record rather than
maintain it. The same reasoning is why superseded text is struck through on these pages instead of
deleted.

## [2026-08-29] build | D4a's UI, and the adapter's hooks failing their only consumer

Built `examples/todo-app` with `dioxus 0.7.10`, wasm-clean under `-D warnings`, and it returned the
finding it was built for: **neither of `frontbox-dioxus`'s hooks could be used.**

`use_frontbox` takes the runner by value and `TodoApp` never surrenders it. `use_sync_loop` calls
`runner.drain()`, but `TodoApp::sync()` is `drain()` *plus* `refresh_pending()`, and skipping the
second half leaves every row's "saving…" marker set forever — the UI correct on first paint and
permanently wrong after. The app transcribed ~45 lines of the hook instead. The adapter's plain
values composed unchanged (`Sleeper`, `SyncCadence`, `OutboxCounts`); both hooks did not. They
assume `SyncRunner` is the top of the object graph, and a real application owns it.

A near-miss that compiles is recorded because someone will write it: cloning `InMemoryStore` into a
second runner shares the queue but not the in-flight `Cell`, so `AlreadyRunning` becomes
unreachable and two drains overlap on one outbox. Register entries 14 and 15 opened; the fix is an
API change to a built deliverable and was deliberately not made.

Separately, **`todo-core` had never compiled for wasm.** `reqwest::Error::is_connect` is
`#[cfg(not(target_arch = "wasm32"))]`, and the trial's two gates were host-only — the crate whose
whole claim is portability was the one never built for the target that claim is about. The mapper
is now `cfg`-split on `is_request`, which names on the web what `is_connect` names natively; a
first attempt mapping every wasm failure to `Offline` was rejected because it renders a typo'd
endpoint as a permanent silent offline state. Three gates added: `build trial wasm`,
`build ui wasm`, `clippy ui wasm`. 19 gates, ALL GATES PASSED.

Pages affected: `wiki/plans/d4a-offline-todo-trial.plan.md`,
`wiki/references/open-decisions.reference.md`, `wiki/roadmaps/extraction.roadmap.md`,
`README.md`, `scripts/verify.sh`, `examples/todo-app/`, `examples/todo-core/src/transport.rs`

## [2026-08-30] maintenance | D4a review fixes

Corrected the review findings that were maintenance on the built D4a surface, without staging or
committing anything.

Pages affected: `examples/`, `Cargo.toml`, `AGENTS.md`, `README.md`, `scripts/verify.sh`,
`wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/references/open-decisions.reference.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`, `wiki/log.md`,
`wiki/plans/d3-drain-and-dioxus-adapter.plan.md`,
`wiki/decisions/028-drain-loop-boundary.decision.md`

- `todo-core` routes direct dispatch through `HttpTransport`, so the offline switch and request
  counter apply to direct writes as well as batch sync. Toggles for rows with pending local work are
  queued behind that work instead of raced against a server that cannot know the row yet.
- Direct refusals now carry `RemoteRejection`, roll back the optimistic toggle, and are covered by
  a regression test. The `!Send` proof is a real negative assertion plus a compile-fail doctest,
  not an unconstrained generic helper.
- `refresh_pending` uses an explicit scan cap and reports envelopes whose row id cannot be
  recovered, and `dead_letters` takes a caller-supplied limit.
- `todo-server` refuses duplicate creates, missing-row rename/toggle/delete operations, and
  malformed `done` bodies. The read endpoint returns 500 on database failure, direct refusals keep
  the structured payload, and the in-memory SQLite pool no longer reaps its only connection.
- `todo-app` gained a deterministic `offline` control, a build-time `TODO_SERVER_URL`, visible
  startup failures, no-op rename suppression, and separate sync-error versus direct-refusal state.
- The trial now has ten original observations plus five review regressions. `examples/README.md`
  and `examples/todo-app/Dioxus.toml` make the demo runnable without guessing.

## [2026-08-30] maintenance | second D4a review pass: the repair's own residue

The first repair pass closed every headline finding. A second review of the result found that three
of the fixes had left something behind, and that two documents had been made *less* falsifiable in
the course of making them accurate. All of it is maintenance on the built D4a surface; core and the
adapter are still untouched, and nothing was staged or committed.

Pages affected: `examples/`, `scripts/verify.sh`, `README.md`, `wiki/index.md`,
`wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/references/open-decisions.reference.md`,
`wiki/log.md`

**A fix that discarded a good drain.** `refresh_pending` raised an `Error` when its scan hit the cap
or an envelope's row could not be placed — which discarded the `DrainReport` for work the server had
already committed, and left the *previous*, older index on screen because the newer one was one
record short of perfect. It now publishes the index it can build and returns `PendingIndexGap`
alongside it; the application caches it and the status bar renders it. Two regressions pin both
arms, one of them through a new `with_pending_scan_limit` so the cap is reachable in a test rather
than only at ten thousand records. Recorded in the plan under Finding 1, because "an application
needs to say *my index is n records short* and can only find that out by counting the queue twice"
is more evidence for register entry 12, not less.

**Two local conditions were reported as `Error::Protocol`**, whose own field documentation reads
"what the server sent, and why it is not representable". No server said anything in either case.
Both are now returned data rather than raised errors, which removes the misuse instead of
relabelling it.

**The sentence that started the first review had moved rather than gone.**
`scripts/verify.sh` still said a type-level assertion "can show a bound is satisfied but never that
it is absent" — true when written, and falsified by the negative assertion and compile-fail doctest
the first pass added. The plan's Verification section already had the correct refinement; the script
now carries it: those assertions prove *named types*, and the grep is for a `Send` bound on a type
nobody wrote an assertion for.

**Register entries 14 and 15 existed only as summary rows.** Every other entry carries an
Origin/Options/Recommendation section, so those two now do, under a new **D3b Rework Clock** group
matching the fourth clock the first pass added to the table. The group is deliberately unlettered:
"section C" names RepForge's answered questions across this project and in the correspondence
itself, so renumbering §C to make room would break the one label with readers outside this
repository. `## Sequencing` gained the two steps that place 12 through 15 in order.

**Staleness had been fixed twice by deletion.** `README.md` and `wiki/index.md` had dropped their
test counts and coverage figures rather than correcting them, while the D4a plan kept its own and
was right. Deleting a number does make it stop being wrong. It also removes the thing a reader can
check. Both now carry dated figures again — 19 gates, 116 tests on `-p frontbox`, 59 conformance
cases, 19 trial checks, 89.03% regions / 96.18% lines — with an explicit note that
`scripts/verify.sh` prints today's and is the only thing entitled to be believed about them.

**Smaller things, each found by reading rather than by a gate.** A direct refusal now rolls back the
optimistic toggle only while the projection still holds what that call wrote, so a write that landed
in the meantime is not undone by a refusal that had nothing to do with it. `DELETE` against an
absent row is applied rather than refused — the one method whose replay is idempotent, which turned
into a third bullet under Finding 3: replay-safety is a per-method judgment the envelope does not
carry, and the server decides it by hand exactly as the client decides `classify` by hand. The
server's direct refusals travel as `Json<Rejection>` rather than a `String`, so the response says
`application/json` and means it. `TodoStore::is_saving` uses `contains_key`, matching the invariant
`saving_rows` already relied on. The two test files share `tests/common/mod.rs` instead of two
copies of a fixed clock. The `!Send` doctest pins `E0277`, since a bare `compile_fail` also passes
when the snippet fails for an unrelated reason. `todo-server`'s binary logs a line per request and
shuts down on ctrl-c, which is what a human debugging a wire disagreement by hand actually needs.

**`app.rs` outgrew the four-hundred-line soft limit** while being fixed, so it became `app/` with a
facade `mod.rs`, exactly as `AGENTS.md` prescribes: `direct.rs` holds `Direct` and `classify`,
`index.rs` holds `PendingIndexGap` and `row_id_of`. The split follows the two findings rather than
the line count, and no public path moved — `todo_core::app::Direct` still resolves, and a `use`
statement cannot tell the difference. The plan's three citations of `src/app.rs` were repointed at
the file each item actually lives in now.

**One gate was considered and refused.** Building `todo-app` with `TODO_SERVER_URL` set, to exercise
the `option_env!` branch, would report `ok` for work it did not do: Cargo does not track arbitrary
environment variables for rebuilds, so the second build returns the first one's artifact. The
script's own comment already says a gate like that is worse than none, and the reasoning is now in
the plan so nobody adds it later thinking it was overlooked.

## [2026-08-30] maintenance | D4a Docker Compose launcher

Added a Docker Compose path for the todo trial's human-run demo. `todo-server` builds into a slim
Debian runtime image, binds `0.0.0.0:3000` in the container, and exposes a health check against
`/api/v1/todos`. `todo-app` builds with a matching `dioxus-cli` version, compiles
`TODO_SERVER_URL` into the WASM bundle, and serves Dioxus' static web output through nginx.

Pages affected: `.dockerignore`, `docker-compose.yml`, `Cargo.toml`, `crates/frontbox-dioxus`,
`examples/README.md`, `README.md`, `examples/todo-core`, `examples/todo-server`,
`examples/todo-app`, `wiki/log.md`

The important operator rule is now written down: the URL baked into the browser bundle must be
reachable from the user's browser, so the default is `http://127.0.0.1:3000`, not Docker's internal
`http://todo-server:3000` service address. Changing the published API port requires rebuilding the
UI image with the matching `TODO_SERVER_URL`.

The UI host port defaults to `8081`. The first launch attempt used `8080`, which was already
allocated on the development machine; keeping that as the default would make the documented
one-command demo fragile.

The first container build also exposed stale MSRV metadata: every manifest still said Rust 1.85,
while the locked dependency graph contains crates requiring Rust 1.88. The workspace
`rust-version` fields and Docker builder images now say 1.88, matching the graph that actually
builds.

## [2026-08-30] maintenance | Swagger UI for the D4a todo server

Added `utoipa` and `utoipa-swagger-ui` to `examples/todo-server`, with vendored Swagger UI assets
so Docker and CI do not download the UI bundle at build time. The server now exposes Swagger UI at
`/swagger-ui/` and the generated OpenAPI JSON at `/api-docs/openapi.json`.

Pages affected: `examples/todo-server/Cargo.toml`, `examples/todo-server/src/lib.rs`,
`examples/todo-server/src/wire.rs`, `Cargo.lock`, `README.md`, `examples/README.md`,
`wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/log.md`

The annotations document the same hand-written wire structs the trial already used. This preserves
the evidence boundary: the example server still does not depend on `frontbox`, and Swagger does not
turn the wire oracle into a serde round-trip.

## [2026-08-30] maintenance | saving marker reactivity in the todo UI

Fixed the row-level `saving...` marker in `examples/todo-app`. `TodoList` already subscribed to the
manual revision signal and `TodoApp::sync()` already rebuilt the pending index after a drain, but
`TodoRow` read `is_saving` from context while its only prop was the unchanged `Todo`. Dioxus could
therefore skip the row after the pending index changed, leaving the old marker rendered forever.

Pages affected: `examples/todo-app/src/rows.rs`, `wiki/log.md`

The parent now computes `saving` while reading the row list and passes it as a prop. A drained row
therefore receives a changed prop even when its title and done state did not change.

## [2026-08-30] research | Chrome's background services, and a guard that guards the wrong thing

Two questions about Chrome DevTools — whether Background Sync, Background Fetch, or the
back/forward cache are usable, and why no frontbox cache appears in the browser's storage panel.
The second has a short answer; the first produced a D5 blocker.

Pages affected: `wiki/decisions/031-cross-realm-single-flight.decision.md`,
`wiki/proposals/browser-background-services.proposal.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`,
`wiki/index.md`, `wiki/log.md`

**The storage panel is empty because it is correct.** `examples/todo-core/src/app/mod.rs:57`
constructs `InMemoryBackend::new(clock)` and that is the only backend the trial has; the queue is an
`Rc<RefCell<State>>` in the page's wasm linear memory. Every IndexedDB reference in this repository
is a wiki page or a doc comment about D5. D4a proves offline tolerance *within a session*, which is
the line its own proposal drew.

**Background Fetch is the wrong instrument** — browser-managed UI for large resumable transfers,
against payloads that are small JSON POSTs needing retry rather than resume. **Background Sync is
the right shape and cannot be built yet**: a service worker is a separate realm and cannot read wasm
linear memory at all, so it needs D5's shared store to exist first. It is also Chromium-only, so it
could only ever be an accelerator over the in-page loop, and its own retry schedule would be a third
retry policy stacked on decision 017's bound and the cadence's backoff.

**The finding needs no service worker.** `in_flight` is a `Cell<bool>` on `SyncRunner`
(`src/runner/mod.rs:46`), while `InMemoryBackend::open` (`src/memory/mod.rs:142`) hands back a
handle over shared state (`:113`). Two handles on one scope are two views of one queue, and each
runner observes its own flag false. A drain pass is *read batch* → POST → *apply outcomes*: two
transactions with a network round trip between them, so storage atomicity never overlaps and cannot
help. The roadmap's own note that the crate "was already single-flight at the pass level" is
accurate and is exactly the gap — **pass level means per runner.**

Idempotency survives it: the server dedupes on `mutation_id`. Ordering does not, and neither does
the retention bound — at `batch_limit = 1` a second drainer ships record 2 while record 1 is in
flight, which is the guarantee decision 016's monotonic `seq` exists to provide.

The D4a plan recorded this as a near-miss a developer could write. It stops needing a mistake the
moment storage is durable: **two browser tabs on one origin are two realms over one IndexedDB
database.** Decision 031 states the obligation in the shape decisions 024 and 025 established —
core states it, the backend picks the mechanism, a conformance case enforces it — with one
difference worth naming: unlike cases 49 and 50, which pass vacuously on in-memory, this case
**fails on the backend that exists today**.

Register entry 17 holds the API question of where the flag lives, because moving it off
`SyncRunner` changes a built deliverable and was recorded rather than made. Entry 16 holds the
bfcache wake-up: a frozen page's timers do not fire, so the cadence resumes believing it is
mid-sleep while `Date.now()` has jumped forward. It batches with entries 14 and 15 as one touch of
D3b, since 15 asks whether the adapter owns the browser clock and 16 asks whether it owns the
browser lifecycle.

Usable today and unchanged: **Network → Offline exercises the real offline path**, and specifically
because of D4a's wasm fix — the throttle rejects `fetch`, reqwest reports `Kind::Request`,
`is_request()` maps to `Error::Offline`, the queue is retained. A token service worker registered
only to populate the DevTools panels was rejected: it would record an empty stream, having nothing
to say about a queue it cannot see. The real DevTools win arrives with D5 for free, when the outbox
becomes a live table in the Application panel — which is a second, independent argument for storing
`mutation_id` as `to_string` text, a rule decision 009 already imposes for ordering.

No code changed in this slice. The next one changed it the same day; see below.

## [2026-08-30] build | The flag moved off the runner and onto the scope

Decision 031 implemented, in-realm half. Register entry 17 closed the day it opened, entry 18
opened by reviewing the work.

Pages affected: `src/runner/exclusion.rs`, `src/runner/mod.rs`, `src/runner/report.rs`,
`src/runner/drain.rs`, `src/store.rs`, `src/testing/cases/single_flight.rs`,
`src/testing/macros.rs`, `crates/frontbox-dioxus/src/handle.rs`,
`crates/frontbox-dioxus/src/sync.rs`, `examples/todo-app/src/sync.rs`,
`wiki/decisions/031-cross-realm-single-flight.decision.md`,
`wiki/references/open-decisions.reference.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/plans/d4a-offline-todo-trial.plan.md`,
`wiki/index.md`, `wiki/log.md`

**Entry 17 promised two things that could not both be kept, and finding that out is what settled
it.** The entry recommended a per-scope flag inside the backend with *no public signature changes*;
the roadmap's proof line promised the second drainer *observes `AlreadyRunning`*. `OutboxStore` has
no channel to say "busy elsewhere" — `pending_batch` returns rows or an error, and an empty batch
makes the runner report `Idle`, which is false while records are pending. No backend can deliver
`AlreadyRunning` unless core gains a way to ask it, which is the change the entry was avoiding.

So the obligation split, and each half sits where it can be enforced. **In-realm is core's**:
`src/runner/exclusion.rs` holds a thread-local set of draining `ScopeKey`s, and `sync_once` claims
the scope and releases it on drop. The `Cell<bool>` and its `InFlightGuard` are gone. No public
signature moved and no dependency arrived. **Cross-realm stays the backend's**, in prose on
`OutboxStore` in the shape decisions 024 and 025 use: two tabs are two wasm instances with their own
memory, and nothing process-local can reach the second one.

Release on drop is now the *only* release path, which makes case 30 load-bearing rather than
belt-and-braces: a cancelled pass — routine, since a Dioxus `use_future` is dropped on every
re-render — has to free the scope or nothing ever drains again. The claim also never holds a
`RefCell` borrow across an `await`; insert and remove are synchronous, which is what makes a
`RefCell` legal here at all. And `try_with` rather than `with` in both places, because the guard
runs in a `Drop` and `with` panics once thread-local storage is torn down.

**Case 61 was verified by making it fail.** Two handles on one scope, the first drain suspended
mid-flight, the second asserted to stand down without sending. Reverting `sync_once` to the
runner-local guard makes it report `Completed` — the second runner drained the same queue — which is
the defect stated as a test. **What it does not prove is written into its own doc comment**: both
runners live in one realm, because a conformance suite drives a backend through one process, so a
durable backend will pass it without holding a cross-realm lock. D5's evidence has to be a two-realm
fixture instead, and the roadmap now says that rather than claiming case 61 covers it.

Exclusion is keyed by scope identity rather than store identity: two runners over two *different*
backends opened under one key exclude each other. That is the intended reading of decision 009 — if
they are not the same queue, the key was not injective.

**Entry 18 came out of the review**, which asked why there is not one table, one runner and one
drain per cached type. Recorded rather than dismissed, because the answer is not obvious: it would
buy head-of-line isolation, which case 53 shows is a real cost. Against it — a mutation has no type
(`MutationIntent` is a replayable method, path and body, and one `POST` can touch several entities,
which is why invalidation returns a *set*); it trades away the cross-type ordering decision 016 made
a guarantee rather than an artifact; and it does not dissolve decision 031, it re-keys it to
`(scope, partition)`. The shape worth arguing about, if the answer ever changes, is a
caller-supplied partition key rather than a taxonomy core invents. On the D5 clock as a durable
column; not blocking.

`ALL GATES PASSED`, 60 conformance cases, 117 tests on `-p frontbox`, coverage 89.02% regions /
96.18% lines / 93.72% functions.

## [2026-08-30] build | The adapter's three open questions, taken together

Register entries 14, 15 and 16 closed as one change, which is what sequencing note 6 asked for: one
crate, one audience, and three breaking releases where one would do. The D3b column of the clock
table is empty for the first time since it was added.

Pages affected: `crates/frontbox-dioxus/src/sync/mod.rs`, `crates/frontbox-dioxus/src/sync/cadence.rs`,
`crates/frontbox-dioxus/src/sync/steps.rs`, `crates/frontbox-dioxus/src/sync/wait.rs`,
`crates/frontbox-dioxus/src/web.rs`, `crates/frontbox-dioxus/src/lib.rs`,
`crates/frontbox-dioxus/Cargo.toml`, `examples/todo-app/src/main.rs`,
`examples/todo-app/src/sync.rs`, `examples/todo-app/Cargo.toml`, `src/clock.rs`,
`scripts/verify.sh`, `wiki/references/open-decisions.reference.md`,
`wiki/compatibility/dioxus-adapter.compat.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/proposals/browser-background-services.proposal.md`,
`wiki/log.md`

**Entry 14 — the hook asks for capabilities, not for an object.** `use_sync_loop(sync, counts,
cadence, sleeper)`, where `SyncStep` is "run one sync and tell me how it ended" and `CountsStep` is
"read the counts", both `Rc`-boxed and type-erased on `Sleeper`'s precedent.
`use_sync_loop_on(handle, ..)` keeps the old behaviour for an application that lets Dioxus own its
runner, and `FrontboxHandle` is untouched.

**The evidence is `examples/todo-app`'s diff.** Its hand-rolled loop is gone: `TodoApp::sync` —
`drain` *plus* `refresh_pending`, whose second half rebuilds the index `is_saving` answers from —
goes into the sync closure whole, and the dead-letter poll and revision bump ride in the counts
closure. The module doc that explained why the hook could not be used now explains what changed.
Entry 14's own complaint was that "a hook that compiles proves only that its signature is
well-formed", so a signature change with no consumer would have proved nothing.

**Entry 15 — `WebClock` behind a `web` feature**, default off. `examples/todo-app` deleted its own
five lines and its direct `js-sys` dependency, which is entry 15's "every browser consumer writes
the same five" stated as a diff. Core's sentence pointing at "their adapter" now names what supplies
the clock — in prose, not as an intra-doc link, because core does not depend on the adapter and the
`docs resolve` gate would say so.

**Entry 16 — the wake went on the wait, not on the loop.** `Wake` is a cloneable latch and
`Sleeper::wakeable` races it against the sleep, which kept `use_sync_loop`'s signature from changing
twice in one release. `use_bfcache_wake()` registers the `pageshow` listener and fires when
`persisted` is set, removing itself when its component drops. `Wake` is public *without* the
feature, which answers the entry's second option for free.

Two details worth keeping. A wake with nothing waiting is **latched**, because a restore arriving
mid-drain that silently did nothing would be the same defect one level down. And the race is
hand-written: this crate has no `futures` dependency, and taking one to poll two futures — one of
them already a `Pin<Box<_>>` — is a dependency per line of code saved.

**The adapter has tests now, and it earned them.** D3b's proof was "compiles for two targets", which
was honest when everything here was hooks. A latch and a race are plain Rust: five tests, driven by
a counting waker rather than `Waker::noop`, because what makes the wake work in a real executor is
that it *reschedules* — a future returning `Pending` without arranging to be polled again is a hang
no noop-waker assertion can see. `verify.sh` gains `tests adapter` and two gates that compile the
`web` feature for wasm, while the feature-less gates keep proving a desktop build takes none of it.

**Measured in a browser, because no test here can reach the listener.** Chromium under an automation
protocol will not back/forward-cache a page, so the restore event was synthesised and everything
downstream of it is the real path: a write queued, the app back online, and the queue drained
**38 ms** after the event against **15 015 ms** for the identical sequence without it —
`offline_ms` served out in full. The same run confirmed the rewritten loop end to end: drains on
mount, three writes queued behind the offline switch with `last drain: Offline`, and all three
cleared on reconnect, with no console errors.

`ALL GATES PASSED` — 21 gates now. 60 conformance cases, 117 tests on `-p frontbox`, 5 on
`-p frontbox-dioxus`, coverage 89.02% regions / 96.18% lines / 93.72% functions.

## [2026-08-30] fix | The trial's read path, and a seam with green gates on both sides

Reported: the todo UI showed an empty list after every reload, while Swagger's
`GET /api/v1/todos` returned rows.

Pages affected: `examples/todo-core/src/app/startup.rs`, `examples/todo-core/src/app/mod.rs`,
`examples/todo-core/src/lib.rs`, `examples/todo-core/tests/regressions.rs`,
`examples/todo-app/src/main.rs`, `wiki/plans/d4a-offline-todo-trial.plan.md`,
`wiki/references/open-decisions.reference.md`, `wiki/log.md`

**The UI never read from the server.** `TodoApp::refresh_from_server` existed all along and
`examples/todo-app` never called it, so the browser projection held only what was typed in that
session and every reload started from `TodoStore::default()`. Swagger disagreeing was the good
news: those rows were on the server because the outbox drained and the POSTs succeeded. The write
path was working end to end; only the read path was unwired.

**Six callers, all of them tests.** `observations.rs:73`, `:209`, `:299` and `regressions.rs:52`,
`:94`, `:129` — and nothing in the application. Every test drove the startup sequence itself,
because arranging a world is what a test does, so the suite thoroughly proved the method *works*
while nothing proved anybody *called* it.

**This is Finding 5's shape a second time.** There, the framework-neutral half had never been
compiled for the browser. Here, the startup sequence lives in the one crate with no tests, whose
only gates are `build ui wasm` and `clippy ui wasm`. Twice now the defect has been in the seam
rather than in either thing the seam joins — which is why the fix relocates the sequence instead of
only adding the missing line.

`TodoApp::start` hydrates and then rebuilds the pending index, returning a `Startup` that says
whether the server was actually read. `Error::Offline` becomes `hydrated: false` and a clean `Ok`,
matching the runner's own posture — `SyncPass::Offline` is an `Ok`, every other transport error is
an `Err` — because refusing to open with no network would be refusing to do the one thing an
offline-first cache promises. A wrong `TODO_SERVER_URL` still propagates and stays loud, which is
the distinction `map_send_error` was written to preserve.

Two regression tests. The first builds a *second* application on the same scope with an empty
projection — a reloaded tab — hands it nothing but `start`, and requires the rows to appear; it was
confirmed to fail against a `start` with the hydrate removed, which is what makes it a regression
test rather than a copy of what the six callers already proved. The second asserts an offline start
succeeds with `hydrated: false` and an untouched projection.

**Register entry 19 records what the fix exposed**: `TodoStore::replace` clears and reinserts, which
is safe only because the outbox is empty at mount. With a durable queue a reload would hydrate over
still-queued writes and drop their optimistic rows — the user watching their own unsent work vanish
and return one drain later. Clock D4b, and it is decision 023 arriving somewhere new.

`examples/todo-app` still has no tests. A `verify.sh` grep for `start(` in `main.rs` was considered
and rejected: it would catch this line and nothing else in its class while reading like coverage.

## [2026-08-30] plan | D5's execution plan, and what persisting the read model changes

The roadmap has recorded `Execution Plan: Not created yet` for D5 since it was drafted. It exists
now, with two scope choices made by the user the same day.

Pages affected: `wiki/plans/d5-persistence-backends.plan.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/index.md`, `wiki/log.md`

**Both backends together, not split web-first.** The alternative was IndexedDB alone, which reaches
a visible browser storage panel sooner. Building both means cross-backend conformance catches
divergence between the implementations rather than deferring it to whenever the second one lands.

**The application persists its own read model.** This is the choice with consequences the roadmap
did not carry. Decision 023 keeps read-model rows out of frontbox, so an application that wants
offline *reads* opens a second durable store beside frontbox's, with its own schema, in every
consumer. That is the honest price of the boundary and D4b is the first time anyone pays it — if it
turns out to be large, that is a finding about decision 023 rather than a defect in the plan.

It also promotes register entry 19 from a footnote to the centre of startup. With local rows, a
durable queue, and a server, `start` has three sources that must agree, and `TodoStore::replace`
cannot be one of the steps: the server has not seen the queued writes, so replacing drops exactly
the rows the user is waiting on. The merge needs `row_id_of` to recover the *change* a queued
envelope makes rather than only the row it names — a real widening, and evidence for entry 12
arriving from a second direction.

**D4b's proof needed adjusting as a result.** Its promise was that the diff touches only the
construction site. Application-side persistence is a deliberate change above the seam, so the diff
now has to be read in two parts — what the backend swap forced, and what was chosen — and the
roadmap says to keep them in separate commits, or the evidence is unreadable.

Three things the plan names that were not obvious. **`rusqlite`, not `sqlx`**, because sqlx is built
around `Send` futures that decision 001 and the `no Send bound` gate both rule out; the source's
`Arc<tokio::sync::Mutex<Connection>>` becomes `Rc<RefCell<Connection>>`, the same structure claiming
one fewer guarantee. **`AUTOINCREMENT` is load-bearing** in SQLite: plain `INTEGER PRIMARY KEY`
reuses values after deletes, which would invert the pairs decision 016 exists to order. And **an
IndexedDB transaction closes when the event loop yields**, so `apply_outcomes` may not await
anything that is not an IDB request — the most likely source of a bug that passes in-process and
loses atomicity in a browser.

The cross-realm half of decision 031 is D5's, and the plan flags that its conformance case may not
be automatable in this suite, since every other case runs in one realm by construction. If it
cannot be, the plan says to record the gap rather than ship an obligation nothing enforces: a gate
that cannot fail is worse than an acknowledged gap.

No code was written.

## [2026-08-30] decision | The rows come inside: decision 032 supersedes 023's one sentence

The user challenged decision 023 directly: an offline cache that stores no cached data "reduces
frontbox to a queue", and should it not own the read model, persistence, encryption, migrations —
the way TanStack and its peers do? The challenge was examined against the prior-art survey rather
than defended against, and it holds.

Pages affected: `wiki/decisions/032-opaque-row-store.decision.md`,
`wiki/decisions/023-read-model-boundary.decision.md`, `wiki/plans/d5-persistence-backends.plan.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`,
`wiki/index.md`, `wiki/log.md`

**The survey's own tables decide it.** frontbox's declared peer group is the HTTP command-queue
cohort — Workbox, Redux Offline, TanStack Query — and two of those three persist application data
as opaque blobs: TanStack the query cache itself, versioned and never interpreted; Redux Offline
the whole store via redux-persist with the outbox inside it. The pure-queue point 023 defended is
occupied only by Workbox, which the survey itself uses as the counterexample that validates
decision 005. And `Cargo.toml` calls the crate an "offline-first cache" — a description the code
did not meet.

**023's revisit clause had already fired.** It said to revisit if applications reimplement the same
blob store above frontbox; the D5 plan's Track D was the *first* application planning exactly that,
before a line of D5 existed. Three more costs traced to the same sentence: the hydration merge as
unaided per-application homework (entry 19), markers that outlive rows frontbox cannot see, and
`row_id_of` re-deriving by parsing paths what the application knew at enqueue and threw away.

**Decision 032, in three parts.** A durable `(scope, entity, row_id) → (blob, version, stale,
schema)` store — the marker table 023 already committed to D5, with the blob riding on it; an
optional row binding on `MutationIntent`, `#[serde(skip)]` beside `traceparent` and `precondition`
so decision 010's wire payload is untouched; and the merge as a library rule — a hydration write
skips rows with pending bound mutations. What 023 got right survives explicitly: markers,
application-owned meaning and serialization and queries, no position on canonical bytes, and
decision 014's pull boundary.

**Where the line stops.** No query layer, no indexes on app fields, no reactive subscriptions —
that is the Replicache/RxDB/PowerSync product, and going there means competing with mature systems
while the actual consumer already owns a read model. The full-database option was put to the user
and declined in favour of the TanStack point. Blob versioning ships with the store (schema stamp
per row, migration function the application's — RxDB's model); encryption is far-future by the
user's explicit choice, with D5 leaving one read path and one write path per backend as the seam
where a cipher could sit.

Fallout: entry 19 closed the day it opened — merge, enforced, D4b builds it. Track D of the D5
plan rewrote from "the application's second storage stack" to the row store, binding, and merge.
Entry 12 gains a second piece of evidence: bound row ids are exactly what a drain report could
name. And `row_id_of` is scheduled to retire, replaced by the binding it was reverse-engineering.

No code was written. `MutationIntent`'s new field, the `RowStore` trait, and the conformance cases
are D5 build work.

## [2026-08-30] plan | Reoriented: the target is RepForge starting for real

The user's direction: speed up, and build as close as possible to a production footing so RepForge
can actually begin. Recorded as a reorientation of the D5 plan rather than a new plan, because the
critical path did not change — what changed is what rides alongside it and what gets out of its
way.

Pages affected: `wiki/plans/d5-persistence-backends.plan.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`,
`wiki/log.md`

**The reframe.** "Production for RepForge" is an adoptable 0.x on a git dependency — durable
backends, the row store, suite green, their blocking questions answered — not a crates.io release.
RepForge's own section-C answer established that neither side has a production; both are at the
starting line, and the fastest way to burn down the remaining risk is their integration, not more
proxy consumers.

**What moved.** Entry 12 left the publication clock for D5's Track E: under decision 032 the
binding maps known mutation ids to rows, so a report naming what drained is nearly free — and it is
simultaneously RepForge's read-model-convergence need and the retirement of Finding 1's index
rebuild. D6 split: its adoption half (compat notes, consumption path, an adoption guide mapping
`persistence/*.rs` to frontbox APIs) rides D5's new adoption lane; its publication half stays put.
D4b folds to the seam proof. The `If-Match` question is relayed to RepForge rather than designed
around, per the register's own follow-up.

**What explicitly does not speed up**, so the floor is on record: the conformance suite on both
durable backends, and columns-decided-before-rows. A schema change after RepForge writes rows onto
user devices is a migration on their users; those two disciplines are what make fast adoption safe
rather than merely fast. Wiki pages get shorter from here; the record stays.

**Build order**: three lanes. Core lane first and immediately — Track E decisions (3, 4-shape, 12,
18), then the 032 core slice: `RowStore`, the `MutationIntent` binding, drained ids on the reports,
in-memory implementations, conformance cases, all testable with no backend. Backend lane follows
the final column set. Adoption lane rides the backends.

## [2026-08-30] build | D5's core lane: the row store, the binding, and four closed doors

The core half of D5 — everything testable without a durable backend. Four decisions that had to be
made before a backend wrote a row, then decision 032's store built against the backend that exists.

Pages affected: `src/record/row.rs`, `src/record/mod.rs`, `src/store.rs`, `src/runner/report.rs`,
`src/runner/mod.rs`, `src/runner/drain.rs`, `src/memory/rows.rs`, `src/memory/mod.rs`,
`src/memory/outbox.rs`, `src/memory/factory.rs`, `src/testing/mod.rs`, `src/testing/macros.rs`,
`src/testing/cases/rows.rs`, `src/lib.rs`, `tests/in_memory.rs`,
`wiki/decisions/033-last-error-on-the-record.decision.md`,
`wiki/decisions/034-no-service-origin-in-core.decision.md`,
`wiki/decisions/035-reports-name-what-drained.decision.md`,
`wiki/decisions/036-no-sub-scope-partitions.decision.md`,
`wiki/references/open-decisions.reference.md`, `wiki/index.md`, `wiki/log.md`

**The register went from nine open decisions to five, and the four that closed were every one with
a storage clock on it.** 033 adds a bounded `last_error`; 034 refuses a service-origin column; 035
makes the reports name what drained; 036 keeps one queue per scope with `(seq)` as the key. The
outbox column set is now final, which was the precondition for starting either backend.

**What was built.** `RowRef` and `StoredRow` in `src/record/row.rs`; a `RowStore` trait with
`get_row`, `list_rows`, `put_rows`, `merge_rows`, `delete_rows` and `set_stale`; the optional row
binding on `MutationIntent`, `#[serde(skip)]` beside `traceparent` and `precondition` so decision
010's payload is untouched; `Drained`/`DrainedAs` on both report types; and the in-memory
implementation of all of it. Five conformance cases, 62 through 66.

**`Disposition::Retain` became a struct variant** carrying the reason. That is what makes decision
033's column reachable: the store already had to write on a retain — it increments `attempts` — so
the reason rides the write that was happening anyway, and the two always describe the same verdict.

**Two sabotage checks, because a test that passes both ways proves nothing.** Case 63 — the merge
skip, which is the case decision 032 exists for — was confirmed to fail with the skip removed.
Earlier the same day the startup-hydration regression test was confirmed to fail with the hydrate
removed. Neither is a regression test otherwise.

**One thing the runner had to be careful about.** `Applied` and `Duplicate` both map to
`Disposition::Delete`, so the disposition alone cannot tell the report which happened; the applied
ids are recorded in the verdict loop where the status is still in hand. And `drained` is built
before `apply_outcomes` but returned only after it succeeds — reporting that a record left the
queue when the apply failed would be a lie a caller could act on.

Gates: ALL GATES PASSED, 19 gates. Coverage 89.09% regions / 96.05% lines on `-p frontbox`, up from
89.02% regions. 124 tests pass, up from 119.

Still owed in D5: both durable backends, decision 031's cross-realm half, and `todo-core` moving
onto the row store — which is where `row_id_of` finally retires, since the binding now records what
it was reverse-engineering.

## [2026-08-30] build | Track D: the trial moves onto the row store, and the API holds

`todo-core` now writes through frontbox's row store instead of keeping its own projection in
memory. Done before the backends deliberately: `RowStore` had shipped hours earlier and no
application had ever held it, and the conformance cases could not catch an API that behaves
correctly but is awkward to use — the failure this project has already produced twice as Findings 5
and 6, and once as register entry 14.

Pages affected: `examples/todo-core/src/app/mod.rs`, `examples/todo-core/src/app/index.rs`,
`examples/todo-core/src/app/startup.rs`, `examples/todo-core/src/store.rs`,
`examples/todo-core/tests/regressions.rs`, `wiki/log.md`

**The API held, and the strongest evidence is how little moved.** `enqueue_with_id` already took a
`row_id: &str` — every call site was already passing the fact the binding needed, and had been
discarding it. Binding was one line. `InMemoryStore` implements `OutboxStore` and `RowStore` both,
so no new field, no second handle, no second storage stack. That last part is the whole of what
decision 032 bought: the first draft of this track had `todo-core` opening its own durable store.

**`row_id_of` is gone.** It recovered which row a queued envelope was about by parsing URL segments
— guesswork that broke on any route it had not been taught. An envelope that names no row now means
a caller that did not bind one, which is a narrower and more honest thing.

**Finding 1 is closed in code.** `sync` decrements the pending index from `DrainReport::drained`
instead of rebuilding it from a full queue scan. Two existing tests failed on this and were right
to: they asserted the gap was refreshed by every drain. The incremental index is **more** accurate
than the rebuild it replaces — `mark_pending`/`unmark_pending` see every enqueue and drain exactly
once, while the scan sees only as far as its limit reaches — so `refresh_pending` became the
startup reconciliation and `pending_index_gap` now describes that rather than the last sync. Both
tests were updated to drive the reconciliation explicitly.

**Two new regression tests, both sabotage-checked.** An offline reload restores the user's rows
from storage with no server — confirmed to fail with the durable load removed from `start`. And a
reload keeps an unsent edit over the server's older copy, with `Startup::protected` naming the row
the library refused to overwrite. That second one is decision 032's merge rule proven from an
application rather than from a conformance case that shares its author.

`TodoApp::reopen` is what a reload means to a crate with no process to restart: same scope, same
backend, fresh projection and fresh index. Everything that survives, survives because it was
durable.

`Startup` gained `restored` and `protected` — how many rows came back without a network, and which
rows the screen deliberately disagrees with the server about.

Gates: ALL GATES PASSED. 23 `todo-core` tests, up from 21. Coverage unchanged at 89.09% regions /
96.05% lines on `-p frontbox`, which is expected: nothing in core moved.

**Still owed in D5: both durable backends**, decision 031's cross-realm half, and the visible
payoff — on the in-memory backend the queue and the rows still die with the tab, so nothing new
appears in a browser's storage panel until IndexedDB lands.

## [2026-08-30] build | The SQLite backend, and three things only an out-of-tree backend could find

`crates/frontbox-sqlite` runs the same 55 conformance functions the in-memory backend does, plus
the single-flight profile, the row suite and fault injection. First backend outside core, and
building it found three gaps that nothing inside core was in a position to notice.

Pages affected: `crates/frontbox-sqlite/*`, `src/record/terminal.rs`, `src/rfc3339.rs`,
`src/lib.rs`, `Cargo.toml`, `scripts/verify.sh`, `wiki/log.md`

**Three findings, all the same shape: core stated a contract it did not export the means to
satisfy.**

- **`QuarantinedRecord` had no constructor.** Decision 008 marks every record `#[non_exhaustive]`
  so later fields are additive, which blocks struct literals outside this crate. That is right for
  *readers* — and a storage backend is a **writer** of these types. `DeadLetterRecord::from_record`
  covered the other terminal transition; the id-less path had nothing, so an out-of-tree backend
  could not produce a quarantine entry at all. `from_raw` now exists.
- **`is_representable` was `pub(crate)`.** It is one of the three things that make a stored row
  corrupt, so a durable backend has to check it — and could only do so by reimplementing the RFC
  3339 range, which is precisely the divergence the shared suite exists to prevent. Exported as
  `timestamp_is_representable`.
- **The in-memory backend never noticed either**, because it lives inside core. That is the same
  seam-with-green-gates-on-both-sides shape as Findings 5 and 6, for the third time.

**`rusqlite 0.31`, not 0.38, and not by preference.** `libsqlite3-sys` sets `links = "sqlite3"`, so
one version must serve the whole workspace, and `examples/todo-server`'s `sqlx 0.8` pins `0.28`.
Recorded because a future upgrade of either will collide again and the reason will not be obvious.

**Fault injection proves something the in-memory backend cannot.** `fail_next_apply_outcomes` arms
a flag that `apply_outcomes` consumes *after* every write and *before* the commit — so a
transaction that has already inserted dead letters, moved quarantine rows and deleted from the
outbox rolls all of it back. A backend that checked a flag first would only demonstrate that it can
decline to start. The in-memory backend's own docs admit it cannot make this claim, because it
commits by replacing state in one assignment.

**And one test no other backend could run**: a queue written through one connection, the connection
dropped, the file reopened, and the mutation, its row binding and its stored row all still there.
That is `observation_3` inverted — the observation D4a asserted the *opposite* of because it had no
backend that could do this.

Three schema notes worth keeping. `AUTOINCREMENT` is load-bearing: plain `INTEGER PRIMARY KEY`
reuses deleted rowids, which would sort a later write ahead of an earlier one and invert exactly
the pairs decision 016 exists to order. `COLLATE BINARY` is written out although it is the default,
because a default is what a later `ALTER` changes silently. And scope is a column rather than a
database, which satisfies decision 024's injectivity trivially — two distinct keys are two distinct
strings — while keeping the suite's sharing contract, which needs two scopes in one physical store
or the isolation cases prove nothing.

Gates: ALL GATES PASSED, now 21. Core coverage dips to 88.74% regions / 95.31% lines from 89.09% /
96.05%, which is expected and worth stating plainly: the two new core APIs exist for an
out-of-tree backend and are exercised by that backend's suite, which `-p frontbox` coverage does not
see. Well above the 80% floor.

**Still owed in D5:** the IndexedDB backend, decision 031's cross-realm half, and a
`CacheVersionStore` for SQLite — deliberately absent, so the gap is visible in its test file rather
than hidden behind a runtime skip.

## [2026-08-30] build | The IndexedDB backend, and what running it in a browser found

`crates/frontbox-indexeddb` implements every storage trait over `web-sys`. It compiles for
`wasm32-unknown-unknown`, is clippy-clean, and **its conformance suite is not green** — see the
honest status at the end, which is the most important part of this entry.

Pages affected: `crates/frontbox-indexeddb/*`, `src/error.rs`, `Cargo.toml`, `scripts/verify.sh`,
`wiki/log.md`

**A fourth core gap, same shape as the three SQLite found.** `Error::storage` takes
`impl std::error::Error`, and a JavaScript exception is a `JsValue`. So a web backend's only
option was `storage_opaque()` — which discards the one thing a browser failure actually tells you:
`QuotaExceededError` and `TransactionInactiveError` are different problems with different fixes.
`Error::storage_message` now exists.

**`web-sys` directly rather than `rexie`**, which the source system used. The reason is transaction
lifetime: `merge_rows` has to read the queue and write the rows as one unit, and a wrapper that
decides when requests are issued is a wrapper that can silently break that.

**Running it in a real browser found three defects a compile gate cannot reach.** All three
type-check perfectly, and all three present as a *hang* rather than a failure:

- **The completion handler raced the commit.** `oncomplete` was attached after the last
  `await_request`, so a transaction that had already finished got its handler on a dead object and
  the future never resolved. `Txn` now arms the handlers at construction, before any request.
- **The rows index was created as `scope_entity` and read as `scope`.** Every row read errored, and
  the erroring read left its transaction unresolved.
- **`deleteDatabase` blocks on open connections** and signals it with `onblocked` — neither
  `onsuccess` nor `onerror`, so a future awaiting those two waits forever. A delete-first test
  factory deadlocked on its second test. Each factory now gets its own database name instead.

**Status, stated plainly: the suite is not green.** Individual cases pass in headless Chrome —
`case_01`, `case_03`, `case_09`, `case_23`, `case_66` were each confirmed — and the full 55-case
run still hangs somewhere after that. The three defects above were found and fixed on the way; at
least one more remains. `frontbox-indexeddb` should be treated as **unproven** until the suite runs
clean.

The `scripts/verify.sh` gates say so too. They build and lint the crate and **compile** the suite;
they do not execute it, and the gate's comment says why — `cargo test` cannot run wasm and Node has
no IndexedDB. A gate that cannot fail is worse than an acknowledged gap, so the gap is
acknowledged rather than dressed up. Running it for real needs a browser and a chromedriver
matching the installed Chrome:

```
CHROMEDRIVER=<matching driver> WASM_BINDGEN_TEST_ONLY_WEB=1 \
  cargo test -p frontbox-indexeddb --release --target wasm32-unknown-unknown --features testing
```

`--release` matters: a debug build of the suite exceeds chromedriver's 300-second renderer timeout
before a single test runs.

**Still owed in D5:** a green IndexedDB suite, decision 031's cross-realm half, and a
`CacheVersionStore` on either durable backend.

## [2026-08-30] fix | Five IndexedDB defects, and two cases that assumed a sync backend

Chasing the IndexedDB suite. **47 of 54 cases now pass together in headless Chrome**, up from a run
that hung before any test completed. Every case also passes individually. The suite is still not
fully green and the remaining failure is characterised below rather than papered over.

Pages affected: `crates/frontbox-indexeddb/src/convert.rs`, `crates/frontbox-indexeddb/src/backend.rs`,
`crates/frontbox-indexeddb/src/factory.rs`, `crates/frontbox-indexeddb/tests/conformance.rs`,
`src/testing/cases/pass_control.rs`, `src/testing/macros.rs`, `tests/in_memory.rs`,
`crates/frontbox-sqlite/tests/conformance.rs`, `wiki/log.md`

**An `i64` outside ±2^53 does not survive IndexedDB.** Values cross in as structured-clonable
JavaScript, and every JavaScript number is an `f64`. Real timestamps sit far inside that range,
which is exactly what makes this dangerous: it looks correct until a value that is *not* a real
timestamp appears — and the one place that happens is a corrupt row, the thing a backend most needs
to report faithfully. `created_at`, `rejected_at` and `quarantined_at` are now stored as text.
Case 26 caught it.

**Two conformance cases assumed every backend is synchronous, and both were written when every
backend was.**

- **Case 28** finished a suspended pass with `loop { poll() }` and a noop waker. That terminates only
  if each `await` inside resolves on its first poll. An IndexedDB backend suspends on a browser
  event, so the loop spun on the only thread the event loop has, the callback could never fire, and
  the case froze the tab rather than failing. Fixed by awaiting: the manual polls are still needed
  to suspend a pass mid-flight, but once that is proven the executor should finish it.
- **Case 30 cannot be fixed the same way** and is now a `frontbox_blocking_only_tests!` profile. It
  cancels a pass mid-flight, which requires every step *before* the transport to resolve on the
  first poll. This backend suspends before then, and no yield available inside a case reaches the
  task queue that would complete it. Split out rather than skipped at runtime — the same capability
  split `FaultInjection` and `VersionStoreFactory` already use — so the gap is visible in the
  IndexedDB test file. It exercises `SyncRunner` logic identical on every backend, so nothing about
  IndexedDB goes untested as a result. In-memory and SQLite both run it.

**Three backend defects, all of which present as a hang rather than a failure**: the transaction
completion handler raced the commit (now armed at construction), the rows index was created under
one name and read under another, and `deleteDatabase` blocks on open connections via `onblocked` —
an event a future awaiting success/error never sees. Databases now also close on drop and carry a
per-run stamp, so one run cannot inherit another's.

**What is still wrong, stated precisely.** With `--skip case_6` — which also skips `case_06` and
`case_60` — 47 cases pass. Add any of those seven back and the *renderer* freezes, now before any
test reports, where it previously froze mid-suite. Every one of the seven passes alone. The cause
is not isolated, and each experiment costs a 300-second browser timeout, so the work was stopped
here rather than continued without a hypothesis.

**`frontbox-indexeddb` therefore remains unproven**, and the gates still compile its suite without
running it, with the gate comment saying so. In-memory (65 cases) and SQLite (56) are green and
unaffected.

## [2026-08-30] fix | The IndexedDB suite is green: 54 of 54 in a real browser

The freeze was in a conformance case, not in the backend. `case_61_two_handles_on_one_scope_do_not_
drain_together` — decision 031's own case — finished a suspended pass with `loop { poll() }` and a
noop waker, the identical defect fixed in case 28 earlier the same day and missed here.

Pages affected: `src/testing/cases/single_flight.rs`, `crates/frontbox-indexeddb/src/backend.rs`,
`crates/frontbox-indexeddb/src/factory.rs`, `scripts/verify.sh`, `wiki/log.md`

**A busy poll loop spins the only thread a browser event loop has**, so the IndexedDB callback that
would complete the pass can never fire. It presents as a frozen renderer — chromedriver's
"Timed out receiving message from renderer", which means *busy*, not idle-waiting. That distinction
was the clue that eventually mattered and it was available from the first failed run.

**What the search cost, and what made it expensive.** Two of the intermediate diagnoses were wrong
and are worth recording as wrong: a per-run database stamp (the serving port changes each run, so
nothing persisted across runs) and per-factory cleanup on drop (real hygiene, not the bug). One
experiment was worse than wrong — an edit that was supposed to disable three suites silently
matched nothing, because the pattern omitted a `!`, so a run reported as "the row suite in
isolation" had every suite enabled. It sent the search after the row cases for several rounds.
**Bisection is only as good as the confirmation that the bisection did what it said.**

What actually found it: reproducing the freeze with hand-written probes, discovering they *passed*
where the macro-generated tests hung, and then re-enabling suites one at a time until single-flight
turned out to be the one that mattered. Four tests, each passing alone.

The cleanup work stays anyway. Databases now close when the last handle drops, and the test factory
deletes its database on drop — the IndexedDB equivalent of `InMemoryFactory::new` giving each test
a store that evaporates. Neither fixed the freeze; both are correct.

**Status: 54 of 54 pass in headless Chrome.** `frontbox-indexeddb` is no longer unproven. The
`verify.sh` gates still compile rather than run the suite, because a build gate cannot assume a
browser and a version-matched driver are present — but the comment now says the suite passes and
gives the exact command, rather than describing an unknown.

Three backends now run the same conformance functions: in-memory (65), SQLite (56), IndexedDB (54).
The differences in count are the capability splits — `frontbox_blocking_only_tests` for case 30,
and the cache suite, which neither durable backend implements yet.

## [2026-08-30] build | D4b: the trial runs on durable storage, and the seam mostly held

`examples/todo-core` now writes to SQLite on native and IndexedDB on web. All 23 of its tests pass
against the durable path, and **observation 3 is inverted**: a real file, a real process boundary,
and the queued write still there afterwards — the observation D4a asserted the *opposite* of.

Pages affected: `examples/todo-core/src/backend.rs`, `examples/todo-core/src/app/mod.rs`,
`examples/todo-core/Cargo.toml`, `examples/todo-core/tests/*`, `examples/todo-app/src/main.rs`,
`crates/frontbox-sqlite/src/backend.rs`, `crates/frontbox-indexeddb/src/backend.rs`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/log.md`

**The seam held for everything a running application does, and gave way at the moment one starts.**

D4b's promise was that the diff touches only where the backend is named. Reads, writes, the drain,
the merge, the pending index and all ten observations are untouched — they are written against
core's traits and the traits did not move. `crate::backend` is the one file that names a backend,
and it is a type alias plus one function per target.

What did not survive: **`TodoApp::new` had to become `async`.** Opening a browser database is
asynchronous and nothing about seam placement changes that. Recorded as the finding rather than
worked around, and the signature is uniform across targets on purpose — a `new` that is sync
natively and async on web is two APIs wearing one name.

That propagated exactly one level, and usefully. The UI now opens through `use_resource` rather
than `use_hook`, which gives the tree a state it did not have before: **opening**. It is rendered
rather than hidden, because a first paint showing nothing while IndexedDB opens is
indistinguishable from a first paint showing nothing because the queue is empty, and those need
different reactions from whoever is looking. Private browsing refuses IndexedDB outright, so the
error arm is not hypothetical.

Two smaller things. `open_scope` is the method name both durable backends use, so the swap did not
have to rename anything at the call site. And the fault-injection helpers on both backends are now
behind `#[cfg(feature = "testing")]` — without the gate they are dead code in every real build, and
`todo-core` depending on `frontbox-sqlite` was the first consumer positioned to notice.

**What this finally buys**: `examples/todo-app` stores into an IndexedDB database named
`frontbox-todo`, so the outbox, dead letters, quarantine and read-model rows are all visible in a
browser's Application panel. That was the original question that started D5, and it is now
answerable by looking.

## [2026-08-30] build | D5's cache half, on both durable backends

Pages affected: `wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`,
`wiki/log.md`

**D5 said "Built" before the cache half existed, and this closes the gap that admission opened.**
`CacheVersionStore` now has three implementations rather than one: `SqliteVersionStore` over a new
`cache_versions` table, and `IdbVersionStore` over a new `versions` object store. Both backends
implement `VersionStoreFactory`, both run `frontbox_cache_tests`, and the D5 proof line — "shared
conformance tests pass on in-memory, SQLite, and IndexedDB" — is now true of every suite instead of
some of them. SQLite goes 56 → 67 tests, IndexedDB 55 → 66 in headless Chrome.

**How the overstatement was catchable is the part worth keeping.** The SQLite test file carried a
comment saying the cache suite was absent, because a backend without the capability does not
implement the factory trait and therefore cannot invoke the macro. The gap was discoverable by
reading a test file. A backend that skipped the suite at runtime instead would have reported a full
pass and left nothing to find, which is the entire argument for splitting `StoreFactory`,
`VersionStoreFactory`, `RowStoreFactory` and `FaultInjection` rather than merging them.

**Writing the second version store found an API gap**, in the shape this project keeps meeting: an
API complete for the backend inside the crate and incomplete for one outside it. `EntityState` is
`#[non_exhaustive]` with three constructors — `unknown` is `(None, false)`, `stale_at` is
`(Some, true)`, `fresh_at` is `(Some, false)`. The fourth combination, **stale with no version**,
had no constructor and could not be written by anyone outside the crate.

It is not a corner. `InvalidationRunner::mark_all_stale` — the error-recovery hammer — reads each
registered entity's state and writes it back with `stale: true`, and for an entity no invalidation
has ever named that read is `unknown()`. So the write is `(None, true)`, on a client that has just
installed, which is the most likely moment to reach for that hammer. `StaleEntity::version` already
documented the state as legitimate. The in-memory backend never noticed because it builds the struct
literal directly.

`EntityState::from_parts` closes it, and **case 67 is what makes the closure hold**: it calls
`mark_all_stale` with nothing known, reopens the store, and requires both that the version reads
back as `None` and that the staleness survived — plus that the entity appears in `all_states`, since
`stale()` walks the enumeration and a backend that filtered null versions out of it would pass the
first assertion and still never refetch. Verified against the implementation the missing constructor
would have forced: matching on the three named constructors makes case 67 fail with
`version: None, stale: false`, which is a client silently cancelling the refetch its own recovery
asked for.

Same precedent as `QuarantinedRecord::from_raw`, found the same way: a `#[non_exhaustive]` record is
only as reconstructible as its constructors, and only a second, out-of-tree implementer finds out.

Two schema notes. SQLite's `version` column is nullable rather than `NOT NULL DEFAULT ''`, because
SQL `NULL` and the empty string are different states and collapsing them is decision 021's bug —
every case would still pass except 44. And IndexedDB's schema version moves 1 → 2, which is this
crate's **first real migration**: a new object store can only be created inside a version-change
transaction, so a build that added the store and left the number alone would throw `NotFoundError`
on the first cache read. It is the cheap kind — nothing rewritten, queued mutations and stored rows
intact — and `create_stores` now guards each creation on `object_store_names().contains` rather than
on swallowing a `ConstraintError`, since an ignored throw cannot be told from a creation that failed
for a reason that mattered.

## [2026-08-30] build | D4c — the same application on web, desktop, iOS and Android

Pages affected: `wiki/plans/d4c-multi-platform-trial.plan.md`,
`wiki/roadmaps/extraction.roadmap.md`, `wiki/references/open-decisions.reference.md`,
`wiki/index.md`, `wiki/log.md`

**`examples/todo-app` now runs on four platforms and nothing below `main.rs` is conditionally
compiled.** `rows.rs`, `sync.rs`, every component and every handler are identical on all of them.
Everything a platform decides lives in one new `platform` module, and the list is four items: the
clock, the timer, where storage lives, and where the server is.

**Neither `frontbox` nor `frontbox-dioxus` was touched.** `Clock` (decision 002) and `Sleeper` both
covered platforms they were not written for — `Sleeper`'s doc comment says "gloo on web, tokio on
desktop" and it turns out to cover iOS and Android unchanged. `todo-core`'s backend module meant all
three native platforms got durable SQLite with no new code at all.

**Android was the one real cost, and it is application code.** There is no `HOME` on Android,
`TMPDIR` points at nothing writable, and `dirs::data_dir()` therefore returns `None`; the
per-application directory is a property of the running `Context` and reachable only by calling
`getFilesDir()` on the `Activity` over JNI. A build reusing the desktop path would not fail to
compile — it would fail at the first write, on a device. `10.0.2.2` is the same shape one layer up:
an emulator's `127.0.0.1` is the emulated device, so the demo would have started permanently offline
and *looked* fine, because starting offline is a state this application renders deliberately.

**Mobile-friendly turned out to be three correctness fixes and one cosmetic one**: 16px inputs, or
the iOS webview zooms on focus and does not zoom back; `env(safe-area-inset-*)`, or the title sits
under the dynamic island; 44px targets under `@media (pointer: coarse)` rather than a width query,
because a touchscreen laptop is wide and still being tapped; and a single column below 30rem.

**The finding: the drain loop dies on Dioxus desktop.** `use_sync_loop` runs inside `use_future`, on
the VirtualDom's task executor, and on desktop that executor stops being polled after a few seconds.
Measured in one process over 95 seconds at a five-second cadence — the sync loop ticked **3** times,
a second `use_future` touching no signal ticked **2**, and a bare `tokio::spawn` loop ticked **18**.
The second number rules out the loop and rendering; the third rules out the timer, the runtime and
OS throttling. Reproduced under `cargo run` and `dx serve` alike, window occluded and frontmost.
Likely mechanism, read from `dioxus-desktop-0.7.10/src/webview.rs:562` rather than instrumented:
`poll_vdom` returns early without polling the VirtualDom whenever `poll_edits_flushed` is pending.

It fails silently — the UI stays responsive, writes keep landing in SQLite, nothing reaches the
server — which is the failure an outbox exists to prevent. Recorded as register entry 20 and not
fixed, because the fix is an adapter change and that needs a go-ahead.

**One consequence belongs in the decision record.** The tokio task that kept ticking *could* carry
the drain, and cannot, because `tokio::spawn` requires `Send` and decision 001 makes every store
`!Send` on purpose. The posture chosen so an IndexedDB future could exist at all is what removes the
desktop escape hatch. Not an argument against decision 001 — the alternative never existed — but the
first time its cost has been visible, and worth recording once rather than rediscovering.

**Web, iOS and Android are unaffected, and the idle window was tested specifically**, because "it
worked when I poked it" is the observation the desktop build would also have produced. iOS: a
mutation written into the running app's SQLite from outside after 120 seconds of no interaction,
drained in ~12 seconds. Android, sharper because the device could be left entirely alone: a todo
queued through the UI with the server stopped, **170 seconds untouched**, then the server restarted
from the host — drained ~10 seconds later with no device input at all.

`scripts/verify.sh` gains `build ui desktop` and `clippy ui desktop`, 25 gates now. The mobile
targets are not gated and the script says so: iOS needs Xcode and Android an NDK, neither of which a
build gate can assume. What the desktop gate buys them is that they compile the same
`not(target_arch = "wasm32")` half, minus Android's JNI block.

## [2026-08-31] decide | invalidation delivery, and the boundary that outlived its reason

Pages affected: `wiki/proposals/invalidation-delivery.proposal.md`,
`wiki/decisions/037-multi-service-routing.decision.md`,
`wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`,
`wiki/plans/d4d-multi-domain-trial.plan.md`, `wiki/roadmaps/extraction.roadmap.md`,
`wiki/references/open-decisions.reference.md`, `wiki/index.md`, `wiki/log.md`

**The challenge: "the whole cache invalidation hinges on SSE — not having it is like having built a
cache that can't be invalidated."** The standing answer was D3b's "SSE glue deliberately unbuilt",
and it defended the wrong boundary. It argued core should not own an `EventSource`, which nobody
disputed. It never explained why the outbound path has a wire contract, a trait, a driver and a
cadence policy while the inbound path has a method you can call.

The empirical version is worse than the structural one: `use_invalidation`, `InvalidationState` and
`InvalidationEvent` appear **nowhere outside the crates that define them.** The D2 invalidation
runtime — made durable on both backends the day before — has never been invoked by an application.

**Decision 014's premise had moved and nobody noticed.** It justified core's position with
*"Core does not gate refetches, because core does not perform them"*, written 2026-08-27. On
2026-08-30 decision 032 gave core the row store, `merge_rows` and the skip rule. Core now holds the
read model and merely does not fetch it. This is the second time a boundary here outlived its
reason — decision 023 was the first, superseded by 032 under the same argument from the same person
— and both times the challenge came from outside rather than from a review. The line that survives,
now stated deliberately rather than inherited: **frontbox owns the storage of the read model and the
knowledge of whether it is stale; it does not own its acquisition.**

**"SSE" turned out to be the wrong name for the hole**, and the naming is part of why it stayed
open — it sounds like a demo detail. Three things hide under it: the bytes (genuinely the
application's, like `reqwest`); the seam (missing, and the asymmetry has no principle behind it);
and the loop that reacts to staleness (missing). The distinction is practical: **a trait covers
polling and "SSE" does not.** A server with no push exposes `GET /api/v1/versions`; both satisfy
one seam, and committing to SSE would bake in a push model many servers cannot offer.

**Polling first, and not for effort.** A versions endpoint computed from current state is
*level-triggered*: it cannot be lost or phantom, and a missed poll costs latency rather than
correctness. SSE is edge-triggered, and a dropped event means a client believes stale data is fresh
forever — decision 015's failure relocated to the server. That is why a stream needs a *server-side*
transactional outbox and a `Last-Event-ID` resume token, and why the endpoint should exist first
regardless: it is what the stream reconciles against on reconnect.

**Decision 037 — one outbox, one scope, a routing transport.** Not a new position, the join of two
existing ones nobody had put together. 034 said a service is a transport concern and left routing
open; 036 refused to partition the outbox *specifically* to keep cross-type ordering, citing "create
the list, then create the todo in it". Read together they already answered it. What the batch
mechanic needed turned out to be nothing: a routing transport splits by destination and may return
only the verdicts it obtained, because core already retains an unnamed record with
`reason: Some("no verdict returned")`. **Synthesis was built for a server that declines to rule, and
a transport that could not reach one is the same fact from a different cause.**

**Decision 038 — the inbound seam is built in the trial first**, with the promotion bar written now
rather than argued later: two sources and ideally three against one seam, more than one origin, the
resume-cursor question answered, and a cadence policy that survives contact. The reason is this
project's own record — every new consumer has produced a finding the previous ones could not — and
invalidation has had none. **A seam with one implementation is a wrapper.**

**D4d is the plan.** A second domain server, and users rather than lists — because the scope key is
caller-composed, so a queue can exist before the server has ever heard of you, and offline signup is
the flow that exercises it. Create your user record and your todos offline, restart, reconnect, and
the user must land before the todos. "User" then means two things, the scope and the record, and
that is the lesson rather than a wart.

**The sabotage half is what makes it evidence.** Run the same flow with one scope per service,
watch ordering vanish, and watch the `404` for a user the todo server cannot find.

**The classification of that `404` is the transport's, and an earlier draft of these pages implied
it was core's.** Decision 019 opens with "core does not classify HTTP responses and will not"; what
it places on a *transport* is the obligation that a missing prerequisite is transient and must never
become `Rejected`. So the trial's routing transport maps it to `MutationStatus::Blocked` — the
status whose condition 019 describes as "this write failed only because a predecessor had not
landed" — rather than `Unknown`, which 019 reserves for what a transport *cannot* confidently
classify. The record is then retained with `reason: Some("blocked")`, counted in `counts.blocked`,
and carries `"blocked"` in decision 033's `last_error`; **it does not reach the anomaly surface**,
which only `Unknown` does. Corrected here because "it shows up in the anomalies" would have sent
whoever writes the test to the wrong assertion.

Without the sabotage, the good path proves a drain worked rather than that order held — and
decision 036 currently has no evidence at all.

**Two requirements arrived with the proposal and both landed better than expected.** Focus-triggered
repolling needs **no adapter change**: `Wake::new` and `Wake::wake` are already public, built for a
bfcache restore, and a `visibilitychange` listener is the same shape of event for the same reason —
with the latch making it safe against focus returning mid-poll. Per-entity polling frequency is in
tension with a batched versions endpoint, since one response answers for every entity; the knob is
therefore restated as a **staleness budget** — "this entity may be up to N seconds out of date" —
which any source can satisfy and over-deliver against, where a literal schedule cannot be honoured.

Nothing was built. Register entries 21 and 22 opened and closed the same day.

## [2026-08-31] fix | A whole-worktree review: two cross-backend divergences, and stale docs

A line-by-line review of all 229 files. Its own summary is the finding worth leading with: **the
engineering was in good shape and the account of it had drifted.** The README, `AGENTS.md`, the
runtime spec and this wiki's index all still described a project where storage was in-memory and
D4b onward was unauthorized — two days after D4b, D4c and D5 shipped. For a project whose first rule
is *document tested truth, not intended truth*, that is the one kind of drift worth treating as a
defect, and it is treated as one here.

Five defects were reported. All five were checked against the source before being accepted, and all
five held.

### Two backends disagreed, and the conformance suite could not see it

**`pending_batch` could return nothing while records were queued.** The SQLite backend asked SQL for
`limit * 4` rows and dropped the ones that would not decode. The `4` is a guess about how much
corruption sits ahead of the next good record, and there is no number that is right: with more than
`3 * limit` corrupt rows at the head, every row in the window was discarded and the batch came back
**empty** while `pending_count` correctly reported work outstanding. The in-memory and IndexedDB
backends decode the whole scope and truncate afterwards, so they returned the records. Three
backends, two behaviours, and the suite green on all three because no case seeds more corruption
than the multiplier absorbed.

`SyncRunner` masked it — a pass sweeps before it reads, so the corrupt rows are in quarantine by the
time the batch is taken. Direct callers do not sweep, and `TodoApp::refresh_pending` is one.

The window is now a loop rather than a multiplier: page until `limit` decodable records are found or
the scope is exhausted, keyed on `seq` rather than `OFFSET` so a deletion between pages cannot skip
a record. With no corruption it is one query, exactly as before. **Case 69** seeds twenty corrupt
rows ahead of two good ones, which is deliberately more than any plausible constant — the point is
not that four was too small but that no constant is right.

**`DeadLetterStore::list` and `QuarantineStore::list` took a `limit` and specified no order**, and
three backends filled the silence three ways: in-memory sorted by timestamp, SQLite by insertion
rowid, IndexedDB by whatever its scope index returned. All three agree while records are parked in
timestamp order, which every existing case did. Under a truncating `limit` they return a **different
set of records**, not merely a different sequence.

The traits now state `(rejected_at, mutation_id)` and `(quarantined_at, raw_mutation_id)`, with the
reason: `rejected_at` first because a human triaging a queue asks *what broke, and when*. **Case 68**
parks two records with the clock moved backwards between them and asserts `list(1)` returns the
older on every backend. Both new cases were confirmed to **fail** against the previous behaviour
before the fix was kept.

### The rest

**A dropped IndexedDB transaction commits.** `rusqlite::Transaction` rolls back on drop;
`IdbTransaction` does the opposite, because the browser owns it and completes it once it goes
inactive. The fault-injection path knew this and aborted by hand; nothing else did, so an
unrecognised `Disposition` or a `to_js` failure returned `Err` after earlier records in the loop had
already been deleted and reinserted — and those writes landed. `Txn` now aborts on drop unless
`commit` consumed it, with handlers detached first so the abort event cannot reach a freed closure.
Read transactions are exempt; they have nothing to roll back.

**One truncation rule was written three times**, byte-identically — which is the coincidence that
hides a divergence rather than proof there is none. `truncate_error` now ships from core beside
`LAST_ERROR_MAX`, which is the argument `src/rfc3339.rs` already makes in the general case.

**Three files were over the 400-line cap**, one by 45% — and `examples/todo-core/src/app/mod.rs`
opened its module doc by explaining that it had become a directory *because of the cap*. It is now
split along the line the trial's findings drew: writes stay, the read half moved to `app/sync.rs`.
`src/testing/macros.rs` split at the `#[doc(hidden)]` boundary; `src/record/mod.rs` gave up
`last_error.rs`.

### Two new gates, because both failures were of the same kind

Prose rots wherever nothing measures it, so two things stopped being matters of discipline:

- **`cited paths resolve`** — every `` `path/to/file.rs` `` citation in the prose points at a file
  that exists. Eleven had gone stale, mostly because splitting a file into a directory renames it
  and nothing recompiles a wiki page. `wiki/log.md` is exempt: it is append-only, so an entry citing
  a path that was correct when written is a historical record and repointing it would edit history
  to make a gate green. Lines mentioning RepForge are exempt too, since
  `wiki/references/repforge-section-c-answers.reference.md` records paths in another repository and
  says so.
- **`no source file over 400 lines`** — the cap, measured rather than remembered.

### What was not changed, and why

Two of the review's suggestions are public API additions, and `AGENTS.md` puts those behind the same
go-ahead a new deliverable needs. Both are now **register entries 23 and 24** with options and a
recommendation, decided by nobody yet: whether a dead letter carries `last_error` and its bound row
(`from_record` drops both, and for a `RetentionBound` letter `last_error` is the one field that says
what the record was up against), and whether `CacheVersionStore` gains a batched multi-entity read
(`mark_all_stale` issues one transaction per entity on IndexedDB to prepare one write).

`docker-compose.yml` did not gain the `user-server` stub the review suggested: D4d has a plan and no
go-ahead, so the crate does not exist and the service would break `docker compose up`.

`wiki/log.md` was not split by month. At 2,621 lines it is past the size the guidelines anticipate,
but splitting an append-only record is a structural change to the thing everything else cites.

### Verification

All 30 gates pass — 28 before, plus the two above. 128 tests on `-p frontbox --all-features`, 69 on
SQLite running the same 68 conformance cases, **68 green in headless Chrome** on IndexedDB including
both new cases and with the abort guard live, 24 trial tests over real HTTP, coverage 89.17% regions
/ 95.79% lines against an 80% floor. Case 25 stopped being near-vacuous on the way past: it asserted
that `serde_json` rejects bad JSON, and now asserts that a body which *did* parse comes back
identical across nine shapes — including an `i64` above 2^53, which is the value every JavaScript
number would silently round.

## [2026-08-31] build | D5's last owed proof: two realms, one lock, observed

**What D5 had been owing since 2026-08-30.** Decision 031 requires at most one drain in flight per
scope across every realm on an origin. The mechanism — Web Locks in `frontbox-indexeddb` — was built
that day and the in-realm half was covered, but *nothing had ever watched two realms contend for
it*. The roadmap's D5 Status said so in as many words and refused to round up, which is the only
reason this was findable rather than folklore.

Pages affected: `crates/frontbox-indexeddb/tests/cross_realm.rs` (new), `scripts/verify.sh`,
`wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/plans/d5-persistence-backends.plan.md`,
`wiki/roadmaps/extraction.roadmap.md`.

### The second realm is a dedicated worker

The D5 plan listed "the cross-realm case may not be automatable" as a risk, with the failure mode
spelled out: *shipping an obligation nothing enforces while the suite reports green*. It was
automatable, and what unlocked it was giving up on the second realm being a second **tab**. A tab
needs a driver session that opens one and the harness drives a single page; a **dedicated worker**
is a separate agent — its own global scope, its own event loop, no view of the page's memory — and
Web Locks are managed per *origin*, shared across every agent on it. That is the isolation two tabs
have and core cannot bridge, which is the property under test.

What a worker does not reproduce is a second *wasm instance*: it is plain JavaScript, so it
exercises the lock manager rather than a second copy of frontbox. **Two tests are shaped so that
costs nothing**, and the composition is the argument rather than an appeal to the worker's
faithfulness:

1. `another_realm_draining_stops_this_one_from_sending` — the worker holds the scope's lock; this
   realm must report `AlreadyRunning`, send nothing, and **leave the queue where it was**. The last
   clause carries the weight: `AlreadyRunning` over an emptied queue is the double-send the
   mechanism exists to prevent, wearing the right label.
2. `a_pass_in_flight_holds_the_lock_against_another_realm` — the worker asks for the same lock from
   inside `send_batch`, an instant the pass provably holds the lease. Probing from inside the
   transport rather than from a second future means there is no timing assumption and nothing that
   could pass by racing.

Realm A's pass holds the lock (2) and realm B's claim refuses while it is held (1). Neither half is
assumed, which is what makes one JavaScript worker enough.

**The lock name is written out in the test, from outside the crate.** `locks.rs::lock_name` is
`pub(crate)`; importing it would make the fixture assert only that the backend agrees with itself.
Spelling `frontbox:drain:<scope>` makes it an origin-wide contract — the kind a service worker could
hold the other end of.

### Three sabotage runs, because the gap was a green test that could not fail

Each applied to `locks.rs`, run, reverted:

| Sabotage | Result |
| --- | --- |
| `try_claim` grants unconditionally — the mechanism is a no-op | Both fail: pass 1 reports `Completed` where `AlreadyRunning` was required; the worker takes the lock mid-pass |
| The lock name gains a `v2:` segment | Both fail identically — a name only this crate knows is indistinguishable from no lock at all |
| The lease's release closure never resolves | **Test 2 only**, on its final assertion; test 1 still passes, because its release comes from the worker rather than from a lease |

The third row is why the release assertion sits where it does, and it is the worst failure of the
three in production: the first drain succeeds, and every later drain in every tab stands down
forever.

### The gate moved, which is the part that generalises

`scripts/verify.sh` carried the browser command as a **comment** — a paragraph explaining how to run
the suite by hand. That is how D5 came to owe a proof line for a day: the claim was checkable and
unchecked. It is now a gate that runs the suite when `CHROMEDRIVER` names a driver, with a `skip`
helper that prints `1 GATE(S) SKIPPED` in the summary when it does not, so a machine without a
browser reports a smaller run rather than a quieter pass.

Two environment variables turned out to be load-bearing and only one was documented. The comment's
command omitted `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER`, and without it cargo hands the `.wasm`
to the shell, which answers **`cannot execute binary file`, exit 126** — a failure that looks like a
broken build rather than a missing runner. There is no `.cargo/config.toml` in the repository to
supply it. Both are now in the gate and in the test file's header.

### Verification

**31 gates, all passing** — 30 before plus `tests idb browser`. In headless Chrome: **68 conformance
cases and the 2 cross-realm fixtures, 70 green**, clippy clean at `-D warnings` for the new test
target. Chrome 151.0.7922.174 against chromedriver 151.0.7922.138. `--release` remains mandatory: a
debug build of the suite exceeds chromedriver's 300-second renderer timeout before a test runs.

## [2026-08-31] build | D4d: two domains, one queue, and the first invalidation that ever ran

**Two guarantees this project had already paid for and never tested.** Decision 036 refused to
partition the outbox and accepted head-of-line blocking as the price, explicitly to keep enqueue
order across types; decision 038 makes multiple origins a criterion for promoting the inbound
invalidation seam. Every test written before today sends to one server, so the first guarantee's
only evidence was that nothing had contradicted it, and the second could not be exercised at all.

Pages affected: `examples/user-server/` (new crate), `examples/todo-server/src/{lib,domain,wire,users,version,reads}.rs`,
`examples/todo-core/src/{invalidation,transport,store,backend,lib}.rs` and `src/app/{mod,identity,cache,rows,sync}.rs`,
`examples/todo-app/src/{main,platform,session}.rs`, `examples/todo-core/tests/multi_domain.rs`,
`docker-compose.yml`, `scripts/verify.sh`, `Cargo.toml`,
`wiki/plans/d4d-multi-domain-trial.plan.md`, `wiki/decisions/037-multi-service-routing.decision.md`,
`wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`,
`wiki/roadmaps/extraction.roadmap.md`.

### The sabotage is the observation

Observation 2 drains one scope and asserts on the **servers'** state: every todo landed owned by the
user record that preceded it. On its own that proves a drain worked.

Observation 3 is what makes it evidence. Same flow, one scope per service — decision 036's rejected
alternative, built on purpose — and the todo reaches a server that has never heard of its user. The
todo server answers `404`; **the classification of that is the transport's, not core's**, because
decision 019 opens by saying core does not classify HTTP and places on a *transport* the obligation
that a missing prerequisite is transient. So the routing transport maps it to
`MutationStatus::Blocked`: retained, counted in `counts.blocked`, no anomaly, no dead letter — and
**it heals on the retry once the user record lands**, which is what makes `Blocked` the right
classification rather than merely the kinder one.

### Six findings, two of which reach past the trial

**1. A partial batch burns decision 017's retention bound twice per drain.** Observation 4 expected
`attempts == 1` and got 2. A drain stops on the first pass making *no progress* (decision 029); one
service draining **is** progress, so the loop runs a second pass and re-sends to the still-silent
one. **Decisions 037 and 029 interact and neither said so** — a client whose services fail
independently reaches the bound in about half the drains a single-service client would. Not a
defect; a number a caller setting that bound has to know.

**2. `InvalidationSource` cannot be `dyn`.** `Vec<Box<dyn InvalidationSource>>` does not compile:
`poll` is an `async fn` in a trait, and those are not dyn-compatible. **Core has this constraint
everywhere and has never paid for it** — `SyncTransport`, `OutboxStore` and `CacheVersionStore` are
all `#[allow(async_fn_in_trait)]` under decision 001 and all used statically, which works because an
application has *one* of each. An invalidation source is the first seam where one application
plainly wants several at once, of different types. The trial enumerates them, which is fine for two
and does not generalise.

**3. The refresh button is not a source.** The plan called it one. Building it as one is what showed
why: **a button has no versions to deliver.** A source answers *what changed*; the only honest answer
a button has is *go and look*. So the two implementations are `ManualSource` and
`VersionPollSource` — push-shaped and pull-shaped, which is what decision 038's bar wanted — and the
button drives the same seam from outside.

**4. "The same URL for both services" is not a valid single-server configuration.** `Config.user_url`
became an `Option`. Pointing both at one host makes a one-service deployment look like a two-service
one whose user endpoints happen to `404`, and keeping such a client working means treating a read
`404` as normal — precisely the failure `transport.rs` documents, where a wrong base URL presents as
a healthy client with no data. Three regression tests failed on exactly this before the `Option`
went in, which is the only reason it was noticed today rather than in production.

**5. A versions endpoint cannot be routed by path.** Both services answer `GET /api/v1/versions`.
Not a hole in decision 037: the routing table places *mutations*, whose paths are domain-specific by
construction. A source carries its base URL instead — **a source speaks for one origin**, which is
the same fact per-source reconnect semantics rest on.

**6. A client's first poll marks everything stale.** An unknown version differs from the server's,
and a differing identity means refetch (decision 021). Correct, and a trap: observation 8 passed for
the wrong reason until a baseline poll and two `mark_fresh` calls went in front of it.

### Decision 038's bar, scored rather than deferred

Two of four criteria closed; the other two **answered**, which is better than left open:

- **No resume cursor is needed.** Reconnect-and-reconcile is one `GET`, a comparison, and done; the
  state that mattered is already core's durable `(version, stale)` pair. Decision 038 had said a
  cursor would be "durable per-scope state and core is the only thing that holds any" — so
  answering this **removes the strongest argument for core owning the inbound path**.
- **`SyncCadence` did not generalise.** `use_sync_loop` takes one call returning one report; an
  invalidation round is several sources with per-source failure and there is nowhere in that shape
  to put "source B dropped, so entity Y is stale and X is not".

Recommendation recorded: **do not promote yet** — on those two grounds rather than on waiting for
SSE.

### Verification

**All 31 gates pass.** Ten D4d observations over real HTTP against two servers, plus the earlier
trials unchanged: 10 D4a observations, 12 regressions, 3 tests each on `todo-server` and
`user-server`. **No `src/` or `crates/*/src/` file changed** — a fact about the diff, which is what
this deliverable's proof line asked for. `todo-server/src/lib.rs` and `todo-core/src/app/mod.rs`
both crossed the four-hundred-line cap on the way and were split, along the lines their own module
docs already drew.

## [2026-09-01] fix | A legacy sweep, and a gate that could not see the citations it was written for

Asked for a pass over deprecated, unused and legacy code on the grounds that a `publish = false`
project with no released version should be carrying none. It was carrying four kinds.

Pages affected: `crates/frontbox-indexeddb/src/convert.rs`, `crates/frontbox-indexeddb/src/versions.rs`,
`crates/frontbox-sqlite/src/schema.rs`, `src/record/mod.rs`, `src/id.rs`,
`crates/frontbox-dioxus/src/{lib,sync/mod,sync/steps}.rs` (and `handle.rs`, `invalidation.rs`,
deleted), `examples/todo-core/src/store.rs`, `examples/todo-server/src/lib.rs`,
`examples/todo-app/src/{main,session,sync}.rs`, `scripts/verify.sh`, and the wiki pages that cited
any of it.

### Schema tolerance for rows that cannot exist

**Seventeen `#[serde(default)]` on the IndexedDB row structs and five SQL column `DEFAULT`s.** Every
write serializes the whole struct and every `INSERT` names every column, so both could only ever
absorb a key or a column from a schema that has never shipped.

**The argument for removing them is not tidiness.** A default on a durable row turns a schema
mismatch into *silent data loss*: rename `precondition` and every stored row quietly reads as having
none, so decision 026's replayable preconditions stop applying and nothing says so. Without it
`from_js` fails, the row is treated as corrupt, and it reaches quarantine — which is the
corrupt-record visibility D5 was built on. The same for `attempts` (decision 017's bound quietly
restarting) and `stale` (an invalidated row reading as fresh).

Two of them were a day old: the D4d build gave `Todo.user_id` a default "for rows written before
D4d" and `todos.user_id` a `DEFAULT ''`, in a trial whose storage is created fresh every run.

### A whole public surface kept for a class of application that never existed

**`FrontboxHandle`, `use_frontbox`, `use_frontbox_context`, `use_sync_loop_on`, and two `from_handle`
constructors — deleted.** Register entry 14 replaced the hook's signature in D3b and kept the handle
"for an application that does let Dioxus own its runner". No such application ever appeared, and
entry 14 had *already written down* that the obvious way to use it is wrong:
`use_frontbox(|| SyncRunner::new(app.outbox().clone(), ..))` type-checks and gives two loops over one
queue, each believing itself alone. A convenience nobody uses, whose natural use is a trap, is not
worth a `0.x` surface.

**`use_invalidation` and `InvalidationState` — deleted** for a different reason: not legacy but
superseded in practice. D3b built them, nothing ever consumed them, and D4d built its own inbound
seam in `examples/todo-core` because decision 038 directed it to. The roadmap's D3b line hoping
"D4's example should shape it" now records what the example actually did.

### The ordering tiebreak, and the stale doc it was hiding

`OutboxRecord::order_key` returned `(seq, created_at, mutation_id)` and its own doc called the last
two "a tiebreak for rows written before the column existed … reached in no other case". `seq` is a
counter in memory, `INTEGER PRIMARY KEY AUTOINCREMENT` in SQLite and an `autoIncrement` generator in
IndexedDB — unique in all three — so it now returns `u64`.

**Removing it exposed a doc that had been wrong since decision 016.** `src/id.rs` justified
`MutationId`'s byte ordering and the binary-collation requirement on the grounds that "pending work
is ordered by the compound key `(created_at, mutation_id)`". Pending work stopped being ordered that
way when `seq` landed. The requirement survives — but for the *terminal* stores, whose lists are
`(rejected_at, mutation_id)` and `(quarantined_at, raw_mutation_id)` and whose tiebreaks case 68
shows are reached routinely. The doc now names the real consumer.

### The citation gate could not see most citations

`scripts/verify.sh`'s "cited paths resolve" gate matched ``​`path/to/file.rs`​`` with the closing
backtick immediately after the extension. **The wiki cites source as ``​`src/runner/mod.rs:275`​``
far more often than bare**, and every citation carrying a line number was invisible to the gate
written to check citations.

Widened to allow an optional `:line`, it found **eleven broken citations, four of them predating
this sweep** — including `src/runner.rs`, split two deliverables ago, and three pointing at
`crates/frontbox-dioxus/src/sync.rs`, which became a directory in D3b. All eleven repointed; the two
files that were *deleted* rather than moved are now described in prose rather than cited, so nothing
claims a dead path still exists.

### Raised, not cut

`StoredRow::schema` and `StoredRow::version` are written by the trial and read by nothing. They are
**not** legacy: both are part of decision 032's accepted `(blob, version, stale, schema)` shape and
both are round-tripped by case 62 on three backends. Removing them is an amendment to an accepted
decision rather than a sweep, so they are recorded here and left alone.

### Verification

All 31 gates pass, browser suite included — which is what shows the seventeen serde defaults and the
five column defaults were unreachable rather than merely unwanted. 455 tests across the workspace.

## [2026-09-01] decide | The row store loses `version` and `schema`, and 032 says why

Raised at the end of the legacy sweep and left alone there, because removing them amends an accepted
decision rather than clearing legacy. Asked to proceed, so both are gone and
`wiki/decisions/032-opaque-row-store.decision.md` carries an `## Amendment` rather than a quiet
diff.

Pages affected: `src/record/row.rs`, `src/testing/cases/rows.rs`, `crates/frontbox-sqlite/src/{rows,schema}.rs`,
`crates/frontbox-indexeddb/src/convert.rs`, `examples/todo-core/src/app/{rows,identity}.rs`,
`wiki/decisions/032-opaque-row-store.decision.md`, `wiki/plans/d5-persistence-backends.plan.md`.

`StoredRow` is now `(row, blob, stale)`.

### The argument is decision 032's own

What makes an opaque row store defensible — and what dissolved decision 023's objection that owning
rows means owning their canonical bytes — is that **core never deserializes the blob**. A
caller-supplied stamp sitting *beside* a blob core cannot interpret duplicates something the blob
can already say. The bytes are wholly the application's, so a shape number or a content version
belongs inside them, next to the code that understands both.

**Neither stamp bought a query.** Nothing filters on them and `list_rows` returns whole rows, so a
caller reading a stamp always had the blob in hand anyway. RxDB's version earns its place by letting
you see a row's shape *without* paying to decode it; that needs a read path returning the stamp
without the blob, and this store has never had one.

### The evidence is the trial's diff

`examples/todo-core` set `.with_schema(1)` at both write sites and read it back at neither — across
D4b, D4c and D4d, the three deliverables that were supposed to exercise the store. `version` was
never set outside the conformance case asserting it round-trips.

### `stale` stays, and the contrast is the rule this establishes

Core writes it (`RowStore::set_stale`), core reads it, and it means the same thing to core as to the
application. **That is what earns a field a place beside an opaque blob: core has to act on it.** A
number core carries and hands back is the application's to keep in its own bytes.

### Case 62 changed rather than shrank

It used to assert the two stamps round-trip. It now round-trips a nested, mixed-typed blob — object,
array, number, bool — which is a **stronger** claim about the promise that survived: a backend that
reshaped a nested object or narrowed a number passed the old flat fixture and fails this one. Losing
a field should not mean losing a case, and here it bought a better one.

### Verification

All 31 gates pass, browser suite included, so the shape is proven on all three backends. The SQLite
`rows_store` table drops two columns and the IndexedDB `RowRecord` two keys; both were rewritten
rather than migrated, which costs nothing for the reason the whole sweep costs nothing — no store
predating today exists anywhere.

## [2026-09-01] fix | The containers rebuilt from scratch, and the port trap that found

Asked to remove every example container and rebuild from nothing. The stack running was 19 hours
old, predated D4d, and had no `user-server` in it at all — so this was the first time the D4d
compose topology had ever been built or run.

Pages affected: `docker-compose.yml`, `examples/README.md`.

### It builds and it works

`docker compose down --rmi all -v --remove-orphans`, then `build --no-cache`: three images from
nothing, no errors. The full D4d story then runs against the containers rather than against an
in-process fixture:

1. A todo for a user nobody has heard of → **`404 unknown_user`** — `todo-server` reached
   `user-server` across the compose network and was told no.
2. The user record applied on `user-server`.
3. The same todo retried → **`Applied`**. It healed, which is the property that makes `Blocked` the
   right classification for that 404 rather than merely the kinder one.

Both versions endpoints moved off `cbf29ce484222325` — FNV-1a's offset basis, which is what an
empty read model hashes to — so the invalidation signal is live end to end.

### The trap: a healthy stack talking to the wrong port

Port 3001 was already held by an unrelated project on this machine, so `user-server` was published
on 3101 instead. **That silently broke the UI**, and nothing anywhere said so.

`platform::BASE_URL` and `platform::USER_BASE_URL` read their values through `option_env!`, so the
service URLs are compiled into the wasm bundle — a browser bundle has no environment to consult at
run time. `docker-compose.yml` wrote them as literals beside the port variables, so moving a
published port left the bundle pointing at the old one.

**There is no error for this.** A fetch to a port nothing is listening on fails the way being
offline does — which `examples/todo-core/src/transport.rs` maps to `Error::Offline` on purpose, and
which is the one condition this application is designed to treat as normal. Three healthy
containers, a green `docker compose ps`, and a UI that quietly never syncs.

Fixed by interpolation rather than by documentation: the build args now derive from the port
variables, `${TODO_SERVER_URL:-http://127.0.0.1:${TODO_SERVER_PORT:-3000}}`, so the two cannot
drift. Confirmed by `docker compose config` in both directions, and by grepping the served bundle —
which does contain `3000` and `3101`, the ports actually published.

The README had carried the manual workaround ("if you change the published API port, change the URL
compiled into the WASM at the same time") since D4a. That instruction is now obsolete rather than
merely unfollowed, and the page also gains the second service, which it had never mentioned.

### Verification

All 31 gates pass. Stack up on the rebuilt images with every service healthy and all three
endpoints answering.

## [2026-09-01] build | Nested user → todo CRUD, and a delete that cascades in the server

The trial UI showed one flat todo list belonging to whichever user a constant named. D4d had added
the user domain and the UI could only *sign up* into it. Asked for the demo to read as it should:
sign up, then manage a list of users, and inside each user a list of their todos.

Pages affected: `examples/user-server/src/{todos,domain,lib,reads,main}.rs`,
`examples/todo-server/src/{lib,direct,reads}.rs`, `examples/todo-core/src/{store.rs,app/{mod,identity}.rs}`,
`examples/todo-app/src/{users,rows,main}.rs`, `examples/todo-core/tests/{common/mod,multi_domain,observations,regressions}.rs`,
`docker-compose.yml`, `examples/README.md`,
`wiki/decisions/039-user-delete-cascades-server-side.decision.md` (new),
`wiki/plans/d4d-multi-domain-trial.plan.md`.

### The delete had been excluded, and the exclusion was waiting for this

`examples/user-server/src/domain.rs` carried an argument for having no delete at all, and the D4d
plan listed one under `## Explicitly Not Built`. Both said the same true thing: `todo-server` checks
a todo's owner only when the todo is *written*, so a bare delete leaves orphans nothing detects.

**Decision 039 is the answer that exclusion was avoiding rather than refusing.** The user service
empties a user of todos before removing them — asks the todo service what they are, deletes each,
then deletes the record — and **applies nothing if any step fails**. A cascade that pressed on past
a failure would be worse than none: record gone, one todo left, no signal.

**The cascade is the server's and not the client's**, and the reason is one sentence: ordering
enforced in one client is a property of that client. A second device deleting the same user gets
nothing from it. `TodoApp::delete_user` therefore enqueues exactly one envelope, which is what
observation 12 asserts by counting the outbox at one.

### The cost, written where it is built

**The two services now call each other.** `todo-server` → `user-server` for the reference check,
`user-server` → `todo-server` for the cascade. That is a dependency cycle, and three things fall out
of it that are worth having found:

- **Compose cannot express it.** `todo-server` already declares `depends_on: user-server`, so the
  reverse is circular and Compose refuses to start. Neither calls the other at *startup*, so the
  address alone is enough — and the absent `depends_on` now carries a comment saying it is a
  decision rather than an omission.
- **Neither service can be built before the other's address is known.** The in-process harness had
  to change shape: bind both ports, *then* build both states, *then* serve. It failed loudly first —
  two observations went red because `common::servers()` wired todo→user and not the reverse.
- In Compose the addresses are static config and none of this is visible, which is exactly why it
  is written down.

### `1 + N` calls, chosen and then confirmed

One `GET` for the list, one `DELETE` each. A bulk endpoint would be one call and is what a real
service should want. The per-todo form was kept on the argument that **a demo whose subject is order
should show the order**, and the containers bore that out: deleting a user with two todos produces
exactly `DELETE /api/v1/todos/b1 -> 204` and `DELETE /api/v1/todos/b2 -> 204` in the log. A trial
that hid its own mechanic behind one call would be a worse trial; a production service should make
the opposite trade, and decision 039 says so.

### What the client deliberately does not do

**It does not withdraw queued work for a deleted user.** A todo create still in the outbox drains,
meets `404 unknown_user`, is classified `Blocked` by the routing transport, and is retained until
decision 017's bound dead-letters it. That is correct and worth showing: an enqueued mutation is a
durable fact, and a client that reached into its own outbox to cancel writes would be a client whose
queue means something different from one drain to the next.

### `create` lost its implicit owner

`TodoApp::create(title)` became `create(user_id, title)`. Once the UI can address any user, "whose
todo is this" stops being a property of the client and becomes a property of the call — and an
implicit owner would be a second, quieter way to say something the caller already knows. Around
forty call sites across three test files; mechanical.

### The UI

Two levels sharing one pattern: a user head is `[expander] [name] [tag] [count] [×]` and a todo row
is `[checkbox] [title] [mark] [×]`. **`TodoRow` needed no changes at all** to be nested inside one,
which is the payoff of making them the same shape.

- The page **gates on having a user record** — nothing todo-shaped before signup — but keeps the
  offline switch and the status bar, because signing up *while offline* is the headline behaviour
  and is only legible if the queue is visible.
- The list is **the server's user table**, not this client's creations, so two browser windows
  converge after a poll. That is the only thing on the page that demonstrates
  `InvalidationRunner::apply` doing anything.
- Deleting a user **confirms inline with the count as the warning**, because it destroys rows on a
  different service. A user with no todos skips the confirm.
- Both servers crossed the four-hundred-line cap on the way and were split along the line
  `todo-server` had already drawn: `reads.rs` for what a client reads, `direct.rs` for the endpoints
  that take one mutation in a header rather than a batch in a body.

### Verification

**All 31 gates pass, 458 tests.** Three new observations carry the weight: a user record round-trips
through the queue; deleting a user empties them of todos **asserted on the todo service**, which
nothing in the client wrote to; and a cascade that cannot run leaves the user alone and heals on
retry. Confirmed again in containers end to end, where the cross-service call is real rather than
in-process.

## [2026-09-01] fix | Driving the UI in a browser, and the defect the conformance suite could not see

Reported: no button to add a user or a todo, and the offline switch had disappeared. Asked to debug
with Playwright and test every feature. Three real defects, and the third is the one worth keeping.

Pages affected: `examples/todo-app/src/{main,sync,session,users,platform}.rs`,
`examples/todo-core/src/store.rs`, `crates/frontbox-indexeddb/src/request.rs`,
`examples/todo-app/e2e/ui.mjs` (new), `examples/README.md`, `scripts/verify.sh`.

### 1. The gate never re-rendered

`Ready` computed "is this client signed up" from `user_name`, which reads a `RefCell` Dioxus cannot
see into, **without reading `Ui::revision` first**. So signing up updated `SessionBar` — which does
read the counter — while the gate underneath went on rendering "sign up to begin". The page
acknowledged the signup and refused to move past it, which is exactly what was reported.

`Ui::revision`'s own doc describes this trap. Writing the doc did not prevent making the mistake one
component later, which is an argument for the e2e harness below rather than for a better comment.

### 2. The offline switch was dropped from the tree

Restructuring `Ready` around the nested list simply left `sync::OfflineControl` out. The plan said
`SessionBar` would keep it; the build did not. It is the control this demo exists to be operated
with, and now it lives in the session bar where the wireframe put it — `OfflineControl` renders a
bare label rather than its own row so the bar can place it.

### 3. Every IndexedDB read transaction threw, and nothing noticed

The console carried a burst of ~47 `closure invoked recursively or after being dropped` at every
page load. A debug build put the stack in `frontbox_indexeddb`, at `IDBTransaction`.

`Txn`'s `Drop` detached its `oncomplete`/`onabort` handlers **only in the abort branch**. That
covers write transactions and misses every read: a reader is dropped without `commit`, its
`completion` future frees the closures, the transaction then completes normally, and the browser
calls a `Closure` the Rust side has already freed.

**The conformance suite has been green through all of this**, and the reason is worth recording: the
throw happens inside the browser's event dispatch, so no Rust future observes it and no assertion
fails. 70 browser tests pass either way. **Only something reading the console finds it** — which is
why the harness below asserts on console errors as loudly as on behaviour.

### The wake, which running it also found

Unticking `offline` left the drain loop serving out `offline_ms` — fifteen seconds of a queue
visibly not draining after the user has just said the network is back. `Wake` exists for precisely
this ("the wait a loop is serving is a bet about what happens next, and some events settle that bet
early") and nothing was firing it.

There is now **one app-wide wake**: bfcache restore, tab becoming visible, and coming back online all
fire the same one, because `Sleeper::wakeable` takes a single `Wake` and that is the right model
rather than a limitation. The e2e run asserts the drain happens within three seconds of unticking,
which is less than the backoff it would otherwise wait out.

### `examples/todo-app/e2e/ui.mjs`

Thirty-two assertions over ten sections: the gate, offline signup, offline todos and their drain,
adding a user, the nested list, todo CRUD, user rename, the delete confirm and its cancel, **the
cascade asserted against the todo service** rather than the projection, and a reload.

Not in `scripts/verify.sh` — it needs a running stack and a browser install, which a build gate
cannot assume — and documented in `examples/README.md` with the two defects it caught in its own
header, so the next person knows what it is for.

**Its waits are matched to `SyncCadence` rather than guessed.** `idle_ms` is 5s and `offline_ms` is
15s; shorter waits fail a working application, which cost a full debugging pass before being written
down.

### Also

Users rendered in **id** order, so a Bob sat above an Alice for no visible reason — client-generated
uuids sort arbitrarily. Now by name, with your own row lifted to the top.

### Verification

**33 gates pass** (the two mobile gates from earlier today included), and the e2e run is **32 passed,
0 failed, no console errors** against the containers.

## [2026-09-01] build | A justfile, and the nested UI opened on all four platforms

Asked to open the iOS, Android and native builds, and for a `justfile` in the style of a sibling
project. Both, and the second turned out to be the fix for the first.

Pages affected: `justfile` (new), `examples/README.md`.

### The justfile is not convenience, it is where the URL rule lives

`platform::BASE_URL` and `USER_BASE_URL` read through `option_env!`, so the service addresses are
compiled into the binary — a wasm bundle or a phone build has no environment to consult at run time.
Every platform therefore needs its URLs supplied at *build* time, and each needs a different answer:
a browser and an iOS simulator reach the host at `127.0.0.1`, an Android emulator only at
`10.0.2.2`, and this machine has something else on 3001 so the user service is published on 3101.

Before today those combinations lived in a shell history. **`just examples <platform>` derives every
URL from the port**, exactly as `docker-compose.yml` was made to after the same trap caught a
container run. Override the port, never the URL.

Recipes: `check` (which is `scripts/verify.sh` and nothing else, because a second list of gates here
would be a second thing to keep in step), `test`, `lint`, `cov-html`, `browser`, `chromedriver`, and
`examples` with `all | web | native | ios | android | e2e | servers | logs | down`.

`chromedriver` earns its place: the version has to match the installed Chrome's build, and that
friction is what kept the IndexedDB suite unrun for two deliverables.

### All four run the same UI

| Platform | How it was confirmed |
| --- | --- |
| Web | Screenshot, plus 32/32 in `e2e/ui.mjs` |
| macOS desktop | Process alive, SQLite WAL written, and both servers logging its hydration and polls — **no screenshot: this process has no screen-recording permission**, so it was verified by effect rather than by sight |
| iOS 26.1 simulator | `simctl io screenshot` — the nested list, signed in, hydrated over the host loopback |
| Android emulator | `adb screencap` — same, reached over `10.0.2.2` |

**The Android screenshot is the most useful of the four**: it shows three users, of which two were
created from the *other* platforms' sessions. The list is the server's user table
(decision from the D4d UI work), so that is cross-platform convergence visible in a single frame —
and the only place on the page D4d's invalidation runtime shows itself.

Two things worth writing down about capturing this:

- The first Android screenshot caught the launcher splash and read as a failure. It was not: `adb
  shell dumpsys activity` showed `dev.dioxus.main.MainActivity` already resumed and the process
  doing GL work. **A screenshot is a sample, not a state** — the activity dump is the answer.
- The desktop arm prints a warning about register entry 20 before it launches. The drain loop stalls
  on Dioxus desktop after a few seconds, and a person running the demo there should be told that by
  the tool rather than discover it as a bug in this project.

### Verification

33 gates pass. The four platform arms were each run through the justfile rather than by hand, which
is the only thing that makes them evidence about the recipes.

## [2026-09-01] fix | Browser invalidation: stale rows and cross-tab polling

Reported: the browser could show a user the user service no longer returned, and a second browser
window did not eventually show users or todos added in the first. Both were real defects.

Pages affected: `examples/todo-core/src/{trace,invalidation}.rs`,
`examples/todo-core/src/app/{cache,identity,mod,rows,sync}.rs`,
`examples/todo-core/src/transport/{mod,error,shared}.rs`,
`examples/todo-core/tests/hydration_regressions.rs`, `examples/todo-app/src/{main,rows,session,sync,users}.rs`,
`examples/todo-app/e2e/ui.mjs`, `examples/todo-core/Cargo.toml`,
`examples/todo-app/Cargo.toml`, `wiki/plans/d4d-multi-domain-trial.plan.md`,
`wiki/proposals/browser-background-services.proposal.md`, `wiki/log.md`.

### Stale users were merged forever

`refresh_users_from_server` merged server rows but never deleted cached users missing from the
server response. After a container restart or server reset, IndexedDB could keep yesterday's user
table on screen indefinitely. User hydration now keeps only server rows, skipped rows protected by
pending work, and rows bound to queued user mutations.

### Cross-tab polling was hidden by the shared version store

Each browser tab has its own polling source state, but same-origin tabs share the IndexedDB cache
version store. One tab could observe a server version change and write the new version before the
second tab polled. Core then correctly reported "no durable version change" in the second tab, and
the app incorrectly treated that as "nothing to hydrate." The app-level tick now preserves known
entities delivered by a source, so a tab hydrates when its own source observes a transition even if
another tab already advanced the shared version row.

### Verification

Added regressions for server-absent user deletion, pending user/todo protection, and the shared
version-store two-tab case. Manual Playwright verification against the rebuilt Compose stack showed
`PW Bob 0458` appear in the mirror tab, then Bob's todo count move to `1 todo`, without pressing
refresh. Browser console: zero errors and zero warnings.

## [2026-09-01] fix | Two windows: the wake that reached one loop, and the marker that never cleared

Reported, from two browser windows open side by side: a todo created in one did not appear in the
other; gaining focus was supposed to refresh the cache and "works sometimes, mostly fails"; todos
reached the server but their row stayed on "saving…"; and a window would stop polling entirely.
Four symptoms, three defects — the second and fourth are one.

Pages affected: `crates/frontbox-dioxus/src/sync/wait.rs`,
`crates/frontbox-dioxus/src/sync/wait/tests.rs`, `examples/todo-core/src/app/sync.rs`,
`examples/todo-core/tests/hydration_regressions.rs`,
`examples/todo-app/src/{main,platform,rows,session,style,sync,toast,users}.rs`,
`examples/todo-app/e2e/ui.mjs`,
`wiki/decisions/040-one-wake-releases-every-waiter.decision.md`,
`wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md`,
`wiki/decisions/042-events-are-toasts-conditions-are-the-status-bar.decision.md`,
`wiki/references/open-decisions.reference.md`, `wiki/index.md`, `wiki/log.md`.

### One wake released one loop, and which one was a race

`Wake` held a single `Cell<bool>` and a single `Option<Waker>`. `Woken::poll` consumed the latch and
overwrote the slot, so of the two loops waiting on the trial's one wake — the drain loop and the
invalidation poll — a fire released exactly one. **Measured before changing anything, over six fires:
both loops woke 1 of 6; the drain alone 3; the poll alone 2.** The alternation is the signature of
the single slot, and it is why the report said "sometimes".

The trial's own comments asserted the behaviour the type did not implement, in two files. Entry 16
had reasoned about one waiter and produced a shape that could only serve one.

`Wake` is now a generation counter and a waker list, with the latch per waiter — a fire releases
every sleeper, and each spends it once. All five original tests pass unchanged; six were added.
Re-measured after: **6 of 6, both loops, 18–24 ms.** Decision 040.

`Sleeper::until_woken` came with it, and a `RefCell` borrow held across `waker.wake()` went with it —
unreachable under Dioxus's channel-based waker, now pinned by a test using one that polls inline.

### The window did stop polling, for ten minutes at a time

`HIDDEN_TICK_MS` was 600 000, sampled *before* the sleep, so losing focus committed the poll to a
ten-minute wait that only the wake could shorten — and the wake was going to the drain loop. The two
defects composed exactly into "the window stops polling completely".

It is a pause now, not a long interval: `until_woken`, no timer running. And `platform::app_wake`
listens for `focus` as well as `visibilitychange`, because **two side-by-side windows are both
`document.visible`** — clicking between them fires `focus` and no `visibilitychange` at all, which is
the entire reported case. The drain loop is deliberately *not* paused: those writes are already the
user's and already queued. Decision 041.

`TICK_MS` also moves 15 000 → 5 000. It had been set equal to the staleness budget under a comment
saying it must be "no larger than the tightest" — equal is the boundary case, putting the worst case
at two budgets. One measured write took 11.3 s to cross.

### A row another window drained said "saving…" for ever

On web the outbox is one origin-scoped IndexedDB database, so two windows are two `TodoStore`s over
one queue — while the "saving…" index is per window and in memory. Decision 035 moves that index by
what a pass's `DrainReport::drained` names, which is correct for work *this* client drained and says
nothing about work another realm drained out from under it. Whichever window won the Web Lock
decremented its own index; the other never heard, and `refresh_pending` had run only at startup since
035.

Reproduced deterministically by switching one window offline: the record reached the server through
the other window, and the two disagreed about the same row — `saving` in one, not in the other.

`TodoApp::sync` now reconciles when a pass accounted for nothing while the index still claims
outstanding work. The ordinary single-window path still decrements and never scans. It also closes
the quarantine case, where a record leaves the queue and is deliberately excluded from `drained`.

And `refresh_pending` stopped counting a queued `user` record as *unplaceable*: signing up offline
and reloading reported a healthy queue as "index incomplete: 1 unplaceable". Only a record naming no
row at all is unplaceable.

### Three message surfaces became one rule

`SyncView::error`, `SyncView::notice` and `SessionBar`'s own `status` had no rule between them, and
the composer had picked wrong: todo failures went to `error`, which the drain loop clears on every
success, so a refusal vanished within five seconds. The file explaining why the two were separate was
next door to the call site conflating them.

What happened is a toast; what is still true is on the status bar. The status bar gained the cache
line this trial has needed repeatedly and never had — per-source freshness, in-flight, and the paused
state — so a service unreachable for an hour is now a sentence rather than an absence. `tick.dropped`
was populated every round and rendered nowhere. Decision 042.

The two `let _ =` refetch bindings are gone with it: a failed hydrate was invisible and bumped the
revision anyway, which is most of why "the second window never showed the first window's write" took
a browser session to find.

### Verification

Unit: 11 in `wait.rs`, including two waiters on one wake and the inline-waker re-entrancy. Two
regressions in `hydration_regressions.rs` — a record drained by a second realm, and the unplaceable
count — **both confirmed to fail without the fix** before being kept.

Browser: the before/after wake table above, and the two-window disagreement reproduced and then
absent. `examples/todo-app/e2e/ui.mjs` gained a focus-gating section and toast assertions.

**Focus is synthesised and said so.** Chromium under an automation protocol will not report a page as
unfocused — `document.hasFocus()` is `true` for every page in the context, headed as well as
headless, and `bringToFront` fires no `visibilitychange`. Measured before the test was written. The
init script overrides the two properties `session::focused()` actually reads and dispatches the real
events; everything downstream is the shipping path, which is the posture the bfcache wake was
verified under.

## [2026-09-01] fix | Being in front, on a phone and a desktop window

Reported: the focus gating built earlier the same day works well in the browser and does nothing on
iPhone, Android or desktop. Correct, and not an oversight to discover — `session::focused()` had a
native arm hard-coded `true` under a comment saying mobile lifecycle gating was named and not built.

Pages affected: `crates/frontbox-dioxus/src/{native,lib}.rs`, `crates/frontbox-dioxus/Cargo.toml`,
`examples/todo-app/src/{platform,session,main}.rs`, `examples/todo-app/Cargo.toml`,
`scripts/verify.sh`, `wiki/decisions/043-in-front-on-every-platform.decision.md`,
`wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md`,
`wiki/compatibility/dioxus-adapter.compat.md`, `wiki/references/open-decisions.reference.md`,
`wiki/index.md`, `wiki/log.md`.

### One handler covers all three, because they are the same crate

`dioxus` aliases both `desktop` and `mobile` to `dioxus_desktop`, and `use_wry_event_handler` hands
over the raw `tao::event::Event`. The adapter gains a default-off `native` feature and
`use_lifecycle_wake`, the counterpart of `use_bfcache_wake` — because an OS suspending a process is
the *correctness* event the bfcache argument describes, not the policy judgement a hidden tab is.
Decision 043.

`platform::app_wake()` now returns `(Wake, Foreground)`, and `session::focused()` lost its `cfg`
split entirely: every platform difference is back in `platform.rs`, which is what that file's header
always claimed.

### Verified on real OS transitions, not synthesised ones

**Android** — `adb shell input keyevent KEYCODE_HOME`, relaunch. Ticks, then
`ui invalidation paused (nobody is looking)`, then a tick *immediately* on return. **iOS** — the
same, backgrounding via `simctl launch com.apple.mobilesafari`. A prompt resume is only possible if
the loop received the wake, so both runs exercise decision 040 on native too.

Unlike the browser, where Playwright cannot report a page as unfocused and `e2e/ui.mjs` has to stub
`document.hidden`, these are the real events.

### Desktop is wired, inert, and the reason is upstream

`WindowEvent::Focused` is the right signal and never arrives: `dioxus-desktop` drops every window
event whose `window_id` differs from the handler's, and they do not match here. Dumping the raw
stream gave **350 events across two deliberate resizes and three focus changes, containing no
`WindowEvent` at all** — the resizes are what make it the filter rather than an absence of focus
changes. `use_window().is_focused()` is no rescue: it reports `false` while the window is frontmost.
A gate built on it paused a healthy loop 13 s after launch, and that version was built, measured and
deleted — a wrong answer is worse than none when the failure is a client that silently stops.

Desktop therefore stays permanently "in front", which is what it did before. Nothing regresses.

### A correction to entry 20

This morning's note that the desktop executor stall was gone — 19 ticks over 95 s — **did not
survive the afternoon.** Three later runs on the same build stalled after 2 ticks and never
recovered, twice with the new event handler and once with it deliberately unregistered, so the
handler is not the cause. Entry 20 is intermittent; the 19/19 was luck, and the UI-clock hypothesis
built on it is withdrawn. Recorded in the register rather than quietly dropped.

## [2026-09-02] verify | The four arms re-run against rebuilt containers, and the desktop window stops floating

Asked to refresh every container and the application and re-test the focus work from 2026-09-01.
`just examples down && just examples all` rebuilt both services and the web bundle from scratch, so
nothing below rests on an image that predates the feature. All three feature arms compile —
`todo-app` with `web`, with `desktop`, with `mobile`, and `frontbox-dioxus` with no features at all,
which is the gate that proves a browser build never links wry.

Pages affected: `justfile`, `wiki/decisions/043-in-front-on-every-platform.decision.md`,
`wiki/log.md`.

### What the arms did

Web: `just examples e2e`, **41 passed, 0 failed**, no console errors, section 11 included. Android:
HOME, `ui invalidation paused (nobody is looking)` 1.6 s later, **38 s with no tick**, and a tick
answering the relaunch to the same second. iOS: Safari over the top, **41 s with no tick**, then the
paused line and a tick 0.42 s behind it. Desktop: 2 ticks, then nothing across a deliberate defocus
and refocus — entry 20, unchanged.

### The desktop window floated, which is why the arm could not be driven by hand

Reported from the outside: the native app is always on top, which makes losing focus awkward to
produce. Correct, and it is `dioxus-desktop`'s default — `always_on_top().unwrap_or(true)` in its
`config.rs`. `just examples native` now passes `--always-on-top false`, which reaches the app as
`DIOXUS_ALWAYS_ON_TOP`. In the recipe rather than in `main()`: `launch(App)` stays free of a
desktop-only branch, which is what `main.rs`'s own header claims about the file.

It does not rescue the arm — the window event still never arrives — but it removes a second reason
the arm could not be tested, leaving only the first.

### The first Android measurement was wrong, and the way it was wrong is the finding

It read as "the gate does nothing": requests at both services every 5 s, straight through a
40-second background, unchanged by force-stopping the app. Three clients were polling those two
services at once — a stale iOS build and a desktop process left running from the previous day, and a
browser tab — and `GET /api/v1/todos -> 200 OK` names none of them.

**A per-application log is the only sound instrument for this claim.** `adb logcat -s
RustStdoutStderr:V` and `dx serve`'s captured stdout each name their process; the server log cannot.
Both then showed the pause the server log had hidden.

A second trap sits behind that one: `dx serve` may exit once it has launched a desktop build, and the
app's stdout dies with the pipe, so a silent log looks exactly like a stalled loop. Running the built
binary directly is what separates them, and it is what confirms today's desktop stall is real.

## [2026-09-02] fix | The 2026-09-01 worktree review, checked claim by claim: two real, one inverted

Eleven findings arrived from a worktree review. Every one was reproduced against the code before
anything was changed, which is the only reason this entry can say that **one of the three High
findings was wrong and the fix it proposed was the dangerous one**.

Pages affected: `examples/todo-app/src/users.rs`, `examples/todo-app/src/toast.rs`,
`examples/todo-app/src/platform/mod.rs`, `examples/todo-app/e2e/ui.mjs`,
`examples/todo-core/src/app/{mod,prune,sync,identity}.rs`,
`examples/todo-core/src/transport/{mod,send}.rs`,
`crates/frontbox-indexeddb/src/{scan,store,rows,versions,factory}.rs`,
`crates/frontbox-indexeddb/tests/schema_mismatch.rs`, `examples/todo-core/tests/**`, `.gitignore`,
`wiki/compatibility/indexeddb-adapter.compat.md`, `wiki/references/open-decisions.reference.md`,
`wiki/log.md`.

### The finding that was wrong, and why it looked right

`HttpTransport::send_batch` logs a non-offline destination failure and drops it, so a batch where
*every* destination failed returns `Ok` carrying no verdicts. The review read that as a bad endpoint
becoming silently retained records, and proposed returning the first failure instead.

Built, it broke `observation_13_a_failed_cascade_leaves_the_user_alone` — a cascade the user service
cannot run answers 503, and that is transient by design. Following the thread further gave the real
reason: `SyncRunner::run` answers a transport `Err` with `return Err(error)` **before any outcome is
applied**, so the attempt is never recorded. The `Ok` path retains each unnamed record with
`reason: Some("no verdict returned")` **and increments `attempts`**, which walks decision 017's
retention bound and dead-letters. **The proposed fix would have made a permanently broken endpoint
retry forever** — the unbounded behaviour it was meant to prevent.

What was real in the finding is that none of this was *visible*: such a pass is identical to a
healthy one with nothing to say. `HttpTransport::last_send_failure` records it — a condition, not an
event, so it is the status bar's half of decision 042. Three regressions pin the contract, including
one whose doc comment exists so the same change is not proposed a third time.

### The finding that was real, and worse than reported

`all_in_scope` filtered decode failures out with `filter_map`. The review called it silent row loss.
For the outbox it was an **unreachable row**: absent from `pending_batch` and `pending_count`, which
is the documented contract, and absent from `sweep_corrupt`'s scan — the one path that exists to
make a corrupt row visible. It sat in storage indefinitely, counted by nothing and named by nothing.

The scan now returns what failed alongside what decoded, with primary keys from `getAllKeys`,
because a value that will not parse cannot be asked for its own key. `sweep_corrupt` quarantines
both kinds in one pass; the entry has no `mutation_id`, which is the id-less path
`QuarantinedRecord` already models. Verified in a real browser: 72 tests, including two new ones.

**No `CorruptKind` could express this**, and that is the reason it went unnoticed: all three shared
variants produce a row that parses and then fails a semantic check. SQLite reads column by column
and cannot reach the state at all, so this is an adapter suite rather than a fourth variant every
factory would have to imitate. The dead-letter and quarantine stores keep the drop, which is a trait
shape rather than an oversight — register entry 25, and stated per store in the new compatibility
page.

### The hydration cap, which cost data

Deletion protection asked for `pending_batch(pending_scan_limit)` and stopped there, so a queue
longer than the limit left its tail unprotected: queued, unseen by the server, absent from its list,
deleted. `prune_rows_absent` is now one function both hydrations call — the decision was being made
twice, which was the actual problem — and a scan that comes back exactly full is treated as
truncated, because the cost of guessing "complete" wrongly is deleted user data and the cost of
guessing the other way is a stale row.

### Two corrections to the review's smaller claims

`use_context::<Ui>()` in the Add-user callback is real and fixed, but it does **not** panic: the
e2e clicks that button and passes. It appends to the scope's hook list on every click, and the extra
slots land after the ones render reads, so the damage stays invisible until a hook is added below
it. The `to_pretty_json().unwrap()` calls in both servers are inside `#[cfg(test)]` — correct where
they are, and not the doc-generation hazard reported.

### Everything else

`.playwright-mcp/` ignored and its 2.6 MB of generated console dumps removed. Required environment
variables asserted at the top of `e2e/ui.mjs`, where an unset `TODO_URL` used to surface minutes
later as a fetch against `undefined/api/v1/todos`. `SAFETY:` comments on both Android JNI blocks.
Six files split to the four-hundred-line cap, three of them ones this work had pushed over it — the
cap gate measures `src/`, so those would have failed `scripts/verify.sh` on the next run.

## 2026-09-02 — A File-By-File Pass, And Two Files That Claimed Things They Did Not Do

A second review, this one per-file rather than per-finding. Nothing it found was a correctness bug.
Two of them were worse than that in a particular way: a file asserting a property it did not
enforce.

### `e2e/ui.mjs` said it asserted on console errors and did not

Its header claims the run asserts on JS errors "as loudly as it asserts on behaviour", and
`justfile` repeats the claim. The errors were collected, printed at the end, and never reached
`check()`; the exit code read `fail` only. So the exact defect the file was written to catch — the
`Txn::drop` handler leak, one uncaught throw per IndexedDB read, invisible to every Rust test
because the throw happens inside the browser's event dispatch — would have scrolled past and exited
0. **A claim a harness does not enforce is worse than no claim, because it is read as coverage.**
Now an assertion like any other.

### Three files cited the wrong document

`style.rs`, `platform/foreground.rs` and `sync/wait/tests.rs` attributed the four-hundred-line cap
to `project_guidelines.md`. It is in `AGENTS.md`; the other document never mentions it. The
cited-paths gate in `scripts/verify.sh` checks that a cited path *resolves*, not that the claim is
there, which is why this passed — worth knowing about the gate rather than a reason to widen it.

### `native.rs` had no test, and now has five

The module carried a table of target-to-signal mappings and a transition rule, all of it inside a
closure whose argument type cannot be named from outside `dioxus-desktop`. That is what made it
untestable, so `in_front` came out as a free function generic over the user-event type and
`Lifecycle::observe` as the transition rule. The rule that earns its test: **a repeated signal is
not a second wake**, because Android emits `onResume` *and* `onWindowFocusChanged` and every resume
there arrives twice.

`WindowEvent::Focused` stays untestable — `Event::WindowEvent` is `#[non_exhaustive]`, so the arm
can be matched and not built. Recorded in decision 043 rather than worked around: a constructed
event would prove the `match` arm and not the delivery, and the delivery is the half that is broken.

### One scan, one key source

`purge_older_than` held a second hand-rolled copy of `all_in_scope` — same two requests, same zip by
position — so the two could drift on the property that makes either correct. `ScopeScan` now carries
the key beside every decoded row, which also removed the last place a delete key was *reconstructed*
rather than read: `apply_outcomes` and `sweep_corrupt` both built one from `seq.unwrap_or_default()`,
and a row missing that field would have resolved to key `0` — a real key belonging to a real record.

### A mixed batch now names the endpoint that answered

Following on from yesterday's transport work. One destination unreachable and another answering 503
reaches nothing, so `send_batch` returns `Error::Offline` — correct, since the queue must not burn
decision 017's bound on an aeroplane — but the early return discarded the failure that *had* been
observed. The pass then presented as an ordinary offline one while a genuinely broken endpoint was
the thing holding the queue up, which is the worst case for that condition to be blank in.

### Duplication removed

Five identical private `log` functions in `todo-app` — same two-arm `cfg`, same `[frontbox]` prefix,
nothing enforcing that they stayed identical. The prefix is what `e2e/ui.mjs` filters the console
on, so one drifting would have made a module silently invisible to the harness rather than obviously
wrong. Also: the terminal-store rationale, duplicated verbatim across two `DeadLetterStore` methods,
moved to the `impl`; and `load_users` decoded every row twice per hydration to produce a count the
first pass already had.

## 2026-09-02 — A Dead-Code Sweep, And Why It Came Back Almost Empty

A deliberate hunt for legacy and unused code across every crate. Worth recording the *method*,
because the result is small and a small result is only reassuring if the search was sound.

**Where dead code can hide here, exhaustively.** `scripts/verify.sh` runs clippy with `-D warnings`
across the workspace, and `dead_code` covers anything not reachable from a public path — so every
private and `pub(crate)` item in the repository is already proven live by a gate that runs on every
change. That leaves exactly three hiding places: fully `pub` items (the compiler assumes an external
caller), explicit `#[allow(dead_code)]`, and a test module's blanket allow. All three were
enumerated rather than sampled.

**What came out.** Four things, plus one correction to the search itself:

- `InMemoryStore::row_count` — an entire `impl` block with its own doc header, for one method with
  no caller in production, in a test, in a conformance case, or in a doc link. No sibling on the
  SQLite or IndexedDB backends and named in no spec or decision: a speculative affordance.
- `InvalidationReport::any_stale` — a one-line predicate whose doc called it "the predicate a caller
  branches on". Nothing branched on it. `marked_stale` is a public field, so the check was always
  available to a caller; the decision-013 nuance it tested is documented on the `unknown` field,
  which is where a reader would look for it.
- `TodoApp::transport_handle` — zero callers. The invalidation sources take `transport.handle()`
  directly during construction, so this wrapped a call nothing made.
- A stale `#[cfg_attr(not(feature = "testing"), allow(dead_code))]` on `SqliteBackend::connection`,
  which is now used unconditionally by five modules. An `allow` that has stopped being true is worse
  than none: it is a standing instruction to the compiler not to check.

Both core removals were taken to the user first. `AGENTS.md` gates removing a public item the same
way it gates adding one, and neither was named in any spec or decision — which is what made them
removable rather than merely unused.

**Two that looked dead and are not**, recorded so the next sweep does not re-derive them.
`Error::corrupt_unidentified` has no caller, but `Error` is `#[non_exhaustive]`, so it is the only
way an adapter can construct the id-less `CorruptRecord` — the exact path
`wiki/decisions/006-corrupt-record-policy.decision.md` describes. `EntityState::fresh_at` is the
third of the `unknown`/`stale_at`/`fresh_at` triple that both adapters' docs point at for
reconstructing a state; removing it would leave an asymmetric API to save four lines.

### The correction: `FixedClock` is not dead

The first pass flagged it, on a script that treated every file under `tests/` as test-only and then
asked which of them production used. `common::open` builds one, and every suite calls `open`. The
lesson is about the tool: a query that partitions by *directory* answers a different question from
one that partitions by *reachability*, and the two disagree exactly where a helper is used by its
own module.

### Stale references to files this branch renamed

The cited-paths gate matches paths beginning `src|crates|examples|scripts|wiki`, so a bare
`` `platform.rs` `` or `` `observations.rs` `` is invisible to it — and this branch turned six files
into directories. Ten such references were left pointing at nothing: in `scripts/verify.sh`,
`examples/todo-core/src/lib.rs`, the shared test harness, decision 043, the open-decisions register,
and the D4a plan. All repointed. Not worth widening the gate for: the same pattern would flag
crate-relative citations like `` `tests/conformance.rs` `` that are correct where they are written.
## [2026-09-02] fix | Two external reviews, seven findings, and the three they both missed

Two independent reviews of this branch were checked finding by finding against the files. **All
seven hold.** The interesting part is where they were imprecise, and what neither looked for.

### What was fixed

- **Status drift, in five documents rather than two.** Both reviews caught `AGENTS.md` and
  `README.md` still calling D4d unbuilt. Neither noticed that `wiki/index.md` and
  `wiki/roadmaps/extraction.roadmap.md` — cited as the *correct* side — say "the three trial crates"
  when there are four, and that `examples/README.md` opens "split into three packages". `README.md`
  also still owed D5's two-realm proof line, met on 2026-08-31, and quoted 30 gates and 89.17%
  region coverage against today's 35 and 89.79%.

  `AGENTS.md` carries a paragraph warning that this exact drift happened before, for D4b and D5,
  "which meant the gate was blocking work already done and the schema was misdescribing its own
  project". It then did it again, for D4d, in the same file.

- **`user-server`'s outage switch covered two reads of three.** `AppState::down` documents itself as
  failing every read; `GET /api/v1/users/{id}` had its own query and never consulted it. That is the
  one read another *service* makes, and it made `Missing::Unavailable` unreachable — the branch that
  exists so an outage cannot present to a client as a durable ordering failure had no test because
  the fixture could not produce the condition. Now a shared `refuse_if_down` gate, plus
  `observation_14_a_user_service_outage_holds_a_todo_rather_than_refusing_it`, which fails with the
  one line removed. See the D4d plan's `### Observation 14`.

- **Three endpoints that answer 503 and never said so.** Both `todo-server` reads and
  `user-server`'s `list` go through a `down`-gated helper with no 503 in their `utoipa` responses.
  Review 1 gestured at this and aimed it at `one` — the single endpoint that correctly omitted 503,
  because it could not return one.

- **A doc block on the wrong function.** `direct.rs` had `direct_error`'s documentation orphaned onto
  `remove`, so `remove`'s rendered summary was "Answer a direct write with a refusal the client can
  parse" and `direct_error` had no doc at all. Two functions wrong, not one.

- **Eight controls with no accessible name.** The reviews named four symbol-only buttons. Beside them
  sat a checkbox and two text inputs with nothing at all — an `<input>`'s *value* is not its label —
  so a screen reader read a todo list as "checkbox, edit text, button" per row and never said which
  todo. All eight named, and the toast's dismiss button gained an `aria-label` alongside its `title`:
  for a `<button>` the name comes from its content, so the glyph was winning and `title` was only
  ever the description.

- **`is_some_and` where the file said `true` three times.** `foreground.rs` states that a build which
  cannot tell whether anybody is looking should keep polling — on the `app_wake` fallback, on
  `has_focus().unwrap_or(true)`, and on the no-renderer arm — and the web arm's outer half defaulted
  the other way. Now `is_none_or`. Unreachable today; recorded in decision 043 because the value of a
  rule stated three times is that the fourth reader can rely on it.

- **`RequestFuture` now detaches its handlers on drop**, the same rule `Txn::drop` already followed.
  Not reachable in this crate — every call site is `await_request(..).await?` in one expression — and
  that is a property of the callers, which is exactly the property that quietly stopped holding for
  read transactions.

- **The waker table is keyed by waiter.** See the amendment on decision 040. The reviews called this
  Medium; it is Low — nothing was ever missed — and the fix they proposed, a slot on the waiter,
  costs the back-reference and `Drop` the original comment declined. Keying the shared table costs
  one `usize`.

### What neither review looked for

**`scripts/verify.sh` misattributed its own rule.** Its line-cap gate said "the line cap
`project_guidelines.md` sets". That file contains no such rule — `AGENTS.md` sets it. Both reviews
treat `verify.sh` as the authority on this repository without checking whether it cites its authority
correctly.

**`todo-server` had the identical 503 omission**, one directory from the file review 1 scored 6.5 for
it, in a file it scored 8.4 and praised for its "good version/read split".

**The D4d plan's observation 4 was right about the design and wrong about the code.** It explains at
length that it stops the todo server because "stopping the user server would make todo mutations fail
too". Under the exempt read, it would not have.

### On the review that scored every file

One review scored 285 files. **203 of them (71%) are untouched by this branch**, and 232 of 285 rows
(81%) carry advice byte-identical to at least one other row: all 43 decision files scored exactly
8.3 with one sentence, all 35 `raw/` files exactly 6.8, 39 of 52 `src/` files exactly 8.7. That is a
directory classifier, and it produces advice that contradicts the repository — `wiki/log.md` marked
down for length and told to add archive indexes, when `project_guidelines.md` specifies it as
append-only. Its 53 file-specific rows were good, and they cluster where the top findings already
were.

The lesson is not that scoring is useless. It is that a per-file score has to be *derived* from
reading the file, and a table that fills in from the path will confidently rate a defect and its
identical twin nine tenths of a point apart.

## [2026-09-05] create | RepForge queued-write coalescing request

Recorded RepForge's request for an opt-in outbox operation that collapses repeated offline writes to
the same row. Checked the frontbox-side claims against the current storage trait, record shape,
runner, and transport semantics. The product need holds; the proposed `attempts == 0` safety rule
does not, because attempts count retaining verdicts rather than every transport handoff.

Pages affected: `wiki/references/repforge-queued-write-coalescing-request.reference.md`,
`wiki/index.md`, `wiki/log.md`

## [2026-09-05] create | queued-write coalescing proposal

Added the proposed frontbox contract for safe same-row queued-write coalescing. The proposal keeps
the opt-in method shape RepForge asked for, but makes durable `transport_started` knowledge the
eligibility condition and records that a safe implementation needs store-level send claims rather
than a wrapper over `pending_batch`.

Pages affected: `wiki/proposals/queued-write-coalescing.proposal.md`, `wiki/index.md`,
`wiki/log.md`

## [2026-09-05] create | queued-write coalescing implementation plan

Drafted the execution plan for implementing queued-write coalescing across core, in-memory, SQLite,
and IndexedDB. The plan adds public coalescing outcome types, durable `in_flight` and
`transport_started` facts, conservative migrations for existing outbox rows, runner claim/release
changes, and conformance cases for replacement, refusal, migration, and attempted-send safety.

Pages affected: `wiki/plans/queued-write-coalescing.plan.md`, `wiki/index.md`, `wiki/log.md`

## [2026-09-05] revise | queued-write coalescing review corrections

Reviewed the coalescing commit against the code it cites and rewrote the proposal and the plan. The
`attempts == 0` correction held; three things below it did not.

**The offline release rule reproduced the bug it was written to fix.** The plan released a pass
ending in `Error::Offline` with `transport_started` still false, so an offline application could
keep coalescing. But `src/transport.rs` tells implementors to return `Offline` when a browser
`fetch` fails for lack of connectivity, and such a `fetch` rejects identically whether the request
never left the device or reached the server and lost its response. The trace: edit 1 sends, the
server applies it, the response is lost, the flag stays false, edit 2 coalesces into the same
`mutation_id`, the next send is deduped as `Duplicate`, and edit 2 is deleted having never been
applied. Inferring "never sent" from how a send failed is the same error as inferring it from
`attempts`, one layer out.

The fix moves the fact ahead of the request: one durable `transport_started`, written inside the
transaction that reads the batch, never cleared. That deleted `in_flight`, the three-way release
taxonomy, and the abandoned-row recovery pass, all of which existed only to serve the unsound rule —
and it dissolved a second finding with them, that a cancelled `use_future` on Dioxus would strand
in-flight rows. Marking before the send costs the motivating scenario unless the runner can decline
to read while offline, so `SyncTransport` gains an `is_offline` probe whose failure mode is
one-sided: a wrong "online" costs coalescibility, and no answer it can give permits an unsafe
rewrite.

**`AppendIfMissing` returned `NotQueued` for an intent with no `RowRef`** — `Ok(..)` for a write that
was silently dropped, which is the failure decision 006 exists to refuse. It now appends in every
non-replacement case and can no longer return `NotQueued` at all.

**Both durable migrations were one line describing work the backends cannot do.**
`crates/frontbox-sqlite/src/schema.rs` is a single `CREATE TABLE IF NOT EXISTS` with no
`user_version`, no `ALTER TABLE`, and no migration step anywhere in the crate, so "adds both columns"
is that crate's first schema-versioning mechanism. `crates/frontbox-indexeddb/src/convert.rs`
forbids `#[serde(default)]` on durable rows by name, so the plan's read-side default needed an
argued exception rather than silence — admissible here because the defaulted value is the
conservative one, which is the boundary now written into the plan. Both migrations shrank to
metadata: one nullable column, one defaulted field, no walk over user data.

Also corrected: replacement now takes `body`, `op`, `traceparent`, and `created_at` from the new
intent and keeps only `seq`, `mutation_id`, and `precondition`, because the first four describe a
body that is being replaced and `op` is what a human reads off a dead letter; the partial-update
hazard is stated, since a caller using `RequireExisting` cannot know the body it is discarding;
neither new `OutboxStore` method may be defaulted, unlike `claim_drain`, whose granted default is
correct rather than merely permissive; and `read_for_send` must read and mark in one transaction,
or a replacement landing between the two lets the server's verdict delete a body that was never
sent.

**Three pages claimed "every deliverable with a plan is built"**, which the new plan falsified the
moment it landed — the failure the roadmap's own Sequencing Principle describes. All three now say
the sentence is about the D0-D6 sequence and name the coalescing plan as sitting outside it,
unbuilt and unauthorized.

Pages affected: `wiki/proposals/queued-write-coalescing.proposal.md`,
`wiki/plans/queued-write-coalescing.plan.md`,
`wiki/references/repforge-queued-write-coalescing-request.reference.md`,
`wiki/roadmaps/extraction.roadmap.md`, `AGENTS.md`, `wiki/index.md`, `wiki/log.md`

## [2026-09-05] build | queued-write coalescing on all three backends

Authorized and built the same day the plan was corrected. `OutboxStore` gains `enqueue_coalescing`
and `read_for_send`, `SyncTransport` gains `offline_now`, and the three public types
`CoalescingPolicy`, `CoalescingEnqueue`, and `CoalescingRefusal` are exported. Decision 044 carries
the argument.

**One durable boolean, written before the request.** `transport_started` is set inside the
transaction that reads a batch for sending and never cleared, so no failure has to be classified
afterwards. That is what the earlier design got wrong twice over — RepForge's `attempts == 0`, then
frontbox's own reading of `Error::Offline` — and recording the fact ahead of the request deleted the
`in_flight` flag, the three-way release taxonomy, and the crash-recovery pass along with the bug.
`offline_now` keeps the feature useful: asked before the runner reads, so an offline application
does not spend its own coalescibility polling, and one-sided by construction, so no answer it can
give permits an unsafe rewrite.

**Two renames and one guard changed from the plan as written.** The probe is `offline_now`, not
`is_offline` — `Error::is_offline` already asks a different question, and two `is_offline` meaning
two things is a name a reader disambiguates every time. And SQLite's migration is guarded by
`PRAGMA table_info` rather than by `user_version` alone: a fresh database gets the column from the
schema batch while still reporting version zero, so a version-only guard would fail on the first
open of every new database.

**The two durable backends cost what the review said they would.** SQLite gained its first
schema-versioning mechanism — there was no `user_version`, no `ALTER TABLE`, and no migration step
anywhere in the crate, so `CREATE TABLE IF NOT EXISTS` would have left every existing database
without the column and every statement naming it failing. The column is nullable and carries no
`DEFAULT`, which keeps `schema.rs`'s stated rule intact and makes the migration DDL-only. IndexedDB
needed no version bump but did need the crate's one exception to its ban on `#[serde(default)]` for
durable rows, with the bound written into the module doc: a default is admissible exactly when the
defaulted value is the conservative one.

Four files passed the four-hundred-line cap and were split: `src/store.rs` into a directory with
`coalescing`, `terminal`, and `rows`; `src/runner/config.rs` out of the runner;
`crates/frontbox-sqlite/src/coalescing.rs` and `crates/frontbox-indexeddb/src/coalescing.rs` out of
their stores; and `crates/frontbox-indexeddb/src/convert.rs` into a directory. The cited-paths gate
caught twenty-one wiki citations left pointing at the two renamed files, which is exactly the rot it
was written for.

Proof: cases 70-79 green on all three backends — in-memory and SQLite natively, IndexedDB in
headless Chrome against a downloaded driver matching the installed browser — plus a SQLite case that
builds a pre-migration database by hand and asserts its rows still send and refuse to be rewritten.
`./scripts/verify.sh` reports ALL GATES PASSED with the browser gate run rather than skipped, and
coverage at 89.87% lines / 94.41% functions.

Not done: the trial crates still use the default `offline_now`, so the probe has no end-to-end
witness in `examples/`. RepForge's own adoption — deleting the profile form's second-write refusal —
is theirs.

Pages affected: `wiki/decisions/044-transport-started-before-the-request.decision.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/proposals/queued-write-coalescing.proposal.md`,
`wiki/plans/queued-write-coalescing.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `AGENTS.md`,
`wiki/index.md`, `wiki/log.md`, and the twenty-one pages whose `src/store.rs` and
`crates/frontbox-indexeddb/src/convert.rs` citations were repointed

## [2026-09-05] build | the trial adopts queued-write coalescing

Closes the gap the previous entry named: it recorded that "the trial crates still use the default
`offline_now`, so the probe has no end-to-end witness in `examples/`". They do now.

`examples/todo-core` gained `TodoApp::rename_coalescing` **beside** the existing `rename` rather
than in place of it. Keeping both is the point — a trial carrying only the coalescing path could
show the feature working and not what it changes, and the difference between one queued write and
two is the whole claim. `HttpTransport` implements `offline_now` from the offline switch it already
had, which is the easy case for the probe and the one worth showing: the plug is known to be out, so
there is nothing to infer and no request to attempt.

Four observations against the real axum/sqlx server, which does not depend on `frontbox` and so
counts what actually crossed the wire rather than what the client believed it sent:

- **15** — two offline renames of one row reach the server as one recorded mutation, carrying the
  later title.
- **16** — the plain `rename` path still queues both and the server records two. Without this, 15 is
  measured against nothing. Both end in the same title, so what coalescing buys is the request that
  never happened, not a different answer.
- **17** — an offline poll between the two renames does not spend coalescibility. This is the
  observation that justifies `offline_now` existing: without it the poll would mark the queued
  rename read-for-sending and the second would append.
- **17b** — a poll that did reach transport *does* spend it, and the second rename appends. The case
  that must keep refusing to collapse, or the feature would be unsafe.

`examples/todo-app`'s rename switched to the coalescing path, and its `onchange`-not-`oninput`
comment — which named a queued write per keystroke as the reason for the choice — now says why the
choice stands anyway: coalescing collapses repeated edits between drains, and stops the moment a
drain reads the record.

**Two things this does not prove, both recorded in the plan rather than left to be found.**
`RequireExisting` has no end-to-end witness: this trial sends no preconditions, RepForge's
server-assigned `updated_at` has no analogue in a todo list, and a fixture built to produce one would
be flattering the feature rather than exercising an application that needed it. And the
`examples/todo-app` change is proven by clippy and two wasm builds and nothing else, because that
crate has no tests — the same seam Finding 6 of the D4a plan records, where both halves were green
and nothing looked at the join.

Also found: **the cited-paths gate reads `git ls-files`, so it never scanned any of the new
untracked files.** The first ALL GATES PASSED of the day was therefore weaker than it looked. Staging
the new files and re-running is what made it mean what it says, and it caught a fabricated path in
`examples/todo-core/src/app/coalescing.rs` that had been sitting green.

Pages affected: `wiki/plans/queued-write-coalescing.plan.md`, `wiki/specs/frontbox-runtime.spec.md`,
`wiki/log.md`

## [2026-09-05] revise | coalescing review: one real hole, and what the review missed

An external review scored every file and returned one High, three Mediums and a Low. Checked each
against the code rather than taken on trust. **The High is real and was a genuine safety bug.**

`read_for_send` was treated as the only way a record reaches the server. It is not:
`apply_outcomes` is public, and `OutboxStore`'s own contract says a backend "cannot assume the runner
is its only caller". A direct caller could read with `pending_batch`, send the batch through its own
transport, and apply a `Retain` — leaving `attempts` incremented and `transport_started` false, so
the record still read as coalescible after the server had certainly seen it. The next edit would then
rewrite its body under an identifier the server dedupes against, and the newer body would be deleted
without ever being applied. Exactly the failure this feature was designed to prevent, reached through
the one door the implementation had not checked.

The rule that closes it is the same reasoning the rest of the design already rests on rather than a
patch on top: **a `Retain` is a verdict, and a verdict cannot exist without a request.** All three
backends now set the mark there too. Conformance case 80 was written before the fix and confirmed
failing on in-memory, then on SQLite, then verified green on all three including IndexedDB in a
browser.

Two of the three Mediums were also right. Several public comments still described `Offline` as
leaving work untouched, which is now true of the queue's contents and false of the durable mark —
and `SyncTransport::offline_now` claimed to produce "the same result as returning `Error::Offline`"
two paragraphs above explaining why it does not, which is the most embarrassing kind of stale doc:
self-contradictory inside one comment. And SQLite stamped `user_version` unconditionally, so an older
binary opening a newer database wrote the marker backwards; it now only moves forward, with a test
mutation-checked to fail without the guard.

The third Medium was not a defect. The review read the wiki's "browser gate run rather than skipped"
as an overclaim because its own `verify.sh` run skipped that gate for want of `CHROMEDRIVER`. The run
did happen — but the page asserted it without saying how, which made it unreproducible and therefore
indistinguishable from an overclaim. Fixed by naming the method: `just chromedriver` then
`just browser`, two recipes this repository already carried and that the earlier work reimplemented
by hand instead of finding.

**Three things the review missed**, found while checking it:

- `sweep_corrupt` no longer runs on every pass — one that ends at `offline_now` touches storage not
  at all — so its contract's promise that "an application that syncs at all closes the window on its
  own" is now bounded by connectivity rather than by the poll interval. The doc says so.
- `SyncPass::Offline` now covers two report shapes: everything zero when the probe answered,
  everything counted when the send did. Documented on the variant.
- `StoreOp` carries no `#[non_exhaustive]` while every other public enum here does, and this work
  added two variants to it. Deliberately left alone — adding the attribute is an unrequested
  public-API change and the present risk is nil at `publish = false` — but recorded rather than
  overlooked.

The Low was correct and needed nothing: `.serena/` is local tool state and stays untracked.

Also worth keeping: **the first ALL GATES PASSED of the day did not cover the new files.** The
cited-paths gate reads `git ls-files`, so anything untracked is invisible to it. Staging before
running is what makes that gate mean what it says.

Pages affected: `wiki/decisions/044-transport-started-before-the-request.decision.md`,
`wiki/plans/queued-write-coalescing.plan.md`, `wiki/log.md`

## [2026-09-05] revise | second coalescing review: the contract had not caught up with the fix

A follow-up review of the staged feature. **No correctness defect survived** — the `apply_outcomes`
bypass is genuinely closed on all three backends and case 80 pins it. What it found instead was a
gap between the fixed behaviour and the prose describing it, which is worth logging because of the
shape rather than the size.

**The public contract still defined coalescing eligibility as "not read for sending."** That was true
before the bypass fix and false after it: `apply_outcomes` applying a `Retain` now sets the mark too,
because a verdict cannot exist without a request. So the trait doc, the refusal variant, and the
spec's behaviour table were all describing a rule the code had outgrown. Nothing was broken by it
*today*, and that is exactly what makes it worth fixing — a backend written against the doc rather
than against the suite would have reintroduced the hole case 80 exists to catch. Eligibility is now
named as one durable fact, `transport_started`, with both of its writers stated wherever it appears.

**Offline wording still led with a claim nobody can make.** Five places said "no request reached the
server" or "no request could be attempted", which decision 044 spends a whole section explaining is
unknowable: a browser `fetch` rejects identically whether the request never left or its response was
lost. They now lead with what is actually known — the transport *reported* the network unavailable —
and say that the weaker phrasing is deliberate. `Reply::Offline` in the scripted transport was the
sharpest case: it is produced by a `send_batch` that was *called*, so "no request could be attempted"
was not merely imprecise but backwards.

**Two runner regressions the plan asked for did not exist.** Now cases 81 and 82: an attempted
transport failure, and a response that omits the record's verdict. Both had been left implicit on the
grounds that they resemble cases 79 and 80, and writing them showed that reasoning was sloppy — the
five no-verdict paths take *three* different values of `attempts` (0 for the probe, offline and
transport failure; 1 for an omitted verdict, which decision 019 synthesizes into a `Retain`; 1 for a
direct `Retain`) while agreeing exactly on the mark. That divergence is the argument for
`transport_started` being a stored fact rather than a derived one, and until now nothing tested it.

**One finding was wrong, and the correction is recorded so it is not "fixed" back.** The review read
the spec's "78 cases" as stale and asked for 80, reasoning from the existence of case 80. Case
numbering is global and stable — a case keeps its number when it moves file, case 30 is blocking-only
and case 45 no longer exists — so the count and the highest number were never the same quantity. With
81 and 82 added the count is 80 and the highest number is 82, and the spec now says so in as many
words rather than leaving a coincidence to be misread again.

Smaller things from its per-file notes, taken because they were right: IndexedDB's `read_for_send`
now returns early on a zero limit instead of opening a read-write transaction and scanning the store
to discover it has nothing to do — which was blocking a concurrent `enqueue_coalescing` for the
length of a scan that could never write. SQLite's replacement `UPDATE` now checks it matched exactly
one row; it cannot fail today, and the reason to check is that the alternative to a loud failure is
a silent `Replaced` for a row that was never written. `crate::lib` states what adding a *required*
trait method means for external backend implementors, and why both of coalescing's new methods are
required rather than defaulted: a default that appended instead of replacing, or one that fell back
to `pending_batch`, would silently skip the mark the safety rests on. The stale "IndexedDB is D5 and
does not exist yet" comment in `tests/in_memory.rs` now says what that compiled-not-run suite is
still *for*, which is that the browser gate is the one most likely to be absent on the machine where
an async-wrapper mistake is made.

Two of its per-file notes were declined with reasons. SQLite's `seq`/`attempts` widening casts stay
casts — `AUTOINCREMENT` issues only increasing positives and `attempts` is written solely as `0` and
`attempts + 1`, so a negative means the file was edited by something other than this crate, and
erroring on a *read* would let one tampered row wedge a healthy queue, which decision 006 exists to
refuse. The argument and the fix-if-it-changes are now written where the cast is. And the in-memory
`enqueue_coalescing`'s branch density stays: the nesting is what makes the `RefCell` borrow scope
visible, and that borrow not outliving the match is the property the comment above it is about.

Pages affected: `wiki/plans/queued-write-coalescing.plan.md`,
`wiki/specs/frontbox-runtime.spec.md`, `wiki/decisions/044-transport-started-before-the-request.decision.md`,
`wiki/index.md`, `wiki/log.md`

## [2026-09-06] create | cache versions are optional, and the docs said otherwise

RepForge reported a wrong turn, and it is the useful kind: not "this API is awkward" but "I did a
thing that cost me time, and here is what would have prevented it". They opened a `CacheVersionStore`
handle while integrating, **because the reference application did**, and removed it later on
realising nothing in their application read it. Nothing broke. What it cost was a detour and a
belief — that invalidation is part of the shape you adopt.

**The belief was our fault, and it was purely a documentation fault.** Versions have been optional
since they were built, in every structural sense that matters: a separate trait in `src/cache/store.rs`
sharing no method with `OutboxStore`, a separate type in every backend, and a separate conformance
suite whose macro doc already said a backend without a version store simply does not invoke it. None
of that is what a reader meets first. What they meet is the trait's own opening line, which called it
*"the third storage trait, alongside `OutboxStore` and its companions"* — an inventory of obligations,
not a menu. Structurally optional, rhetorically mandatory. That sentence is the whole defect.

Their proposal was two changes: say in adoption docs that a plain periodic re-read is a valid
strategy with guidance on when versions are worth it, and make the trait visibly optional. Both
accepted. Both turned out cheaper than they look — the second needs no restructuring at all, and the
first is largely a *promotion* job, because the analysis already exists in
`examples/todo-core/src/invalidation.rs` under "A staleness budget, not a poll schedule" and in
`wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`. It was filed where an adopter
would never look: inside a trial crate's module docs, and in a decision about internal sequencing.

**A third point was added, and it is the one that would have caused the next bug.** "Periodic
re-read is fine" is true and incomplete. `InvalidationRunner::stale` is generic over
`CacheVersionStore`, so dropping the version store also drops the report that refetching an entity
would discard unsent local work. Decision 014 exists because the source system had exactly that
defect — `src/cache/runner/conflict.rs` describes its listener as one that "refetches eagerly and
never consults the outbox". An adopter told only that periodic re-read is acceptable, who then
removes the version store and re-reads on a timer, has rebuilt that defect faithfully, and its
failure mode is silent loss of the user's unsent edits. The page says give it up on purpose, and
shows how to keep the check without the runner: it needs the outbox, not versions, and
`pending_batch` is inspection-only so an application can attribute pending work itself.

Kept separate from all of that, because conflating them is the tempting mistake: **"do I need
versions" and "must versions be durable" are different axes.** Decision 015 answers the second yes,
and a page arguing the first could easily be read as licence for an in-memory version store — the
machinery with none of the benefit.

**One defect found while writing it.** `DEFAULT_CONFLICT_SCAN` was `pub` inside a private module, so
nothing downstream could name it, while its twin `DEFAULT_BATCH_LIMIT` is reachable — and the
constant's own doc argues the two match deliberately "so nobody assumes one of them is a
coincidence", a comparison no external caller could make. Re-exported. The page tells an application
skipping versions to bound its own conflict scan, so it has to be able to name the default.

Pages affected: `wiki/compatibility/cache-versions-are-optional.compat.md`, `wiki/index.md`,
`wiki/log.md`
