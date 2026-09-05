# Build Queued-Write Coalescing For RepForge Adoption

Document Class: Plan
Status: Completed — built 2026-09-05
Date: 2026-09-05
Category: Outbox API
Scope: Implementation plan for safe opt-in same-row queued-write coalescing across core, in-memory, SQLite, and IndexedDB.
Sources: `wiki/proposals/queued-write-coalescing.proposal.md`, `wiki/references/repforge-queued-write-coalescing-request.reference.md`, `src/store/mod.rs`, `src/runner/mod.rs`, `src/record/mod.rs`, `src/transport.rs`, `crates/frontbox-sqlite/src/schema.rs`, `crates/frontbox-sqlite/src/store.rs`, `crates/frontbox-indexeddb/src/backend.rs`, `crates/frontbox-indexeddb/src/convert/mod.rs`, `crates/frontbox-indexeddb/src/store.rs`
Related: `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/026-replayable-preconditions.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/proposals/queued-write-coalescing.proposal.md`

## Goal

Make this RepForge behavior available safely: if a user edits the same profile row twice while the
first edit is still safely queued, frontbox keeps one pending mutation and eventually sends only the
latest body, guarded by the original precondition.

This plan does not implement D6, automatic coalescing, cross-row compression, or create/delete
annihilation.

## Authorization

Every item in the Core Contract below is a design change under `AGENTS.md` — new public types, new
trait methods on `OutboxStore` and `SyncTransport`, and a changed meaning for `pending_batch` — so it
needed the same explicit go-ahead a new deliverable does. **Given 2026-09-05, with the proposal
accepted in the same breath. Built the same day.**

The work sits **outside the D0-D6 deliverable sequence**. It answers an external request rather than
a roadmap step, which is why `wiki/roadmaps/extraction.roadmap.md` carries no deliverable number for
it and why its existence never contradicted "every deliverable with a plan is built".

## What Was Built, And Where It Differs From This Plan

Everything below was implemented as written, with two corrections found while building:

- **`SyncTransport::offline_now`, not `is_offline`.** `Error::is_offline` already exists and asks a
  different question — *did this failure mean offline* rather than *are we offline now*. Two
  `is_offline` in one crate meaning two things is a name a reader has to disambiguate every time.
- **SQLite's migration is guarded by `PRAGMA table_info`, not by the version alone.** A fresh
  database gets the column from the schema batch and still reports `user_version = 0`, so a
  version-only guard would try to add a column that is already there and fail on the first open of
  every new database. The version is still stamped, for the next migration's benefit.

Proof: 80 conformance cases green on all three backends — in-memory and SQLite natively, IndexedDB
in headless Chrome via `just chromedriver` then `just browser`, on ChromeDriver 152.0.7977.82
against Chrome 152.0.7977.76 — plus two SQLite cases the shared suite cannot express. With
`CHROMEDRIVER` exported, `./scripts/verify.sh` reports ALL GATES PASSED and no skips; without it the
browser gate is announced as SKIPPED, which is the script working as designed rather than a
different result.

## Core Contract

- Add `CoalescingPolicy`, `CoalescingEnqueue`, and `CoalescingRefusal` to the public API and export
  them from `src/lib.rs`.
- Add `OutboxStore::enqueue_coalescing(intent, policy)`. It must be atomic with respect to the
  store's pending rows and must either replace exactly one safe record, append one new record, or
  commit nothing.
- Add `OutboxStore::read_for_send(limit) -> Vec<OutboxRecord>`: read up to `limit` pending records in
  `order_key` order **and durably mark each `transport_started`, in one transaction**. The runner
  uses this instead of `pending_batch` to obtain records for transport.
- Add `SyncTransport::offline_now() -> Result<bool, Error>`, defaulting to `Ok(false)`.
- Neither `OutboxStore` method may carry a default implementation. See "Why no defaults" below.
- `pending_batch`, `pending_count`, `sweep_corrupt`, `apply_outcomes`, and `claim_drain` keep their
  present signatures and their present behaviour.

### Why no defaults

`claim_drain` defaults to granted, and `src/store/mod.rs` argues that the default is *correct*
for a backend with one realm by construction rather than merely convenient. Neither new method has
that property. A `read_for_send` defaulting to `pending_batch` would hand back unmarked rows on any
backend that had not implemented it, and `enqueue_coalescing` would then rewrite bodies the server
may already hold — the feature would not fail, it would go quietly unsafe. Required methods make an
unported backend a compile error instead.

`SyncTransport::offline_now` is the opposite case and does take a default, for the reason `claim_drain`
does: an adapter that ignores it gets today's behaviour and *no* coalescing, never unsafe coalescing.

## Store State And Migration

- Store one durable fact per pending row: `transport_started`.
- New ordinary enqueues and new coalescing appends write it `false`.
- Coalescing is permitted only when it is `false`.
- `read_for_send` sets it `true` in the same transaction that reads the batch. Nothing ever clears
  it.

### SQLite: this needs a schema-versioning mechanism the crate does not have

`crates/frontbox-sqlite/src/schema.rs` is a single `CREATE TABLE IF NOT EXISTS` blob. There is no
`PRAGMA user_version`, no `ALTER TABLE`, and no migration step anywhere in the crate. **An existing
database therefore keeps its old `outbox` table and every statement naming the new column fails.**
Adding a column here is not one line; it is the crate's first migration mechanism.

The steps:

- After the `CREATE TABLE IF NOT EXISTS` batch, add the column when `PRAGMA table_info(outbox)` says
  it is absent, then stamp `PRAGMA user_version`. **The column check is the authority, not the
  version**: a fresh database gets the column from the schema batch and still reports
  `user_version = 0`, so driving the `ALTER` off the version alone would try to add a column that is
  already there and fail on the first open of every new database. The version is still stamped, so
  the next migration has a cheap answer that does not mean inspecting every table.
- The column is **nullable and carries no `DEFAULT`**. That keeps `schema.rs:42-48`'s stated rule
  intact — every `INSERT` in the crate names every column, so the value is always supplied by a
  writer rather than by the schema, and a writer that forgets it still fails loudly. `ALTER TABLE ...
  ADD COLUMN ... NOT NULL` would have *required* a `DEFAULT` and broken that rule to add the column
  that enforces it.
- Reads map `NULL` to `transport_started = true`. That is the conservative reading, and it means the
  migration is DDL only: no `UPDATE` walks user data.

### IndexedDB: no version bump, and one narrow exception to the no-`default` rule

`VERSION` does **not** need to move. `crates/frontbox-indexeddb/src/backend.rs:19-34` bumps it to
create object stores inside `onupgradeneeded`; this change adds no store and no index, and IndexedDB
holds whole objects, so a new field simply appears on new writes.

What it does need is an exception to `crates/frontbox-indexeddb/src/convert/mod.rs`, which forbids
`#[serde(default)]` on every field of a durable row. That rule's argument rests on a premise this
change retires: "this crate has never shipped [a different schema], so no such row exists anywhere."
After this, one does.

The exception, and its bound: **a `default` is admissible exactly when the defaulted value is the
conservative one — the value that turns a feature off.** `transport_started` defaults to `true`,
meaning "assume the server may already hold this id", so a row missing the key — whether written by
the older schema or hidden by some future rename — becomes non-coalescible but stays resendable. That
is degradation. Contrast `precondition`, the field the rule was written about: absent there means
conflict detection silently stops, which is loss. Update the module doc to state the exception and
its boundary, so the next field addition has to make the same argument rather than cite the
precedent.

The alternative — bump `VERSION` and walk a cursor rewriting every stored outbox object — is rejected.
`backend.rs:28-33` records that the only migration this crate has run is "the cheap kind: nothing is
rewritten", `create_stores` is a synchronous closure inside the version-change transaction, and the
absence of the key already has a correct reading. Paying for a rewrite to avoid a `default` whose
value is conservative would be the expensive kind of migration bought for nothing.

### In-memory

Adds the same fact to its private row shape. No migration.

## Runner Changes

- `SyncRunner::run` asks `transport.offline_now()` **before** sweeping or reading. `true` returns
  `SyncPass::Offline` with the queue untouched and no durable write — the same report today's offline
  path produces, reached without marking anything.
- Otherwise it uses `read_for_send` in place of `pending_batch`, which marks the batch before the
  transport request is built.
- Everything after that is unchanged. `Error::Offline` from `send_batch` still returns
  `SyncPass::Offline` and still does not count an attempt; any other transport error still returns
  `Err`; a response still goes to `apply_outcomes`.

There is no claim to release, no `in_flight` state, and no recovery pass for abandoned rows. The flag
is set before the request and never cleared, so a crash, a cancelled `use_future`, or a lost response
all leave it already true — which is the reading those cases need. This is why the design sets the
fact before the send rather than deriving it from how the send failed; the proposal's "Durable Send
Knowledge" section carries the argument and the data-loss trace it avoids.

## Replacement Semantics

- Match only decodable pending records in this store's scope with identical `RowRef`, method, and
  path.
- Replace when there is exactly one such record and its `transport_started` is `false`.
- Keep `seq`, `mutation_id`, and `precondition` from the queued record. Take `body`, `op`,
  `traceparent`, and `created_at` from the new intent — all four describe the body, and the body is
  being replaced. `attempts` and `last_error` need no rule: a record that is not `transport_started`
  has received no verdict, so they are already `0` and `None`.
- Return `Replaced { kept, discarded }`, where `kept` is the queued record's mutation id and
  `discarded` is the new intent's.
- Under `AppendIfMissing`, every non-replacement case appends: no `RowRef`, no match, several
  matches, or a match that is already `transport_started`. **This policy never returns `NotQueued`.**
- Under `RequireExisting`, those four cases return `NotQueued` with `Unbound`, `MissingMatch`,
  `AmbiguousMatch`, or `TransportStarted`, and commit nothing.

## Documentation Changes Inside The Crate

Not optional, and not covered by any gate:

- `OutboxStore::pending_batch`'s doc (`src/store/mod.rs`) and the trait-level note on the drain
  handoff (`src/store/mod.rs`) both explain the ordering guarantee as a property of the read the
  runner performs. After this change that read is `read_for_send`. Move the guarantee and leave
  `pending_batch` described as what it now is — inspection.
- `crates/frontbox-indexeddb/src/convert/mod.rs`'s no-`default` module doc gains the exception above.
- `crates/frontbox-sqlite/src/schema.rs`'s "No column carries a `DEFAULT`" note gains the nullable
  `transport_started` column and why it does not weaken the rule.

## Verification

- Add conformance cases for replacement success, append-if-missing across all four of its
  non-replacement paths, require-existing refusals for each `CoalescingRefusal`, and queue-order
  preservation.
- Add a conformance case proving a record that has been marked `transport_started` is not
  coalescible — by either writer of the mark, `read_for_send` or a `Retain` through `apply_outcomes`.
- Add a conformance case for the field split: after a replacement, `seq`, `mutation_id`, and
  `precondition` are the queued record's and `body`, `op`, `traceparent`, and `created_at` are the new
  intent's.
- Add a conformance case that an unbound intent under `AppendIfMissing` is **queued**, not dropped.
- Add runner cases for every way a pass can end with the record still queued, because each is a
  chance to leave the mark unset. All five exist:

  | Path | Case | `attempts` after |
  | --- | --- | --- |
  | The probe answered offline, so nothing was read | 78 | 0 — and the record stays coalescible, the one case where it should |
  | `Error::Offline` from a send that had already been handed the batch | 79 | 0 |
  | An attempted transport failure | 81 | 0 |
  | A response that omitted this record's verdict | 82 | 1 — decision 019 synthesizes a `Retain` |
  | A `Retain` applied by a direct caller that never used `read_for_send` | 80 | 1 |

  The `attempts` column is the reason the mark is a separate fact rather than a derived one: it takes
  three different values across five paths that agree exactly on whether the server may have seen the
  record.
- Add a case for the read/replace interleaving: a coalescing call between `read_for_send` and
  `apply_outcomes` must not leave the newer body deletable by the older body's verdict.
- Add durable-backend reopen cases: a database written before the migration reopens, its rows read as
  `transport_started = true`, they are refused for coalescing, and they still send.
- Run `./scripts/verify.sh`; the feature is not ready while any gate fails. The IndexedDB browser
  gate must be run with `CHROMEDRIVER` set before claiming browser proof — the SQLite reopen case
  does not witness the IndexedDB one.

  **Say how it was run, not just that it was.** `verify.sh` announces this gate as SKIPPED when
  `CHROMEDRIVER` is unset, so a reader running the script sees a skip and a wiki page asserting a
  pass. The reproduction is two commands the repository already carries: `just chromedriver` prints
  an export line for a driver matching the installed Chrome, and `just browser` runs the suite.

## RepForge Adoption Guidance

- A first profile write should use ordinary `enqueue` or `AppendIfMissing` with the current
  server-confirmed precondition.
- A later profile write while the app knows that row has pending work should use `RequireExisting`; if
  it returns `NotQueued`, keep the current refusal/refresh behavior rather than appending a write with
  a guessed precondition.
- **Opt in only where the body carries full row state.** Replacement discards the queued body, so
  coalescing two partial updates drops whatever the first one changed and the second does not mention.
  The profile form qualifies; a `PATCH` that sends only touched fields does not.
- **The adapter must implement `offline_now` or the feature will rarely engage.** Without it, the first
  cadence tick after edit 1 marks the batch and edit 2 appends instead of coalescing. Nothing becomes
  unsafe; the feature just does not fire.
- `Replaced { kept, discarded }` means the mutation id the app minted for edit 2 was never queued. It
  will appear in no `SyncReport::drained` and no `Drained`, so an app correlating its own writes to
  drain reports must map edit 2 onto `kept`.
- Preferences can keep their existing chained-precondition path, or opt into coalescing if RepForge
  wants the same last-write-wins offline UX there.

## Corrections After Review

An external review on 2026-09-05 found one real hole and three real inaccuracies. All are fixed; the
hole is the one worth reading.

**`apply_outcomes` could bypass the mark.** `read_for_send` was treated as the only way a record
reaches the server, but `apply_outcomes` is public and `OutboxStore` says a backend cannot assume the
runner is its only caller. A direct caller reading with `pending_batch`, sending itself, and applying
a `Retain` left the record coalescible after the server had certainly seen it. `Retain` now sets
`transport_started` on all three backends, and **conformance case 80 fails on all three without the
fix** — it was written before the fix and confirmed failing. Decision 044 carries the argument.

**Three documentation claims had gone stale against this feature**, and one of them was self
contradictory: `SyncTransport::offline_now` described itself as producing "the same result as
returning `Error::Offline` from `send_batch`" two paragraphs above explaining why it is not.
`SyncPass::Offline` and `DrainEnd::Offline` said "work is untouched", which is now true of the
queue's contents and false of the durable mark. `Error::Offline` and `Error::is_offline` still read
as observations rather than as the implementor's claim they are.

**SQLite stamped `user_version` unconditionally**, so an older binary opening a newer database wrote
the marker backwards. Now it only moves forward, with a test that fails without the guard.

The review did not catch three things, found while checking it:

- **`sweep_corrupt` no longer runs on every pass.** A pass that ends at `offline_now` touches storage
  not at all, which is the property it exists to have — but `OutboxStore::sweep_corrupt` promised
  that "an application that syncs at all closes the window on its own", and that is now bounded by
  connectivity rather than by the poll interval. The contract says so.
- **`SyncPass::Offline` now covers two different report shapes** — `sent`/`retained`/`quarantined`
  all zero when the probe answered, all non-zero when the send did. Documented on the variant.
- **`StoreOp` carries no `#[non_exhaustive]`**, unlike every other public enum here, and this work
  added two variants to it. Left alone deliberately: adding the attribute is a public-API change
  nobody asked for, and with `publish = false` and no external implementors the present risk is nil.
  Recorded so it is a decision rather than an oversight.

### The Second Review Round

A follow-up review on 2026-09-05 checked the fixes and found no correctness defect left. What it did
find was that **the prose had not caught up with the fix**, in one specific way worth naming:
`apply_outcomes` now sets the mark, but the public contract still defined eligibility as "not read
for sending". The behaviour and its description had diverged, which is the state that produces the
*next* bug — a backend implemented against the doc rather than the suite would reintroduce exactly
the hole case 80 closes. `OutboxStore::enqueue_coalescing`, `CoalescingRefusal::TransportStarted`,
`Disposition::Retain`, and the spec's behaviour table now all define eligibility as the durable mark
and name both of its writers.

The same round found the offline wording still leading with "no request reached the server" in five
places, which is the claim decision 044 spends a section explaining nobody can make. Those now lead
with what is actually known — the transport *reported* the network unavailable — and say why the
weaker phrasing is deliberate.

Two of its findings were about missing coverage rather than wording, and both are now cases: 81 and
82, in the table above.

**One of its findings was wrong, and the correction is worth recording** so it is not "fixed" back:
it read the spec's "78 cases" as a stale count that should say 80 because case 80 exists. Case
numbering is global and stable while the count is not — case 30 is blocking-only and case 45 no
longer exists — so the two numbers were never meant to match. The count is now 80 and the highest
number is 82, and the spec says so explicitly.

## Evidence In The Trial

Added 2026-09-05, after the feature was built, because a library feature with no application using
it is a feature whose ergonomics nobody has tested.

`examples/todo-core` gained `TodoApp::rename_coalescing` beside the existing `rename`, and
**both stay**. A trial carrying only the coalescing path could show the feature working but not what
it changes, and the difference between one queued write and two is the entire claim.

`HttpTransport` implements `offline_now` from the switch it already had. That is the easy case for
the probe and the one worth showing: the plug is *known* to be out, so there is nothing to infer and
no request to attempt.

Four observations, against the real axum/sqlx server that does not depend on `frontbox`:

| # | Claim |
| --- | --- |
| 15 | Two offline renames of one row reach the server as **one** recorded mutation, carrying the later title |
| 16 | The plain `rename` path still queues both and the server records two — the before, without which 15 is measured against nothing |
| 17 | An offline poll between the two renames does not spend coalescibility, so the second still replaces |
| 17b | A poll that *did* reach transport does spend it, and the second rename appends |

15 and 16 are the same user sequence through the two paths, and the end state agrees in both: the
later title wins either way. What coalescing buys is the request that never happened, not a different
answer. 17 and 17b are the two halves of why `offline_now` exists — without it 17 fails, and 17b is
the case that must keep failing to collapse or the feature would be unsafe.

### What the trial does not demonstrate

**`RequireExisting` has no witness here, and the trial says so rather than manufacturing one.** This
application sends no preconditions at all. RepForge's difficulty is that a profile row's hash covers
a server-assigned `updated_at`, so the state write 1 will produce is unknowable until write 1 drains
— and nothing in a todo list has that shape. A fixture arranged to produce it would be flattering the
feature rather than exercising an application that needed it. `RequireExisting` is covered by
conformance cases 72, 74, 75, and 76 and by the argument in decision 044; it is not covered by
end-to-end evidence.

### The UI change is compile-checked, not test-checked

`examples/todo-app`'s rename now calls `rename_coalescing`, and its `onchange`-not-`oninput` comment
— which described a queued write per keystroke as the reason for the choice — now says why the choice
stands anyway.

**That crate has no tests, so this is proven by `cargo clippy` and two wasm builds and nothing else.**
It is the same seam `wiki/plans/d4a-offline-todo-trial.plan.md`'s Finding 6 records: both halves
individually green, and nothing looking at the join. Recorded here rather than left to be discovered.

## Close Criteria

The plan closes when the public API is implemented on all three backends, RepForge can delete the
profile form's second-write refusal for the coalescible case, all conformance and durable reopen tests
pass, the in-crate documentation listed above is corrected, and the wiki promotes the accepted
behavior into `wiki/specs/frontbox-runtime.spec.md` and a durable decision covering the
`transport_started` ordering rule and the `offline_now` probe.

**Closed 2026-09-05.** All of it, plus the trial adoption above, which the plan did not originally
ask for.
