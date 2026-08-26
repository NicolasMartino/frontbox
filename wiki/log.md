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
