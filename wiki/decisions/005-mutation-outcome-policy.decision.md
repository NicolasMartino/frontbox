# Mutation Outcome Policy Retains Blocked Records

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: Sync Semantics
Scope: How frontbox maps server mutation statuses to durable outbox dispositions.
Sources: `raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md`, `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`

## Decision

D1 uses this mapping from server status to local disposition:

| Server status | Meaning | Local disposition |
| --- | --- | --- |
| `Applied` | Server applied the mutation | `Delete` |
| `Duplicate` | Server already saw the mutation id | `Delete` |
| `Rejected` | Server terminally refused the mutation | `DeadLetter` |
| `Blocked` | Server skipped it because an earlier ordered mutation failed terminally | `Retain` |
| `Pending` | Server did not finish processing it | `Retain` |

This deliberately diverges from the copied source for `Blocked`.

## Why

The product sync spec defines `Blocked` as "skipped because an earlier mutation in the same ordered
sequence failed terminally" (`08-offline-sync.spec.md:89`). The ordering section says later
mutations in a correlated sequence may be blocked when an earlier one fails
(`08-offline-sync.spec.md:96-99`).

A blocked mutation was not evaluated on its own merits. Moving it to dead letters would discard
valid work simply because it appeared after a failed record in the same batch.

The source does dead-letter `Blocked` together with `Rejected`
(`persistence/mutations.rs:270`), and the D1 plan originally ported that behavior. Extraction must
treat the spec as normative here and the source as buggy.

The source also loads the entire pending outbox as one batch (`persistence/mutations.rs:513-514`)
with no backend `LIMIT` (`persistence/native.rs:248-269`, `persistence/web.rs:99-115`). If one
early mutation is terminally rejected, every later correlated mutation can come back `Blocked`.
Dead-lettering all of them is a data-loss policy.

## Consequences

- `Blocked` and `Pending` both remain queued in D1.
- `Rejected` is the only status that creates a dead letter.
- Bounded batch size is a D1 design requirement, not a source extraction, to limit the blast radius
  of any one server verdict.
- Retention is unbounded. Nothing in D1 ages, counts, or escalates a record that is retained
  repeatedly, because `mark_attempt` was dropped for having no source counterpart. See Liveness.

## Liveness

Retaining work instead of draining it raises the question of whether the queue still makes progress.
It does for `Blocked`, and the argument is worth recording because an earlier draft of this page got
it backwards.

**`Blocked` self-clears in one sync round.** `Blocked` is only emitted after an earlier mutation
failed *terminally* (`08-offline-sync.spec.md:89,98`). The only terminal status is `Rejected`, and
`Rejected` is dead-lettered and removed from the outbox in the same `apply_outcomes` call. So the
mutation that caused the blocking is gone before the next batch is built, and the previously blocked
records are evaluated on their own merits on the following sync. No user action is required.

An earlier version of this page claimed the opposite: that `Blocked` would repeat "until the user
resolves or removes the earlier rejected mutation." That was wrong. Removal is automatic.

**`Pending` is the status with no bound.** `Pending` means accepted but not confirmed before timeout
(`08-offline-sync.spec.md:90`), and the product contract says to retry it with the same mutation id
(`08-offline-sync.spec.md:125`). A record can therefore stay `Pending` indefinitely if a server job
never settles. Combined with `pending_batch(limit)`, a `Pending` prefix as long as the batch limit
starves every record behind it: the runner reads the same oldest-first window every sync and never
reaches the tail.

This is head-of-line blocking, not deadlock. It resolves when the server settles the jobs. But D1
must make it observable rather than silent, so:

- The sync runner reports when a completed sync produced no `Delete` and no `DeadLetter` outcomes,
  so a caller can surface a stalled queue instead of an unexplained pending count.
- D1 tests cover both cases: `Blocked` clears on the sync after its blocker is dead-lettered, and a
  fully retained batch is reported as no-progress.

Attempt counters, aging, and skip-past policies remain deliberately out of D1. They are the natural
fix if a real product hits a stall, and the no-progress signal is what will show whether that
happens.

## Revisit If

The server contract changes so `Blocked` means "known terminal refusal of this mutation" rather
than "not evaluated because an earlier mutation failed." Until then, retaining is the safer
default.
