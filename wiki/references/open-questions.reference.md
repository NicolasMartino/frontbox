# Open Questions Register

Document Class: Reference
Status: **Q1-Q8 answered 2026-08-30**; Q9 and Q10 open, and neither has been asked. See `## How Much Of This Page Is Live`
Date: 2026-08-29
Category: Architecture
Scope: Every question this project is waiting on an answer to, what produced it, what it changes, and what happens while it stays open.
Sources: `wiki/proposals/`, `wiki/references/repforge-section-c-answers.reference.md`, `src/`
Related: `wiki/references/open-decisions.reference.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`, `wiki/proposals/dead-letter-report-and-preconditions.proposal.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/020-observability-surface.decision.md`

## How Much Of This Page Is Live

**Two questions of ten.** Q1 through Q8 were answered by RepForge on 2026-08-30, and their replies —
plus the two findings that checking those replies produced — are at
`wiki/references/repforge-eight-answers.reference.md`. Sections A and B below are the questions as
asked, kept for what they were asked *against*: an answer means much less without the reasoning that
made the question worth a round trip.

**Q9 and Q10 are the live ones**, in section C, and their status is worse than "waiting": *kafkaman
has never been asked.* They have sat here since 2026-08-29 with no round trip spent on them. That is
a fact about this project, not about kafkaman, and it belongs in the open rather than behind a page
header that says "superseded".

One question outside this page is also live and also unsent: **Q4's `If-Match` follow-up**, which
`open-decisions.reference.md` records as owed back to RepForge.

The page is not archived, because the guidelines' archive rule is for a document nothing cites any
more. This one is cited by the answers page, by the decision register, and by decision 019.

## Why This Page Is Separate From The Decision Register

`open-decisions.reference.md` tracks what **we** owe an answer to. This tracks what **someone else**
does. The distinction matters for planning: a decision can be made on any afternoon we choose, and a
question cannot — it costs a round trip, it can be ignored, and three of the ones below have been.

Ten questions. Two of them block a schema column, three have been asked before and never answered,
and two rest on an assumption two teams are already building on without confirmation.

| # | Question | To | State |
| --- | --- | --- | --- |
| 1 | Is enqueue order causal order in your flows? | RepForge | **Asked twice, unanswered** |
| 2 | The `contracts` crate's wasm constraints, in writing | RepForge | **Asked once, unanswered** |
| 3 | Early sight of the per-service `PUT` shape | RepForge | **Asked once, unanswered** |
| 4 | Where does the client get `If-Match` at drain? | RepForge | Asked 2026-08-29 |
| 5 | What is the stable error `code` for `conflict`? | RepForge | **New** |
| 6 | Is envelope-absence a reliable "the gateway failed" signal? | RepForge | **New** |
| 7 | What retention bound should a RepForge client set? | RepForge | **New** |
| 8 | Will the report body model cause as a discriminator? | RepForge | Awaiting reply |
| 9 | Does kafkaman emit `tracing` and disown the sink? | kafkaman | **Never asked by us** |
| 10 | Is a shared event vocabulary worth agreeing? | kafkaman | Asked by RepForge, ignored |

---

## A. Asked Before, Never Answered — **all three answered 2026-08-30**

Three asks have now survived one or two rounds of correspondence without being addressed. That is
not necessarily neglect — RepForge answered everything in section C thoroughly and these were not in
section C — but they have aged, and two of them get more expensive with every week of their build.

### 1. Is enqueue order causal order in RepForge's flows?

**What led to it.** Decision 016 makes the drain *faithful* to enqueue order — the store assigns a
monotonic `seq` and `pending_batch` reads by it. Asked in the single-flight response §9 on
2026-08-27 and again in the read-model response §7 on 2026-08-28.

**Why it matters.** **A sequence cannot make enqueue order correct; it can only preserve it.** If a
RepForge flow enqueues a set before the session it belongs to — an optimistic UI that writes the
child as soon as the user taps, then backfills the parent — then `seq` faithfully replays a broken
order, and `batch_limit = 1` turns that into a `404` against a parent that does not exist. The whole
motivating example in their §5 is a three-write parent-child chain, so this is not hypothetical.

**Where it leaves us.** Decision 016 is built and correct either way; this does not block it. What it
blocks is knowing whether 016 is *sufficient*. If the answer is "no, some flows enqueue out of
order", the remedy is application-side and theirs, and they should learn that before D4 rather than
during it.

### 2. The `contracts` crate's wasm constraints, in writing

**What led to it.** Their §5b.3 puts the shared read-model DTOs and the canonical row-hash function
in a `contracts` crate both sides compile, and says it must be "serde and pure types only". Asked in
the read-model response §7.

**Why it matters.** **frontbox has watched this constraint erode silently.** Decision 011 removed
`chrono` from this crate only after a compatibility note exposed it as public surface *by behaviour*
rather than by type — nothing in the API named it, and it still governed the wire format. A shared
crate compiled into a wasm client has the same failure mode: something gets added for the server's
convenience, nothing breaks in CI, and the browser build fails months later or silently bloats.

**Where it leaves us.** Nothing blocked. But if the constraint is not written down and gated, it is
not a constraint, and frontbox will be the consumer that discovers it.

### 3. Early sight of the per-service `PUT` shape

**What led to it.** Decision 019 makes verdict synthesis a documented obligation on `SyncTransport`
and says plainly that the conformance suite cannot assert it — every case drives an in-memory
transport, so the suite tests what the *runner* does with a verdict, never how a real adapter
arrived at one. Asked in the single-flight response §9.

**Why it matters.** The RepForge transport is the first real synthesizer, and it is the artifact
that shows whether decision 019's obligation is followable or merely well-intentioned. A transport
that maps every 4xx to `Rejected` would be evidence the obligation is unusable as stated, not
evidence the author was careless.

**Where it leaves us.** Decision 019's documentation is the next thing worth building, and writing
it against a real shape is much better than writing it against an imagined one.

---

## B. New, From Their Section C Answers — **all five answered 2026-08-30**

Adding the `retry` field solved the largest problem and created three smaller ones. All three are
about turning a three-value enum into something a transport can act on without guessing — which is
exactly the failure mode decision 019 exists to prevent, one level down.

### 4. Where does the client get the `If-Match` value at drain time?

**What led to it.** Their `conflict` value is defined as "`If-Match` failed against the current row
hash", so writes carry a precondition header. `MutationIntent` has no field for one — its keys are
`mutation_id`, `method`, `path`, `client_datetime`, `body` plus optional `op`, and there is no
header concept anywhere in `src/`. Asked in the dead-letter proposal §2 on 2026-08-29.

**Why it matters.** An `If-Match` means something only if it carries the hash the user was looking
at **when they enqueued**. Computed at drain from current local state it is vacuous — it matches
whatever the client holds at that moment and detects nothing. So the value must be captured at
enqueue and replayed days later, which is decision 022's `traceparent` shape exactly. Two instances
of one shape is where the general case becomes worth considering rather than a third bespoke column.

**Where it leaves us.** **This is the one question actively blocking a schema column.** If the value
comes from the outbox, it is a fourth field and D5's plan must carry it. If it comes from the
application's read model keyed by `mutation_id`, we document that as the supported pattern and close
register entry 10. Note the asymmetry either way: losing trace context degrades diagnostics, losing a
precondition **silently disables conflict detection**.

### 5. What is the stable error `code` for a `conflict`?

**What led to it.** Decision 019's amendment worked out where the three `retry` values land.
`terminal` becomes `Rejected` → dead letter. `transient` becomes a retaining status. `conflict` is a
third end state — terminal for this attempt, resolvable by a human — and **frontbox's type system
does not separate it from a plain refusal.** Both are dead letters with `Some(rejection)`;
decision 017's `None` separates only the client-gave-up case.

**Why it matters.** `RemoteRejection::code` is the field where the two differ, and it is
machine-readable by design. But a code is only useful if it is *stable and documented*: without one,
every transport picks its own string, and an application trying to show "someone else edited this,
reload and retry" versus "this was refused" has to match on prose.

**Where it leaves us.** Decision 019's documentation can be written either way, but it is
substantially more useful with a concrete value in it. This is a cheap ask with a compounding
payoff.

### 6. Is the absence of an envelope a reliable "the gateway failed" signal?

**What led to it.** They wrote that gateway routing failures and transport errors "never reach a
service and so have no envelope", and that classifying those by status line "is still correct and
still specified". So envelope-presence is the discriminator between *a service ruled on this* and
*this never got there*.

**Why it matters.** That is a load-bearing invariant stated in passing. A gateway that ever returns
a JSON body which happens to parse as the envelope — an auth rejection, a rate limiter, a
maintenance page from a proxy in front of the gateway — would make a transport read a gateway
failure as a service verdict, and `terminal` from a rate limiter dead-letters work that would have
succeeded. This is the same shape of error decision 019 was written about, relocated one hop
upstream.

**Where it leaves us.** If the invariant holds, decision 019's documentation says "no envelope means
classify by status" and that is sound. If it does not, the transport needs a positive signal that a
service authored the response, and that is a contract change while the contract is still cheap.

### 7. What retention bound should a RepForge client set?

**What led to it.** Decision 017 is now built. `with_retention_bound` ships with **no default**,
deliberately — turning it on silently would move work out of the send path without the caller
asking. But at `batch_limit = 1` a queue without a bound has no liveness argument at all, so a
RepForge client has to set one.

**Why it matters.** The right number falls out of their own definition. `transient` means "the
condition will stop being true without the user changing anything" — so the bound is a function of
how long that takes. If a missing prerequisite resolves in one drain cycle, a low bound is right and
generous. If a dependency being down means minutes, a low bound dead-letters work that would have
landed.

**Where it leaves us.** Nothing blocked — the bound is caller configuration and they can pick one
any time. But we can give a defensible number instead of a shrug if they can say what `transient`
costs in practice, and it is worth asking while the services are still being designed rather than
after the first support ticket.

### 8. Will the dead-letter report body model cause as a discriminator?

**What led to it.** Our reply of 2026-08-29 gave them the field list and made three requests that
change their schema rather than fill it in: cause as a discriminated field rather than "error
present or absent", `scope` not sent at all, and `body` opt-in rather than default.

**Why it matters.** The first is the one that decays badly if ignored. Decision 017 makes
`error: None` mean *the client reached its retention bound and no server ever refused this* — a
different incident from a refusal, wanting different handling. Serialized as a null it reads as a
missing field, and the distinction is lost at exactly the boundary it was built to survive.

**Where it leaves us.** Awaiting their reply. Nothing blocked; the field list is theirs whenever
they want it.

---

## C. To kafkaman, Who Has Not Been Part Of This — **still unasked**

Two questions, and the awkward part is that **nobody has asked kafkaman anything directly from this
project**. RepForge asked them about the shared vocabulary and reports no response at all.

### 9. Does kafkaman emit `tracing` and leave the sink to the application?

**What led to it.** RepForge's §5b.6 quotes kafkaman's own observability proposal — a library
"should not own the application's subscriber, formatter, or sink" — and decision 020 accepted that
boundary on frontbox's behalf. In answering our convergence question, RepForge stated that
"kafkaman neither pushes nor returns — it emits into a subscriber we installed."

**Why it matters.** **That is RepForge's description of kafkaman, not kafkaman's.** Two teams have
now built a convergence plan on it, and one of them wrote a decision page. If kafkaman's actual shape
differs — if it returns structured results, or owns an exporter — the adapter has to bridge two
shapes, and that asymmetry should be deliberate rather than discovered in D4.

**Where it leaves us.** Decision 020 is correct about frontbox regardless. What is unconfirmed is the
claim that all three libraries already agree, which is the premise the convergence rests on.

### 10. Is a shared event vocabulary worth agreeing, or is it over-specifying?

**What led to it.** RepForge's own open question 8 to kafkaman, which they report as unanswered.
Decision 020 lists the shared vocabulary as one of four things it cannot settle alone.

**Why it matters.** Less than it looks, and saying so is useful. frontbox's hook already exists:
`OperationMeta { name, version }` is caller-owned and core never reads it, so an application can put
whatever taxonomy the three libraries agree on into it without frontbox changing. A vocabulary that
never converges costs correlated dashboards, not correctness.

**Where it leaves us.** Genuinely optional. RepForge offered to write a unilateral strawman; if
kafkaman stays silent, that is better than a blank, and frontbox can adopt it through
`OperationMeta` with no code change at all.

---

## What This Does Not Cover

Decisions this project owes **itself** are in `wiki/references/open-decisions.reference.md`: six
open, of which one (entry 10, the precondition column) is downstream of question 4 above and the
rest are ours to make on any afternoon we choose.

One item sits in both registers and belongs to neither cleanly: **which `MutationStatus` a transport
should synthesize from `retry: transient`.** Decision 019's amendment observes that this gives
`Blocked` a producer back on the client side — but `Blocked` in frontbox's taxonomy means "skipped
because a predecessor *in the same batch* failed", and at `batch_limit = 1` there is no batch. So
`Pending` may be the more honest synthesis, or the vocabulary may need a word neither has. That is
ours to settle when decision 019's documentation is written, and it is recorded here because it was
discovered while reading their answer rather than ours.
