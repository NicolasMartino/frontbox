# RepForge Queued-Write Coalescing Request

Document Class: Reference
Status: Recorded from conversation; not filed in `raw/`
Date: 2026-09-05
Category: External Input
Scope: RepForge's request for an opt-in outbox operation that keeps only the latest unsent write for the same row.
Sources: RepForge architecture, "Request to frontbox: collapse queued writes to the same row", received in conversation 2026-09-05.
Related: `wiki/proposals/queued-write-coalescing.proposal.md`, `wiki/plans/queued-write-coalescing.plan.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`

## Provenance

This request was received in conversation, not as a file under `raw/`. The frontbox-side claims
below were checked against this repository. The RepForge file paths named in the request
(`code/frontend/app/src/store/writes.rs` and its tests) are recorded as RepForge-owned evidence and
were not independently inspected in this operation.

## What RepForge Asked For

RepForge wants an opt-in enqueue operation for row-bound mutations. If the outbox already contains
one unsent mutation for the same row, method, path, and scope, the new mutation should replace that
record's body instead of queueing behind it. The collapsed record should keep the existing queue
position, mutation id, and precondition, while taking the newest body.

The product outcome is simple: a user who edits their profile twice while offline should cause only
the second profile body to be sent when connectivity returns. The sent mutation still uses the
precondition captured before the first edit, so another device changing the profile in the meantime
still becomes a conflict.

## The Need, As Claimed

RepForge writes profiles and preferences through the outbox. Both are keyed by user id and both use
version preconditions. Preferences can chain offline because their optimistic projection can compute
the hash the server will see after each local write. Profiles include a server-assigned
`updated_at`, so the client cannot honestly compute the hash that write 1 will produce. A second
offline profile write therefore has no valid precondition to put behind the first one.

The general form is real: any row hash that covers a server-assigned field, version counter, or
server-side normalization can make a later local precondition unknowable until the earlier write has
drained.

## Claims About frontbox, Checked

| Claim | Verdict |
| --- | --- |
| The current `OutboxStore` surface cannot rewrite or remove a pending record except through outcomes | Correct. The trait exposes `claim_drain`, `enqueue`, `pending_batch`, `pending_count`, `sweep_corrupt`, and `apply_outcomes`; a wrapper cannot preserve `seq` and precondition while replacing body |
| `MutationIntent::with_row` already carries the row identity needed for this feature | Correct. `RowRef` is `(entity, row_id)`, opaque and equality-compared |
| Preserving the existing `seq` is the right ordering shape | Correct. Decision 016 makes `seq` the faithful enqueue order; replacing body in place avoids moving unrelated records |
| Preserving the existing precondition is the point of the feature | Correct. Decision 026 says preconditions are caller-supplied, durable, opaque, and replayed unchanged |
| The cache runner already collapses same-entity events last-wins | Correct as a quote, partial as an analogy. Cache invalidation events are observations; queued mutations are commands with idempotency and transport uncertainty |
| `attempts == 0` proves the record has never been sent | Incorrect. Decision 017 says `attempts` counts retaining verdicts. An attempted transport failure can leave `attempts == 0` after the record was handed to transport |
| The store can interlock coalescing with the drain via `claim_drain` | Incomplete. Backend `claim_drain` is visible to the store, but the in-realm `DrainClaim` is private to `SyncRunner`. A safe design needs a store-level send claim, not only the drain lease |
| Create followed by update naturally composes | Narrowly true for same-method, same-path idempotent creates such as `PUT` guarded by `If-None-Match: *`. It is not true for a general `POST` create followed by `PATCH` or `PUT` |

## Resulting frontbox Finding

The request is justified, but the proposed `attempts == 0` eligibility rule is not safe. The store
needs durable knowledge that a record has never been handed to transport. That knowledge must be
separate from `attempts`, because `attempts` deliberately ignores offline passes and attempted
transport failures that produced no readable verdict.

The safe shape is: coalesce only a row-bound pending record whose durable state says it is neither
currently in flight nor ever handed to transport. Existing durable records that predate this marker
must migrate conservatively as "transport may already have started", so they remain resendable but
not coalescible.

**Superseded in detail, not in direction.** This page records the check as it was made on
2026-09-05, and the finding above still holds. What it got wrong is the shape of the remedy: the
first design carried two durable facts and inferred "never sent" from how a send failed, which
`wiki/proposals/queued-write-coalescing.proposal.md` then had to correct for the same reason this
page corrects `attempts == 0`. One fact, written before the request rather than derived from its
failure, does the whole job. Read the proposal for the contract; this page is provenance.
