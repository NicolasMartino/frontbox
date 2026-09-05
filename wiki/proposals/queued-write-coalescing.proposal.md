# Coalesce Queued Writes Only Before Transport Can Have Seen Them

Document Class: Proposal
Status: Accepted 2026-09-05; built 2026-09-05 (decision 044)
Date: 2026-09-05
Category: Outbox API
Scope: An opt-in outbox operation that replaces the body of one safe same-row queued write, so RepForge can keep only the latest offline profile edit.
Sources: `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `src/store/mod.rs`, `src/record/mod.rs`, `src/runner/mod.rs`, `src/transport.rs`, `src/error.rs`, `crates/frontbox-indexeddb/src/convert/mod.rs`, `crates/frontbox-sqlite/src/schema.rs`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`
Related: `wiki/decisions/044-transport-started-before-the-request.decision.md`, `wiki/plans/queued-write-coalescing.plan.md`, `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `wiki/compatibility/public-dependencies.compat.md`

## Proposal

frontbox should add an opt-in coalescing enqueue operation for row-bound mutations. When a caller
has a newer body for exactly one safe queued mutation on the same row, method, and path, the store
replaces that queued record's body in place, keeping the record's queue position and its
precondition.

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
    Unbound,
    MissingMatch,
    AmbiguousMatch,
    TransportStarted,
}

async fn enqueue_coalescing(
    &self,
    intent: MutationIntent,
    policy: CoalescingPolicy,
) -> Result<CoalescingEnqueue, Error>;
```

**`AppendIfMissing` never returns `NotQueued`.** It replaces when there is exactly one safe match and
appends in every other case — no `RowRef` on the intent, no match, several matches, or a match that
is no longer safe to rewrite. The caller under this policy has said it holds a valid precondition, so
appending is the honest fallback and is exactly what `enqueue` would have done.

That is a deliberate correction to the first draft of this proposal, which returned `NotQueued` and
committed nothing when the intent carried no `RowRef`. A caller who forgets `with_row` would have got
`Ok(..)` back for a write that was silently dropped — the failure this crate spends
`OutboxStore::sweep_corrupt` and `wiki/decisions/006-corrupt-record-policy.decision.md` refusing to
commit anywhere else.

`RequireExisting` never appends, and is the only policy that can return `NotQueued`. This is the
profile-form path, where the application already knows a profile write is pending and cannot honestly
compute a new post-write precondition. A refusal means the caller must fall back to its own
refresh-or-refuse behaviour; it must not retry through `enqueue` with a guessed precondition.

## Replacement Rule

A replacement is allowed only when all of these hold:

- The new intent has a `RowRef`.
- Exactly one decodable pending record in this store's scope has the same `RowRef`, method, and path.
- That record is not marked `transport_started`.

The last condition is the load-bearing correction to RepForge's request. It is one durable boolean
with **two** writers, because there are two ways a record reaches the server: `read_for_send` marks
what it hands to the transport, and `apply_outcomes` marks when it applies a `Retain`, since a
verdict cannot exist without a request. The first draft of this proposal named only the first, and
review found a caller could reach the second without ever touching the first — see decision 044's
*Every Verdict Spends It, Not Only Every Read* and conformance case 80. `attempts == 0` is not the
test: `src/record/mod.rs:277-284` makes `attempts` a count of *verdicts received*, so an offline pass
and an attempted transport failure both leave it at zero while the server may already hold the
mutation id.

### What survives, and what the new intent supplies

The queued slot and its guard are the queue's. The content is the caller's latest.

| Kept from the queued record | Taken from the new intent |
| --- | --- |
| `seq`, `mutation_id`, `precondition` | `body`, `op`, `traceparent`, `created_at` |

`seq` keeps the record where decision 016 put it. `mutation_id` keeps idempotency stable. The
precondition is the whole point of the feature: it names the last state the server confirmed, which
is the only state a conflict can honestly be detected against.

Everything in the right-hand column **describes the body**, and the body is being replaced. Keeping
edit 1's `op` would put edit 1's operation name and `OperationMeta::version` — documented at
`src/record/mod.rs:48` as "what wrote the body" — over a body edit 1 did not write, and `op` exists
so a human reading a dead letter can tell what a mutation was. Keeping edit 1's `traceparent` would
point the trace at the user action whose body is being discarded, which is the causal link decision
022 exists to preserve. Keeping edit 1's `created_at` would send edit 2's body to the server stamped
`client_datetime` of edit 1. None of the four is safe to order on, because `seq` is the only sort key
(`src/record/mod.rs:340-355`), so nothing is lost by taking the newer ones.

`attempts` and `last_error` need no rule. A record that is not `transport_started` has received
no verdict, so both are already `0` and `None`.

### Replacement discards the queued body, and that is not always what a caller wants

For a full-state `PUT` this is last-write-wins and correct — it is the profile case. For a partial
update it is not: coalescing two `PATCH` bodies drops every field the first one changed that the
second does not mention.

The contract must say so, because **the caller is in no position to work it out**. Under
`RequireExisting` the application does not know what body is queued — not knowing is the stated reason
it cannot compute a precondition. So the obligation belongs on this method: *opt in only where your
bodies carry full row state.* The reference page already reached the neighbouring finding, that
create-then-update composes only for same-method idempotent creates and not for a general `POST` then
`PATCH`; this is the same edge one step along, and it did not survive into the first draft of this
rule.

## Durable Send Knowledge

The store needs one durable fact per pending row:

- `transport_started`: this row has been read for the purpose of sending.

Coalescing requires `transport_started == false`. It is written **inside the transaction that reads
the batch**, before the runner builds the transport request, and is never cleared.

### Why one field and not two, and why it is set before the send rather than after

The first draft of this proposal carried two facts — `in_flight` and `transport_started` — plus a
three-way release taxonomy and a recovery pass for abandoned in-flight rows. All of it existed to
serve one rule: that a pass ending in `Error::Offline` released its rows with `transport_started`
still false, so an offline application could keep coalescing.

That rule is unsound, and it is unsound in exactly the way `attempts == 0` is. `src/transport.rs:22-23`
tells implementors to return `Offline` when "a browser `fetch` fails for lack of connectivity" — and a
browser `fetch` rejects with an opaque `TypeError` whether the request never left the device or
reached the server and lost its response. An adapter cannot tell those apart. So:

1. Edit 1 is queued as `M` with `transport_started = false`.
2. A pass sends it. The server applies edit 1 under `M`. The response is lost. The adapter returns
   `Offline`.
3. The rule preserves `transport_started = false`.
4. Edit 2 coalesces: body replaced, `M` kept.
5. The next pass sends `M` carrying edit 2's body. The server dedupes on `mutation_id`, answers
   `Duplicate`, and the record is deleted. **Edit 2 is gone, silently.**

Today that same ambiguity costs nothing — the record stays queued and idempotency covers the resend.
Building coalescing on it converts a tolerated under-count into silent loss of the user's most recent
edit.

Setting the flag before the request removes the inference entirely: no classification of a failure is
required, because the fact is recorded before there is anything to classify. A crash, a cancellation,
or a lost response all leave the flag already true, which is the conservative reading and needs no
recovery pass to restore. `in_flight` then has nothing left to do — under this ordering a claimed row
is a started row — and the release taxonomy has nothing left to decide.

### The runner must not read for sending while the application knows it is offline

Setting the flag at read time has one cost, and it is the one that decides whether this feature works
at all: a cadence loop polling while offline would mark the head batch started on its first tick, and
the motivating scenario — two edits made *while offline* — would never coalesce.

So `SyncTransport` should gain a connectivity question the runner asks **before** it reads a batch:

```rust
async fn offline_now(&self) -> Result<bool, Error> { Ok(false) }
```

A pass that gets `true` sends nothing, reads nothing, and reports `SyncPass::Offline` with the queue
untouched — the same report it produces today, reached without touching the durable flag.

**The probe's failure mode is one-sided, which is what makes it safe to rely on.** Saying "offline"
while online costs a skipped pass that the next tick recovers. Saying "online" while offline costs a
burned coalescibility and nothing else, because the runner then goes on to mark the batch before
sending. There is no answer it can give that lets a body be rewritten under an id the server may
hold.

The default is `false`, and that default is honest in the way `claim_drain`'s granted default is: an
adapter that does not implement it degrades to *no coalescing*, never to unsafe coalescing.

## Consequences

This is larger than one trait method, but smaller than the first draft claimed.

`OutboxStore` gains `enqueue_coalescing` and one read-and-mark method — call it `read_for_send(limit)`
— which the runner uses in place of `pending_batch` to obtain records for transport. The read and the
mark must be one transaction: a plain `pending_batch` followed by a separate marking write would let a
coalescing replacement land in between, so the runner would send the body it had already read while
the store held a newer one, and the server's `Applied` would delete the newer body with it.

Neither new method may be defaulted. A `read_for_send` that fell back to `pending_batch` would return
unmarked rows on a backend that had not implemented it, and coalescing would go quietly unsafe there —
the opposite of `claim_drain`, whose granted default is *correct* for a single-realm backend rather
than merely permissive. `pending_batch` keeps its present contract and becomes inspection-only, so its
documentation and `OutboxStore`'s trait-level note about the drain handoff need updating to say where
the ordering guarantee now lives.

`enqueue_coalescing` and `read_for_send` must serialize against each other in the backend. Both are
read-write transactions over the outbox in SQLite and IndexedDB, which supplies this; a backend
tempted to make the eligibility check a read-only transaction would lose it, because IndexedDB runs
read-only transactions concurrently with read-write ones.

The feature remains opt-in. Ordinary `enqueue` keeps its existing append contract. No annihilation is
included: create+delete, update+delete, and cross-row compression stay out of scope.

## What This Costs A Caller That Does Not Want It

Nothing durable and nothing at runtime, which is the test this proposal has to pass before it earns a
schema field. `transport_started` is one boolean on a pending row, and pending rows are the shortest
lived thing this crate stores. `offline_now` defaults to `false`, so an adapter that ignores it behaves
exactly as it does today. `read_for_send` replaces a read the runner already performed.

## Revisit If

- RepForge needs create/delete annihilation rather than same-method body replacement.
- The one-sided connectivity probe turns out to be one-sided in practice only for browsers, and a
  native adapter can be offline in a way it cannot observe before attempting a request.
- A later public storage seam hides runner-only methods behind a sealed internal trait, making the
  public surface smaller without weakening the safety rule.
