# Single-Flight Drain Is A Supported Mode, Not The Default

Document Class: Decision
Status: Accepted 2026-08-27; profile implemented 2026-08-29; drain loop implemented and one claim corrected 2026-08-29
Date: 2026-08-27
Category: Public API Shape
Scope: Whether `batch_limit = 1` becomes frontbox's default, and what "first-class support" for it costs.
Sources: `src/runner/mod.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

`DEFAULT_BATCH_LIMIT` stays at 100. `batch_limit = 1` becomes a first-class supported configuration
with its own conformance profile, and is not adopted as the default.

- **The conformance suite gains a single-flight profile**: an *adapted* run of the outbox cases at
  `batch_limit = 1`. It is not the existing list re-emitted unchanged, and saying so would
  understate the work by most of it. See `## What The Profile Actually Costs` below.
- **The profile requires a retention bound** (decision 017). A single-flight runner without one has
  no liveness argument, and the suite is where that stops being advice.
- **Drain cadence becomes part of the contract's documentation**, because at limit 1 it determines
  whether a queue drains in seconds or in hours.

## Why

**Nothing is blocked, so the ask is narrower than it was written.** `with_batch_limit(1)` compiles
and works today; `limit.max(1)` already treats 1 as the floor rather than a degenerate value.
RepForge's real request is that single-flight be covered rather than merely permitted, and that is a
test obligation. It is granted in full.

**The default is the one place the coupling would actually bite.** RepForge's own objection list
raises this and answers it by keeping `batch_limit` configurable. That answer is right and it is
also the answer to the default question: a default is what an application gets when it has expressed
no opinion, and an application with no opinion is not necessarily talking to a per-resource `PUT`
API behind a dumb gateway. The configurability that makes the coupling survivable is the same
property that makes changing the default unnecessary.

**A limit of 1 is safe only in combination with something else.** Decision 017 exists because at
limit 1 a single retained record freezes the whole queue, where at 100 it starves only its window. A
default whose liveness depends on a second feature being switched on is not a default; it is a trap
with a documented workaround. Defaults belong at the safe end of a setting, and 100 is the end where
a wedged record still lets ninety-nine others through.

**The profile is worth having regardless of RepForge.** Two behaviours are unobservable at
`batch_limit = 100` and obvious at 1: a wedged head, and whether ordering is actually respected
rather than accidentally correct because everything went out together. A defect that only manifests
at limit 1 is invisible to the current suite. That is a coverage gap in frontbox, not a favour to a
consumer.

**The round-trip estimate needs restating before anyone plans against it.** RepForge estimates a
200-record backlog at ~20 s of background drain, from 200 sequential round trips at 100 ms RTT. That
arithmetic assumes back-to-back requests. `sync_once` sends one batch per call and returns
`SyncPass::AlreadyRunning` on re-entry, so the drain rate is set by how often the adapter calls it.
The source's loop sleeps between passes:

```rust
const SYNC_INTERVAL_MS: u32 = 5000; // 5 seconds
```
(`persistence/mutations.rs:731`)

At that cadence, 200 records at `batch_limit = 1` is roughly seventeen minutes, not twenty seconds —
**and the two figures measure different things, which is the point rather than a disagreement.**
200 x 100 ms RTT = 20 s is the *floor*: what a back-to-back drain costs. 200 x 5 s poll = 1000 s is
what the source's loop actually delivers. Neither is wrong; the gap between them is exactly the
value of a drain-until-idle loop. RepForge read the seventeen-minute figure as RTT-bound on
2026-08-29 and inferred it implied 10,000 queued mutations. It does not — it is cadence-bound at
the stated 200 —
a factor of fifty. The estimate is recoverable, but only by adding something that does not exist: a
D3 adapter that keeps calling `sync_once` until it reports `Idle`. That is a real deliverable, and
it has a prerequisite of its own, since a drain-until-idle loop against a frozen head is a spin. So
the honest sequence is 017, then the drain loop, then the estimate.

## Consequences

- ~~**D3 owes a drain-until-idle loop**, and it is now a correctness-adjacent requirement rather
  than a performance nicety: at limit 1 the sync interval multiplies the backlog. The loop must
  terminate on `Idle` and `Offline` and must not spin on a retained head, which decision 017 is
  what makes possible.~~ **Built 2026-08-29, and the last clause is wrong** — see
  `## The Drain Loop, And What This Page Got Wrong About It` below. The loop is
  `SyncRunner::drain`, it lives in core rather than in the adapter
  (`wiki/decisions/028-drain-loop-boundary.decision.md`), and it terminates on *no progress*
  rather than on `Idle` (`wiki/decisions/029-drain-termination.decision.md`).
- **The conformance profile is not a third emission of one list.** The sync and async macros can
  share a list because they differ only in how a case is driven. The single-flight profile differs
  in what a case may assert, so it needs a parameterized helper, a visible exclusion list, and cases
  of its own. Sizing it as "run the existing list again" is the estimate this decision most wants
  corrected.
- **`is_stalled()` becomes sharper at limit 1**, because every pass that sends the wedged record and
  drains nothing reports stalled. That is a small benefit of the mode rather than a reason for it.
- **`SyncOutcomeCounts` reads differently but is unchanged.** Every count is 0 or 1 per pass, which
  makes a caller aggregating across passes the normal case rather than an unusual one. Nothing in
  the type needs to change; it is worth saying because a reader of a single-flight report will find
  it uninformative in isolation. **Amended 2026-08-28:** under a cross-library observability
  requirement this is more than a readability note — a 200-record backlog produces 200 reports
  instead of 2, and aggregation moves out of the library into every caller. It is a second and
  independent argument for D3's drain-until-idle loop, which is the natural place to aggregate.
  See decision 020.
- **Decision 005's blast-radius argument reaches its endpoint.** RepForge is right that
  `limit = 1` is where "bound the blast radius of any one server verdict" was always heading. What
  that argument never covered is the cost on the other side, which decision 017 now carries.

## What The Profile Actually Costs

Written out because the first draft of this decision claimed the case list could be re-run unchanged,
and the suite says otherwise. The outbox cases fall into three groups at `batch_limit = 1`.

**Cases whose subject is a multi-record batch.** These do not fail on a detail; their subject stops
existing at limit 1, so no rewrite recovers them.

| Case | Assertion that cannot hold | Why limit 1 removes the subject |
| --- | --- | --- |
| 06 (`cases/status.rs:91`) | five statuses applied in one atomic call | there is no mixed batch |
| 19 (`cases/liveness.rs:15`) | one pass reports `dead_lettered: 1` and `blocked: 1` | `Blocked` means "skipped because an earlier mutation *in the same batch* failed", which is precisely the producer RepForge says disappears |
| 32 (`cases/anomalies.rs:46`) | the uncontested record drains while the contested one is held | only one record is sent, so the second verdict becomes a surplus-verdict anomaly instead |
| 34 (`cases/anomalies.rs:160`) | "an unrecognised status must not spoil the verdicts around it" | at limit 1 there are no verdicts around it |

**Cases that need parameterizing.** The subject survives; the arithmetic does not. Cases 07, 20, and
33 assert `sent` or `retained` equal to the seeded count (2, 2, and 3), which becomes 1 in every
case. They are the profile's real content — each one describes a pass that drains nothing, which is
the single-flight failure mode — and each needs its expectations expressed against the batch limit
rather than the seed count.

**Cases the profile cannot reach at all.** Cases 10 and 21 build their own runner with
`.with_batch_limit(2)`, and cases 19, 28, 30 call `SyncRunner::new` directly — case 19 appears in
both lists, since it would sit inert *and* has lost its subject. A profile that
parameterizes the shared `runner()` helper (`cases/mod.rs:86`) leaves all of these running at their
own limit — passing, and testing nothing about single-flight. This is the quieter half of the
problem: a profile built the obvious way reports a full green suite while silently skipping the
cases nearest its subject.

So the profile needs four things, not one: a batch-limit-aware `runner()` helper, expectations
derived from the limit rather than the seed count, an exclusion list for the four cases above, and
new cases for what only limit 1 exposes — a head that never clears, and an ordering violation that a
single wide batch would have hidden.

**The exclusions must be visible, not skipped.** The suite already takes this position for backends
that cannot implement a factory: the gap lives in the test file rather than behind a runtime skip,
so a reader sees what is not covered. The single-flight profile inherits that rule. Four unreachable
cases named in one place is a fact about the mode; four cases quietly absent from a green run is a
coverage claim that is not true.

## Implementation Outcome

**Built 2026-08-29**, and the shape is not the one this page described.

**The profile is its own case list, not the outbox list re-run at a different limit.** This page
established that a re-run does not work; building it showed that the *adaptation* does not work
either. Of the seven cases named here, four lose their subject entirely and no rewrite recovers
them, and the remaining three — 07, 20 and 33 — assert `sent` or `retained` equal to the seed count.
Rewriting those three in place would have broken them at the default limit, and parameterizing every
case function to know a batch limit means threading a value through forty signatures for the benefit
of three.

So the profile is a fifth suite macro beside the four that already exist —
`frontbox_single_flight_tests!`, with `__frontbox_single_flight_suite!` holding one list. That is the
pattern `frontbox_cache_tests!` and `frontbox_fault_injection_tests!` already set: a backend that
cannot satisfy a profile does not invoke it, which leaves the gap visible in its test file.

**Three cases, each earning its place**, rather than a wide re-emission of behaviour that is
identical at either limit:

- **52** — one record per pass whatever the outcome, across an offline pass, a fully retained pass
  and an omitted verdict. This is what cases 07, 20 and 33 were reaching for, stated against the
  limit instead of the seed.
- **53** — a wedged head freezes the entire queue, and a retention bound is the only thing that
  frees it. The case this mode exists to make observable, and the whole argument for decision 017 in
  one place: three passes, three requests carrying the same record, nothing behind it moving, then
  the same queue with a bound draining.
- **54** — the server sees one record at a time in enqueue order, with the records enqueued against
  both orders the old key would have produced.

**The four exclusions are named in the macro's own documentation**, which is where a reader looking
at the profile will be. Four cases quietly absent from a green run would be a coverage claim that is
not true.

## The Drain Loop, And What This Page Got Wrong About It

**Built 2026-08-29.** The estimate this page spends its longest paragraph on is now recoverable,
and conformance case 60 measures it: five records at `batch_limit = 1` drain in one `drain()` call
and five requests, where the source's loop spreads the same five over five poll intervals.

**One claim in `## Consequences` was wrong, and it was the load-bearing one.** This page said the
loop "must not spin on a retained head, which decision 017 is what makes possible". Building it
showed that 017 does not make it possible. It makes the spin *finite*, and it pays for that by
destroying its own meaning.

017's bound counts verdicts **received**, not requests sent, and a back-to-back drain receives them
as fast as the network allows. A bound of eight against a head the server never resolves becomes
eight requests inside a second at a 100 ms round trip, and then a dead letter — so a policy meaning
*give the server eight chances* is spent before the server has had one. Without a configured bound,
and 017 ships with no default, the same loop is an unbounded hot loop.

The correction is that a drain stops on the first pass that **makes no progress**, not on `Idle`.
That also supplies a termination argument this page never had: a pass that continues the loop
removed at least one record, so the queue is strictly shorter each time round. The retention bound
goes back to being what 017 designed — one attempt per drain, spread by the caller's cadence.
See `wiki/decisions/029-drain-termination.decision.md`, and conformance case 58, which asserts a
send count of one and fails against the reading this page originally implied.

## Revisit If

D4's migration trial shows that every real consumer sets 1. That would be evidence the population
changed rather than that the default was wrong, and changing a default is cheap while the crate is
unpublished. Or if the bounded-batch server disappears from the target set entirely — but note that
frontbox's own conformance suite is a consumer of the batched path, and the multi-verdict handling
in `SyncRunner::run` is exercised only by the four cases named above — the ones the single-flight
profile cannot reach. Dropping the batched path would delete the coverage, not inherit it.
