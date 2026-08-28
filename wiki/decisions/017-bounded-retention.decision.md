# Retention Is Bounded, And The Bound Dead-Letters

Document Class: Decision
Status: Accepted 2026-08-27; implementation not authorized
Date: 2026-08-27
Category: Sync Semantics
Scope: What stops a record that is retained on every pass from being retained forever, and what happens at the bound.
Sources: `src/runner.rs`, `src/store.rs`, `src/record.rs`, `wiki/references/prior-art-survey.reference.md`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/proposals/single-flight-drain.proposal.md`

## Decision

A record that is sent and retained repeatedly is eventually dead-lettered rather than retained
forever.

- **Core counts attempts** on the record. The count is durable, carried on `OutboxRecord`, and
  written by the store.
- **An attempt is a pass where the record was sent and was still queued afterwards.** That includes
  `Blocked`, `Pending`, an unrecognised status, and a verdict the server omitted entirely.
- **It is not an attempt when no usable verdict came back.** `SyncPass::Offline` and an
  attempted-but-failed transport both leave the count alone, but not for the same reason, and the
  difference is worth stating precisely. Offline means the record was never sent. A transport
  failure means a request *was* attempted — decision 004 defines `Error::Transport` as exactly that,
  and the runner's own comment says "a request was attempted and failed" (`src/runner.rs:275`) — so
  the server may well have received and evaluated it, and only the response was lost. What the two
  share is that neither produced a verdict this client can read. The count measures verdicts
  received, not requests made, which is why both leave it alone. A resend is safe in either case
  because `mutation_id` is the idempotency key.
- **At the bound the disposition is `DeadLetter`, never discard**, and the dead letter carries no
  `RemoteRejection`.
- **The bound is caller-configured with no default.** Absent one, retention stays unbounded and D1's
  behaviour is unchanged.
- **There is no skip-past.** The runner does not step over a stuck head to reach the record behind
  it.

## Why

**`batch_limit = 1` converts bounded head-of-line blocking into total head-of-line blocking.**
`pending_batch` is oldest-first with no cursor and no skip, so the next pass reads the same window.
At 100, a retained head starves its window while the other ninety-nine records still go out. At 1,
the window *is* the head: one retained record freezes the entire queue permanently.

Decision 005 already described the mechanism and did not follow it to this endpoint:

> a `Pending` prefix as long as the batch limit starves every record behind it: the runner reads the
> same oldest-first window every sync and never reaches the tail.

At `batch_limit = 1` the prefix length that starves everything is one.

**Decision 012 made the permanent case reachable, and RepForge's own argument concedes it will
fire.** The three retaining statuses are not alike. `Blocked` self-clears in one round, because the
record that caused it was dead-lettered in the same `apply_outcomes`. `Pending` resolves when the
server settles. `Unknown` resolves only when the client is rebuilt with a vocabulary it did not have
— which is to say, never, from the running client's point of view. RepForge's proposal argues that
decision 012 becomes *more* important during the taxonomy migration while also arguing that `Retain`
is unreachable because `Blocked` and `Pending` lose their producers. Both cannot hold: during
exactly the migration window where 012 earns its keep, one unrecognised verdict at the head of a
single-flight queue stops all sync indefinitely.

**Skip-past is unavailable once ordering is load-bearing.** The obvious cheap fix for a wedged head
is to send the record behind it. Decision 016 forbids that: sending records out of enqueue order is
the defect that decision exists to remove, and a skip is a reorder chosen by the queue rather than
by the clock. So the two decisions are a package. 016 removes the cheap liveness mechanism and 017
supplies the only remaining one — terminate the stuck record.

**Dead-letter rather than discard, because that was already promised.** Decision 005's prior-art
section committed to it in advance:

> If frontbox adopts a bound it should dead-letter at that bound rather than discard, consistent
> with the policy above.

Every surveyed peer discards or rolls back — Redux Offline's `retry() -> null`, Workbox's
`maxRetentionTime`, Amplify's `DISCARD` sentinel. Preserving the record for inspection and requeue
is the crate's existing improvement on the cohort and there is no reason to abandon it at the one
moment it matters most.

**A count, not a duration.** Workbox bounds by age, and for frontbox that is the wrong axis: a
record queued for a week because the user was offline has aged without ever being evaluated, and
dead-lettering it would punish the operating mode this crate exists to support. An attempt count
measures what actually happened — the server has now told us the same unusable thing N times — and
it is why the offline and transport-failure paths must not increment.

**Counting an omitted verdict is the uncomfortable part, and it is still right.** A server that
answers but says nothing about this record has not refused it, so counting that as an attempt can
dead-letter work no server ever rejected. Weighed against the alternative: a server that
permanently omits one verdict freezes a single-flight queue forever, and this decision fails at the
job it was written for. The asymmetry decides it. Being wrong in the counting direction produces a
dead letter a human can inspect and requeue; being wrong in the other direction loses every write
behind the head, indefinitely, with the no-progress signal firing and nothing able to act on it.
A caller worried about transient omission sets a high bound.

**No `RemoteRejection` on the dead letter, because the server never rejected it.** `DeadLetter`
already carries `error: Option<RemoteRejection>` and the honest value here is `None`: synthesising a
refusal would put words in the server's mouth, which is the thing decisions 012 and 014 both refuse
to do. The attempt count on the record is what makes the reason legible, so `DeadLetterRecord` needs
to carry it across the transition.

**No default bound, because turning this on silently would change what an existing queue does.**
Dead-lettering is not destructive — the record survives — but it moves work out of the send path
without the caller asking. Decision 005 shipped unbounded retention deliberately, and reversing that
by default is a bigger change than this decision is making. The consequence is that
`batch_limit = 1` without a bound is a configuration with **no liveness argument**, which is exactly
what decision 005 spent its Liveness section refusing to ship; decision 018 makes the pairing a
conformance requirement rather than a suggestion.

A fallible builder that rejected the combination was considered. It was dropped because
`with_batch_limit` and a `with_retention_bound` are order-independent and `#[must_use]`, so the
validation would have to happen inside `sync_once` and surface as a runtime error on a configuration
mistake the type system saw first.

## Consequences

- **`OutboxRecord` gains `attempts`**, additive under decision 008, written by the store during
  `apply_outcomes` rather than by the caller. `DeadLetterRecord` carries it across the transition,
  since without it a dead letter with no `RemoteRejection` is indistinguishable from a refusal the
  server declined to explain.
- **`apply_outcomes` gains a responsibility**, not a signature: a `Retain` outcome now increments,
  which makes `Retain` a write rather than a no-op. This is why the runner already emits `Retain`
  outcomes for records it was ruled on instead of relying on silence — decision 003's atomicity
  requirement covers the increment for free.
- **D5 gains a column**, and the in-memory backend needs it first so conformance can assert the
  bound before a durable backend exists.
- **Conformance cases owed.** A record retained to the bound is dead-lettered with no
  `RemoteRejection` and its attempt count intact; an offline pass does not increment; a pass that
  drains the record resets nothing because the record is gone. The third is worth asserting because
  it is where a partial implementation goes wrong.
- **`is_stalled()` stops being the only signal** and becomes the early one. A caller currently
  learns a queue is wedged and can do nothing about it; after this it learns, and the queue also
  resolves itself.
- **This closes the open item** *"Decide whether retained work needs aging, attempt tracking, or
  last-error metadata"* (`wiki/index.md`, Open Work), which decision 012 had already escalated. Of
  the three, attempt tracking is adopted and aging is rejected above. Last-error metadata is not
  settled here: the attempt count plus the anomaly report covers the diagnosis this decision needs,
  and a durable last-error field is a separate question. **Amended 2026-08-28:** that separate
  question is an observability one, and decision 020 raises its priority — a last error is the only
  proposed field that survives a restart and explains why a record is still queued. Everything else
  in `SyncReport` describes one pass and is gone with the process.
- **Decision 005's "Retention is unbounded" consequence is superseded**, and its statement that
  "attempt counters, aging, and skip-past policies remain deliberately out of D1" was a D1 scoping
  note whose promised revisit this is. Skip-past is now rejected on the merits rather than deferred.

## Revisit If

Per-status bounds turn out to be needed. The three retaining statuses have genuinely different
expected resolution times — `Pending` waits on a server job that will probably settle, `Unknown`
waits on a client rebuild that will not — and one number for both either buries `Pending` work too
early or leaves `Unknown` work queued too long. A single bound is the right first version because it
is the one that can be reasoned about; if real traffic shows the two cases pulling apart, the bound
becomes a function of the status.
