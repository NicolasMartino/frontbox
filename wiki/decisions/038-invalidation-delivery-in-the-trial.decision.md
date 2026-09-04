# Invalidation Delivery Is Built In The Trial Before It Is Built In Core

Document Class: Decision
Status: Accepted 2026-08-31; **bar scored by D4d the same day** — two of four criteria closed, two answered, recommendation is not to promote; see `## What D4d Produced`
Date: 2026-08-31
Category: Cache Versioning
Scope: Where the inbound half of cache invalidation lives while it is being designed, and what would have to be true before any of it moves into `frontbox` or `frontbox-dioxus`.
Sources: `src/cache/runner/mod.rs`, `examples/todo-core/src/invalidation.rs`. The adapter's own `use_invalidation` and `InvalidationState`, cited here when this page was written, were removed on 2026-09-01: D4d built the seam in the trial as this decision directed, and the adapter's never acquired a consumer.
Related: `wiki/proposals/invalidation-delivery.proposal.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/032-opaque-row-store.decision.md`, `wiki/plans/d4a-offline-todo-trial.plan.md`, `wiki/plans/d4d-multi-domain-trial.plan.md`

## Decision

**Core and the adapter gain nothing yet.** No `InvalidationSource` trait, no polling loop, no
resume cursor, no server contract for a versions endpoint. All of it is built in
`examples/todo-core` first, and promotion is a later decision made against evidence.

What core already owns and keeps: the event type, comparison by equality (decision 021), the durable
`(version, stale)` pair (decision 015), and the pending-write conflict report (decision 014).

## Why Not Design The Trait Now

**Because this project's own method says not to, and it has been right every time.** D4a's whole
thesis was to point a real application at the API before committing to it, and the record since is
unbroken: every new consumer produced a finding the previous ones could not — D4a, D4b, D4c, the
SQLite backend, the IndexedDB backend, and D5's cache half on the same day it landed.

**Invalidation has had zero consumers, ever.** `use_invalidation`, `InvalidationState` and
`InvalidationEvent` appear nowhere outside the crates that define them. Designing the inbound seam
in the abstract would be the one place this project guessed at an API with no application holding
it — and it would be guessing about the half that has never run.

**A seam with one implementation is a wrapper.** `RowStore` is a real trait because three backends
implement it. The conformance suite exists because divergence between them had to be catchable. An
`InvalidationSource` with only an SSE implementation would be an `EventSource` with extra steps, and
nothing in the type would reveal that.

## Two Bars, And They Are Not The Same

Kept apart deliberately, because "two sources, ideally three" reads as a requirement on D4d and is
not one.

**What D4d must produce** — the minimum for the deliverable to be finished: **two** sources against
one seam (manual and polling), **two** origins, and the trial's own observations passing. Nothing
about SSE, no cursor, no promotion.

**What promotion into core would additionally require** is the list below. D4d is expected to
satisfy some of it and not all of it, and that is the normal outcome rather than a failure.

## What Would Have To Be True To Promote It

Stated now, so promotion is a judgement against a written bar rather than a fresh argument later:

1. **Three sources against one seam without deforming it** — manual, polling, and a stream. D4d
   builds the first two; the third is what tests whether the seam survives a push model, and it is
   the one criterion D4d cannot close on its own. If the polling source cannot be expressed
   cleanly, the seam is stream-shaped and should be named one.
2. **More than one origin.** A single-server client cannot exercise per-source reconnect semantics,
   and those are where the design is load-bearing: when one service's source drops, only *that*
   service's entities may be marked stale.
3. **The resume-cursor question answered.** If reconnect-and-reconcile against a versions endpoint
   turns out to be cheap, no cursor is needed and the strongest argument for core owning the inbound
   path weakens with it. If a cursor is needed, it is durable per-scope state and core is the only
   thing that holds any — see the proposal.
4. **A cadence policy that survives contact.** Whether `SyncCadence` generalises to polling, or
   invalidation needs its own, is unknown and is exactly what a trial answers.

## What D4d Produced (2026-08-31)

Built the same day this page was written. Two sources against one seam, two origins, ten
observations passing — the minimum this decision set, met — and the bar above scored rather than
deferred.

| Criterion | Outcome |
| --- | --- |
| 1. Three sources against one seam | **Not closed, as expected.** `ManualSource` and `VersionPollSource` are built; a stream is the third and D4d could not close it alone. What the attempt turned up instead is below. |
| 2. More than one origin | **Closed.** Observation 8: the user service stops answering, its entity is marked stale, the todo entity is left alone. Unwritable with one entity, because the two sets coincide. |
| 3. The resume-cursor question | **Answered: no cursor is needed.** |
| 4. A cadence policy that survives contact | **Answered: `SyncCadence` did not generalise.** |

### Criterion 3, and why answering it weakens the case for promotion

Reconnect-and-reconcile against a versions endpoint is **one `GET`, a comparison, and done**.
`VersionPollSource` keeps its last-seen map in memory and loses nothing worth keeping on restart,
because the state that actually mattered — the durable `(version, stale)` pair — is already core's
under decision 015.

This page said the cursor question cut both ways: *"if a cursor is needed, it is durable per-scope
state and core is the only thing that holds any."* It is not needed. **So the strongest argument for
core owning the inbound path is gone**, which is a result about promotion rather than about polling.

### Criterion 4: the adapter's loop does not fit

`use_sync_loop` takes a `SyncStep` and a `CountsStep` — one call returning one report, and one call
returning counts. An invalidation round is **several sources with per-source failure**, and there is
nowhere in that shape to put "source B dropped, so entity Y is stale and entity X is not". The trial
runs its own `use_future` over a `Sleeper` (`examples/todo-app/src/session.rs`), and the wake it
races is the application's own `visibilitychange` rather than the adapter's `pageshow` — a tab being
hidden is a *policy* judgement about spending requests, where a bfcache restore is a correctness
event.

### The finding criterion 1 turned up instead: the seam cannot be `dyn`

`Vec<Box<dyn InvalidationSource>>` does not compile. `poll` is an `async fn` in a trait, and an
`async fn` in a trait is not dyn-compatible — the returned future has no name to put behind a
pointer.

**Core has this constraint everywhere and has never paid for it.** `SyncTransport`, `OutboxStore`
and `CacheVersionStore` all carry `#[allow(async_fn_in_trait)]` under decision 001 and are all used
statically, which works because an application has *one* of each. **An invalidation source is the
first seam where one application plainly wants several at once, of different types.**

The trial enumerates them (`todo_core::invalidation::Source`), which is fine for two and does not
generalise. A core `InvalidationSource` would have to answer a question no core trait has faced: box
the future and take an allocation per poll, or make the set generic and fix it at compile time.

### The recommendation

**Do not promote yet**, and for a sharper reason than "wait for SSE": criterion 3 removed the
argument that made core the natural owner, and the `dyn` finding named a cost core has never had to
pay. Both point the same way — the seam is doing fine where it is, and the next thing that would
change the answer is a stream implementation, not more polling.

## What This Decision Is Not

**It is not a restatement of "SSE is out of scope."** That answer was too glib and the proposal says
why: it defended core not owning an `EventSource`, which nobody disputed, while leaving unexplained
why the outbound path has a wire contract, a trait, a driver and a cadence policy and the inbound
path has a method you can call.

**It is not a claim that the inbound path stays out of core.** The expectation is the opposite. This
decision says only that the shape is not known yet and that the trial is how it becomes known.

**It does not touch decision 014's holding.** Core still does not gate refetches. But 014's stated
reason — *"because core does not perform them"* — rests on a premise decision 032 moved when it gave
core the row store, and that sentence should not be quoted as settled while the trial is running.

## Revisit If

- The trial ships and the criteria above are met. Then promotion is owed, and this page is what it
  is measured against.
- RepForge needs the inbound path before the trial finishes. Then the bar above is the thing to
  argue with explicitly, rather than quietly shipping a trait to unblock a consumer.
- A second application adopts frontbox and writes its own invalidation loop. Two independent
  hand-rolled loops is the same evidence D4a produced for `use_sync_loop`, and it would close this
  faster than the trial will.
