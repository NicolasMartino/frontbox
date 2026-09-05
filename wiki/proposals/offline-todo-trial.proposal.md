# The Offline Todo Trial Splits Into An API Proof And A Durability Proof

Document Class: Proposal
Status: Accepted; D4a built 2026-08-29
Date: 2026-08-29
Category: Architecture
Scope: How to build the requested Dioxus todo application and axum/sqlx server without either overturning the roadmap's D4-before-D5 ordering or demonstrating offline-first against a store that forgets.
Sources: `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/extraction-boundary.proposal.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/references/repforge-eight-answers.reference.md`, `src/lib.rs`, `src/runner/drain.rs`, `src/runner/mod.rs`, `src/store/mod.rs`, `src/protocol.rs`, `src/record/mod.rs`, `src/testing/mod.rs`, `crates/frontbox-dioxus/src`, `raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md`, `raw/initial/2026-08-25T083750Z/sources/frontend/dto.rs`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/extraction-boundary.proposal.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/027-dead-letter-reason.decision.md`, `wiki/decisions/028-drain-loop-boundary.decision.md`, `wiki/decisions/029-drain-termination.decision.md`, `wiki/references/repforge-eight-answers.reference.md`, `wiki/specs/frontbox-runtime.spec.md`

## 0. Summary

The ask is a simple todo application with a full Dioxus CRUD interface and an axum/sqlx server, "to
see the offline multiplatform at work". Every part of that is buildable today except the one word
the demo exists to prove.

**Storage is in-memory only.** `InMemoryBackend` is the only backend that exists; SQLite and
IndexedDB are D5 and unwritten. `AGENTS.md` defines this library as "a durable local queue of writes
that survives restarts", and *durable* is the one adjective an in-memory run cannot show. A demo
whose queue evaporates on reload demonstrates a retry buffer, not the thesis.

The roadmap orders D4 before D5 deliberately, and that ordering is right and should not be
overturned to get a nicer demo. The resolution is not to reorder anything but to stop treating the
trial as one deliverable: **D4a proves the API against `InMemoryBackend` and can start now; D4b
proves durability against D5's backends, and proves it by the application not changing.**

## 1. The Two Claims That Cannot Both Be Made Today

The roadmap's sequencing principle is unambiguous: the migration trial "is the only real test of
whether the extracted API is right, so it comes before the full SQLite and IndexedDB backend ports",
because porting both first "would move roughly 2,300 lines of backend code before any real
application flow validates the API". Wrong-way cost: two backends written against a public API no
application has ever held, with every later correction becoming a two-backend migration.

The offline claim is equally unambiguous and points the other way. A todo app that loses its writes
when the browser tab closes has not shown offline-first working. It has shown a queue.

**Both survive if the trial is split rather than reordered.** Nothing about D4a needs a durable
backend to test whether the API expresses a real flow, and nothing about D4b re-decides the API.
They answer different questions and only one of them is blocked.

## 2. D4a Is Not New — It Is D4 As Already Written

The roadmap's D4 already includes "Run against an in-memory or thin adapter backend before full
backend ports." D4a therefore invents nothing. What this proposal adds is the refusal to let that
run be *presented* as the offline demonstration, and the naming of the half that actually is one.

D4a's work, unchanged from D4: one thin flow migrated onto frontbox abstractions, always-enqueue
writes, an optimistic projection, network-first reads with cache fallback, and the preferences-style
direct-dispatch-then-fallback pattern reproduced in application code under a caller-supplied
`MutationId`. A todo list stands in for the exercise flow honestly: create, rename, toggle, and
delete are four writes with different shapes, and toggle is the one that wants direct dispatch.

## 3. D4b's Result Is The Diff, Not The Demo

D4b runs the same application against SQLite on desktop and IndexedDB on web. **The fact that the
application needs no changes above the storage seam is itself the result being demonstrated**, and
that is a stronger claim than "it works" because it is falsifiable by inspection: either the diff
touches only the construction site, or the seam is in the wrong place.

The seam is concrete. An application is generic over the store traits — exactly the bound the
adapter's loop already takes, `S: OutboxStore + DeadLetterStore + QuarantineStore + 'static`
(`crates/frontbox-dioxus/src/sync/mod.rs`) — so swapping backends is a type substitution at the
single call that builds the runner. `StoreFactory`
(`src/testing/mod.rs:100`) is the conformance suite's seam behind the `testing` feature; it proves
the *backends* agree, not what the application holds.

If D4b's diff reaches into a component, the finding is worth more than the demo: it means either the
traits or the adapter leak something backend-specific, and both are cheap to fix while the crate is
`publish = false`.

## 4. What The Server Has To Be

**An axum service exposing `POST /api/v1/sync`, accepting `{ "mutations": [...] }` and answering
`{ "results": [...] }`** — the wire format decision 010 already fixed, at the source system's path
(`08-offline-sync.spec.md:24`). The request is `MutationBatchRequest` with one `mutations` field
(`src/protocol.rs:200`), each element carrying exactly `mutation_id`, `method`, `path`,
`client_datetime`, `body`, the five keys `MutationIntentDto` defines (`frontend/dto.rs:98-108`),
with `client_datetime` as RFC 3339. The answer is one `MutationResult` per mutation ruled on, each
`{ mutation_id, status, error }` (`src/protocol.rs:169`), `status` PascalCase across `Applied`,
`Duplicate`, `Rejected`, `Blocked`, `Pending`. A server may omit a result; the runner leaves that
record queued rather than inventing a verdict.

**`mutation_id` is the idempotency key** (`frontend/dto.rs:36`), and it obliges the server to
persist it *in the same sqlx transaction as the domain effect*. A separate insert makes the key
decorative: a crash between the two commits leaves an applied write with no record that it was
applied, and the resend duplicates it.

**The demo must actually exercise a resend or the key proves nothing.** `apply_outcomes` is atomic
on the client only (decision 003). A process that dies after the server commits and before the
verdict is applied leaves the record queued, so the next drain sends it again — and on web that
window is a page reload, not an exotic fault.

**One thing the batch endpoint cannot carry.** `precondition` and `traceparent` are both
`#[serde(skip)]`, documented as travelling as headers on the drain request
(`src/record/mod.rs:121-122`, `:147-148`). A batch of N records with N different preconditions has
one request and therefore one header. Decision 026 is implemented, and this is the first place it
meets a batch transport. See §6.

## 5. What The Demo Must Actually Demonstrate

Falsifiable observations, not features.

1. **Kill the network, make three writes.** `OutboxCounts::read` reports `pending == 3`
   (`crates/frontbox-dioxus/src/counts.rs:32`) and the list still renders all three from the
   application's own optimistic projection.
2. **Restore it; one drain empties the queue.** `SyncRunner::drain` (`src/runner/drain.rs:197`)
   returns a `DrainReport` with `ended: DrainEnd::Drained` and `counts.applied == 3`
   (`src/runner/drain.rs:61`, `:28`). Assert `passes`: two at the default batch limit — one that
   sends and one that finds the queue empty — and four at `with_batch_limit(1)`
   (`src/runner/mod.rs:69`), because a drain stops when a pass drains nothing, not when the queue is
   known empty (`wiki/decisions/029-drain-termination.decision.md`). The number is the observation.
3. **Reload mid-queue and see the work survive.** *This is the observation D4a cannot make.* It is
   the whole reason D4b exists, and owing it openly beats quietly not testing it.
4. **Make the server refuse one.** It lands in dead letters carrying
   `DeadLetterReason::Rejected { error: Some(_) }` (`src/store/mod.rs`, `src/record/terminal.rs:34`) and the UI
   renders the server's message rather than the row vanishing. `dead_letters` rises as `pending`
   falls, the pair `OutboxCounts` documents as meaningful only together
   (`crates/frontbox-dioxus/src/counts.rs:7-9`).
5. **Resend safely.** Per §4: kill the client between the server's commit and the client's apply,
   drain again, assert one todo and a `Duplicate`.
6. **Terminate a wedged record.** Hold the server at `Pending`, set a low bound with
   `with_retention_bound` (`src/runner/mod.rs:98`), and watch the record dead-letter as
   `DeadLetterReason::RetentionBound` — a positive assertion on the variant rather than on an absent
   payload, which is what decision 027 exists to make possible.

Observations 1, 2, 4, and 6 run against `InMemoryBackend` today. Observation 5 runs today only in a
desktop shell, since a web reload also wipes the queue. Observation 3 does not run at all.

## 6. What The Trial Is Expected To Find Wrong

A trial that is expected to succeed is a demo. Four places the API is most likely to feel wrong.

**Direct dispatch may not express cleanly in application code.**
`wiki/proposals/extraction-boundary.proposal.md` keeps direct-dispatch-then-fallback out of core on
the bet that a caller-supplied `MutationId` is enough, and says to revisit if D4 cannot express a
real flow that way. Todo toggle is the flow to try it on. The bet fails if the component ends up
reimplementing the transport's terminal-versus-retryable split to decide whether to enqueue, which
is decision 019's problem relocated into a UI event handler. Cost if it fails: a second send path in
core, a third status vocabulary, and a terminal-refusal error variant — the three things that
proposal priced and refused.

**The read-model boundary is decided and unbuilt, and the todo list is a read model.**
Decision 023 rules that frontbox stores `(entity, row_id, stale)` markers and never the rows. Those
markers do not exist yet, so D4a builds the blob store above frontbox itself — and in D4a that store
is *also* in memory, giving the demo two forgetful layers rather than one. Concretely the app cannot
say "this row is stale" and must choose between refetching the whole list on any invalidation or
refetching nothing, and both are visible in a UI. This is the largest gap the trial will hit, it is
known in advance, and it is the strongest argument for treating D4a's findings as input to D5.

**`OperationMeta` probably does not carry enough for a UI to label optimistic state.**
`OperationMeta { name, version }` (`src/record/mod.rs:39`) rides on the intent uninterpreted, which
is right. But a row rendering "saving…" must answer *is there a queued mutation for row X*, and
nothing answers that cheaply: `pending_count` and `OutboxCounts` are cardinalities, and
`pending_batch` (`src/store/mod.rs`) is a queue scan per render. The likely outcome is an
application-owned row-id-to-pending index, which is fine and should be recorded as the supported
pattern rather than rediscovered by each consumer. The cache side already has the shape it would
mirror: `InvalidationRunner::stale_classified` attributes conflicts per entity through a
caller-supplied classifier (`src/cache/runner/conflict.rs:73`).

**And the precondition-versus-batch tension from §4.** A per-record header on an N-record request
has nowhere to go, and the trial meets it the first time two queued todos both carry an `If-Match`.
Whether the answer is moving preconditions into the body, a batch of one, or a transport that splits
the batch is not settleable from here.

## 7. The Blocker That Already Exists

`wiki/references/repforge-eight-answers.reference.md` records it plainly: **neither repository has a
git remote**, and RepForge's layout decision specifies revision-pinned git dependencies rather than
path dependencies.

**It does not block D4a.** The example lives in this workspace as a member crate with a path
dependency on `frontbox`, exactly as `crates/frontbox-dioxus` already does. It blocks the
RepForge-side consumption, which is a different deliverable, and it is not a code problem.

## 8. What It Costs

Estimates, not a budget.

- **A Dioxus todo crate**, roughly 600-900 lines: CRUD components, an application-owned todo store,
  a `SyncTransport` over the platform's HTTP client, a `Sleeper` per platform, and the wiring of
  `use_frontbox`, `use_sync_loop`, and `use_invalidation`.
- **An axum/sqlx server**, roughly 300-500 lines: the sync endpoint, a read endpoint, an idempotency
  table written in the domain transaction, a deliberate rejection path, and a switch that holds a
  mutation at `Pending` so observation 6 is reachable.
- **Wiki work**: a D4a plan, a roadmap edit splitting D4, and whatever decisions the findings force.

Call it 1,000-1,500 lines, none of it in core, against the ~2,300 lines of backend port the
sequencing principle is protecting. **The estimate's weakest part is the application-owned read
model**, which §6 makes larger than a todo list sounds; if it dominates, that is itself evidence for
decision 023's `Revisit If`. D4b costs almost nothing by construction — one backend swapped at one
construction site and the observations re-run — and if it costs more, the trial has found something.

## Recommendation

**Split D4 into D4a and D4b.**

**D4a — the API-proving half. Buildable now.** A Dioxus CRUD todo application plus an axum/sqlx
server speaking the batch wire format, running against `InMemoryBackend`. This is D4 as the roadmap
already scopes it, and its job is to answer whether the extracted API expresses a real flow without
churn. Its findings feed D5, exactly as the roadmap says D4's should. It must not be described as
the offline demonstration, because against an in-memory store it is not one.

**D4b — the durability-proving half. After D5.** The same application, unchanged above the storage
seam, repointed at SQLite on desktop and IndexedDB on web. The demonstration is the absence of a
diff, plus observation 3: reload mid-queue and the work is still there.

Neither half reorders the roadmap. D4a is what D4 always meant; D4b is the deliverable that has been
implicit in "offline multiplatform at work" and was never separately named.

## Open Questions

- **Which platform D4a runs on first.** The `Sleeper` seam exists because the answer differs
  (`crates/frontbox-dioxus/src/sync/wait.rs`), but the demo must pick one, and "multiplatform" is only
  shown by running both. Desktop first makes observation 5 reachable in D4a; web first makes the
  absence of observation 3 more obvious, which may be the more useful embarrassment.
- **One service or several.** Decision 019's verdict synthesis and the open total-versus-partial
  ordering question appear only with more than one origin. One axum service is simpler and proves
  less.
- **Whether `POST /api/v1/sync` survives, and whether this trial can honestly say.** Decision 010's
  premise note makes D4 the deliverable that answers whether the batch envelope outlives RepForge's
  BFF. A todo server written by us answers in the easy direction, because it was written to fit the
  client. This is the one question the trial structurally cannot settle.
- **Whether the application-owned read model should ship as an example backend.** Decision 023's
  `Revisit If` anticipates exactly this, and D4a is the evidence it asks for.
- **Whether D4b's no-diff claim should be mechanically enforced**, by checking that the application
  crate's sources are identical across the two backend configurations. That turns the claim into a
  gate rather than an assertion, and it is cheap only if decided before D4a is written.
