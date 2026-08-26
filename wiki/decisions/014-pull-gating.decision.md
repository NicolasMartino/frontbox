# Core Reports Pending-Write Conflict; It Does Not Gate The Pull

Document Class: Decision
Status: Accepted; implemented 2026-08-27
Date: 2026-08-27
Category: Cache Versioning
Scope: What happens when an entity is invalidated while the local outbox still holds unsent mutations, and where the decision to refetch anyway belongs.
Sources: `raw/initial/2026-08-25T083750Z/sources/listener.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`, `wiki/references/prior-art-survey.reference.md`
Related: `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/proposals/extraction-boundary.proposal.md`, `wiki/plans/d2-cache-invalidation.plan.md`

## Decision

Core does not gate refetches, because core does not perform them. It makes the conflict *visible* so
the application can gate, and reports it in the same breath as the staleness that would prompt the
refetch.

- **Staleness and pending-write conflict are two facts, delivered together.** Asking whether an
  entity is stale also answers whether replacing it would discard unsent local work. A caller cannot
  read one without the other being available.
- **Attribution is caller-supplied.** Core cannot know which mutations touch which entity, so the
  application provides a classifier — roughly `fn(&OutboxRecord) -> Option<K>` — which core calls
  and groups by. Core still interprets nothing; it counts what the caller classified.
- **Without a classifier, the answer is whole-outbox.** "Some mutation is pending" rather than "a
  mutation for *this* entity is pending". Coarser, always available, and never wrong in the unsafe
  direction.
- **No hard gate anywhere in core.** An application that decides to refetch over pending work can.

## Why

**The harm is concrete and worse than "the cache is briefly wrong".** In the source, an invalidation
runs `eager_refetch` (`listener.rs:472`), which calls `ExerciseStore::replace_all`, and that is
`DELETE FROM exercises` followed by re-inserting the server's rows (`persistence/native.rs:585`;
`persistence/web.rs:435` is the same shape). Any optimistic local projection of a queued mutation is
deleted outright. The user watches an edit they made offline revert, and if that mutation later
dead-letters it never comes back — they saw a change accepted and then silently undone, with no
error anywhere.

The source already half-knows this. `replace_all` opens with a guard:

```rust
// Guard: Don't delete data if we have nothing to replace it with
if exercises.is_empty() { … return Ok(()); }
```

That protects against an *empty server response* wiping the cache — a lesson clearly learned the
hard way. It does nothing about pending local work, because the listener never consults the outbox
at all.

**Rebase is unavailable to this crate by construction.** The prior-art survey's only precedent for
this problem is Replicache, and its answer is not gating: pending mutations are *rebased over pulled
canonical state* (`wiki/references/prior-art-survey.reference.md:122`). That works because
Replicache's write model is **named mutator invocation with JSON args** — the client holds the
function and can re-run it against new state. frontbox's outbox holds a raw HTTP envelope, and
decision 008 keeps the body as *uninterpreted* JSON precisely so core stays domain-neutral. Core
cannot replay a `PATCH /api/v1/exercises/123` against a local read model because it has no idea what
that means.

So the survey's one precedent does not support gating; it describes a different mechanism that this
crate's write model forecloses. That is worth stating plainly rather than borrowing as agreement.

**A hard gate has exactly the failure decision 005 exists to prevent.** If refetch were blocked
while the outbox is non-empty, one permanently stuck record would freeze the cache forever. That is
not hypothetical: decision 005 retains `Blocked` and `Pending` indefinitely, and decision 012 now
retains an unrecognised status indefinitely too. Under a hard gate, a single mutation the server
will not resolve would leave every entity permanently stale — the data equivalent of a queue that
never drains, which decision 005 spent its whole liveness argument avoiding. Reintroducing it one
layer up would be a poor trade.

**Core does not own the pull.** The refetch in the source is `fetch_exercises` plus
`ExerciseStore::replace_all` — application endpoints writing application read models. Whether local
read-model persistence belongs in core at all is still an open question
(`wiki/proposals/extraction-boundary.proposal.md:262`). Gating an operation core neither performs
nor understands is not a policy core can implement; the honest version is to give the caller the
fact it needs at the moment it needs it.

**Reporting both facts together is what makes the safe path the easy one.** The source's defect is
not that it chose to refetch eagerly — it is that nothing in the call path could have chosen
otherwise, because the outbox was never consulted. If the only way to learn an entity is stale also
tells you what refetching would cost, an application has to actively ignore the warning to hit the
bug.

## Consequences

- **This satisfies the roadmap's proof obligation, and it is worth being precise about how.** The
  roadmap requires *"a test that fails against the copied listener behavior"*
  (`wiki/roadmaps/extraction.roadmap.md:188`). The test is: with a mutation queued for an entity and
  an invalidation arriving for it, the staleness report names the conflict. It fails against the
  source not because the source decides differently but because the source produces no such value —
  `handle_invalidation_event` returns `()` and consults no outbox. The divergence is that the
  information exists.
- **A classifier is optional, and its absence is safe.** Falling back to whole-outbox means a caller
  with unrelated pending work sees a conflict that would not actually have clobbered anything. That
  is a false positive in the conservative direction, and the caller can refetch anyway.
- **`OperationMeta` is not extended.** It is `{ name, version }` and carries no entity key
  (`src/record.rs`). A caller wanting per-entity attribution can key off `path`, `method`, or its
  own `op.name` inside the classifier — core reads none of them.
- **The classifier runs over pending records, so it must be cheap**, and the D2 plan needs to say
  what happens when it panics or is inconsistent between calls.
- **Nothing here forces a read-model API.** This decision is compatible with read models staying
  entirely app-owned, which keeps the open question in the extraction proposal open rather than
  answering it by accident.
- ~~**Conformance cases owed**~~ **Implemented 2026-08-27** as cases 41, 42, and 43.
- **Implementation settled a question this page left open.** A bounded classifier scan had no stated
  conclusion for the case where the queue is larger than the scan. It now degrades every entity to
  `Unattributed` rather than reporting a clean partial view, because a false "nothing is queued" is
  the answer that loses data. Case 42 covers it.

## Revisit If

Read models move into core, at which point core would perform the pull and could gate it
meaningfully — the reasoning above turns on core not owning the operation. Or if D4's migration
trial shows applications consistently writing the same gate by hand, which would be evidence the
default belongs one level down.
