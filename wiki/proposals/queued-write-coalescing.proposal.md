# Coalesce Queued Writes Only Before Transport Can Have Seen Them

Document Class: Proposal
Status: Proposed
Date: 2026-09-05
Category: Outbox API
Scope: An opt-in outbox operation that replaces the body of one safe same-row queued write, so RepForge can keep only the latest offline profile edit.
Sources: `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `src/store.rs`, `src/record/mod.rs`, `src/runner/mod.rs`, `src/transport.rs`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`
Related: `wiki/plans/queued-write-coalescing.plan.md`, `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `wiki/compatibility/public-dependencies.compat.md`

## Proposal

frontbox should add an opt-in coalescing enqueue operation for row-bound mutations. When a caller
has a newer body for exactly one safe queued mutation on the same row, method, path, and scope, the
store replaces that queued record's body in place. The earlier record's `seq`, `mutation_id`,
`precondition`, `created_at`, `traceparent`, and `op` survive.

The user-visible effect RepForge needs is: profile edit 1 and profile edit 2 while offline become
one eventual send, carrying profile edit 2's body and edit 1's precondition. Other-device conflicts
are still detected because the precondition still names the last server-confirmed state.

## Public Shape

The storage seam should grow an explicit method, not a flag on `MutationIntent`:

```rust
#[non_exhaustive]
pub enum CoalescingPolicy {
    AppendIfMissing,
    RequireExisting,
}

#[non_exhaustive]
pub enum CoalescingEnqueue {
    Appended { mutation_id: MutationId },
    Replaced { kept: MutationId, discarded: MutationId },
    NotQueued { mutation_id: MutationId, reason: CoalescingRefusal },
}

#[non_exhaustive]
pub enum CoalescingRefusal {
    MissingRow,
    MissingMatch,
    AmbiguousMatch,
    UnsafeState,
}

async fn enqueue_coalescing(
    &self,
    intent: MutationIntent,
    policy: CoalescingPolicy,
) -> Result<CoalescingEnqueue, Error>;
```

`AppendIfMissing` appends only when no matching pending record exists. If a matching record exists
but is ambiguous or unsafe to replace, the method returns `NotQueued` and commits nothing. A caller
that wants to append behind an unsafe record must use ordinary `enqueue` and supply a valid
precondition itself.

`RequireExisting` never appends. This is the profile-form path when the application already knows a
profile write is pending and cannot honestly compute a new post-write precondition.

## Replacement Rule

A replacement is allowed only when all of these hold:

- The new intent has a `RowRef`.
- Exactly one decodable pending record in the same scope has the same `RowRef`, method, and path.
- That record is not currently claimed by a drain.
- That record has never been handed to `SyncTransport::send_batch`.

The last condition is the load-bearing correction to RepForge's request. `attempts == 0` is not the
test: a transport error after an attempted request can leave `attempts` at zero while the server may
already have seen the mutation id.

## Durable Send Knowledge

The store needs two durable facts about each pending row:

- `in_flight`: this row has been claimed by a drain pass and must not be rewritten.
- `transport_started`: this row has been handed to transport at least once, or has been recovered
  from an abandoned in-flight state where frontbox cannot prove otherwise.

Coalescing requires `in_flight == false` and `transport_started == false`.

Existing durable rows that do not have these fields must migrate with `transport_started == true`.
That is conservative: an already-queued row remains resendable by idempotency, but frontbox will not
rewrite it under an id the server may already know.

## Consequences

This is larger than one trait method. To make `enqueue_coalescing` safe, the runner cannot keep
using `pending_batch` as the send handoff. It must atomically claim a batch in the store before it
builds the transport request, then release that claim as offline, attempted, or resolved by
`apply_outcomes`.

The feature remains opt-in. Ordinary `enqueue` keeps its existing append contract. No annihilation
is included: create+delete, update+delete, and cross-row compression stay out of scope.

## Revisit If

- RepForge needs create/delete annihilation rather than same-method body replacement.
- A backend cannot add the durable send facts without an unacceptable migration cost.
- A later public storage seam hides runner-only methods behind a sealed internal trait, making the
  public surface smaller without weakening the safety rule.
