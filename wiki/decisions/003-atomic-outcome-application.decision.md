# Atomic Outcome Application Replaces Remove-Plus-Insert

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: API Shape
Scope: How the outbox applies server verdicts, and why dead-lettering and deletion must commit together.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/references/prior-art-survey.reference.md`

## Decision

The outbox trait exposes one transactional method for applying a batch of verdicts, replacing
separate `delete` and `dead-letter insert` calls.

```rust
pub enum Disposition {
    Delete,
    DeadLetter { error: Option<RemoteRejection> },
    Retain,
    Quarantine { reason: String },
}

pub struct Outcome {
    pub id: MutationId,
    pub disposition: Disposition,
}

/// All deletions and dead-letter inserts commit together, or none do.
async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error>;
```

`DeadLetterStore` deliberately exposes no public `insert`. Dead letters are created through
`apply_outcomes`, where the backend still has the corresponding outbox record and can transition
state atomically. `QuarantineStore` follows the same rule for the same reason.

`Quarantine` covers records that are identifiable but undecodable, per
`wiki/decisions/006-corrupt-record-policy.decision.md`. Rows with no usable `MutationId` cannot be
expressed as an `Outcome` at all and go through `OutboxStore::sweep_corrupt` instead. The
authoritative trait signatures live in `wiki/plans/d1-core-cache-runtime.plan.md`; the snippet above
shows only what this decision constrains.

## Why

The source performs the transition in two disconnected steps and drops errors from the second.

1. `apply_sync_results` inserts the dead letter and records the id for later deletion
   (`persistence/mutations.rs:270-289`).
2. Back in `sync`, deletion happens in a separate loop that discards its result:
   `let _ = OutboxStore::delete(&self.inner.db, mutation_id).await;`
   (`persistence/mutations.rs:658`).

If the dead-letter insert succeeds and the outbox delete fails, the record is both pending and
dead-lettered. The next sync re-sends it and the server rules on it again.

The earlier version of this decision claimed the second retry would fail because the dead-letter
primary key was already occupied. That was wrong. Both target backends upsert dead letters:
SQLite uses `INSERT OR REPLACE` (`persistence/native.rs:390`), and IndexedDB uses `put`
(`persistence/web.rs:246`). The real consequence is quieter: the pending record is re-sent every
sync, the dead letter is rewritten, and `rejected_at` can keep moving forward so retention never
purges it.

The problem is still serious because it is a state-transition bug. The trait boundary should make
the invalid intermediate state unrepresentable for backend implementors.

## Feasibility

Both target backends can support the atomic transition.

- SQLite can wrap the delete and insert operations in one transaction.
- IndexedDB transactions can span multiple stores; the source helper currently passes one store
  name at a time, but the `rexie` transaction API accepts a store-name slice
  (`persistence/web.rs:20-26`).

## Consequences

- Outcome application is batch-shaped rather than per-record, avoiding the source's N-delete loop.
- `Retain` is explicit, so a backend can verify that every known outcome was considered.
- Test backends need failure injection to prove atomic rollback.
- Backends that cannot span the outbox and dead-letter stores transactionally must use a
  single-store representation or be excluded from the durable backend set.

## Revisit If

A target storage engine cannot atomically transition across two logical stores. The acceptable
fallback is a single physical table with a status column, not a return to unchecked two-step
writes.

## Prior-Art Support

Added 2026-08-26 from D0a (`wiki/references/prior-art-survey.reference.md`). The survey did not
contradict this decision and supplies two independent supports:

- **WatermelonDB** documents the same hazard from the opposite side. Its sync implementation warns
  "do not mark record as synced if it changed locally since fetch local changes step (user could
  have made new changes that need syncing)." Applying an outcome must not clear state the caller
  has modified since the batch was built. Its backend contract likewise requires a push to abort
  transactionally rather than partially apply, and notes that unsafe per-collection batching breaks
  that transactionality.
- **RxDB** requires backends to tolerate duplicate transmissions, because retries after a partial
  failure are expected. Atomic application plus idempotence is the combination that survives a
  torn write.

No surveyed system applies outcomes as a non-transactional remove-plus-insert. This decision stands
as written.
