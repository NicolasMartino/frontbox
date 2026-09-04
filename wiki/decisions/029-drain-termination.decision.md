# A Drain Stops When It Stops Draining, Not When The Queue Is Empty

Document Class: Decision
Status: Accepted 2026-08-29; implemented 2026-08-29
Date: 2026-08-29
Category: Public API Shape
Scope: The condition that ends a drain loop, why it is not `Idle`, and what the aggregated report can and cannot be read to mean.
Sources: `src/runner/drain.rs`, `src/runner/mod.rs`, `src/testing/cases/drain.rs`, `src/testing/cases/single_flight.rs`
Related: `wiki/decisions/028-drain-loop-boundary.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/020-observability-surface.decision.md`

## Decision

`drain` runs a pass, folds it into a `DrainReport`, and goes round again **only if that pass made
progress** — at least one record applied, deduplicated, or dead-lettered. Every other ending stops
it.

| Pass | `DrainEnd` |
| --- | --- |
| `Idle` | `Drained` |
| `Completed`, having drained nothing | `Stalled` |
| `Offline` | `Offline` |
| `AlreadyRunning` | `AlreadyRunning` |

A failing pass aborts the drain and its error is returned.

## Why Not "Until Idle"

**Decision 018 says the loop "must terminate on `Idle` and `Offline` and must not spin on a retained
head, which decision 017 is what makes possible." Building it showed that 017 does not make it
possible. It only makes the spin finite, and it pays for that by destroying its own meaning.**

017's bound counts verdicts *received*, not requests sent. A back-to-back drain receives them as
fast as the network allows, so a bound of eight against a head the server never resolves becomes
eight requests inside a second at a 100 ms round trip, and then a dead letter. The bound is supposed
to say *give the server eight chances*. A loop that consumes all eight in under a second has turned
a policy about time into a policy about nothing, and it terminates a record the server might have
resolved on the next poll.

Without a configured bound — and 017 ships with no default, deliberately — the same loop is an
unbounded hot loop against a wedged head. A library shipping that is a defect, not a configuration
question.

**A fruitless pass provably cannot be improved by repeating it.** `pending_batch` is oldest-first
with no cursor, so a pass that changed nothing leaves the next pass reading the same records and
sending the same request. Going round again cannot help. It can only hurt, in the way above.

So the stop condition is the one that already exists: `SyncReport::made_progress`, which is exactly
`applied + duplicate + dead_lettered > 0`. The retention bound stays what it was designed to be — a
mechanism across a caller's cadence, one attempt per drain — and `SyncCadence::stalled_ms` in the
adapter is what spreads those attempts over time (`wiki/decisions/028-drain-loop-boundary.decision.md`).

## Why It Terminates, Without A Pass Cap

Not by a cap, because a cap would be a number with no defensible value. It terminates on the store
contract that already exists.

Every pass that continues the loop removed at least one record from the outbox: `made_progress`
sums exactly the three counts whose dispositions are `Delete` and `DeadLetter`, and both remove the
row. The queue is therefore strictly shorter each time round, and finite. A store where that is
untrue is a store already violating `apply_outcomes`, which decision 003 makes atomic and which
conformance case 31 already polices.

Work enqueued *during* a drain is drained by it. That is the useful behaviour rather than a hazard:
the loop is bounded by the queue, and the queue is finite whenever enqueueing stops. A caller that
enqueues faster than the server accepts has a problem no loop shape fixes.

## What The Report Can Be Read To Mean

`DrainReport` aggregates over passes, and two of its fields are sums over *sends* rather than over
records. A record the server left queued in three passes contributes three to `sent` and three to
`retained`.

That is the honest reading of "how much work did this drain do", and it is deliberately **not** an
answer to "how much is left". A caller that wants what is still queued asks
`OutboxStore::pending_count`, and the field documentation says so, because the alternative is a
number that looks authoritative and is wrong by the number of retries.

`applied`, `duplicate` and `dead_lettered` are per-record and can be read as totals, because each
removes the record and so can happen to it once. `blocked`, `pending` and `unknown_status` are
per-send, like `retained`. The split follows from what the dispositions do, not from a convention.

## Alternatives Rejected

**Sleep inside the drain.** It would let the loop wait out a stall rather than return, and it needs
a clock core does not have and will not take (decision 011 keeps every date library out of the
runtime graph, checked against `cargo tree`). It would also make `drain` a cadence policy, which is
the adapter's.

**A `max_passes` cap.** The store contract already gives termination, so a cap adds a knob whose
only correct value is "larger than any real queue". It would earn its place only if a backend were
found that reports progress without shortening the queue, and that backend is already broken.

**`DrainEnd::Failed(Error)` instead of returning `Err`.** It would preserve the aggregate across a
failure, which is real value. It loses on two counts: `Error` is neither `Clone` nor `PartialEq`, so
it would cost `DrainReport` both derives and the ability to be asserted on directly in a conformance
case; and it makes a failure something a caller can ignore by not matching on it. The existing trade
is the one `sync_once` already makes for cancellation — the outcomes commit, the return value is
lost — and prior passes' work is readable from the store either way.

## Consequences

- **Four conformance cases, and case 58 is the one that matters.** It puts a wedged head behind a
  bound of three, drains, and asserts a send count of **one**. That assertion fails against the
  naive reading of decision 018, which is the point of writing it down. It then drains three more
  times to show the bound is still reached — across drains, one attempt each, which is the behaviour
  the caller's cadence is meant to control.
- **Case 60 recovers decision 018's estimate.** Five records at `batch_limit = 1` drain in one call
  and five requests, where the source's loop would spread them over five poll intervals. That is
  the ~20 s against ~17 min gap made observable rather than argued.
- **Decision 018's `## Implementation Outcome` needs correcting**, since it credits 017 with
  preventing a spin that 017 does not prevent.
- **`Offline` stops after one attempt.** Not a retry storm behind a dropped connection: offline is
  not a failed attempt (decision 004), and a caller waiting for connectivity is not helped by being
  asked again immediately.
- **Two overlapping drains interleave rather than double-send.** `drain` holds no flag of its own;
  each pass takes the same in-flight guard `sync_once` does, so the second drain ends
  `AlreadyRunning` on its first call if it meets a pass mid-flight, and otherwise splits the work.
  Adding a second flag would have been machinery for a case the first flag already covers.

## Revisit If

A server contract appears where retrying an unchanged batch is expected to produce a different
answer — a queue-position or throttle protocol where the same request legitimately resolves on the
second ask. That would make a fruitless pass worth repeating, and the stop condition would have to
move from "made progress" to something the status vocabulary distinguishes. Decision 012's
`Unknown(String)` is where such a status would first appear, which is the place to look for the
evidence.
