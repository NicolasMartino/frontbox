# A Dead Letter Says Why It Is One

Document Class: Decision
Status: Accepted 2026-08-30; implemented same day
Date: 2026-08-30
Category: Public API Shape
Scope: How a dead letter records the reason it was parked, and why a nullable rejection payload was the wrong encoding for it.
Sources: `src/record/terminal.rs`, `src/store/mod.rs`, `src/runner/mod.rs`, `wiki/references/repforge-eight-answers.reference.md`
Related: `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/019-verdict-synthesis.decision.md`

## Decision

`DeadLetterRecord` carries a **`DeadLetterReason`**, and `Disposition::DeadLetter` carries one too.
`error: Option<RemoteRejection>` is replaced on both.

```rust
#[non_exhaustive]
pub enum DeadLetterReason {
    /// A server refused this mutation on its merits.
    Rejected {
        /// What the server said, when it said anything.
        error: Option<RemoteRejection>,
    },
    /// The client reached its retention bound. No server ever refused this.
    RetentionBound,
    /// The application parked it, in its own words.
    Caller(String),
}
```

`DeadLetterRecord::rejection()` stays available as a convenience for callers that only want the
payload.

## Why The Old Encoding Was Wrong

**`Option<RemoteRejection>` answered two questions with one field.** *Did a server refuse this?* and
*did it explain itself?* are different, and decision 017 loaded the first onto the absence of an
answer to the second: `Some` meant a refusal, `None` meant the client gave up at its bound. That
worked exactly as long as those were the only two ways a record could be parked.

They are not, and `apply_outcomes` is public, so they never were. An application that dead-letters a
record for a reason of its own — RepForge runs a wall-clock watchdog beside the retention bound —
produces `None`, indistinguishable from the bound firing. `attempts` narrows it and does not settle
it: a caller-driven dead letter can carry any attempt count, including one that happens to equal the
bound.

**We sent RepForge this exact argument and did not apply it to ourselves.** Reviewing their
dead-letter report body, this project asked them to model cause as a discriminated field rather than
as a nullable error, because *"serialized as a null it reads as a missing field, and the distinction
is lost at exactly the boundary it was built to survive."* They accepted, went further, and added a
third kind we did not have — and in doing so demonstrated that our own type could not express what
they had just been persuaded to express. The argument was right when we made it; it was right about
us too.

**The new encoding puts the `Option` where it belongs.** Inside `Rejected`, it means *the server
refused and may or may not have explained*, which is a real distinction decision 005 already
recognised — a `Rejected` status with no payload is a well-formed server answer. Outside, absence
was carrying a meaning it could not name.

## Why An Enum Rather Than A Second Field

A `reason` beside the existing `error` would have been additive and is worse. It permits states that
cannot exist — `RetentionBound` beside a `Some(rejection)`, `Rejected` beside a `None` that now
means nothing — and every reader would have to learn which combinations are real. The enum makes the
illegal states unrepresentable, which is the same reason decision 006 refuses to model a corrupt
record as a normal one with a flag.

`#[non_exhaustive]` because there will be more. A fourth kind is likelier than not, and adding one
should stay additive for callers that already match with a wildcard arm.

## Why `Caller` Carries A String

For the reason `OperationMeta::name` and `MutationStatus::Unknown` do: it is the application's own
vocabulary, core never reads it, and validating it would make a constructor fallible over a field
this crate does not interpret. An empty string is accepted and is exactly as useful as omitting the
variant.

Core does not enumerate application reasons. RepForge's watchdog is one; a user cancelling a queued
write is another; neither is core's to name.

## Consequences

- **This changes what an existing public item means**, which `AGENTS.md` gates. Cheap only because
  the crate is `0.0.0` with `publish = false`; a major version afterwards.
- **The runner sets it.** `Rejected { error }` from a server verdict, `RetentionBound` at the bound.
  Those are the only two kinds core produces, and the third exists so a caller can say something
  core would otherwise force into silence.
- **Decision 017's `Some`/`None` discriminator retires.** It was the right encoding for the two
  states that existed when it was written, and it is superseded rather than wrong.
- **Conformance cases change.** Case 03 and case 17 assert on a rejection payload; case 47 asserts
  that the bound produces no rejection, which becomes a positive assertion of `RetentionBound`
  rather than an absence.
- **The dead-letter report gains a clean mapping.** RepForge's three kinds — `rejected`,
  `retention_exhausted`, `watchdog` — now correspond to `Rejected`, `RetentionBound`, and a `Caller`
  value they choose, rather than to two states plus an inference.
- **D5's dead-letter schema carries a discriminant**, not a nullable blob.

## Implementation Outcome

**Built 2026-08-30.** Case 56 pins the property the change exists for, and is deliberately arranged
so that **no other field separates the three**: both unexplained kinds have an absent rejection, and
the caller-driven one is parked at an attempt count equal to the bound the other reached. It asserts
that too, so nobody later mistakes `attempts` for the discriminator.

Two things worth recording.

**`attempts` had quietly become a discriminator and is now documented as not being one.** Its doc
comment said the count plus an absent rejection "say which happened". That was true of two kinds and
false of three, and it is the same conflation at one remove — a field pressed into a role it was not
built for because the field that should have carried it did not exist.

**The new `docs resolve` gate caught the rename, on the day it was added.** Removing the `error`
field left a doc link pointing at it, which `cargo doc -D warnings` failed on. Without the gate that
would have shipped as a broken link and joined the eighteen that accumulated before it.



Core acquires a way to park a record that is neither a server verdict nor a client bound — the most
plausible being an expiry, if a future decision reverses 017's rejection of age-based bounds. That
would be a fourth variant rather than a reshaping, which is what `#[non_exhaustive]` is for.
