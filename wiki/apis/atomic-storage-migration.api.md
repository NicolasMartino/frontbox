# Atomic Storage Migration

Document Class: API Spec
Status: Active
Date: 2026-09-26
Category: Storage
Scope: Application-owned format changes across pending mutations, rejected edits and cached rows.
Sources: `src/store/migration.rs`, `src/store/lease.rs`, `src/memory/migration.rs`, `crates/frontbox-sqlite/src/migration.rs`, `crates/frontbox-indexeddb/src/migration.rs`, `src/testing/cases/migration.rs`, `crates/frontbox-indexeddb/tests/cross_realm.rs`; RepForge user approval XVI, 2026-09-26.
Related: `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/044-transport-started-before-the-request.decision.md`

## Contract

`MigrationStore::migrate` accepts synchronous `StorageMigration` callbacks for
pending records, dead letters and cached rows. All reads, validation and writes
share one scoped transaction. A callback returning `None` leaves its record
unchanged. Any callback, decoding, collision or storage error commits nothing.
Memory, SQLite and IndexedDB implement the same contract.

The return is `None` when the scope is busy, otherwise a `MigrationReport`
counting the changes actually committed. No callback runs on a busy scope.
`DrainLease::claim` acquires both core's in-realm slot and the backend lease;
both `SyncRunner` and migration implementations use it. IndexedDB uses the
existing origin-wide Web Lock, including against a separate browser realm.

The editable mutation payload contains the body, operation metadata and guard.
Everything else is retained in place: sequence, method, path, creation time,
trace context, row binding, attempts, last error and transport-started mark.
Dead letters retain their identifier, rejection time and rejection reason.
Their payload may be converted for current review code, but conversion neither
requeues them nor fabricates a fresh server verdict. Cached rows retain their
keys and stale flags. Other scopes, quarantine and cache versions are untouched.

`PendingMigration::local` preserves the identifier. The caller must establish
that the transport still emits the same request; core cannot interpret opaque
JSON or an application-specific transport. `PendingMigration::new_request`
requires a fresh identifier when the outgoing request changes. It must differ
from all pending and rejected identifiers in the scope and all other replacement
identifiers in this migration. The original slot and attempt history survive.

## Application obligations

- Transformations are pure, deterministic, synchronous and idempotent. Unknown
  application versions return an error, rather than guessing a format.
- Derive replacement identifiers from the original identity and migration
  version. Preserve unknown conflict bases as unknown; never invent a base.
- Stop old-format writers before migrating. Transactional exclusion cannot
  prevent an old client from writing again after the transaction commits.
- Retry a busy scope later. Do not begin hydration or draining with a codec
  that cannot read that scope until migration succeeds.
- Unswept corruption prevents migration. An application may first use the
  existing explicit corruption sweep; migration itself never drops a record.
- Cancellation before commit rolls back. Cancellation during commit can leave
  the complete old or complete new state, so repeatability remains required.

This is an additive API; existing store implementors need not implement the
new optional trait. It changes no physical storage schema and adds no runtime
or framework dependency. Calling `claim_drain` alone remains the backend half;
custom orchestration needing both forms of exclusion uses `DrainLease::claim`.

## Local proof, 2026-09-26

Cases 83–90 run against all three backends. They cover mixed parent/child order,
audit preservation, reopen and repeat, application rejection, account and
quarantine isolation, identifier collisions, drain exclusion, unknown versions,
unswept corruption, failure after writes before commit, and a started request
with no received verdict. A browser worker also holds the drain lock while a
migration attempts to start, proving the cross-realm refusal and later release.

`scripts/verify.sh` was run locally. Core and backend tests, native and WASM
Clippy, browser IndexedDB tests, framework adapter tests, server/client trial
tests and the core coverage gate passed (89.83% regions; 97.68% lines).
Five desktop/docs gates could not build because this Linux environment lacks
GLib/GTK development metadata. iOS and Android gates were explicitly skipped
for absent SDKs. These are not a claim that the complete platform matrix passed.
The changed public API also receives targeted Rustdoc checks for all three
storage crates, including the IndexedDB WASM target.

This change is local and unpublished. RepForge integration is tested with a
temporary Cargo override; no machine-specific dependency path is a release pin.
