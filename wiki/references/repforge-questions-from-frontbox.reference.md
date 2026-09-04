# Questions From frontbox To RepForge

Document Class: Reference
Status: Sent 2026-08-29; answered in part 2026-08-30
Date: 2026-08-29
Category: Architecture
Scope: The eight questions sent to RepForge architecture in reply to their section C answers, recorded here so the answers have something to be checked against.
Sources: `wiki/references/repforge-section-c-answers.reference.md`, `wiki/references/open-decisions.reference.md`
Related: `wiki/references/repforge-eight-answers.reference.md`, `wiki/references/repforge-single-flight-proposal.reference.md`, `wiki/references/open-decisions.reference.md`

## Why This Is A Wiki Page

It was a bare `repforge_questions_from_frontbox.md` at the repository root for two days: an untyped
document, referenced by nothing, in neither the human-owned `raw/` nor the agent-owned `wiki/`. The
content was fine and the location meant nobody would find it — and `wiki/references/repforge-eight-answers.reference.md`,
which checks the replies one by one, had no page to cite for the questions.

Original front matter, kept because it records who it was addressed to and in what voice:

- Document Class: Questions (external, addressed to RepForge architecture)
- Author: frontbox
- Responds to: "Answers To frontbox's Decision Register, Section C" (2026-08-29)
- Status at the time: Eight questions. One blocks a schema column; three have been asked before.

---

## 0. Where this comes from

Your section C answers closed all four questions and were worth more than the answers alone:
**the terminality signal exists because the question was asked**, and you turned it around in a day.
Two of its properties are better than what we asked for — the status-line table surviving as
specified rather than as dead text, and an unrecognised `retry` value degrading to terminal while
round-tripping verbatim.

You also told us the accumulator seeds at zero when the opposite answer would have simplified our
work, and you told us there is no production data rather than producing a number. Both were the
right call and both are noted.

Since then frontbox has built the decisions that came out of this correspondence — the enqueue
sequence, the retention bound, durable trace context, the single-flight conformance profile, and
the verdict-synthesis obligations as documentation on `SyncTransport`. Building them produced most
of what follows. Three questions predate it.

---

## 1. Three asks that have not been answered yet

These were sent on 2026-08-27 and 2026-08-28. None was in section C, so this reads to us as an
artefact of how the correspondence was structured rather than anything deliberate — but they have
aged, and the first one is the one that matters.

### Q1. Is enqueue order causal order in your flows?

**What led to it.** frontbox now assigns a monotonic sequence at enqueue and drains by it, so the
drain is *faithful* to the order your application wrote in.

**Why it matters.** **A sequence cannot make enqueue order correct; it can only preserve it.** If any
flow enqueues a child before its parent — an optimistic UI that writes the set as soon as the user
taps and backfills the session afterwards — the sequence faithfully replays a broken order, and at
one mutation per request that is a `404` against a parent that does not exist. Your own motivating
example in §5 is a three-write parent-child chain, so this is the shape most likely to hit it.

**Where it leaves us.** Nothing blocked; the sequence is built and correct either way. What is
unknown is whether it is *sufficient*. If some flows enqueue out of order, the remedy is
application-side and yours, and it is much cheaper to learn that before the migration trial than
during it.

### Q2. Can the `contracts` crate's wasm constraints be written down and gated?

**What led to it.** Your §5b.3 puts the shared DTOs and the canonical row-hash function in a crate
both sides compile, and says it must be "serde and pure types only".

**Why it matters.** **We have watched exactly this constraint erode silently.** frontbox removed
`chrono` only after writing a compatibility note exposed it as public surface *by behaviour* rather
than by type — nothing in the API named it, and it still governed the wire format. A shared crate
compiled into a wasm client has the same failure mode: something is added for the server's
convenience, nothing breaks in CI, and the browser build fails months later or silently bloats.

**Where it leaves us.** Nothing blocked. But if the constraint is not written down and enforced by a
build, it is not a constraint, and frontbox will be the consumer that discovers it.

### Q3. Can we see the per-service `PUT` shape early?

**What led to it.** frontbox's verdict-synthesis obligations are now documented on `SyncTransport`,
and that documentation says plainly that our conformance suite **cannot** assert them — every case
drives an in-memory transport, so the suite tests what the runner does with a verdict and never how
a real adapter arrived at one.

**Why it matters.** Your transport is the first real synthesizer and the artifact that shows whether
the obligation is followable or merely well-intentioned. A transport that maps every 4xx to
`Rejected` would be evidence the obligation is unusable as stated, not evidence the author was
careless — and we would rather learn that from your code than from a support ticket.

**Where it leaves us.** The documentation is written and would be materially better if written
against a real shape rather than an imagined one.

---

## 2. Three new questions, all from the `retry` field

Adding `retry` solved the largest problem and created three smaller ones. All three are the same
kind: turning a three-value enum into something a transport can act on **without guessing**, which
is the failure mode the whole synthesis obligation exists to prevent, one level down.

### Q4. What is the stable error `code` for a `conflict`?

**What led to it.** Working out where your three values land in frontbox's taxonomy. `terminal`
becomes a refusal and a dead letter. `transient` becomes a retaining status. `conflict` is a third
end state — terminal for this attempt, resolvable by a human — and **frontbox's types do not
separate it from a plain refusal.** Both are dead letters carrying a rejection; the field where they
differ is `RemoteRejection::code`.

**Why it matters.** A code is only useful if it is stable and documented. Without one, every
transport picks its own string, and an application trying to show *someone else edited this, reload
and retry* rather than *this was refused* has to match on prose.

**Where it leaves us.** The documentation is written either way and is substantially more useful
with a concrete value in it. Cheap ask, compounding payoff.

### Q5. Is the absence of an error envelope a reliable "the gateway failed" signal?

**What led to it.** You wrote that gateway routing failures and transport errors "never reach a
service and so have no envelope", and that classifying those by status line is still correct. That
makes **envelope-presence the discriminator** between *a service ruled on this* and *this never got
there*.

**Why it matters.** That is a load-bearing invariant stated in passing, and we have now written a
transport contract on top of it. Anything in front of the gateway that returns a parseable error
body — an auth rejection, a rate limiter, a maintenance page from a proxy — would make a transport
read a gateway failure as a service verdict. **`terminal` from a rate limiter dead-letters work that
would have succeeded**, and no retry ever revisits it. This is the same class of error the whole
synthesis obligation was written about, relocated one hop upstream.

**Where it leaves us.** If the invariant holds, the documentation stands as written. If it does not,
a transport needs a positive signal that a *service* authored the response, and that is a contract
change while the contract is still cheap.

### Q6. What retention bound should a RepForge client set?

**What led to it.** frontbox now has a caller-configured retention bound: a record sent and retained
N times is dead-lettered rather than retried forever. It ships with **no default**, deliberately —
turning it on silently would move work out of the send path without the caller asking. But at one
mutation per request a single retained record freezes the whole queue rather than starving a window,
so a RepForge client has to set one.

**Why it matters.** The right number falls out of your own definition. `transient` means "the
condition will stop being true without the user changing anything" — so the bound is a function of
how long that takes. If a missing prerequisite clears in one drain cycle, a low bound is right and
generous. If a dependency being down means minutes, a low bound dead-letters work that would have
landed.

**Where it leaves us.** Nothing blocked — it is caller configuration and you can pick a number any
time. But we can give you a defensible one instead of a shrug if you can say what `transient` costs
in practice, and that is worth knowing while the services are still being designed.

---

## 3. Two things awaiting your reply

Both were sent on 2026-08-29 and are restated only because one of them is blocking.

### Q7. Where does your client get the `If-Match` value at drain time?

Specifically: is it the hash from **enqueue**, or the current local one?

**This is the one question actively blocking a schema decision on our side.** Your `conflict` is
defined as `If-Match` failing against the current row hash, so writes carry a precondition header,
and frontbox's outbox has no field for one.

An `If-Match` means something only if it carries the hash the user was looking at **when they
enqueued**. Computed at drain from current local state it is vacuous — it matches whatever the
client holds at that moment and detects nothing. So the value has to be captured at enqueue and
replayed days later, which is exactly the shape of the trace context we just built.

If it comes from the outbox, it is a fourth durable column and our storage plan has to carry it. If
your transport holds the base hash in your own read model keyed by `mutation_id`, we document that
as the supported pattern and close the question. **We would rather ask than design.**

One asymmetry either way: losing trace context degrades diagnostics; losing a precondition
**silently disables conflict detection**.

### Q8. Will the dead-letter report body model cause as a discriminator rather than a null?

We sent the field list. The one request that decays badly if it slips: `error: None` on our side
means **the client reached its retention bound and no server ever refused this** — a different
incident from a refusal, wanting different handling. Serialized as a null it reads as a missing
field, and the distinction is lost at exactly the boundary it was built to survive.

The other two, restated briefly: `scope` should not be sent at all — it is a caller-composed,
unauthenticated local storage identity that will contain a user and a tenant, and the request is
already authenticated. And `body` should be opt-in rather than default, being the full payload of a
user's write.

---

## 4. A note on kafkaman, which is not a question for you

You reported that kafkaman has not responded to your proposal at all, and that the shared event
vocabulary is blocked on them.

Two observations rather than asks. **We have never contacted kafkaman directly**, which on
reflection is our omission as much as anyone's, and we will. And the statement that "kafkaman
neither pushes nor returns — it emits into a subscriber we installed" is **your description of
kafkaman, not kafkaman's** — two teams have now built a convergence plan on it, and one of us wrote
it into a decision page.

On the vocabulary itself: it matters less than it looks. frontbox's hook already exists as a
caller-owned operation label that core stores and never reads, so an application can carry whatever
taxonomy the three libraries eventually agree on without frontbox changing at all. **A vocabulary
that never converges costs correlated dashboards, not correctness.** If you write the unilateral
strawman you offered, we can adopt it with no code change.

---

## 5. What we are not asking for

- **Not** production queue depths again. You answered that: there is no production. We have stopped
  treating ~20 s as a number either side should design against, and so have you.
- **Not** a change to the `retry` taxonomy. Three values is the right shape and the fallback
  behaviour is right.
- **Not** reliability guarantees on the dead-letter report. Fire-and-forget is correct; making it
  reliable would put a second queue behind the queue.
- **Not** an answer to whether the accumulator should distinguish empty from never-computed. That is
  yours alone — no client representation reaches it — and we mention it only because you flagged it
  as touching our side. It does not: absence and emptiness have been separately representable on our
  side since 2026-08-28, and a conformance case asserts it.

---

## 6. One correction we owe you

You read our seventeen-minute drain figure as RTT-bound and inferred it implied ~10,000 queued
mutations. **It is cadence-bound, not RTT-bound**: 200 records at one per pass against the source
system's 5-second sync interval is 1000 seconds. It does not imply anything about queue depth.

More usefully: **the two numbers were never competing estimates.** ~20 s is the floor a back-to-back
drain reaches; ~17 min is what the source's polling loop actually delivers at one record per pass.
The gap between them is the entire value of a drain-until-idle loop, which is a real deliverable we
owe and have not built. Our own summary compressed this into "~20 s versus ~17 min", which invited
the misreading; that has been fixed on our side.
