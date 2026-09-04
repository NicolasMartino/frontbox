# Trace Context Is Durable, Caller-Supplied, And Stamped At Enqueue

Document Class: Decision
Status: Accepted 2026-08-28; implemented 2026-08-29
Date: 2026-08-28
Category: Public API Shape
Scope: Whether the outbox record carries W3C trace context, who generates it, and why a span cannot do the job instead.
Sources: `src/record/mod.rs`, `Cargo.toml`, `scripts/verify.sh`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/020-observability-surface.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`

## Decision

The outbox record carries durable trace context, stamped at enqueue and replayed on every send.
**Core does not generate it. The caller supplies it**, exactly as it supplies `MutationId`.

RepForge's §5.2 asks that "frontbox generates a `traceparent` at enqueue and stores it on the
record". The storage half is accepted without reservation. The generation half is declined, for a
reason that is structural rather than stylistic.

## Why The Field Is Right

**A span cannot model enqueue-to-send, and that is the interval that matters here.** A span is an
in-memory, in-process object; it cannot stay open across a browser tab close, an application
upgrade, or a week offline. The number an outbox exists to make visible — how long a write waited
before the server saw it — routinely spans all three.

RepForge states this correctly and briefly: generating the `traceparent` at drain time "would lose
the enqueue-to-send interval, which is exactly the span that matters for an offline queue". The
underlying reason is the one above, and it generalises: **durable trace context exists precisely
because a span is not durable.** Decision 020 accepts `tracing` emission and this decision is what
makes that emission useful, because without a stored parent every drain event is the root of its own
trace and the enqueue is unreachable from it.

**It is a schema field, so it belongs in this conversation and not after D5.** Same argument as
decision 016's `seq` and decision 017's `attempts`: three columns decided together cost one schema,
and three columns decided separately cost three migrations.

## Why Core Must Not Generate It

**Core has no randomness, deliberately, and a `traceparent` is mostly randomness.** The W3C field is
`00-<16 random bytes>-<8 random bytes>-<flags>`. frontbox's only randomness is `uuid/v4` behind the
`v4` feature, and `scripts/verify.sh:30` builds `--no-default-features` for wasm specifically to
prove the randomness-free path does not rot. Generating trace context in core either breaks that
build or hides behind the same feature flag — which would make observability optional in a way
nobody intends, and would make the *storage* of a caller's trace context depend on whether core can
generate one it was never going to use.

**And the crate already has this exact rule, for the same reason.** `Cargo.toml` says it plainly:

> Core never generates a `MutationId` — the caller supplies it, which is what makes
> application-owned direct dispatch expressible.

Trace context is the same shape of thing: an identifier minted by whoever owns the surrounding
context, which core stores faithfully and never interprets. An application already inside a span
when the user taps a button has the *correct* parent; core generating a fresh root would discard the
causal link to the user action, which is the link the whole convergence exists to preserve.

**Core must not parse it either.** frontbox has no business validating W3C version bytes or
rejecting a malformed `traceparent`, for the reason decision 008 gives about `OperationMeta::version`:
a field core never reads should not make a constructor fallible. An application using a different
propagation format stores that instead and frontbox is none the wiser.

## Shape

`OperationMeta` is the wrong home despite being the obvious one. It is documented as the *caller's
own vocabulary* for what an operation is, is caller-interpreted, and is optional in a way that says
"label this if you find it useful". Trace context is not a label; it is a correlation identifier
with an external standard behind it, and burying it in a free-form metadata struct would make it
invisible to the transport that has to put it in a header.

A dedicated optional field, additive under decision 008's `#[non_exhaustive]` records, and carried
onto `DeadLetterRecord` across the transition for the same reason `op` and `attempts` are: a parked
mutation whose trace context was dropped cannot be joined to the request that parked it, which is
the one moment anybody goes looking.

## Consequences

- **`OutboxRecord` gains a third decided-but-unbuilt field**, after `seq` (016) and `attempts` (017).
  D5's outbox schema should now be planned as one change carrying all three.
- **`DeadLetterRecord` carries it too**, joining `op` and `attempts`.
- **The transport is what sends it.** Core stores and returns the value; putting it in a header is
  `SyncTransport`'s job, alongside the synthesis obligations of decision 019.
- **Replay semantics need stating.** A record retried twenty times sends the same stored context
  twenty times, so every attempt shares a parent. With decision 017's `attempts` alongside it,
  `(traceparent, attempts)` distinguishes the tries; without it they are indistinguishable, which is
  a third independent argument for 017 being implemented first.
- **Nothing in the runtime dependency graph changes.** The field is a string to core.

## Implementation Outcome

**Built 2026-08-29**, and building it exposed a requirement this decision missed.

**The field is `#[serde(skip)]` on `MutationIntent`.** The decision said trace context is sent "as a
header", and `MutationIntent` *is* the wire body — so serializing the field would have put the
context in the payload, silently changing the shape decision 010 fixed and breaking the passthrough
compatibility that decision exists to preserve. Skipping it means the value rides the type without
riding the wire: the transport reads it off the intent and sets the header. Case 27 still sees
exactly five keys.

Case 51 asserts all three properties together, because separately they prove little: the context is
durable across the store, absent from the serialized payload, and carried onto the dead letter — and
a caller that supplies nothing gets nothing invented for it.

## Revisit If

An application needs core to correlate its *own* internal operations — the sweep, the quarantine
move, the version compare — rather than only the mutation lifecycle. That is a span question, not a
durable-context question, and it belongs to decision 020's emission half.
