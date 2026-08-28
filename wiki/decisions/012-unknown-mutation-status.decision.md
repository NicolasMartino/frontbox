# An Unknown Mutation Status Is Retained And Reported, Never Guessed

Document Class: Decision
Status: Accepted; implemented 2026-08-27
Date: 2026-08-27
Category: Public API Shape
Scope: What happens when a server answers with a `MutationStatus` this crate has never heard of, and what signal the caller gets.
Sources: `src/protocol.rs`, `src/runner.rs`, `src/store.rs`
Related: `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/013-unknown-entity-name.decision.md`, `wiki/specs/frontbox-runtime.spec.md`

## Decision

An unrecognised status is tolerated, retained, and reported. It is never mapped onto a known status
and never silently dropped.

Three parts:

- **`MutationStatus` gains a catch-all that keeps the original string.** `Unknown(String)`, with
  hand-written `Serialize`/`Deserialize` so the wire form round-trips exactly: `"Throttled"`
  deserializes to `Unknown("Throttled")` and serializes back to `"Throttled"`, not to a nested
  `{"Unknown": …}`. `MutationStatus` therefore stops being `Copy`.
- **Its disposition is `Retain`**, extending decision 005's table. `drains()` returns `false`.
- **It is reported as an anomaly**, which requires giving anomalies a reason. `SyncReport::anomalies`
  becomes `Vec<Anomaly>`:

```rust
#[non_exhaustive]
pub struct Anomaly {
    pub mutation_id: MutationId,
    pub kind: AnomalyKind,
}

#[non_exhaustive]
pub enum AnomalyKind {
    /// A verdict naming a record this pass did not send.
    UnknownMutation,
    /// One of two or more verdicts for the same id. None was applied.
    RepeatedVerdict,
    /// A status this crate cannot act on, as the server spelled it.
    UnknownStatus(String),
}
```

**Implemented 2026-08-27.** All three parts shipped as written, alongside
`SyncOutcomeCounts::unknown_status` — added so that `blocked + pending + unknown` still reconciles
against `retained` without walking the anomaly list. Conformance case 34 asserts that a batch
containing one unknown status still applies every known verdict in it.

## Why

**Failing the response is the worst of the available outcomes, and it is what happens today.**
`MutationStatus` derives `Deserialize`, so one unrecognised string fails the *whole*
`MutationBatchResponse` — every verdict in it, including the ones this crate understands perfectly
well. A fifty-record batch containing one `"Throttled"` discards forty-nine good verdicts.

That is not a transient failure. The next pass sends the same records, the server answers the same
way, and the response fails again. A server rolling out a sixth status would wedge every client at
once, permanently, and the queue would report *no progress* without being able to say why. Weighed
against that, a single retained record is cheap.

**Retain is the only honest disposition.** Decision 005 fixed the table: `Applied`/`Duplicate` →
`Delete`, `Rejected` → `DeadLetter`, `Blocked`/`Pending` → `Retain`. An unknown status cannot be
`Delete` without assuming the server accepted it, and cannot be `DeadLetter` without assuming it
refused. Both are guesses, and both are destructive if wrong — one loses a write, the other buries a
successful one in dead letters. `Retain` assumes nothing and costs a resend, and `mutation_id` is
the idempotency key, so the resend is safe.

This is the same shape as the two rulings either side of it. Decision 006 surfaces a corrupt record
rather than dropping it; decision 011's runner reports a repeated verdict rather than picking one.
The crate's consistent answer to *"I do not know what this means"* is to stop, keep the data, and
say so.

**Keeping the string is the point of tolerating at all.** A bare `Unknown` unit variant — the
cheapest option, and the one that would preserve `Copy` — tells a caller that something was
unrecognised but not what. That is a stall with no diagnosis, which is close to the silent stall
this decision exists to avoid. `Unknown("Throttled")` is actionable: it names the vocabulary
mismatch, and it names it in the server's own words.

The `Copy` loss is real and small. It is also free right now, and would cost a major version after
publication.

**Anomalies need a reason once there are three of them.** `anomalies: Vec<MutationId>` already
carries two distinct meanings — a verdict for a record we did not send, and one of several verdicts
for the same id. A third would make the field a list of identifiers with no way to tell why any of
them is there, at exactly the moment a caller most needs to know: "the server ruled on something I
did not send" and "the server speaks a dialect I do not understand" call for completely different
responses from an operator.

## Consequences

- **The stall is visible and, since decision 017, self-clearing.** A record whose verdict is always
  `Unknown` reports loudly on every pass — `made_progress()` is false, `is_stalled()` is true, and
  the anomaly carries the server's own spelling. Under **decision 017 (2026-08-27)** it is also
  dead-lettered once its attempt count reaches a caller-set bound, so the queue behind it is
  released. Absent a configured bound the record is still retained indefinitely, because 017 ships
  with no default; the original stall text below therefore still describes the unconfigured case
  exactly.
- **`MutationStatus` is no longer `Copy`.** Call sites take it by reference or clone. Internal
  churn only, since nothing is published.
- **`drains()` stays total and stays honest.** `Unknown` does not drain, so a batch of nothing but
  unknown statuses is correctly reported as no progress.
- **Decision 005's table gains a row**, and the runtime spec's disposition table with it.
- ~~**The `#[non_exhaustive]` note on `MutationStatus` needs rewriting.**~~ Done: it described the
  limitation this decision removes.
- ~~**A conformance case is owed**~~ **Implemented as case 34**, asserting that a batch containing
  one unknown status still applies every known verdict in it, retains the unknown one, and reports
  it with the server's spelling intact.


## Superseded Text

Kept verbatim because decisions 017 and 018 both cite this paragraph as the argument that forced
them, and paraphrasing the thing that was superseded makes the citation unverifiable.

> **The stall is visible but not self-clearing.** A record whose verdict is always `Unknown` is
> retained forever. `made_progress()` is false and `is_stalled()` is true, and every pass reports
> the anomaly with the offending string — so the condition is loud, not silent. Escalating it
> automatically would need attempt tracking or aging, which D1 deliberately does not have. This
> decision therefore makes the open question *"whether retained work needs aging, attempt tracking,
> or last-error metadata"* (`wiki/index.md`, Open Work) materially more urgent: it is now the only
> route out of a vocabulary mismatch that never resolves.

That last sentence is what decision 017 acted on. What turned it from urgent into blocking was
RepForge's request for `batch_limit = 1`: at a limit of one, this record freezes the entire queue
rather than starving a window.

## Revisit If

A server contract appears that treats unknown statuses as fatal by design, and wants the client to
refuse the whole response rather than proceed on the part it understood. That is a legitimate
posture for a system where verdicts are ordered and interdependent; it is not this one, where
decision 005 already establishes that each verdict is independent and an omitted verdict simply
retains (conformance case 33).
