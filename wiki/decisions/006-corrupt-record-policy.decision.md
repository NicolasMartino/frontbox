# Corrupt Local Records Are Quarantined, Not Silently Dropped

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: Storage Semantics
Scope: How frontbox handles local outbox and dead-letter records that cannot be decoded or converted into sync intents.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`
Related: `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`

## Decision

The library needs an explicit corrupt-record path. D1 should expose a quarantine capability for
records that cannot be decoded, parsed, or converted into a sync intent.

At minimum, the core model needs:

- `Error::CorruptRecord { id: Option<MutationId>, reason: String }`
- a storage-level way to identify and move or mark corrupt rows without needing a valid
  `MutationId`
- tests proving that corrupt records do not stay counted as pending forever

The exact store shape can be either a quarantine store or a status column in the outbox backend.
The important requirement is that malformed durable data cannot vanish from reads while still
occupying storage.

## Why

The copied source has several silent-drop paths:

- `sync` drops records that fail `to_intent()` (`persistence/mutations.rs:643`).
- `apply_sync_results` omits records whose `mutation_id` cannot parse
  (`persistence/mutations.rs:252-255`).
- A server result for an unknown mutation is logged but not repaired
  (`persistence/mutations.rs:293-298`).
- Native reads hide row decode errors with `filter_map(|r| r.ok())`
  (`persistence/native.rs:269,382`).
- Web reads hide deserialization failures with `filter_map(...ok())`
  (`persistence/web.rs:112,229`).

That can wedge an offline-first application: a record may remain in storage, continue affecting
counts, never be sent, never be deleted, and never reach a user-visible dead-letter view.

## Consequences

- Store APIs cannot return only valid `OutboxRecord` values if malformed rows are possible; they
  need either a raw-record scan path or a backend-owned quarantine transition.
- Pending counts should distinguish valid pending records from quarantined or corrupt records.
- Quarantined records should preserve enough raw data and error context for diagnostics.
- This is not a user-retry dead letter. Dead letters are server verdicts; quarantine is local data
  integrity failure.

## Revisit If

The implementation chooses a single physical table with status columns rather than separate
outbox/dead-letter/quarantine stores. The policy remains the same: corrupt records must become
visible and recoverable, not silently ignored.
