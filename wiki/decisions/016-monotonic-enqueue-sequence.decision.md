# Enqueue Order Is Recorded, Not Inferred

Document Class: Decision
Status: Accepted 2026-08-27; implemented 2026-08-29
Date: 2026-08-27
Category: Sync Semantics
Scope: What determines the order in which pending mutations are sent, and where that order is stored.
Sources: `src/store/mod.rs`, `src/runner/mod.rs`, `src/record/mod.rs`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/proposals/single-flight-drain.proposal.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

`OutboxStore::enqueue` assigns a durable, monotonically increasing sequence number, and
`pending_batch` orders by it. `(created_at, mutation_id)` stops being the ordering basis and becomes
a tiebreak for rows written before the column existed.

Four properties, each of which is load-bearing:

- **Assigned by the store, in the insert's own transaction.** The caller has no field to put one in,
  exactly as it has none for `ScopeKey` (decision 009). Only the store knows what it has already
  issued, so only the store can issue.
- **Globally monotonic, not per-scope.** Reads are scope-filtered already, so a globally monotonic
  sequence is monotonic within every scope for free.
- **Durable across restart.** A counter that resets to zero re-orders the queue, which is the exact
  failure the sequence exists to prevent.
- **Carried on `OutboxRecord`, not on `MutationIntent`.** The same split decision 009 draws for
  scope, for the same reason: a field the caller cannot write is a field the caller cannot forge.

## Why

**The gap is already conceded, in `pending_batch`'s own doc comment:**

> This buys determinism, not causality. Real causal ordering needs a monotonic sequence assigned at
> enqueue, which is a durable-storage concern and is not claimed here.

`created_at` is a client clock reading, and same-millisecond ties break on `mutation_id`, which for
a v4 UUID is arbitrary. Two mutations enqueued in the same millisecond can therefore be sent in the
wrong order. RepForge's worked case is a rapid create-session, add-exercise, log-set chain where the
set can sort ahead of the session and arrive against a parent that does not exist.

**Batching never protected against this, which is why it is a bug fix rather than the price of
single-flight.** RepForge presented the sequence as the cost of moving to one mutation per request,
on the reasoning that a batch is evaluated in request order and so survives a bad sort. It does not.
`runner.rs:263-264` builds the request from `pending_batch` order:

```rust
let request =
    MutationBatchRequest::new(records.iter().map(|record| record.to_intent()).collect());
```

A mis-sorted queue is therefore a mis-*ordered batch*, and a server evaluating in request order
meets the set before the session at `batch_limit = 100` exactly as it would at 1. The hazard exists
in the crate today. What single-flight changes is nothing about the defect and everything about how
visible it is.

This matters for how the change is priced. It is owed whether or not `batch_limit = 1` is ever used,
and it should not be counted against that configuration.

**Global rather than per-scope, because per-scope is the expensive way to buy the same guarantee.**
Decision 009 already filters every read by scope. A total order restricted to a subset is still a
total order on that subset, so global monotonicity gives per-scope monotonicity with nothing added.
Requiring per-scope counters would mean a durable counter row per scope, read-modify-written inside
every enqueue — a second round trip on the web backend — to buy a property that falls out of the
cheaper design.

That also answers RepForge's first question. The hard part of "a durable per-scope monotonic
sequence in IndexedDB without a second round trip" is the *per-scope* part; drop it and an
`autoIncrement` object store supplies the rest in the insert's own transaction.

**Sequence numbers do not make ordering causal.** They make it *faithful*: the queue is drained in
the order the application enqueued. Whether enqueue order matches causal order is the application's
property, and core cannot check it. An application that enqueues a child before its parent gets what
it asked for. This is worth stating because "causal ordering" is what RepForge's proposal calls it,
and the crate should not claim a guarantee it cannot verify.

## Consequences

- **`OutboxRecord` gains a field**, which is additive because decision 008 made every record
  `#[non_exhaustive]` and constructor-built. `OutboxRecord::stamp` becomes the store's, taking the
  sequence alongside the scope.
- **D5 gains a column and an index**: `seq`, unique, the primary sort key. The in-memory backend
  needs it too, so the conformance suite can assert the behaviour before D5 exists — the same reason
  `InMemoryBackend` exists at all.
- **Two conformance cases are owed.** Two records enqueued at the same `created_at` come back in
  enqueue order, not UUID order — which is the case `(created_at, mutation_id)` gets wrong and which
  no existing case covers. And the order survives reopening the store, which is what makes the
  counter durable rather than a process-lifetime convenience.
- **`pending_batch`'s determinism note is superseded**, not merely amended. The paragraph explaining
  that the compound key buys determinism instead of causality describes the behaviour this decision
  replaces.
- **Skip-past becomes unavailable as a liveness mechanism**, which is why decision 017 exists.
  Stepping over a stuck record to reach the one behind it is precisely the reordering this decision
  forbids, so the two are a package: 016 removes the cheap fix for a wedged queue and 017 supplies
  the only remaining one.
- **`u64` does not wrap in any realistic lifetime** — one enqueue per nanosecond for five centuries
  stays inside it — so no wrap policy is specified. Saying so is cheaper than leaving a reader to
  wonder.
- **This closes the open item** *"Decide whether mutation ordering needs a monotonic sequence number
  before durable backends"* (`wiki/index.md`, Open Work), which has been open since D0a and was
  raised there on one precedent. It is now settled on a defect in this crate rather than on prior
  art.

## Implementation Outcome

**Built 2026-08-29 as written**, with two things the decision did not anticipate.

**Two existing cases asserted the defect and were rewritten in place**, not adapted. Case 11 asserted
that records come back in *timestamp* order and case 12 that same-millisecond records break on the
UUID — which is precisely the behaviour this decision removes. They keep their numbers, as case 37
did under decision 021, because descending timestamps and a same-millisecond tie are exactly where
the old rule and the new one disagree. Case 12 is now the case this decision's Consequences called
for. Case 46 is new and covers the other one: enqueue order survives a reopen, with the counter
resuming above what it issued rather than restarting.

**`order_key` returns a triple, not a pair.** `(seq, created_at, mutation_id)` — the sequence is the
order, and the remaining two are the tiebreak this decision specified for pre-column rows, which
read as `seq` zero. They keep the order total on a partially migrated store and are reached in no
other case.

One detail worth recording for D5: `insert_raw_row`, the corruption test affordance, also takes a
sequence. Skipping it would let a corrupt row sort ahead of everything written before it, and
quarantine reads the same ordered store.

## Revisit If

An application needs a partial order rather than a total one. The per-service `PUT` shape makes this
concrete: with four service origins, a strict global sequence serializes a `billing` write behind a
`workout` write that has nothing to do with it, and a stuck record on one origin holds up every
other. Parallel per-origin drain would need either per-origin sequences or explicit dependency edges
between mutations, and both are larger designs than this one. They become worth their cost if drain
latency turns out to matter more than the simplicity of one queue.
