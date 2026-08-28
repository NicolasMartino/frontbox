# Open Decisions Register

Document Class: Reference
Status: Active
Date: 2026-08-28 (section C answered 2026-08-29)
Category: Architecture
Scope: Every decision this project has not yet made, why it is open, what each option costs, and when it stops being cheap. Section C was answered by RepForge on 2026-08-29 and is now closed except where noted.
Sources: `wiki/index.md` (Open Work), `wiki/decisions/`, `src/`, `scripts/verify.sh`
Related: `wiki/references/repforge-section-c-answers.reference.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/020-observability-surface.decision.md`

## How To Read This

**Updated 2026-08-29.** RepForge answered section C in full. Two questions are settled and citable,
one produced decisions they had not made, and one they cannot answer and neither can we. Section C
is rewritten below with the answers and the three things they open. **Eleven decisions are now
open**, not nine: their reply adds two, both needing an answer before D5.

Nine decisions were genuinely unmade when this was written. They are not equally urgent, and
urgency here is not about importance — it is about **which door closes first**. Three clocks are
running:

| Clock | Closes when | What it governs | Cost of being late |
| --- | --- | --- | --- |
| **D5** | The first durable row is written | Storage schema | A migration, on data that already exists on user devices |
| **Publication** | The crate is first published | Public API | A major version, for every consumer |
| **RepForge** | Their service design settles | Things only they can answer | The answer stops being free to change |

The crate is `0.0.0` with `publish = false`, and D5 has not started, so **every door is still open**.
That will not be true for long, and the D5 door is the one closing first because it is the next
deliverable anyone would authorize.

A fourth group has no clock at all and is listed last so it is not confused with the rest.

## Summary

| # | Decision | Clock | Recommendation |
| --- | --- | --- | --- |
| 1 | How a `ScopeKey` becomes a storage name | D5 — **blocking** | Hash with a readable prefix |
| 2 | Quarantine: separate table or status column | D5 — **blocking** | Status column |
| 3 | Last-error metadata on the record | D5 | Yes, but transport-shaped and bounded |
| 4 | Does core learn about service origins | D5 *or* publication | Classifier, deferred — but decide the *shape* now |
| 5 | `Serialize` on the report types | Publication | Yes, `Serialize` only, not `Deserialize` |
| 6 | `tracing` default-on or feature-gated | Publication | Feature-gated, default on, mirroring `v4` |
| 7 | Per-status retention bounds | Publication | Defer to D4 evidence |
| 8 | A cross-scope diagnostic | None | Defer; it needs D5 to exist first |
| 9 | Naming a checkable server-contract version | None (D4) | Defer to D4 |
| 10 | Whether the outbox carries replayable request preconditions | D5 — **new 2026-08-29** | Ask first; it may be a real gap or an app-side concern |
| 11 | What frontbox contributes to the dead-letter report body | RepForge — **new 2026-08-29** | Answer now; they asked and the window is open |

Plus **seven items already decided but not built** (§E), which are work rather than decisions and
are listed so they are not mistaken for it. Section C's four questions are answered and the section
now records what came back.

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

## B. The Publication Clock — Public API

Free while the crate is `0.0.0` with `publish = false`. A major version afterwards.

### 5. `Serialize` on the report types

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

## C. RepForge's Clock — Answered 2026-08-29

All four came back. Full text at `wiki/references/repforge-section-c-answers.reference.md`.

| Question | Answer |
| --- | --- |
| Does the XOR accumulator seed at zero? | **Yes** — BLAKE3, 256-bit XOR, empty set all zeroes, asserted by a property test on native and wasm. **Case 44's scenario is live** and decision 021 does not simplify |
| Structured terminality signal? | **Yes, added in response.** A required `retry` field of `terminal` / `transient` / `conflict`, with the status-line table kept as a documented fallback for errors that never reach a service |
| Convergence specification | Collector, target, and sink boundary were already settled. `mutation_id` → **span attribute** and **clock authority** were decided in response. Shared vocabulary is **blocked on kafkaman**, who has not replied |
| Production queue depths | **Cannot supply — there is no production.** They asked us the same question. Neither party has the number |

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
| 016 | A durable, globally monotonic `seq`, primary sort key | Schema column |
| 017 | A durable `attempts` count; dead-letter at a caller-set bound | Schema column |
| 018 | The single-flight conformance profile | Adapted fixtures, **not** the case list re-run |
| 019 | `SyncTransport`'s verdict-synthesis obligations | Documentation |
| 022 | Durable W3C trace context, caller-supplied | Schema column |
| 023 | Row-level staleness markers, never the rows | A third durable schema |
| — | D3's drain-until-idle loop | Adapter work; correctness-adjacent at `batch_limit = 1` |

**The one thing worth carrying into the D5 plan:** `OutboxRecord` now has **three**
decided-but-unbuilt columns — `seq`, `attempts`, `traceparent` — and §A.3, §A.4, and §C.10 could
each make it four, five, or six. They should land as **one** schema change. Three separate ones
cost three migrations against data on devices that may be offline for weeks.

## Sequencing

1. ~~**Ask RepForge §C now.**~~ **Done 2026-08-29, and it paid.** The terminality signal exists
   because the question was asked, which collapses decision 019's obligation to a lookup for that
   consumer. Two follow-ups replace it: **answer §11 this week** (cheapest item here, and their
   window), and **ask how their client attaches `If-Match`** before designing §10.
2. **Settle §A.1 and §A.2**, which are what a D5 plan cannot be written without.
3. **Settle §A.3, §A.4's *shape*, and §C.10** in the same pass, so the record's column set is final
   before anything writes a row.
4. **Take §B.5 and §B.6** whenever the code is next touched; both are cheap and both get more
   expensive at publication rather than at D5.
5. **Defer §B.7, §D.8, and §D.9** with the reasons recorded above.
