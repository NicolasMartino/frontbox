# A Write Precondition Is A Named Field, Not A Header Bag

Document Class: Decision
Status: Accepted 2026-08-30; implemented same day
Date: 2026-08-30
Category: Public API Shape
Scope: Whether the outbox carries a request precondition captured at enqueue, what shape it takes, and why two instances of "an opaque value replayed as a header" do not become a general mechanism.
Sources: `src/record/mod.rs`, `src/transport.rs`, `wiki/references/repforge-eight-answers.reference.md`
Related: `wiki/decisions/022-durable-trace-context.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/references/open-decisions.reference.md`

## Decision

`OutboxRecord` gains a fourth durable field: an **opaque, caller-supplied precondition** captured at
enqueue, replayed on every send, never parsed by core, and carried onto the dead letter.

- **`Option<String>`, opaque.** Core does not know whether it says `If-Match: <hash>` or
  `If-None-Match: *`, and must not: a `PUT` does not reveal whether a given mutation is a create,
  and only the application does.
- **`#[serde(skip)]`**, exactly as decision 022's trace context. It travels as a header, so
  serializing it would put it in the body and change the payload decision 010 fixed.
- **Caller-supplied on `MutationIntent`**, like `op` and `traceparent`, not assigned by the store
  like `seq`.
- **And this is where the general case stops.** There is no header map, and there will not be one.

## Why The Field Exists

**A precondition is only meaningful if it carries the state the user was looking at when they
enqueued.** Computed at drain from current local state it compares the client against itself and
detects nothing — it matches whatever is there at that moment and every conflict passes. So the
value has to be captured at enqueue and replayed unchanged, possibly days later, which is the same
durability requirement decision 022 established for trace context.

**Decision 023 is what made this a column rather than free.** RepForge's original ask assumed the
base hash could ride along with a persisted row hash. Decision 023 declined to store row contents —
holding them would have made frontbox take a position on byte-exact canonical serialization of an
application's DTOs, which is precisely what a shared contracts crate exists so nobody has to guess
at. That reasoning stands, and it leaves the precondition with nowhere else to live. RepForge
re-priced the ask themselves rather than letting it be discovered, which is the right way for it to
have arrived.

**The failure mode is silent, and that decides its weight.** Losing trace context degrades
diagnostics. **Losing a precondition disables conflict detection with no symptom** — the write
succeeds, a concurrent edit is clobbered, and nothing anywhere reports it. The alternative to a
column is an application holding base hashes in its own store keyed by `mutation_id`, which means a
second write outside `enqueue`'s transaction: a crash between the two leaves a queued record whose
precondition is gone, and the lost update follows. A precondition cannot afford that; trace context
can.

## Why Not A Header Map

Two fields now share one shape — caller-supplied, stored durably, replayed as a header, never
interpreted. That is normally the point at which the general case is worth taking, and here it is
refused.

**Because decision 004 would not survive it.** That decision holds that *"core persists no auth data
on outbox records"*, because *"tokens expire, and a queued mutation may sit in the outbox for days
before it is sent. Persisting credentials on the record would replay stale or sensitive state."*
A generic header map is exactly the mechanism by which an `Authorization` header ends up durable on
disk — not through misuse, but because it is the obvious thing a map is for. A named field cannot
hold a credential without someone deliberately putting it somewhere it plainly does not belong; a
map invites it as the intended use. **The generalization would quietly repeal an explicit decision**,
and that is a bad trade for saving one field.

Three further reasons, none of them sufficient alone:

- **The two are not the same kind of thing.** One is diagnostic and one is load-bearing for
  correctness. A map flattens that into two equally optional entries, and an implementor skimming it
  cannot tell which one they must not drop.
- **A map is unbounded caller-controlled state in a durable store**, with no natural size limit and
  no answer to what a duplicate or a malformed key means.
- **Decision 022's argument against `OperationMeta` weakens but does not vanish.** A transport can
  iterate a map, but it must then *know which key it needs* — which is a contract expressed in
  string literals rather than in the type.

If a third instance appears, that is the moment to revisit, and this section is what it should be
argued against.

## Consequences

- **`OutboxRecord` reaches four caller- or store-supplied fields beyond the envelope**: `seq` (016),
  `attempts` (017), `traceparent` (022), and this. D5's outbox schema carries all four, and the
  register's argument about landing them as one change is now discharged for the outbox.
- **`DeadLetterRecord` carries it**, joining `op`, `attempts` and `traceparent`. A dead letter that
  failed on a conflict is much easier to act on when it says what state it expected, and requeueing
  one without its precondition would re-run exactly the unchecked write this decision exists to
  prevent.
- **`SyncTransport`'s documentation gains a line**, not a section: a transport that sets a
  precondition header reads it off the intent, and one that does not is unaffected because the field
  is `None`.
- **The conformance suite owes a case** in case 51's shape: durable across the store, absent from
  the serialized payload, carried onto the dead letter, and never invented for a caller who supplied
  nothing.
- **Core still takes no position on HTTP.** It stores a string. The decision that a `412` means a
  conflict, and that a conflict is terminal-but-human-resolvable, stays in the transport where
  decision 019 put it.

## Implementation Outcome

**Built 2026-08-30**, as case 55 alongside case 51's trace-context assertions. Two notes.

**`src/record/mod.rs` reached 442 lines and split** into `record/mod.rs` and `record/terminal.rs`. The
boundary is a real one rather than a line count: the pending path — `OperationMeta`,
`MutationIntent`, `OutboxRecord` — against the two terminal states a record can reach.
`DeadLetterRecord` and `QuarantinedRecord` are never written directly; both are produced by an
outcome transition, which is what makes decision 003's atomicity requirement expressible.

**`SyncTransport` gained a short section rather than the line this page predicted.** Describing one
header field alone would have been odd when there are now two of the same shape, so the section
covers both and states the asymmetry between them: dropping `traceparent` costs diagnostics,
dropping `precondition` clobbers a concurrent edit with no symptom. It also records RepForge's own
mitigation as the recommended one — a transport that cannot attach the header should refuse to send
and let the record dead-letter, which turns a lost update into something a human can see.



A third value of this shape appears — a caller-supplied opaque string captured at enqueue and
replayed as a header. Two is a coincidence; three is a pattern, and the argument above is what a
header map would then have to defeat. The auth objection would still stand, so the shape to reach
for is probably a bounded, named set rather than an open map.
