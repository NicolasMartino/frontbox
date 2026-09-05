# The Outbox Does Not Partition Below Scope

Document Class: Decision
Status: Accepted 2026-08-30; closes the primary-key question before D5
Date: 2026-08-30
Category: Storage Contract
Scope: Whether the outbox splits into per-type queues below the scope, and therefore whether the durable primary key is `(seq)` or `(partition, seq)`.
Sources: `src/record/mod.rs`, `src/store/mod.rs`, `src/testing/cases/liveness.rs`
Related: `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/018-single-flight-drain-mode.decision.md`, `wiki/references/open-decisions.reference.md`

## Decision

**One queue per scope. The durable primary key is `(seq)`, not `(partition, seq)`.**

## Why

**A mutation has no type to partition on.** `MutationIntent` is a replayable `method`, `path` and
`body`. The cache half can key on `(scope, entity)` because a version is *about* one entity; a write
is not, and one `POST` can touch several — which is why invalidation returns a *set* of stale
entities. Core cannot derive a partition, so the caller would have to declare one, and a
caller-declared partition that is wrong is a silent ordering break.

**It would trade away cross-type ordering for free.** One queue replays the caller's enqueue order
across types at no cost. Split it and "create the list, then create the todo in it" may drain in
either order — reintroducing exactly the failure decision 016's monotonic `seq` was added to
prevent, one level up.

**The problem it solves is real but already bounded.** Head-of-line blocking at `batch_limit = 1`
is case 53, and decision 017's retention bound is what terminates it. Partitioning would narrow the
blast radius; the bound already stops it being permanent. Narrowing is not worth an ordering
guarantee.

## Revisit If

An application demonstrates head-of-line blocking that the retention bound does not adequately
contain — a long-lived `Pending` prefix on one type starving an unrelated one, in a real workload
rather than a constructed case. **The remedy would then be multiple scopes**, which partitions the
queue, the storage and the single-flight claim together and needs no new concept — not a partition
column inside one scope.

## Consequences

- **D5's schemas are simpler**: `seq` alone orders, and the single-flight claim stays per scope
  exactly as decision 031 leaves it.
- **`(scope, seq)` remains globally monotonic**, so an `autoIncrement` object store supplies it with
  no extra round trip, which was decision 016's whole argument.
