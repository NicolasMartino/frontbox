# Mutation Outcome Policy Retains Blocked Records

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: Sync Semantics
Scope: How frontbox maps server mutation statuses to durable outbox dispositions.
Sources: `raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md`, `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/plans/prior-art-survey.plan.md`, `wiki/references/prior-art-survey.reference.md`

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

## Prior-Art Support

Added 2026-08-26 from D0a (`wiki/references/prior-art-survey.reference.md`). This decision has the
strongest external support of any in the project, and the survey did not contradict it.

**The terminal-versus-transient split is near-universal.** Replicache's server-push reference
states the rule frontbox arrived at independently:

> "If a mutation is invalid or cannot be handled, the server must still mark the mutation as
> processed by updating the `lastMutationID`. Otherwise, the client will keep trying to send the
> mutation and be blocked forever."

> "If a permanent error is encountered such that the mutation will never be appliable, ignore that
> mutation and increment the `lastMutationID`. If a temporary error is encountered that might be
> resolved on retry, halt processing mutations and return."

Redux Offline resolves every request to "success and commit, temporary failure and retry, and
permanent failure and rollback." Zero skips throwing mutators, reverts the optimistic effect, and
lets later mutations proceed. Amplify DataStore's conflict handler returns a `DISCARD` sentinel.
PowerSync keeps transient failures queued and asks backends to return 2xx for validation failures
so the queue is not blocked.

**frontbox's version is the most recoverable in the cohort.** All four peers above *discard* or
*roll back* the terminal case. frontbox dead-letters it, preserving the record for inspection and
user action. That is a deliberate improvement, not a divergence.

**Workbox Background Sync is the counterexample that proves the point.** It has no
terminal/transient distinction: a failed request "is put back in the same position in the queue,"
indefinitely, bounded only by `maxRetentionTime`. That is exactly the head-of-line wedge this
decision exists to prevent, shipped in the browser platform's own background-sync library.

**Retaining `Blocked` is independently supported.** Electric documents its own rejected-write
rollback as "very naive... clearing all local state and writes in the event of any write being
rejected by the server," and suggests implementers may prefer clearing "only the set of writes that
are causally dependent on the rejected operation." A `Blocked` record is precisely a write that was
never evaluated on its own merits, so dead-lettering it would be the naive behavior Electric warns
against.

**The unbounded-retention gap has shipped reference designs.** This decision deliberately leaves
out attempt counters and aging. Prior art shows what a bound looks like when frontbox wants one:
Redux Offline's `retry()` returns a delay or "`null` if the action should be discarded" over a
1s-to-1h schedule, after which "it will be discarded"; Workbox uses `maxRetentionTime`; Amplify
emits `outboxStatus{isEmpty}` plus enqueued/processed/failed events. If frontbox adopts a bound it
should dead-letter at that bound rather than discard, consistent with the policy above.

**One caution on status-code classification.** `Rejected` is a server verdict, not an HTTP status
class, and it must stay that way. Redux Offline's default treats all 4xx as permanent, and its own
documentation immediately overrides that for `401`, refreshing the token and retrying. See
`wiki/decisions/004-transport-auth-and-offline.decision.md`.

## Revisit If

The server contract changes so `Blocked` means "known terminal refusal of this mutation" rather
than "not evaluated because an earlier mutation failed." Until then, retaining is the safer
default.
