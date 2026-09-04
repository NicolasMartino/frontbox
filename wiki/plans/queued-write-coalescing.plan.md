# Build Queued-Write Coalescing For RepForge Adoption

Document Class: Plan
Status: Draft
Date: 2026-09-05
Category: Outbox API
Scope: Implementation plan for safe opt-in same-row queued-write coalescing across core, in-memory, SQLite, and IndexedDB.
Sources: `wiki/proposals/queued-write-coalescing.proposal.md`, `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `src/store.rs`, `src/runner/mod.rs`, `src/record/mod.rs`, `crates/frontbox-sqlite/src/schema.rs`, `crates/frontbox-indexeddb/src/backend.rs`, `crates/frontbox-indexeddb/src/store.rs`
Related: `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/proposals/queued-write-coalescing.proposal.md`

## Goal

Make this RepForge behavior available safely: if a user edits the same profile row twice while the
first edit is still safely queued, frontbox keeps one pending mutation and eventually sends only the
latest body, guarded by the original precondition.

This plan does not implement D6, automatic coalescing, cross-row compression, or create/delete
annihilation.

## Core Contract

- Add `CoalescingPolicy`, `CoalescingEnqueue`, and `CoalescingRefusal` to the public API and export
  them from `src/lib.rs`.
- Add `OutboxStore::enqueue_coalescing(intent, policy)`. It must be atomic with respect to the
  store's pending rows and must either replace exactly one safe record, append one new record, or
  commit nothing.
- Add a runner-facing send claim to `OutboxStore`, because `pending_batch` is read-only inspection:
  `claim_send_batch(limit) -> Vec<OutboxRecord>` and `release_send_batch(ids, release)`, where
  `release` distinguishes "no request attempted" from "transport may have seen it".
- Keep `apply_outcomes`' public signature, but make retained outcomes release claimed rows and mark
  them as transport-started.

## Store State And Migration

- Store each pending row with two durable facts: `in_flight` and `transport_started`.
- New ordinary enqueues start with both facts false.
- Coalescing is permitted only when both facts are false.
- SQLite adds both columns with conservative migration: existing outbox rows get
  `in_flight = false` and `transport_started = true`.
- IndexedDB bumps the database version and migrates existing outbox objects the same way.
- In-memory adds the same two facts to its private row shape; no migration is needed.

## Runner Changes

- `SyncRunner::run` begins by letting the store recover abandoned in-flight rows for the current
  scope as `transport_started = true`.
- It then uses `claim_send_batch` instead of `pending_batch` to obtain records for transport.
- If the transport returns `Error::Offline`, release the claim as "no request attempted" and
  preserve each record's existing `transport_started` value.
- If the transport returns any other error, release the claim as "transport may have seen it" before
  returning the error.
- If the transport returns a response, call `apply_outcomes`; retained rows become
  `transport_started`, terminal rows leave the outbox as they do today.

## Replacement Semantics

- Match only decodable pending records in the same scope with identical `RowRef`, method, and path.
- Missing `RowRef`, no match under `RequireExisting`, multiple matches, and unsafe send state all
  return `NotQueued` without appending.
- Under `AppendIfMissing`, append only when no matching pending record exists at all.
- On replacement, preserve `seq`, existing `mutation_id`, precondition, `created_at`, `traceparent`,
  `op`, attempts, and last error; replace only `body`.
- Return `Replaced { kept, discarded }`, where `kept` is the existing mutation id and `discarded` is
  the new intent's id.

## Verification

- Add conformance cases for replacement success, append-if-missing, require-existing no-op, missing
  row, different row/method/path/scope, ambiguous matches, and queue-order preservation.
- Add conformance cases proving in-flight and transport-started records are not coalescible.
- Add runner cases for offline release, attempted transport failure, retained verdicts, omitted
  verdicts, and abandoned in-flight recovery.
- Add durable-backend reopen/migration cases proving old rows migrate as non-coalescible but still
  resendable.
- Run `./scripts/verify.sh`; the feature is not ready while any gate fails. The IndexedDB browser
  gate must be run with `CHROMEDRIVER` set before claiming browser proof.

## RepForge Adoption Guidance

- A first profile write should use ordinary `enqueue` or `AppendIfMissing` with the current
  server-confirmed precondition.
- A later profile write while the app knows that row has pending work should use `RequireExisting`;
  if it returns `NotQueued`, the app should keep its current refusal/refresh behavior rather than
  append a write with a guessed precondition.
- Preferences can keep their existing chained-precondition path, or opt into coalescing if RepForge
  wants the same last-write-wins offline UX there.

## Close Criteria

The plan closes when the public API is implemented on all three backends, RepForge can delete the
profile form's second-write refusal for the coalescible case, all conformance and durable migration
tests pass, and the wiki promotes the accepted behavior into the runtime spec and a durable
decision.
