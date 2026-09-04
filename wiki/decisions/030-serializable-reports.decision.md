# Report Types Serialize And Never Deserialize

Document Class: Decision
Status: Accepted 2026-08-29; implemented 2026-08-29
Date: 2026-08-29
Category: Public API Shape
Scope: Whether the diagnostic types this crate returns can leave the process, and in which direction.
Sources: `src/runner/report.rs`, `src/runner/drain.rs`, `wiki/compatibility/public-dependencies.compat.md`
Related: `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/029-drain-termination.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/references/open-decisions.reference.md`

## Decision

`SyncReport`, `SyncOutcomeCounts`, `SyncPass`, `Anomaly`, `AnomalyKind`, `DrainReport` and
`DrainEnd` derive `Serialize`. **None of them derives `Deserialize`.**

This closes entry 5 of `wiki/references/open-decisions.reference.md`.

## Why Now

Decision 020 found the gap and called it "the sharpest finding on this page": the durable records
serialize because storage required it, and the diagnostics never had a reason — so the types a
converged observability pipeline most wants to ship are exactly the ones that cannot leave the
process without being hand-mapped field by field.

What made it urgent rather than merely open is that `DrainReport` is new. A new report type either
joins that gap or closes it, and closing it later means touching the same seven types twice.

It is free today. The crate is `0.0.0` with `publish = false`, and the derive widens serde's
existing commitment rather than making a new one — `serde` is already a public dependency on every
durable record, and `wiki/compatibility/public-dependencies.compat.md` already tracks it.

## Why Not `Deserialize`

**Because it has no caller.** Nothing writes a `SyncReport`. The durable records round-trip because
a store persists them and reads them back; a report is produced, read, and discarded. `Deserialize`
would be public surface with no use, and unused surface on a `#[non_exhaustive]` type is a semver
commitment bought for nothing.

**And because a report is a finding, not a document.** `SyncReport` carries relationships a
constructor upholds and a deserializer would not: `retained` equals `sent` minus the three draining
counts, `counts.unknown_status` matches the number of `AnomalyKind::UnknownStatus` entries, and
`anomalies` is in the order the server sent them. A caller able to build one from JSON can produce a
report describing a pass that never happened and hand it to code that branches on `is_stalled()`.
The one-way derive makes the type mean what it says: *this crate observed this*.

The asymmetry is deliberate and it is the same one decision 010 draws from the other side. There,
the wire format's shape is a contract and must round-trip byte for byte. Here there is no
counterparty — nothing on the far end of a `SyncReport` is going to send one back.

## What The Serialized Shape Is Not

`AnomalyKind::UnknownStatus(String)` serializes externally tagged, as
`{"UnknownStatus": "Throttled"}`. That is serde's default for a newtype variant and it is fine,
because **this is a diagnostic shape and not a wire format.** Decision 010 pins
`MutationIntentDto` byte for byte against the source protocol and holds it with an oracle test;
nothing of the kind applies here, and no server parses these bytes.

Worth stating because the two live in one crate and a reader could reasonably assume the same rules
bind both. They do not: a future release may reshape a report's JSON, and the only cost is a
consumer's dashboard query.

## Consequences

- **`serde` joins the semver contract of seven more public types.** Already true of every durable
  record, so this widens an existing commitment; the compatibility note gains the rows.
- **`Serialize` on `DrainReport` is what makes decision 020's aggregation boundary useful.** An
  aggregate that has to be hand-mapped before it can be shipped is an aggregate every caller maps
  differently, which is the disagreement between dashboards that decision 020 exists to prevent.
- **The runtime dependency graph is unchanged.** No new crate; `serde` with `derive` is already
  there.

## Revisit If

A caller needs to persist a report rather than emit it — a durable local diagnostics log that
survives a restart and is read back on the next launch. That is a real thing an offline-first client
might want, and it would be the first genuine caller for `Deserialize`. The answer then is probably
still not this derive, but a caller-owned record built from a report, so the invariants stay with
the type that can uphold them.
