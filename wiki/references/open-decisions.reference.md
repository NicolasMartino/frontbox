# Open Decisions Register

Document Class: Reference
Status: Active
Date: 2026-08-28 (section C answered 2026-08-29; D4a review reconciled 2026-08-30; entries 14-17 closed and 18 opened 2026-08-30; entry 20 opened 2026-08-30; entries 21 and 22 opened and closed 2026-08-31; entries 23 and 24 opened 2026-08-31 by the worktree audit; entry 25 opened 2026-09-02 by the review response)
Category: Architecture
Scope: Every decision this project has not yet made, why it is open, what each option costs, and when it stops being cheap. Section C was answered by RepForge on 2026-08-29 and is now closed except where noted; D4a opened the application and adapter follow-ups.
Sources: `wiki/index.md` (Open Work), `wiki/decisions/`, `src/`, `scripts/verify.sh`
Related: `wiki/references/repforge-section-c-answers.reference.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/020-observability-surface.decision.md`

## Index

Twenty-five entries, of which **nine are open and none is blocking**. The prose below explains how
each got here; this table is so a reader looking for what is *still* open does not have to scan
sixteen resolved ones to find them. Entries keep their number when they close, and the closed ones
are kept rather than deleted — the record of a question is what makes the answer legible later.

| # | Question | Status |
| --- | --- | --- |
| 6 | `tracing`: default-on or feature-gated | **Open** |
| 7 | Per-status retention bounds | **Open** |
| 8 | A cross-scope diagnostic | **Open** |
| 9 | Naming a checkable server-contract version | **Open** |
| 13 | Whether direct-dispatch classification gets a documented pattern | **Open** |
| 20 | Whether the drain loop may live on the renderer's task executor | **Open** |
| 23 | Whether a dead letter carries `last_error` and its bound row | **Open** — opened by the worktree audit |
| 24 | Whether `CacheVersionStore` gains a batched multi-entity read | **Open** — opened by the worktree audit |
| 25 | Whether a terminal-store listing can carry per-row read failures | **Open** — opened by the 2026-09-01 review |
| 1 | How a `ScopeKey` becomes a storage name | Closed — decision 024 |
| 2 | Quarantine: a separate table, or a status column | Closed — decision 025 |
| 3 | Last-error metadata on the record | Closed — decision 033 |
| 4 | Does core learn about service origins | Closed — decision 034 |
| 5 | `Serialize` on the report types | Closed — decision 030 |
| 10 | Whether the outbox carries replayable request preconditions | Closed — decision 026 |
| 11 | What frontbox contributes to the dead-letter report body | Answered |
| 12 | Whether a report names the mutations that drained | Closed — decision 035 |
| 14 | Whether `use_sync_loop` should take closures | Closed — built 2026-08-30 |
| 15 | Whether `frontbox-dioxus` ships a `WebClock` behind a `web` feature | Closed — built 2026-08-30 |
| 16 | Whether the sync loop wakes on `pageshow` after a bfcache restore | Closed — built 2026-08-30; **reopened and closed again 2026-09-01**, see decisions 040 and 041 |
| 17 | Where the single-flight flag lives | Closed — decision 031 |
| 18 | Whether the outbox partitions below scope | Closed — decision 036 |
| 19 | Whether hydration replaces the projection or merges into it | Closed — decision 032 |
| 21 | Whether a multi-service client partitions its outbox | Closed — decision 037 |
| 22 | Whether a backgrounded phone deserves the bfcache wake | Closed — decision 043; **desktop stays open**, see entry 20 |
| 22 | Where the inbound invalidation seam lives | Closed — decision 038 |

## How To Read This

**Updated 2026-08-30 after the D4a review.** Decisions 024 and 025 close entries 1 and 2 — the two
that a D5 plan could not be written without — entry 11 is answered, entry 10 is closed by decision
026, and **entry 5 is closed by decision 030**, forced by `DrainReport` arriving with the D3 drain
loop. The **D4a trial then opened four entries, 12 through 15**, which is what a migration trial is
for: they are the first questions raised by an application holding this API rather than by a
conformance case that already knows the answer.

The **Chrome DevTools background-services question then opened 16 and 17** on 2026-08-30, which is
the same pattern one step further out: the first questions raised by asking what the *platform* does
with this API rather than what an application does. ~~**Twelve remain open.** Eleven are
non-blocking; **entry 17 is not**~~ — **entry 17 closed the same day**, when decision 031 was built,
and closed on an option its own list did not contain. Reviewing that work opened **entry 18**, the
partition question, which is on the D5 clock but not blocking.

**Then the D3b clock fired.** Entries 14, 15 and 16 were taken together on 2026-08-30, as note 6
argued they should be. **Eight remain open — 6, 7, 8, 9, 13, 20, 23 and 24 — none of them
blocking.** D5's core lane closed four in one pass on 2026-08-30 (3, 4, 12 and 18, as decisions 033
to 036), because the outbox column set had to be final before a durable backend wrote a row into it.
What is left is publication-clock and no-clock work, plus entry 20's platform question and the two
the worktree audit opened: **the storage doors are shut.** Entry 19 closed the same day it opened:
decision 032, which supersedes 023 in part, settles the merge as a library-enforced rule rather than
per-application homework. The D3b column is empty for the first time since it was added, which is
what makes the next adapter change free again rather than a second breaking release.

**Entries 23 and 24 came from a whole-worktree review on 2026-08-31**, and both are the same
species: an API that is complete for its first caller and not quite for its second. Neither was
changed on the spot, because both are public additions and `AGENTS.md` puts those behind the same
go-ahead a new deliverable needs. The five defects that review found *were* fixed, since those are
maintenance.

**Entry 19 arrived 2026-08-30 from a bug report**, which is a first for this register: every earlier
entry came from a review, a decision, or a trial's own tests. This one came from someone reloading
the page and finding it empty. It is worth noting where it did *not* come from — the ten tests in
`todo-core` all passed, and both of the UI's gates were green.

**Entry 20 arrived 2026-08-30 from running the application somewhere nobody had run it**, which is a
second first: not a review, not a bug report, but a platform the library was never pointed at. It is
also the first entry whose subject is a defect in something this project does not own, and the only
one so far that a passing test suite could never have surfaced — every gate was green, on every
platform, while the queue silently stopped draining on one of them.

**Entries 21 and 22 arrived 2026-08-31 from a challenge to the project's own boundary** — a third
first, and the only entries so far opened by someone disputing a decision rather than by code. The
question was whether "SSE is out of scope" meant frontbox had built a cache that cannot be
invalidated. Both closed the same day, but the pattern is what to keep: it is the **second time a
boundary this project drew was found to have outlived its reason**, decision 023 being the first,
and both times the challenge came from outside rather than from a review. Decision 014's premise —
*"core does not gate refetches, because core does not perform them"* — was moved by decision 032 on
2026-08-30 and nobody re-examined the sentence.
Earlier that day:

** RepForge answered section C in full. Two questions are settled and citable,
one produced decisions they had not made, and one they cannot answer and neither can we. Section C
is rewritten below with the answers and the three things they open. **Eleven decisions are now
open**, not nine: their reply adds two, both needing an answer before D5.

Nine decisions were genuinely unmade when this was written. They are not equally urgent, and
urgency here is not about importance — it is about **which door closes first**. Four clocks are
running:

| Clock | Closes when | What it governs | Cost of being late |
| --- | --- | --- | --- |
| **D5** | The first durable row is written | Storage schema | A migration, on data that already exists on user devices |
| **Publication** | The crate is first published | Public API | A major version, for every consumer |
| **D4b** | The example is repointed at durable storage | Hydration semantics | A reload that silently drops the user's unsent writes from the screen |
| **D3b rework** | The adapter API is touched again | Adapter usability | Either migration churn for early users or leaving known-unusable hooks in place. **Fired 2026-08-30**: entries 14, 15 and 16 taken together, and the column is empty |
| **RepForge** | Their service design settles | Things only they can answer | The answer stops being free to change |

The crate is `0.0.0` with `publish = false`, and D5 has not started, so **every door is still open**.
That will not be true for long, and the D5 door is the one closing first because it is the next
deliverable anyone would authorize.

A further group has no clock at all and is listed last so it is not confused with the rest.

**One group below carries no letter.** The D3b rework clock arrived after the lettered sections were
written, and it belongs between §B and §C by urgency. It is not lettered `C` because "section C" is
how this project names RepForge's answered questions — in `wiki/log.md`, in
`references/repforge-section-c-answers.reference.md`, and in the correspondence itself — so
renumbering to make room would break the one label with readers outside this repository.

## Summary

| # | Decision | Clock | Recommendation |
| --- | --- | --- | --- |
| ~~1~~ | ~~How a `ScopeKey` becomes a storage name~~ | **Closed 2026-08-29** | Decision 024 — core states injectivity, backend picks, a case proves it |
| ~~2~~ | ~~Quarantine: separate table or status column~~ | **Closed 2026-08-29** | Decision 025 — backend picks, core requires four properties |
| ~~3~~ | ~~Last-error metadata on the record~~ | **Closed 2026-08-30** | Decision 033 — one bounded, opaque column, written in the same step as the attempt increment |
| ~~4~~ | ~~Does core learn about service origins~~ | **Closed 2026-08-30** | Decision 034 — no column, no core concept. The path and `OperationMeta` already carry it |
| ~~5~~ | ~~`Serialize` on the report types~~ | **Closed 2026-08-29** | Decision 030 — `Serialize` only; `Deserialize` has no caller |
| 6 | `tracing` default-on or feature-gated | Publication | Feature-gated, default on, mirroring `v4` |
| 7 | Per-status retention bounds | Publication | Defer to D4 evidence — **D4a produced none**; the trial's server answers `Pending` only under an explicit hold |
| 8 | A cross-scope diagnostic | None | Defer; it needs D5 to exist first |
| 9 | Naming a checkable server-contract version | None (D4) | Defer to D4 |
| ~~10~~ | ~~Whether the outbox carries replayable request preconditions~~ | **Closed 2026-08-30** | Decision 026 — a fourth durable field, and no header map |
| ~~11~~ | ~~What frontbox contributes to the dead-letter report body~~ | **Answered 2026-08-29** | `proposals/dead-letter-report-and-preconditions.proposal.md` |
| ~~**12**~~ | ~~Whether a report names the mutations that drained~~ | **Closed 2026-08-30** | Decision 035 — `drained: Vec<Drained>` with the disposition and the bound row. **From D4a** |
| **13** | Whether direct-dispatch classification gets a documented pattern | Publication | A pattern, probably not an API. **From D4a** |
| ~~**14**~~ | ~~Whether `use_sync_loop` should take closures instead of a `FrontboxHandle`~~ | **Closed 2026-08-30** | Closures. `use_sync_loop_on` was kept for handle-owning applications and removed 2026-09-01, having acquired none |
| ~~**15**~~ | ~~Whether `frontbox-dioxus` ships a `WebClock` behind a `web` feature~~ | **Closed 2026-08-30** | Yes, default off. The trial deleted its own five lines and its `js-sys` dependency |
| ~~**16**~~ | ~~Whether the sync loop wakes on `pageshow` after a bfcache restore~~ | **Closed 2026-08-30** | Yes, and on the *wait* rather than the loop: `Wake` plus `Sleeper::wakeable` |
| ~~**17**~~ | ~~Where the single-flight flag lives, now that it has to hold per scope~~ | **Closed 2026-08-30** | The scope, in core — a fourth option. Neither candidate listed could report `AlreadyRunning` |
| ~~**19**~~ | ~~Whether hydration replaces the projection or merges into it~~ | **Closed 2026-08-30** | Decision 032 — merge, enforced by the row store's skip rule; D4b builds it. **From the empty-list bug** |
| ~~**18**~~ | ~~Whether the outbox partitions below scope~~ | **Closed 2026-08-30** | Decision 036 — no. The key is `(seq)`; the remedy for head-of-line blocking is more scopes, not a partition column |
| **20** | Whether the drain loop may live on the renderer's task executor at all | D3b rework | **Open.** On Dioxus desktop the executor stops being polled and the queue stops draining. Web, iOS and Android are unaffected. **From the multi-platform trial** |
| ~~**21**~~ | ~~How a multi-service client routes, which 034 left open~~ | **Closed 2026-08-31** | Decision 037 — one outbox, one scope, routing inside `SyncTransport`. The join of 034 and 036. **D4d is planned to prove it by sabotage** |
| ~~**22**~~ | ~~Where the inbound invalidation seam lives~~ | **Closed 2026-08-31** | Decision 038 — the trial first, core against a written bar. **From the "a cache that can't be invalidated" challenge** |
| **23** | Whether a dead letter carries `last_error` and its bound row | Publication | **Open.** `from_record` drops both. For a `RetentionBound` letter, `last_error` is the only field that says what the record was up against. **From the worktree audit** |
| **24** | Whether `CacheVersionStore` gains a batched multi-entity read | D5 follow-up | **Open.** `mark_all_stale` and `reconcile_pairs` issue one `state()` per entity before one atomic write; on IndexedDB that is N transactions to prepare one. **From the worktree audit** |

Plus the items already decided but not built (§E), which are work rather than decisions and are
listed so they are not mistaken for it. Section C's four questions are answered and the section now
records what came back.

---

## A. The D5 Clock — Schema

Decide these before a durable backend writes its first row. After that, each one is a migration
against data sitting on user devices that may be offline for weeks.

### 1. How a `ScopeKey` becomes a storage name

**Blocking. No decision page exists.**

**Origin.** Decision 009's Consequences: *"Backends must derive physical storage identity
injectively."* The source sanitizes by character replacement (`persistence/native.rs:65`,
`persistence/web.rs:33`), which was safe for the UUIDs it was written for and is unsafe for
arbitrary keys — `tenant/1` and `tenant_1` both collapse to `tenant_1`, **silently merging two
scopes into one database**. Decision 009 states the obligation and deliberately does not pick a
mechanism.

**Why it is still open.** It only bites at D5. In memory a `ScopeKey` is a map key and needs no
encoding at all.

**Options.**

- **Reversible encoding** (percent-encoding, base32 of the raw bytes). Names stay recoverable and
  semi-readable. **Fails on length**: a caller-composed key is arbitrary, and a typical filesystem
  caps a path component at 255 bytes. `user:…@tenant:…@schema:1` percent-encoded can exceed it, and
  the failure arrives at `open()` on a real user's device.
- **Hash** (SHA-256 hex or similar). Fixed length, injective for practical purposes, immune to every
  character class. Names become opaque, so an operator looking at a directory of SQLite files cannot
  tell whose is whose.
- **Hash with a readable prefix** — a sanitized, truncated, *non-authoritative* prefix for humans
  plus the full hash for identity.

**The argument that decides it.** Reversibility looks valuable and is not needed: **decision 009
already stamps `scope` on every record** (`OutboxRecord::scope`), so the mapping from storage back
to scope is recoverable from any row's contents. Nothing needs to read it out of the file name. That
removes the only real advantage of reversible encoding and leaves length — where hashing wins
outright.

**Consequences of getting it wrong.** This is the worst failure mode in the crate: two users' data
in one store, discovered after the fact, with no way to separate them. It is also the one decision
here that a conformance case can prove — decision 009 already calls for one.

**Recommendation:** hash with a readable prefix.

### 2. Quarantine: a separate table, or a status column

**Blocking. No decision page.**

**Origin.** Decision 006 settled the *policy* — corrupt rows are quarantined, not dropped — and
explicitly left the shape: *"The exact store shape can be either a quarantine store or a status
column in the outbox backend. The important requirement is that malformed durable data cannot vanish
from reads while still occupying storage."* D1 built the `QuarantineStore` trait and an in-memory
implementation; no durable backend has had to choose.

**Why it is still open.** Deliberately deferred at D1, and nothing since has forced it.

**Options and what they actually cost.**

- **Status column.** The quarantine transition is a single-row `UPDATE` in the table the row already
  lives in. Decision 003 requires `apply_outcomes` to be atomic across outbox, dead letters, *and*
  quarantine — with a status column that atomicity is nearly free. Cost: every pending read carries
  a predicate and needs an index, and `pending_count` must never count quarantined rows.
- **Separate table.** Reads are naturally clean with no predicate. Cost: the transition is a
  cross-table `DELETE` + `INSERT`. Trivial in SQLite; in **IndexedDB it requires both object stores
  named in one transaction, declared upfront**, which is the fiddliest part of that backend and the
  easiest place to get atomicity subtly wrong.

**The constraint that applies either way.** `sweep_corrupt` moves rows *that failed to decode*. So
whichever shape wins, the row identifier must be readable independently of the payload — the id
cannot live inside the blob. That is already true, because ordering needs it.

**Recommendation:** status column. It makes decision 003's atomicity requirement trivial on the
backend where atomicity is hardest.

### 3. Last-error metadata on the record

**Origin.** Decision 017 adopted attempt counting and explicitly declined this: *"the attempt count
plus the anomaly report covers the diagnosis this decision needs, and a durable last-error field is
a separate question."* Decision 020 then promoted it, because under an observability requirement it
is **the only proposed field that survives a restart and explains why a record is still queued** —
everything in `SyncReport` describes one pass and dies with the process.

**Why it is still open.** 017 was right that it did not need it. 020 changed the premise.

**Consequences, including one nobody has raised.**

- It is a column, and three others are already queued for the same schema (§E). Deciding it now
  costs nothing; deciding it later costs a fourth migration.
- **It makes server error bodies durable on the client device.** A rejection payload can carry
  anything the server put in it, including personal data, and this would retain it locally until the
  record drains. That is a data-retention decision, not just a schema one, and it is the reason the
  field should not simply be "the last response body".

**Recommendation:** adopt it, but shaped by the transport rather than captured raw — the transport
decides what is worth recording, so the privacy call belongs to the application, consistent with
decision 019 making the transport responsible for judgment core will not make.

### 4. Does core learn about service origins

**Origin.** Decision 016's Revisit If: with four per-service origins, a strict global sequence
*"serializes a `billing` write behind a `workout` write that has nothing to do with it, and a stuck
record on one origin holds up every other."*

**Why the index's framing of this is wrong, and why that matters.** `wiki/index.md` files this as
"total order or partial order". It is not a sequence question. Decision 016 chose a *globally*
monotonic `seq` on the argument that global monotonicity gives per-scope monotonicity for free once
reads are scope-filtered — and **that argument transfers exactly to origins**: a global `seq`
filtered to one origin is still monotonic within it. The sequence design already serves both orders.

What a partial order actually needs is the ability to **partition the queue by origin**, and core
has no idea what an origin is. `OutboxRecord` carries `method` and `path`, and `path` is an opaque
string core never interprets.

**So the real decision is the shape, and it has two answers with very different clocks:**

- **A caller-supplied classifier**, exactly like decision 014's conflict classifier. Additive,
  free, and can be added any time — **no clock**.
- **A field on the record.** A schema column, which puts it on the **D5 clock** with the other three.

**Recommendation:** the classifier, deferred — but **say so now**. The whole point of listing this
is that if the answer were "a field", it would have to land with `seq`, `attempts`, and
`traceparent`, and discovering that after D5 is the expensive path.

---

### ~~17. Where the single-flight flag lives, now that it has to hold per scope~~

**Closed 2026-08-30**, the day it was opened, by building it. See
`wiki/decisions/031-cross-realm-single-flight.decision.md`.

**The answer was a fourth option, and the reason matters more than the answer.** The two candidates
below could not both keep this entry's promise of *no public signature changes* and the roadmap's
promise that the second drainer *observes `AlreadyRunning`*. `OutboxStore` has no channel to say
"busy elsewhere": `pending_batch` returns rows or an error, and an empty batch makes the runner
report `Idle` — false while records are pending. A backend cannot deliver `AlreadyRunning` unless
core gains a way to ask it, which is the signature change the entry was written to avoid.

**What was built.** The flag left `SyncRunner` for the *scope*, inside core:
`src/runner/exclusion.rs` keeps a thread-local set of `ScopeKey`s with a drain in flight, and
`sync_once` claims the scope and releases it on drop — cancellation included, which is the only
release path and therefore load-bearing. No public signature changed, no dependency arrived, and
case 61 asserts it. The obligation to extend exclusion *across realms* stays with the backends, in
prose on `OutboxStore`, because no process-local structure can see a second browser tab.

**What it cost, stated plainly.** Case 61 runs in one realm, so a durable backend will pass it
without holding a cross-realm lock. That is weaker than this entry imagined — it expected the case
to fail for a backend that had not done the work — and it is the price of keeping core's signature
still. The compensation is that the case is not vacuous on the backend that exists today: it fails
against the old runner-local guard, which was verified by reverting `sync_once` and watching the
second runner report `Completed`.

**Options as they stood, kept for the record.**

- **A per-scope flag inside the backend's own state.** Recommended here, and it is what the *durable*
  half still needs — but on its own it cannot report `AlreadyRunning`, which is what this entry
  missed.
- **Move the flag onto the `OutboxStore` trait** as an explicit claim/release pair. Honest and
  testable, and it names a mechanism in core and forces every backend to model a lock. Declined,
  as decisions 024 and 025 declined the equivalent.
- **Leave it on the runner and forbid two.** The status quo restated; fails on the two-tab case.

### 18. Whether the outbox partitions below scope

**Opened 2026-08-30 while reviewing decision 031.** The question asked was why there is not one
table, one runner and one drain *per cached type*, and it is worth recording because the answer is
not obvious and the alternative is not silly.

**What it would buy.** Head-of-line isolation, which is a real cost today: at `batch_limit = 1` a
wedged head freezes the whole scope's queue until decision 017's bound kills it — that is case 53,
and per-partition queues would contain the blast radius to one partition.

**Why the answer is currently no.**

- **A mutation has no type.** `MutationIntent` (`src/record/mod.rs:79`) is a replayable `method`,
  `path` and `body`. The *cache* half can key on `(scope, entity)` because a version is about one
  entity; a write is not, and one `POST` can touch several — which is why invalidation returns a
  *set* of stale entities. Core cannot derive a partition from method and path, so the caller would
  have to declare one.
- **It trades away cross-type ordering.** One queue per scope replays the caller's enqueue order
  across types for free. Split it and "create the list, then create the todo in it" can drain in
  either order. Decision 016 made `seq` the primary sort key precisely so order is a guarantee
  rather than an artifact; partitioning makes it an artifact again.
- **It does not dissolve decision 031, it re-keys it.** Two tabs both draining the same partition
  still collide. The exclusion key becomes `(scope, partition)` — same defect, more keys.
- **It multiplies the runtime.** N runners is N cadences, N in-flight claims, N retention budgets,
  and N interleaved batch streams at one server.

**Options.**

- **Nothing.** One queue per scope, ordering guaranteed across everything in it. Head-of-line
  blocking stays bounded by decision 017 and nothing else.
- **A caller-supplied partition key on the envelope**, defaulting to one partition per scope. The
  application declares which writes are independent; ordering is guaranteed *within* a partition;
  drains, head-of-line blocking and the retention budget divide along it; "per cached type" becomes
  one available choice of key rather than a taxonomy core invents. This is the shape worth arguing
  about if the answer ever changes.
- **Per-entity queues, derived by core.** Rejected on the first bullet above: there is nothing to
  derive them from.

**Why it is on the D5 clock.** A partition key is a durable column and a sort-key change, so it is
free before the first durable row and a device-data migration afterwards — the same clock decision
031 ran on. **Not blocking**, because "nothing" is a defensible answer and is what is built; but the
D5 plan should say which answer it is assuming rather than inherit one by silence.

## B. The Publication Clock — Public API

Free while the crate is `0.0.0` with `publish = false`. A major version afterwards.

### ~~5. `Serialize` on the report types~~ — closed 2026-08-29

**Settled by `wiki/decisions/030-serializable-reports.decision.md`**, on the recommendation below
plus one argument this entry did not have. What forced it was `DrainReport` arriving with the D3
drain loop: a new report type either joins this gap or closes it, and closing it later means
touching the same seven types twice. `DrainEnd` and `DrainReport` derive `Serialize` alongside the
five below.

The decision also sharpened why `Deserialize` is refused. This entry argues from
`#[non_exhaustive]`, which is true but secondary; the primary reason is that **nothing writes a
report**, so the reverse derive is public surface with no caller — and it would let someone build
a `SyncReport` describing a pass that never happened and hand it to code branching on
`is_stalled()`. The original text is kept below.

**Origin.** Decision 020. `SyncReport`, `SyncOutcomeCounts`, `SyncPass`, `Anomaly`, and
`AnomalyKind` are **the only public types in the crate that do not derive `Serialize`**.
`OutboxRecord`, `DeadLetterRecord`, `MutationId`, `OperationMeta`, `EntityState`, `CacheVersion` all
do — storage required it of them, and nothing required it of the diagnostics until observability
convergence did.

**The consequence that makes it worth doing.** The types a telemetry pipeline most wants to ship are
exactly the ones that cannot leave the process without being hand-mapped field by field, by every
caller, differently.

**The catch, which is specific.** `SyncPass` and `AnomalyKind` are `#[non_exhaustive]`. Deriving
`Deserialize` on them cannot round-trip a variant a future build adds — the same problem decision
012 solved by hand-writing `Serialize`/`Deserialize` for `MutationStatus`. Nothing needs to
deserialize a report *back into* the crate.

**Recommendation:** derive `Serialize` only. It widens an existing `serde` commitment rather than
making a new one, and the public-dependency note already tracks `serde` for every other type.
**Taken.**

### 12. Whether a report names the mutations that drained

**Opened 2026-08-29 by the D4a trial**, which is the first time anything held this API for real.
See `wiki/plans/d4a-offline-todo-trial.plan.md`, Finding 1.

**Origin.** A UI showing "saving…" per row needs to know *which rows have queued work*. The
application keeps an index for it, because `pending_count` and `OutboxCounts` are cardinalities
and `pending_batch` is a queue scan. The index then cannot be maintained: `SyncReport` and
`DrainReport` name mutation ids **only when something went wrong** — `UnknownMutation`,
`RepeatedVerdict`, `UnknownStatus`. Nothing names an id that was applied, deduplicated, or
dead-lettered, so there is no event to decrement on.

**Why nobody noticed before.** Every consumer until D4a was a conformance case, and a case knows
which ids it seeded. An application does not.

**Why it is not urgent.** The workaround is a full rebuild from `pending_batch` after each drain,
which is what the trial does. Once per drain is affordable; the shape is wrong for a queue of ten
thousand records on a phone, which is a real target rather than a hypothetical one.

**Options, and they differ enough to need arguing rather than picking.**

- **Ids on the report.** `SyncReport` gains `applied: Vec<MutationId>` and siblings, or one
  `Vec<(MutationId, Disposition)>`. Additive, since the type is `#[non_exhaustive]`. Cost: a
  report's size becomes proportional to the batch, and the aggregate `DrainReport` becomes
  proportional to the whole drain — which is exactly the growth decision 020 wanted aggregation
  to avoid.
- **A callback per outcome.** No allocation, and it inverts control into a caller the runner
  currently knows nothing about. It also runs mid-pass, before `apply_outcomes` commits, which
  makes "drained" a claim the callback cannot yet rely on.
- **A store query for pending ids only.** `OutboxStore::pending_ids()` alongside
  `pending_count`. Cheapest for the backend, keeps reports as they are, and does not answer
  *which drained* — only *which remain*, which is what the index actually wants. Probably the
  right answer, and it is a D5 schema question as much as a publication one.

**Recommendation:** decide before publication, prefer the third option, and take the evidence from
D4b — because a durable backend is where the cost of the rebuild first becomes measurable.

### 13. Whether direct-dispatch classification gets a documented pattern

**Opened 2026-08-29 by the D4a trial.** See `wiki/plans/d4a-offline-todo-trial.plan.md`,
Finding 2.

**Origin.** `wiki/proposals/extraction-boundary.proposal.md` keeps direct-dispatch-then-fallback
out of core on the bet that a caller-supplied `MutationId` is enough, and says to revisit if D4
cannot express a real flow that way. **The bet held.** One id covers the direct write and its
queued replay, and the server answers `Duplicate` if both arrive.

What the bet does not cover is deciding *whether* to fall back, which means deciding whether the
direct failure was terminal — `wiki/decisions/019-verdict-synthesis.decision.md`'s problem, in an
application. The trial's `classify` is fifteen lines and encodes 019's own rule, that `4xx` is not
automatically terminal because `401` and `429` are retryable.

**Why it matters despite being fifteen lines.** Every caller writes them, and writes them
differently. The ones who get it wrong queue a permanent refusal forever, or drop a write that
would have succeeded. Both failures are silent.

**Options.** A documented pattern in the crate docs; a `#[non_exhaustive]` helper enum in core
with no HTTP knowledge, which the caller maps onto; or nothing, on the grounds that the mapping is
transport-specific by construction and a helper would be a shape without content.

**Recommendation:** a documented pattern, not an API. Core has no HTTP vocabulary and decision 019
already refuses to acquire one; what is missing is that the rule 019 states for a transport is
nowhere stated for an application, and the fix for that is prose.

### 6. `tracing`: default-on or feature-gated

**Origin.** Decision 020 as amended, after RepForge's §5b.6 — *"emit `tracing` spans and events; let
the application choose the sink"* — was accepted. The boundary is settled; the packaging is not.

**Correction this rests on.** Decision 020 originally argued emission was *structurally blocked* by
the `!Send` gate and the no-date-library gate. Both were wrong and were checked before withdrawal:
`no_send_bound` greps `src/` for `Send` bounds and a `tracing::info!` adds none; `tracing` pulls
`pin-project-lite`, `tracing-core`, and `once_cell`, none of them a date crate.

**What is genuinely unknown:** wasm binary-size cost. **Nobody has measured it**, and it is
measurable rather than arguable.

**Recommendation:** a feature, default **on**, mirroring the existing `v4` precedent exactly.
Consumers who care about size opt out with `--no-default-features`, and `scripts/verify.sh` already
builds that configuration for wasm — so the off-path is gated against rot on day one rather than
discovered broken later.

### 7. Per-status retention bounds

**Origin.** Decision 017's Revisit If. The three retaining statuses have genuinely different
expected resolution times: `Pending` waits on a server job that will probably settle; `Unknown`
waits on a client rebuild that will not. One number either buries `Pending` work too early or leaves
`Unknown` work queued too long.

**Why it can wait.** It is configuration API, not schema — `with_retention_bound(n)` versus a
per-status map. And 017's reasoning holds: a single bound is the version that can be reasoned about,
and splitting it needs evidence about which way real traffic pulls.

**Recommendation:** defer to D4. Note that 017 ships **no default bound at all**, so this is a
refinement of an opt-in, not a change to what an unconfigured queue does.

---

## The D3b Rework Clock — Adapter Usability

Both entries here were opened by D4a's UI, and both are about `crates/frontbox-dioxus` rather than
core. The clock closes the next time the adapter's API is touched at all: while it is unpublished,
either fix is free, and after publication either one is a breaking change for a crate whose entire
audience is Dioxus applications.

Neither is blocking, and neither was fixed in D4a, because an API change to a built deliverable
needs the go-ahead `AGENTS.md` requires. They are recorded here so that the next authorized touch of
D3b starts from the evidence rather than from a fresh guess.

### ~~22. Whether a backgrounded phone application deserves the wake a restored tab gets~~

**Opened 2026-08-30 by D4c, closed 2026-09-01 by decision 043.** Recorded here late:
`examples/todo-app/src/platform/mod.rs`
cited this register for the question from the day it was asked, and the entry did not exist — the
record lived only in `wiki/plans/d4c-multi-platform-trial.plan.md`. Written down and closed in the
same edit rather than left as a dangling citation.

**The question.** A page in the back/forward cache is frozen and gets `use_bfcache_wake` for it. An
application the OS backgrounds stops being scheduled in the same way, and D4c ran both phones on
emulators where that never happens, so the trial had no evidence either way and `focused()` returned
`true` on every native target.

**The answer is yes, and it is the same mechanism.** `Event::Suspended`/`Resumed` on tao are
`applicationWillResignActive`/`DidBecomeActive` on iOS and `onPause`/`onResume` on Android, and one
`use_wry_event_handler` observes them alongside `WindowEvent::Focused` for desktop. Verified against
real OS transitions on both emulators: pause on backgrounding, and a poll **immediately** on return
rather than at the next cadence tick. See
`wiki/decisions/043-in-front-on-every-platform.decision.md`.

**What stayed open is desktop, for an unrelated reason.** The event exists and `dioxus-desktop`
filters it out before any handler sees it — 350 raw events across two resizes and three focus
changes contained no `WindowEvent` at all — and `use_window().is_focused()` answers `false` while
the window is focused. 043 has the measurements and the route that would work
(`Config::with_custom_event_handler`). It is parked behind entry 20 above: there is little point
waking a loop the desktop executor has already stopped polling.

### 20. Whether the drain loop may live on the renderer's task executor at all

**Opened 2026-08-30 by the multi-platform trial, and it is the sharpest question the adapter has
been asked.** `use_sync_loop` runs the drain inside `use_future`, which puts it on the
VirtualDom's task executor — the same executor that renders. On three of the four platforms that is
fine. **On Dioxus desktop the executor stops being polled after a few seconds and the queue never
drains again.**

**Measured, not inferred.** In one process, over 95 seconds, with the drain cadence at five seconds:

| Task | Where it runs | Ticks observed | Expected |
| --- | --- | --- | --- |
| The sync loop | Dioxus `use_future` | **3** | ~19 |
| A second `use_future` touching no signal | Dioxus `use_future` | **2** | ~19 |
| A bare `tokio::spawn` loop | The tokio runtime | **18** | ~19 |

The second row is what makes this the executor rather than the loop: a task that writes no signal
and triggers no render stalls the same way. The third row is what rules out the timer, the runtime
and OS-level throttling — the same process kept a tokio timer firing on schedule throughout.
Reproduced under both a bare `cargo run` and `dx serve --platform desktop`, and with the window
both occluded and frontmost.

**The likely mechanism**, from `dioxus-desktop-0.7.10/src/webview.rs:562`: `poll_vdom` returns
early — *without polling the VirtualDom* — whenever `poll_edits_flushed` is pending. If the webview
stops acknowledging an edit flush, the VirtualDom's futures stop being polled with it. Stated as a
reading of their source rather than as a diagnosis, because it was not instrumented.

**Why it matters more here than it would for most libraries.** An offline-first queue that stops
draining is the failure the queue exists to prevent, and it fails *silently*: the UI keeps working,
writes keep landing in SQLite, and nothing reaches the server. It is also the exact case
`SyncPass::AlreadyRunning` and decision 031 were careful about — except no claim is held and no
error is raised, so nothing reports it.

**Why the obvious workaround is closed to us**, and this is the part worth recording: the tokio
task in row three *could* do the draining, and cannot, because `tokio::spawn` demands `Send` and
decision 001 makes every store `!Send` on purpose. **The posture chosen for wasm is what removes
the desktop escape hatch.** That is not an argument against decision 001 — an IndexedDB future
cannot be `Send`, so the alternative was never available — but it is the first time the cost has
been visible, and it should be recorded as a cost rather than rediscovered.

Options, none taken:

- **Leave it, and document that desktop needs the host to poll.** Honest and cheap. It leaves the
  adapter's headline feature broken on a platform Dioxus supports.
- **A desktop-specific loop driven off a `LocalSet` the application owns.** Keeps `!Send`. Needs
  somebody to drive the `LocalSet`, which is the same problem one level down.
- **Report it upstream and pin a working version.** Probably right, and orthogonal to the other two.

Clock: **D3b rework**, because any of these touches the adapter's surface. Not blocking: web, iOS
and Android all drain correctly, and those are the three platforms with a stated consumer.

**Re-measured 2026-09-01, and the reading is: intermittent.** A run that morning showed **19 sync
ticks and 19 invalidation ticks over 95 seconds** — the full expected rate, on a build carrying a new
500 ms UI clock. That looked like the clock keeping the VirtualDom flushing past `poll_edits_flushed`,
which is exactly the mechanism this entry suspects, and it was written up that way.

**That reading did not survive the afternoon.** Three later runs on the *same* UI clock stalled after
**2 ticks** and never resumed — frozen for minutes, unrevived by clicking or activating the window.
Twice with a new `use_wry_event_handler` registered and once with it deliberately not registered, so
the handler is not the cause either; that A/B was run precisely because the timing was suspicious.

So the honest correction is that **entry 20 is not deterministic**, the morning's 19/19 was luck
rather than a fix, and a single healthy 95-second window is not evidence that this is closed. Any
future claim that it is fixed needs repeated runs, not one.

### ~~14, 15 and 16 — the D3b batch~~

**All three closed 2026-08-30**, taken as one change exactly as sequencing note 6 argued: one crate,
one audience, and three breaking releases where one would do. `wiki/log.md` carries the full entry;
the three questions as they stood are below the summary, because what each one *cost* is the part
worth keeping.

**14 — the hook takes closures.** `use_sync_loop(sync, counts, cadence, sleeper)`, where `SyncStep`
is "run one sync and tell me how it ended" and `CountsStep` is "read the counts". Both are
`Rc`-boxed and type-erased, following `Sleeper`'s own precedent. `use_sync_loop_on(handle, ..)`
kept the old signature's behaviour whole, for an application that does let Dioxus own its runner,
and `FrontboxHandle` was left unchanged.

**Both were removed on 2026-09-01.** The class of application they were kept for never acquired a
member, and the trap described above stayed available the whole time — which is the argument the
"leave it" option lost on, arriving a day late.

**The evidence is the trial's diff, not the hook's signature.** `examples/todo-app`'s hand-rolled
loop is gone: `TodoApp::sync` — `drain` *plus* `refresh_pending` — goes into the sync closure whole,
and the dead-letter poll and the revision bump ride in the counts closure. What is left in the
application is what only the application knows. That is the test this entry was really asking for,
since "a hook that compiles proves only that its signature is well-formed" was the entry's own
complaint.

**15 — `WebClock` behind a `web` feature**, default off, so a Dioxus desktop build takes no `js-sys`
and no `web-sys`. The feature-less adapter gates in `scripts/verify.sh` are what prove that, and two
new gates compile the feature for wasm. `examples/todo-app` deleted its own five lines and its
direct `js-sys` dependency, which is the entry's "every browser consumer writes the same five"
stated as a diff. Core's dangling sentence now names what supplies the clock — in prose rather than
as an intra-doc link, because core does not depend on the adapter.

**16 — the wake sits on the wait, not on the loop.** `Wake` is a cloneable latch and
`Sleeper::wakeable(wake)` races it against the sleep, so `use_sync_loop`'s signature did not have to
change twice in one release. `use_bfcache_wake()` behind the `web` feature registers the `pageshow`
listener, fires when `persisted` is set, and removes the listener when its component is dropped.
`Wake` is public *without* the feature, which answers this entry's second option for free: an
application on a platform this crate does not cover drives the same seam from its own listener.

Two details worth recording. A wake that fires while nothing is waiting is **latched** rather than
dropped — a wake arriving mid-drain that silently did nothing would be this entry's own defect one
level down. And the race is hand-written: this crate has no `futures` dependency, and taking one to
poll two futures, one of them already a `Pin<Box<_>>`, would be a dependency per line of code saved.

---

The three entries as they stood, kept because the arguments are the record:

### ~~14. Whether `use_sync_loop` should take closures instead of a `FrontboxHandle`~~

**Opened 2026-08-29 by D4a's UI.** See `wiki/plans/d4a-offline-todo-trial.plan.md`,
`## The UI, And What It Cost To Wire`.

**Origin.** `frontbox-dioxus` offers two hooks and **the only application in this repository could
use neither.** Both failures share one root, and either alone is fatal.

- `use_frontbox` takes the runner **by value** — `init: impl FnOnce() -> SyncRunner<S, T>`
  — because the handle had to own the one `Rc` every
  consumer shares. `TodoApp` holds its runner privately and hands out `store`, `transport`,
  `outbox`, `sync`, `refresh_pending`, `refresh_from_server` and `dead_letters`. There is nothing to
  hand over, and adding a `runner()` accessor would be an API change to the *application*
  demonstrating the problem.
- `use_sync_loop` calls `handle.runner().drain()` (`crates/frontbox-dioxus/src/sync/mod.rs`). But
  `TodoApp::sync()` is `drain()` **followed by** `refresh_pending()`, and the second half is
  load-bearing: it rebuilds the row-to-pending index `TodoStore::is_saving` answers from, which
  cannot be maintained incrementally for entry 12's reason. Route the loop through the hook and
  every row's "saving…" marker is set on enqueue and never cleared — correct on first paint,
  permanently wrong after the first drain.

**The near-miss that compiles, recorded because someone will write it.** `InMemoryStore` is `Clone`
and its clones share one backend (`src/memory/mod.rs:113`), so
`use_frontbox(|| SyncRunner::new(app.outbox().clone(), HttpTransport::new(BASE_URL)))` type-checks.
It is wrong in the exact way this crate exists to prevent: the in-flight flag is a `Cell` on the
*runner* (`src/runner/mod.rs:46`), not on the store, so two drain loops share one queue and each
believes itself alone. `SyncPass::AlreadyRunning` becomes unreachable — the guard is still there,
guarding the wrong thing.

**Why nobody noticed before.** D3b was gated by compiling for two targets. There was no application,
and a hook that compiles proves only that its signature is well-formed.

**Options.**

- **Take closures.** The hook needs exactly two capabilities: "run one sync, and tell me how it
  ended" and "read the counts". Both are closures. `FrontboxHandle` then survives as a
  `use_sync_loop_on(handle, ..)` convenience for applications with no wrapper of their own. Cost:
  two entry points instead of one.
- **Add `FrontboxHandle::from_rc` or a `runner()` accessor.** Smaller diff, and it hands out the
  runner to anyone — which re-opens the two-runners-one-queue mistake above as a supported path.
- **Leave it.** Defensible only if the hook is honestly framed as being for applications that let
  Dioxus own the runner, which the current docs do not say and the only real consumer is not.

**Recommendation:** take closures. It is the smallest change that makes the hook usable by an
application shaped like every application — one that owns its runner and wraps `drain` in something
larger — and it is free while the crate is `publish = false`.

### ~~15. Whether `frontbox-dioxus` ships a `WebClock` behind a `web` feature~~

**Opened 2026-08-29 by D4a's UI.**

**Origin.** `SystemClock` is `#[cfg(not(target_arch = "wasm32"))]` (`src/clock.rs:36`) and its own
doc says web callers "supply a clock over `Date.now()` through their adapter, which keeps the
JavaScript glue out of core". **`frontbox-dioxus` is that adapter and supplies none**; its module
doc says instead that "time enters through the caller"
(`crates/frontbox-dioxus/src/lib.rs:21`). Core points at the adapter, the adapter points back at the
caller, and the caller writes five lines and takes a `js-sys` dependency — as `examples/todo-app`
does. Every browser consumer writes the same five.

**Why it is not urgent.** Five lines, no correctness trap, and a wrong clock in this position
affects `created_at` — recorded, and explicitly not trusted for ordering since decision 016 made
`seq` the sort key.

**Options.**

- **Ship it behind a `web` feature**, default off, so a desktop Dioxus build takes no `js-sys`. The
  adapter already owns the other browser seam (`Sleeper`), which is the precedent.
- **Ship it unconditionally.** Simpler, and wrong for Dioxus desktop, which is a supported target of
  the framework this crate is named after.
- **Document the five lines instead.** Free, and leaves core's "through their adapter" sentence
  pointing at something that does not exist.

**Recommendation:** ship it behind a `web` feature, and take it in the same touch as entry 14 rather
than as its own release. Whichever way it goes, `src/clock.rs`'s sentence should name what actually
supplies the clock.

### ~~16. Whether the sync loop wakes on `pageshow` after a bfcache restore~~

**Opened 2026-08-30 by the Chrome DevTools background-services question.** See
`wiki/proposals/browser-background-services.proposal.md`, §5.

**Origin.** A page in the browser's back/forward cache is **frozen**: its timers do not fire. The
`Sleeper` future backing `SyncCadence` (`crates/frontbox-dioxus/src/sync/mod.rs`) therefore stalls for
the whole time the page sits in the cache, and on restore the loop resumes as though its wait had
been continuous — while `Date.now()`, which is what a browser `Clock` reads, has jumped forward by
minutes. A user who navigates away and comes back gets a client that believes it is mid-sleep and
will not drain until a wait chosen for a live page finishes.

**Why it is small.** One event listener and one wake-up. `pageshow` with `event.persisted` true is
the documented restore signal, and the loop already has a "drain now" path in its first-iteration
`first_ms: 0` behaviour.

**Why it is not zero.** The adapter takes no `web_sys` dependency today; `Sleeper` exists precisely
so the browser seam stays with the caller. A `pageshow` listener is a *second* browser seam, and it
is the one that decides whether the adapter's `web` feature is a convenience (entry 15) or the point
of the crate.

**Options.**

- **Wake in the adapter, behind the same `web` feature entry 15 proposes.** One place, every Dioxus
  web consumer benefits, and it makes the `web` feature carry two things rather than five lines.
- **Expose a `wake` handle** the application drives from its own listener. No new dependency; every
  consumer writes the listener, which is entry 15's complaint one level up.
- **Leave it.** The cost is bounded — a late drain, never a wrong one — which is why this is not
  blocking.

**Recommendation:** wake in the adapter, and **take it with entries 14 and 15 as one change**. All
three are the same crate, the same audience, and the same clock; sequencing note 6 already argues
that they should not become three breaking releases.

**What the built version missed, found 2026-09-01.** This entry reasoned about *one* loop waiting on
the wake, and the shape it produced could only serve one: `Wake` held a single `Cell<bool>` and a
single `Option<Waker>`. The trial has **two** consumers — the drain loop and the invalidation poll —
and one fire released exactly one of them, measured at 5 of 6 fires with the winner alternating. The
losing loop served out its full interval, which for the poll was ten minutes.

Nothing above is wrong; it is incomplete in a way that only shows up with a second consumer, and the
application had written comments asserting the broadcast behaviour it was not getting.
`wiki/decisions/040-one-wake-releases-every-waiter.decision.md` makes the wake a broadcast and the
latch per-waiter; `wiki/decisions/041-the-poll-stops-when-nobody-is-looking.decision.md` records the
policy — `focus` as well as `visibilitychange`, and a real pause rather than a ten-minute interval —
that the working wake made expressible.

## The D4b Clock — Hydration Semantics

### ~~19. Whether hydration replaces the projection or merges into it~~ — closed 2026-08-30

**Settled by decision 032, hours after opening.** The answer is the recommendation below — merge —
but not in the shape this entry imagined: 032 moves the rows *into* frontbox as opaque blobs, and
the merge becomes a rule the library enforces (a hydration write skips rows with pending bound
mutations) rather than a discipline every application rediscovers. The widening of `row_id_of` this
entry priced as the merge's cost is retired outright — mutations bind to rows at enqueue instead.
D4b builds and exercises it. The text below is kept as the record of the question as asked.

**Opened 2026-08-30 by the empty-list bug.** See `wiki/plans/d4a-offline-todo-trial.plan.md`,
`## Finding 6`.

**Origin.** The UI never read from the server at all, and the fix — `TodoApp::start`, which
hydrates and then rebuilds the pending index — exposed the question the missing call had been
hiding. [`TodoStore::replace`](../../examples/todo-core/src/store.rs) clears the projection and
reinserts the server's rows. That is correct **only because the outbox is empty at mount**, which is
true of an in-memory queue and stops being true the moment D5 lands.

**What goes wrong at D4b.** A durable queue survives the reload that clears the projection. So
startup finds unsent writes already queued, hydrates from a server that has not seen them, and
`replace` drops their optimistic rows. The user reloads and watches their own unsent work disappear
— then reappear seconds later when the drain lands. Nothing is *lost*: the mutations are in the
outbox and they do send. But the screen lies for the width of one drain, and it lies about exactly
the property the trial exists to demonstrate.

**Why it is not urgent yet.** It cannot be reached on an in-memory backend, because there is never
anything queued at mount to lose. It becomes reachable the same day D4b repoints at SQLite and
IndexedDB, which is why the clock is D4b rather than D5 — D5 writes the durable row, D4b is where a
reload first reads one.

**Options.**

- **Merge.** Hydrate into a scratch projection, then re-apply everything still queued on top of it,
  which is what `refresh_pending` already walks the queue for. Costs a second pass over the pending
  batch at startup, and needs `row_id_of` to yield not just the row but the change to replay —
  which is more than it currently recovers.
- **Hydrate only when the queue is empty.** Trivial, and wrong in the case that matters: a client
  that reloads with work queued would then never pick up anyone else's changes until it drained.
- **Replace, and accept the flicker.** Defensible if drains were instant. They are not, and an
  offline client's queue does not drain at all — so "the width of one drain" is unbounded, and the
  user sees the server's stale list for as long as they stay offline.

**Recommendation:** merge, and note that this is decision 023 arriving in a new place. The read
model is the application's, so reconstructing it over a queue is the application's problem too —
`frontbox` stores the queue and takes no position. What D4b owes is the *demonstration* that an
application can do it, since that is the half of decision 023 nothing has exercised.

## C. RepForge's Clock — Answered 2026-08-29

All four came back. Full text at `wiki/references/repforge-section-c-answers.reference.md`.

| Question | Answer |
| --- | --- |
| Does the XOR accumulator seed at zero? | **Yes** — BLAKE3, 256-bit XOR, empty set all zeroes, asserted by a property test on native and wasm. **Case 44's scenario is live** and decision 021 does not simplify |
| Structured terminality signal? | **Yes, added in response.** A required `retry` field of `terminal` / `transient` / `conflict`, with the status-line table kept as a documented fallback for errors that never reach a service |
| Convergence specification | Collector, target, and sink boundary were already settled. `mutation_id` → **span attribute** and **clock authority** were decided in response. Shared vocabulary is **blocked on kafkaman**, who has not replied |
| Production queue depths | **Cannot supply — there is no production.** They asked us the same question. Neither party has the number |

### ~~21. Whether a multi-service client partitions its outbox~~ — closed 2026-08-31

**Settled by decision 037, the day it was opened**, by the invalidation-delivery proposal noticing
that decisions 034 and 036 had never been read together. The answer is one outbox under one scope,
with routing inside `SyncTransport`: 034 already said a service is a transport concern, and 036
already paid for cross-service ordering by refusing to partition. Neither page had stated the join.

**What the entry was actually asking.** Whether a client whose writes span several backend services
needs a durable `origin` column, a queue per service, or anything in core at all. The answer is
none of the three — and the batch mechanic needed no new protocol either, because a routing
transport that cannot reach one service may return the verdicts it obtained and stay silent about
the rest, which core already retains with `reason: Some("no verdict returned")`.

**What it cost to close.** The guarantee is still untested: every test in this repository sends to
one server, so decision 036's ordering claim has no evidence beyond nothing having contradicted it.
D4d is the deliverable that makes it falsifiable, and its plan requires the sabotage half — the
same flow under one scope per service — because the good path alone proves a drain worked rather
than that order held.

### ~~22. Where the inbound invalidation seam lives while it is being designed~~ — closed 2026-08-31

**Settled by decision 038, the day it was opened.** Built in `examples/todo-core` first, with the
promotion bar written down now so promotion is later measured against it rather than re-argued.

**What the entry was actually asking.** Whether frontbox owes an `InvalidationSource` trait, a
polling loop, a resume cursor, or a server contract for a versions endpoint — the standing answer
having been "SSE is out of scope", which defended a boundary nobody disputed while leaving the real
asymmetry unexplained: the outbound path has a wire contract, a trait, a driver and a cadence
policy, and the inbound path has a method you can call.

**Why closing it meant building nothing.** `use_invalidation`, `InvalidationState` and
`InvalidationEvent` appear nowhere outside the crates that define them — the runtime has had zero
consumers, ever. Designing the seam in the abstract would be the one place this project guessed at
an API with no application holding it, and a seam with one implementation is a wrapper. The four
promotion criteria are on decision 038; D4d is expected to satisfy some and not all, which is the
normal outcome rather than a failure.

### 23. Whether a dead letter carries `last_error` and its bound row

**Opened 2026-08-31 by the worktree audit.**

**Origin.** `DeadLetterRecord::from_record` (`src/record/terminal.rs`) rebuilds the record from the
outbox row and carries `op`, `traceparent`, `precondition` and `attempts` across the transition. It
drops two fields that decisions added later: `last_error` (033) and `row` (032).

**Why it matters, and why it is not obviously a bug.** For a
[`RetentionBound`](../decisions/017-bounded-retention.decision.md) dead letter, `last_error` is the
single most diagnostic field there is. The letter says *the client gave up after N attempts* and
`attempts` says how many; `last_error` is the only thing that says **what it was up against** — and
that is the whole reason 033 exists. A `Rejected` letter does not need it, because
`DeadLetterReason::Rejected` already carries the server's payload. So the field is redundant for
one of the three reasons and load-bearing for another, which is exactly the shape that gets a field
quietly dropped.

`row` is a weaker case in the same direction: an application clearing a "saving…" marker on a dead
letter has to know which row, and `DrainReport::drained` already carries it — so the information is
reachable at drain time and lost by the time anyone reads the dead-letter list.

**Options.**

1. **Carry both onto `DeadLetterRecord`.** Two new public fields on a `#[non_exhaustive]` struct, so
   additive. Both backends already store them on the outbox row and would write two more columns.
2. **Carry `last_error` only.** The stronger half, and it avoids duplicating a binding that
   `drained` reports at the moment it is actionable.
3. **Neither, and say so on the page.** Defensible if the position is that a dead letter is a
   *record of a mutation*, not a record of a sync attempt — but then decision 033's page should say
   the field dies at the transition, rather than leaving a reader to find out from the code.

**Recommendation.** Option 2, plus a line on decision 033. It is additive, it is the field that
answers the question a human actually asks of a dead letter, and it costs one column per backend.

**Clock: Publication.** Adding a public field after the crate ships is additive and fine; the cost
of being late is a durable schema without the column, which is a migration on user devices.

### 24. Whether `CacheVersionStore` gains a batched multi-entity read

**Opened 2026-08-31 by the worktree audit.**

**Origin.** `InvalidationRunner::mark_all_stale` and `reconcile_pairs`
(`src/cache/runner/mod.rs`) call `state()` once per registered entity, then write once. On the
in-memory backend that is a borrow each. On SQLite it is a query each. **On IndexedDB it is a
transaction each** — N round trips through the browser's event loop to prepare one atomic write.

**Why it has not bitten.** A registry names entity *types*, and the applications here have one and
six. The cost is real and the multiplier is small, which is why this is a register entry rather
than a defect.

**Options.**

1. **Add `states(&self, entities: &[&str]) -> Result<Vec<(String, EntityState)>, Error>` with a
   default implementation** that loops `state()`. Backends that can do better override it; nothing
   breaks. Additive to the trait, so it needs the same go-ahead as any public addition.
2. **Reuse `all_states()`**, which already exists and returns everything. Free today, and wrong at
   the size where option 1 starts mattering — reading every entity to look at six is the opposite
   trade.
3. **Nothing.** Correct until a caller registers enough entities to notice.

**Recommendation.** Option 1 when a second application exists to size it — the same rule decision
038 applies to the invalidation seam, and for the same reason: a default no consumer has asked for
is a guess.

**Clock: D5 follow-up.** Not a schema question, so it is cheap for as long as the trait is
unpublished.

### 25. Whether a terminal-store listing can carry per-row read failures

**Opened 2026-09-02 by the response to the 2026-09-01 worktree review.**

**Origin.** That review found `frontbox-indexeddb`'s `all_in_scope` dropping rows that would not
decode. For the **outbox** it was a genuine defect and is fixed: such a row was invisible to
`sweep_corrupt` as well as to reads, so nothing could ever see or delete it. For the **dead-letter
and quarantine** stores the drop remains, and it is a trait shape rather than an oversight.

`DeadLetterStore::list` returns `Vec<DeadLetterRecord>`. A row that does not match its type can
therefore be dropped or it can fail the whole call, and there is no third answer the signature can
express. Failing the call is worse in the direction that matters: that listing is what a person
reads to find out what the server refused, so one unreadable entry would hide every readable one
beside it. Sweeping is not available either — a terminal store is where rows go to stop, and
quarantining a quarantine entry names nothing new.

**Why it has not bitten.** Only this adapter writes those stores, and only at one schema version.
The failure needs an older build's rows, a partially-completed migration, or another writer in the
same origin — none of which this project's trials produce.

**Options.**

1. **`Vec<Result<DeadLetterRecord, ReadFailure>>`.** Exact, and it makes every caller handle the
   case at the point where the row is read. It also changes the shape of the most-read method on two
   traits for a condition that is rare by construction.
2. **A companion report, as `PendingIndexGap` accompanies the pending index.** The precedent is
   already here and the posture is the project's: report the shortfall, never raise it, and leave
   the listing usable. Costs a second return value or a second call.
3. **Nothing, recorded.** What is built today: the behaviour is stated per store in
   `wiki/compatibility/indexeddb-adapter.compat.md`, so it is a known limit rather than a surprise.

**Recommendation.** Option 2 if a backend appears whose stores another writer can touch, which is
the condition that makes this reachable at all. Option 1 buys exactness that nothing has asked for.

**Clock: cheap while the traits are unpublished, and this is the last moment it is cheap.** Both are
public trait methods, so the change stops being free the day something outside this repository
implements them.

### What this project owes back

**On the zero-versus-never-computed follow-on, their framing is wrong in our favour.** They write
that it *"touches your side as much as ours"*. It does not: **frontbox settled it on 2026-08-28 and
shipped it.** `EntityState::version` is `Option<CacheVersion>`, so absence is `None` and is
representable separately from every value including a 32-zero-byte hash, and conformance case 44
asserts that a client which has never synced still fetches a collection the server reports as empty.
Their own suggested remedy — *"a nullable stored value rather than a sentinel, since zero is
genuinely taken"* — is exactly what was built.

What remains is theirs alone and no client representation can reach it: `SetHash` derives `Default`,
so a server that has **not computed** an accumulator reports the same 32 zero bytes as one whose
collection is genuinely empty. If the server says zero, a client must believe it. The fix is
server-side initialization, not protocol.

**On the ~17 min figure, they misread it and the register's phrasing is why.** RepForge infers that
seventeen minutes at 100 ms RTT implies ~10,000 queued mutations, fifty times their stated case.
The figure is not RTT-bound. Decision 018 derives it from the source's poll interval —
`SYNC_INTERVAL_MS = 5000` — so 200 records at one per pass is 200 x 5 s = 1000 s. **Both numbers are
correct and they measure different things:** ~20 s is the floor a back-to-back drain would reach,
~17 min is what the source's loop actually delivers, and the gap between them is the entire value of
D3's drain-until-idle loop. The register's earlier "~20 s versus ~17 min" phrasing presented them as
competing estimates, which they never were. Corrected here and in decision 018.

### 10. Whether the outbox carries replayable request preconditions

**New, from their C2. On the D5 clock.**

`conflict` is defined as *"`If-Match` failed against the current row hash"* — so the write protocol
sends a precondition header. **`MutationIntent` has no field for one.** Its five wire keys are
`mutation_id`, `method`, `path`, `client_datetime`, `body`, plus the optional `op`; there is no
header map and no precondition concept anywhere in `src/`.

**Why it may be real.** An `If-Match` is only meaningful if it carries the version the user was
looking at when they enqueued. Computed at drain from current local state it is vacuous — it would
match whatever the client now holds and detect nothing. So it is the same shape as decision 022's
`traceparent`: a caller-supplied value captured at enqueue, stored durably, replayed as a header at
drain, never interpreted by core. That is now **two** instances of that shape, which is the point at
which the general case is worth considering rather than a third bespoke column.

**Why it may not be ours.** The transport is application-owned. An application could hold the
intended base hash in its own read model, keyed by `mutation_id`, and attach the header at drain.
That works — at the cost of maintaining a parallel per-mutation structure alongside the outbox,
which is what the outbox exists to be.

**They differ from `traceparent` in one way that matters:** losing trace context degrades
diagnostics, losing a precondition **silently disables conflict detection**. That argues against
folding both into one optional header bag.

**Recommendation:** ask RepForge how their client attaches `If-Match` today before designing
anything. If it comes from the outbox, this is a column and belongs with `seq`, `attempts`, and
`traceparent`. If it comes from the read model, record that as the supported pattern and close this.

### 11. What frontbox contributes to the dead-letter report body

**New, from their C3. On RepForge's clock, and the window is open now.**

Only the endpoint path exists — `POST /api/v1/{service}/telemetry/dead-letters`. They invite input
and make this project's own argument back at it: *"now is free and later is not — which is the same
argument your register makes about your own D5 columns."*

frontbox knows exactly what it holds, so this is cheap to answer. `DeadLetterRecord` carries
`mutation_id`, `method`, `path`, `body`, `created_at`, `op`, `scope`, `rejected_at`, and
`error: Option<RemoteRejection>`. Three notes worth sending with it:

- **`error` being `None` is information, not a gap.** Decision 017 makes `Some` mean the server
  refused and `None` mean the client gave up at its retention bound. Their `occurred_at` /
  `received_at` split pairs with `created_at` / `rejected_at` the same way.
- **`scope` must not be sent.** It is a caller-composed local identity that may contain a tenant and
  a user; decision 009 makes it storage identity, not a wire field.
- **`attempts` and `traceparent` are decided and unbuilt** (016/017/022). If the report body wants
  them — and it should — that is one more reason the columns land together.

**Recommendation:** answer it, this week, at the field level. It costs an afternoon and it is the
cheapest item on this register.

## D. No Clock

Real, and nothing forces them.

### 8. A cross-scope diagnostic

**Origin.** Decision 009 rule 4: work retained under a scope no store currently opens is unreachable
from every other store, so D1's no-progress signal cannot see it. 009 calls this *"the accepted cost
of not destroying offline writes on user switch"* and notes a diagnostic *"may be needed at D5"*.

**Why it waits.** It cannot be built before D5 — enumerating scopes means enumerating what a durable
backend holds, and there is no durable backend. It also depends on §A.1: if scope names are hashed,
enumerating stores does not by itself tell you which scopes they are, and the answer is to read one
row from each rather than to parse the name.

### 9. Naming a checkable server-contract version

**Origin.** The third of decision 010's release obligations. The compatibility note currently names
the target as "RepForge as of 2026-08-25", which is a date and not something a build can check.

**Why it waits.** It is a D4 question by construction: D4 is when a second consumer first exists, and
a contract version means nothing with one consumer. RepForge's redesign makes it sharper — decision
019 records that the premise decision 010 was argued against is expiring for that consumer.

---

## E. Not Decisions — Decided But Not Built

Listed so they are not mistaken for open questions. All need an implementation go-ahead per
`AGENTS.md`; none needs a decision.

| Decision | What it adds | Status |
| --- | --- | --- |
| ~~016~~ | ~~A durable, globally monotonic `seq`~~ | **Built 2026-08-29** |
| ~~017~~ | ~~A durable `attempts` count; dead-letter at a caller-set bound~~ | **Built 2026-08-29** |
| ~~018~~ | ~~The single-flight conformance profile~~ | **Built 2026-08-29** as its own suite macro |
| ~~019~~ | ~~`SyncTransport`'s verdict-synthesis obligations~~ | **Documented 2026-08-29** |
| ~~022~~ | ~~Durable W3C trace context, caller-supplied~~ | **Built 2026-08-29** |
| 023 | Row-level staleness markers, never the rows | A third durable schema |
| ~~—~~ | ~~D3's drain-until-idle loop~~ | **Built 2026-08-29** as `SyncRunner::drain`, in **core** rather than the adapter (decision 028) |

**Updated 2026-08-29: the three columns are built** in core and the in-memory backend — `seq`,
`attempts`, `traceparent` — so D5 inherits a settled record shape rather than a decision. The
one-change discipline was never binding here: `InMemoryBackend`'s state is a `Vec` and a `HashMap`,
so adding a field costs nothing to migrate. It binds **D5's plan**, where §A.3 and §C.10 could still
make the column set four or five, and where three separate changes really would cost three
migrations against data on devices that may be offline for weeks.

## Sequencing

1. ~~**Ask RepForge §C now.**~~ **Done 2026-08-29, and it paid.** The terminality signal exists
   because the question was asked, which collapses decision 019's obligation to a lookup for that
   consumer. Two follow-ups replace it: **answer §11 this week** (cheapest item here, and their
   window), and **ask how their client attaches `If-Match`** before designing §10.
2. ~~**Settle §A.1 and §A.2**~~ **Done 2026-08-29** — decisions 024 and 025. Both resolved to the
   same shape: core states an obligation, the backend picks a mechanism, a conformance case is the
   enforcement. **The D5 plan is now writable**, and is the next artifact.
3. **Settle §A.3, §A.4's *shape*, and §C.10** in the D5 plan itself, so the record's column set is
   final before anything writes a row.
4. ~~**Take §B.5 and §B.6** whenever the code is next touched~~ — half done. §B.5 was taken
   2026-08-29 with the D3 drain loop (decision 030), on exactly this reasoning: the code was
   being touched anyway and a new report type would otherwise have widened the gap. **§B.6, the
   `tracing` packaging question, is still open** and still cheap.
5. **Defer §B.7, §D.8, and §D.9** with the reasons recorded above.
6. ~~**Take entries 14, 15 and 16 together**, in the next authorized touch of D3b.~~ **Taken
   2026-08-30**, as one change for the reason this note gave: one crate, one audience, and three
   breaking releases where one would do. 15 asked whether the adapter owns the browser clock and 16
   whether it owns the browser lifecycle, and answering one without the other would have left the
   `web` feature half-drawn.
7. ~~**Settle entry 17 in the D5 plan, before any durable row is written.**~~ **Settled 2026-08-30
   by building it**, which was cheaper than planning it: the in-realm half is core's and is done,
   and what reaches the D5 plan is the cross-realm half — an obligation on the backends plus a
   two-realm browser fixture the conformance suite cannot express. **Entry 18 replaces it as the
   D5-clock question to answer before the schema is written**, and unlike 17 it is not blocking.
8. **Decide entry 12 with D4b's evidence, and entry 13 before publication.** Entry 12 is the one to
   keep last on purpose: the rebuild it describes is affordable today and measurably wrong only on a
   durable backend, which is the thing D4b builds. Entry 13 is prose, not API, so it costs a
   documentation pass whenever someone is writing one.
