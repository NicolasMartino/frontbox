# A Transport That Synthesizes A Verdict Owes The Care A Server Would Have Taken

Document Class: Decision
Status: Accepted 2026-08-27; documented on `SyncTransport` 2026-08-29
Date: 2026-08-27
Category: Sync Semantics
Scope: Who decides terminal versus transient when no server produces a `MutationBatchResponse`, and what `SyncTransport` obliges an implementor to get right.
Sources: `src/transport.rs`, `src/protocol.rs`, `wiki/references/prior-art-survey.reference.md`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/proposals/single-flight-drain.proposal.md`

## Decision

Core does not classify HTTP responses and will not. When there is no server producing a
`MutationBatchResponse` — a per-resource `PUT` API behind a gateway that only routes — the transport
adapter synthesizes `MutationResult` values, and `SyncTransport`'s contract gains three obligations
on it:

- **`Rejected` means the server terminally refused *this mutation on its merits*.** It is not a
  status class. A transport must not map 4xx onto `Rejected` as a rule.
- **A response the transport cannot confidently classify is `MutationStatus::Unknown(_)`**, carrying
  what the server actually said. Not `Rejected`, not `Applied`.
- **A missing prerequisite is transient.** A `404` or `409` produced because a parent resource has
  not been created yet resolves when the predecessor lands, and dead-lettering it discards valid
  work for a reason that will stop being true.

This is a documented obligation on implementors, not a signature change.

## Why

**`Blocked` loses its producer but keeps its hazard.** RepForge's proposal is correct that under
one mutation per request there is no "same batch" and so nothing to call `Blocked`. What it does not
say is that the *condition* `Blocked` named — this write failed only because a predecessor had not
landed — still occurs, and now presents as a bare `404` that nothing distinguishes from a genuine
refusal. Removing the word does not remove the state. It relocates the judgment from the server,
which knew it had skipped the mutation without evaluating it, to the client, which sees only a
status code.

That relocation is the largest consequence of removing the BFF, and it is the one the proposal
prices at zero.

**Decision 005 wrote this caution about someone else's library and it now describes ours.** Its
closing note reads:

> `Rejected` is a server verdict, not an HTTP status class, and it must stay that way. Redux
> Offline's default treats all 4xx as permanent, and its own documentation immediately overrides
> that for `401`, refreshing the token and retrying.

Under the BFF that was an aside about prior art, because a server assigned the status and the
transport passed it through. Under per-resource `PUT` it is the requirement the whole design rests
on, because frontbox's own transport becomes the thing making Redux Offline's mistake or not.

**This is decision 016's hazard seen from the other side, and neither substitutes for the other.**
Correct enqueue ordering prevents the missing-parent `404` from arising. Verdict synthesis is what
has to be right when ordering is violated anyway — and it can be, even with a perfect sequence,
because a drain interleaved across four service origins does not stop origin B when origin A fails.
Ordering is the prevention; classification is the containment.

**`Unknown` is the pressure valve, and decision 017 is what stops it becoming a wedge.** A transport
that declines to classify retains the record and reports the anomaly with the server's own words —
which is decision 012 working as designed, and is strictly better than a confident wrong guess.
Without decision 017 that same restraint freezes a single-flight queue permanently, so the three
decisions have to ship together or the safe behaviour becomes the expensive one.

**Core will not classify, for the reason it does not do several other things.** It does not
interpret the body (decision 008), does not gate the pull it does not perform (decision 014), and
does not attribute mutations to entities without a caller-supplied classifier (decision 014 again).
A `409` is a version conflict on one service and a uniqueness violation on another; a `422` is
terminal for a malformed body and transient for a reference that has not replicated yet. Core has no
way to tell, and a default would be a policy the caller could not opt out of.

## Consequences

- **`SyncTransport`'s documentation grows a section**, alongside the existing "Partial and surplus
  responses" and "Offline is not a transport failure". This is cheap now and much harder later:
  applications will write transports against whatever the contract says on the day they read it, and
  a looser contract cannot be tightened once they have.
- **Decision 010's premise expires more thoroughly than RepForge argued — for RepForge.** They read
  it as the envelope becoming one transport's choice rather than a mandate. Against the BFF-less
  shape it is stronger than that: `SyncTransport::send_batch` returning a *server-produced*
  `MutationBatchResponse` stops describing anything that exists, because no RepForge service
  produces one. That is a claim about one consumer and it must not be read as a claim about the
  protocol. frontbox is a library, a batch mutation endpoint remains a perfectly reasonable server
  design, and a transport talking to one still passes a genuine server response through untouched —
  for which none of this decision's three obligations apply, because nothing is being synthesized.
  The types survive intact as the runner's input vocabulary; what changes, for consumers without
  such a server, is who authors them and on what evidence. Whether the shape is still right is a D4
  question, and this decision does not pre-empt it.
- **D4 owes the first worked synthesizer.** The RepForge transport is where this contract is either
  followable or not, and it is the artifact that shows which. A trial that produces a transport
  mapping every 4xx to `Rejected` would be evidence the obligation is unusable as stated, not
  evidence the transport was written carelessly.
- **The conformance suite cannot assert this**, and that should be said plainly rather than
  discovered. Every conformance case drives an in-memory transport that returns whatever the case
  hands it, so the suite tests what the *runner* does with a verdict, never how a real adapter
  arrived at one. This obligation is enforced by documentation and review, which is weaker than the
  standard the rest of the crate holds itself to.
- **`Duplicate` becomes hard to produce and stays in the enum.** Under `PUT` with a client-generated
  id, a replay and a first write can both answer `200`, so a transport usually cannot distinguish
  them. RepForge is right that this is no reason to prune the variant: both map to `Delete`, a
  server may still distinguish them, and a transport that *can* tell should say so.

## The Ask Was Granted, 2026-08-29

The Revisit If below asked for a structured terminality signal in service error bodies, and called
it *"the one request this project should make of the redesign rather than accommodate"*. RepForge
added one, and says this question caused the change. Every service error now carries a required
`retry` field of `terminal`, `transient`, or `conflict`.

**What that changes.** For RepForge's transport, the three obligations become a lookup rather than a
judgment: `terminal` maps to `Rejected`, `transient` to a retaining status, `conflict` to a dead
letter carrying a `RemoteRejection` whose `code` names it. The hard case this decision was written
about — a `404` for a resource that will never exist versus a `404` for a parent that has not
drained — is answered by the server, which is where the information was all along.

**What it does not change, and this is why the decision stands rather than retires.**

- **The obligations still bind the fallback path.** Gateway routing failures and transport errors
  never reach a service and carry no envelope. RepForge keeps the status-line table for exactly
  those, so a transport still has to classify without help — which is the case this decision's three
  rules govern.
- **The obligations bind every *other* transport.** This is a library. A consumer whose server has
  no such field is in precisely the position this decision was written for.
- **`transient` gives `Blocked` a producer back.** RepForge's proposal argued `Blocked` loses its
  producer under single-flight because there is no same batch. A transport synthesizing a retaining
  status from `retry: transient` **is** a producer, on the client side. Decision 005's refusal to
  delete the variant is vindicated from a direction nobody argued at the time.
- **`conflict` is a third end state that the type system does not separate.** It is terminal for the
  attempt and resolvable by a human, which is neither "refused on merits" nor decision 017's "client
  gave up". All three land as dead letters; `Some(rejection)` versus `None` separates the third, and
  `RemoteRejection::code` is where the first two differ. That is workable and it is a convention
  rather than a guarantee — worth stating so nobody expects the enum to carry it.

## Implementation Outcome

**Written into `SyncTransport`'s documentation 2026-08-29**, as a section beside "Partial and
surplus responses" — which is where a transport author is already reading. It carries the three
obligations, a concrete mapping table for RepForge's `retry` field, and two things this page did not
anticipate.

**The status-line fallback rests on an invariant worth checking rather than assuming.** RepForge's
answer makes body-*absence* the discriminator between "a service ruled on this" and "this never got
there". The documentation says so and then tells the implementor to satisfy themselves it holds in
their deployment, because a proxy in front of the gateway returning a parseable error body would
break it — and a rate limiter read as a service verdict dead-letters work that would have succeeded.
That is this decision's own failure mode relocated one hop upstream.

**The vocabulary has no word for "transient", and the documentation says so rather than forcing a
fit.** All three retaining statuses are wrong in different ways: `Blocked` covers a missing
prerequisite and not a dependency being down; `Pending` asserts the server *accepted* the mutation,
which a transient refusal is not; `Unknown` is honest but reports a routine condition through the
channel that exists for vocabulary mismatches. So the documented obligation is **the disposition,
not the spelling** — a transient condition must retain — and which status carries it is a diagnostic
choice. A variant that means what it says would be the fix, and that is a public API change not made
here.



RepForge's services return a structured error body that classifies terminality explicitly — a field
saying "this will never succeed" versus "retry after the parent exists". That moves the judgment
back to the server, where it belongs and where it has the information, and reduces this decision to
a mapping table. It is worth asking for while the services are still being designed, which is the
one request this project should make of the redesign rather than accommodate.

## Where The Mapping Table Actually Lives

`src/transport.rs` carries the concrete status-to-verdict table this page argues for, in the
longest doc comment in the crate — including the admission that **the section is enforced by
review, which is weaker than the standard the rest of the crate holds itself to.** That sentence is
the honest one and it should stay where an implementor reads it.

This page is now cited by decision 037 as the operative reference for a *routing* transport, which
raises the stakes: a client whose writes span services has more ways to fail than one whose writes
do not, and the classification of each is still the transport's. Two consequences worth stating
here rather than leaving to be rediscovered.

**A missing prerequisite is `Blocked`, not `Unknown`.** This page describes the condition — *"this
write failed only because a predecessor had not landed"* — and it is precisely what a `404` means to
a routing transport that sent a todo for a user whose creation has not drained yet. `Unknown` is
reserved for a response the transport genuinely *cannot* classify, and this one it can. The
difference is observable: `Blocked` retains with `counts.blocked`, and only `Unknown` raises an
anomaly.

**A transport that could not reach a service is the same fact from a different cause.** Returning
the verdicts it obtained and staying silent about the rest is already modelled — `src/runner/mod.rs`
retains any record the response did not name, with `reason: Some("no verdict returned")`. Synthesis
was built for a server that declines to rule, and it covers a service that could not be asked.
