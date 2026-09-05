# Cache Versions Are Optional, And What You Give Up By Skipping Them

Document Class: Compatibility
Status: Accepted 2026-09-06
Date: 2026-09-06
Category: Cache Versioning
Scope: Whether an adopting application has to implement `CacheVersionStore`, how to decide, and what stops working if it does not. Not about how invalidation events arrive, which core does not own.
Sources: `src/cache/store.rs`, `src/cache/runner/conflict.rs`, `src/cache/runner/mod.rs`, `src/cache/runner/report.rs`, `src/memory/versions.rs`, `crates/frontbox-sqlite/src/versions.rs`, `crates/frontbox-indexeddb/src/versions.rs`, `src/testing/macros/mod.rs`, `examples/todo-core/src/invalidation.rs`, `examples/todo-core/src/backend.rs`
Related: `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`, `wiki/proposals/invalidation-delivery.proposal.md`

## The Wrong Turn This Page Exists To Prevent

An adopter opened a `CacheVersionStore` handle while integrating the cache runtime, **because the
reference application did**, and removed it later on discovering that nothing in their application
read it. Nothing broke. It cost them a detour and the belief, for a while, that versions were part
of the shape they had to adopt.

That belief is wrong and this repository caused it. Versions have been structurally optional since
they were built; no page said so, and the one place a reader meets the trait introduces it as *"the
third storage trait, alongside `OutboxStore` and its companions"* — which reads as an inventory of
what a backend owes rather than a menu. This page is the correction.

## Optional Is Already True In The Code

Nothing needs to change for an application to skip versions entirely. The separation is structural:

- **A separate trait.** `CacheVersionStore` lives in `src/cache/store.rs` and shares no method with
  `OutboxStore`. Skipping it removes nothing from the outbox, the runner, the drain loop, dead
  letters, or quarantine.
- **A separate type in every backend.** `SqliteVersionStore`, `IdbVersionStore` and
  `InMemoryVersionStore` are distinct from their outbox stores — see
  `crates/frontbox-sqlite/src/versions.rs`, `crates/frontbox-indexeddb/src/versions.rs`, and
  `src/memory/versions.rs`. The trial takes it as a *second handle* rather than a second trait on
  one type, and `examples/todo-core/src/backend.rs` says why: both traits declare `scope()`, so one
  type implementing both would make every existing `store.scope()` call ambiguous.
- **A separate conformance suite.** `frontbox_cache_tests!` needs `VersionStoreFactory` and is
  invoked on its own. `src/testing/macros/mod.rs` states the rule directly: a backend with no
  version store simply does not invoke it, which leaves the gap visible in its test file rather than
  hidden behind a runtime skip.

An application that never constructs an `InvalidationRunner` never touches any of it.

## Deciding: Do You Need Versions At All

A version is a fact the *server* keeps per entity per user, which the client compares by equality
(`wiki/decisions/021-cache-version-identity.decision.md`). What it buys is knowing **which** entities
changed without asking what they now contain.

| Reach for versions when | Re-reading on a schedule is the better answer when |
| --- | --- |
| One screen aggregates many sources and refetching all of them is the expensive part | The screen has one or two sources and the read is cheap |
| A read is costly — large payloads, paginated collections, an expensive server query | The read is already paginated, so a refetch fetches one page rather than a collection |
| Invalidation should be selective: five entities changed out of forty | Almost everything changes together anyway, so selectivity buys nothing |
| Refetch cadence must be tight and request volume must stay low | The staleness budget is loose enough that polling the data directly is affordable |
| A server already publishes versions | Adding a versions endpoint is net-new server work |

**A plain periodic re-read is a legitimate invalidation strategy, not a shortcut.** It is what most
applications should start with. Versions are an optimisation over it, and like any optimisation they
are worth adopting when the cost they remove is one you are actually paying.

One sizing note the trial learned the hard way, in `examples/todo-core/src/invalidation.rs`: a
cadence is a **staleness budget, not a poll schedule**. "This entity may be up to N seconds stale"
does not mean "issue a request every N seconds" — one `GET /versions` typically answers for every
entity a service owns, so a literal per-entity schedule costs N times the requests and buys nothing.
Poll at the tightest budget among the entities a source covers and let the rest be fresher than
promised. That reasoning applies unchanged to a periodic re-read.

## What You Give Up, And The One That Bites

Two things stop being available, and they are not equally important.

**Selective invalidation.** Expected, and the whole point of the trade. Without versions you refetch
on a schedule instead of refetching what changed.

**The pending-write conflict report — and this is the one to read twice.**
`InvalidationRunner::stale` is generic over `CacheVersionStore` (`src/cache/runner/conflict.rs`), so
dropping the version store drops it. What it answers is not a question about versions at all:

> refetching this entity would discard unsent local work.

It reports `PendingConflict::None`, `ForEntity { pending }`, or `Unattributed { pending }`
(`src/cache/runner/report.rs`), the last being deliberately conservative — *something is queued and
it might be this* — because a false "nothing is queued" is the answer that loses data.

`wiki/decisions/014-pull-gating.decision.md` exists because of a real defect in the system this crate
was extracted from, described in `src/cache/runner/conflict.rs` as a listener that *"refetches
eagerly and never consults the outbox"*. An adopter who reads "periodic re-read is fine", removes the
version store, and re-reads on a timer has faithfully reconstructed that defect. Its failure mode is
silent: the user's unsent edits are overwritten by server state and nothing reports it.

**So give this up on purpose, not by accident.**

## What To Do Instead, If You Skip Versions

The conflict check does not need versions. It needs the outbox, which you already have. Before
replacing local state with a fresh read, ask whether anything is queued:

- `OutboxStore::pending_count` is the cheap, conservative form and is exactly what
  `InvalidationRunner::stale` itself calls. Non-zero means *something* is unsent; treat a refetch as
  destructive.
- `OutboxStore::pending_batch` is the precise form. It is inspection-only — it does **not** mark
  records transport-started — so an application may attribute pending records to entities itself and
  narrow "something is queued" to "this entity has queued work". That attribution is what
  `stale_classified` does with a caller-supplied classifier, and there is nothing privileged about
  doing it inside the runner.
- Bound the scan. `DEFAULT_CONFLICT_SCAN` matches `DEFAULT_BATCH_LIMIT` on purpose; a queue longer
  than the scan should report "unattributed" rather than "clear".

What core will not do either way is perform the refetch or gate it
(`wiki/decisions/023-read-model-boundary.decision.md`). It reports the cost; the application decides.
That boundary is why the check is portable out of the runner in the first place.

## Optional Is Not The Same As In-Memory

Two questions that look alike and are not:

1. **Do I need versions?** Answered above. "No" is a common and correct answer.
2. **If I use versions, must they be durable?** Yes, effectively.
   `wiki/decisions/015-cache-version-persistence.decision.md` has the argument: version and staleness
   are written as one atomic unit, and a client that loses them reads every entity as version zero
   and refetches everything on every launch — which for an offline-first application defeats the
   point of having kept the data. `InMemoryVersionStore` exists for tests and examples, and is the
   exception rather than a supported deployment.

Blurring the two turns "versions are optional" into "an in-memory version store is fine", which is
the worst of both: the machinery, none of the benefit.

## What This Page Does Not Claim

- **Not that invalidation delivery is solved.** How events *arrive* — polling, SSE, websockets — is
  not in core and is not promised. `wiki/decisions/038-invalidation-delivery-in-the-trial.decision.md`
  records why it is built in the trial first and what would have to be true to promote it.
- **Not that periodic re-read is always sufficient.** It is a legitimate default. An application with
  many sources and expensive reads will outgrow it, and the table above is where to check.
- **Not a deprecation.** `CacheVersionStore` is fully supported, conformance-tested on all three
  backends, and used by the trial. This page changes documentation, not status.

## Revisit If

- Invalidation delivery is promoted into core, at which point "what does an adopter have to wire up"
  changes shape and this page's first two sections need rereading.
- The conflict report is extracted so it can be asked without an `InvalidationRunner`, which would
  remove the sharpest consequence of skipping versions and make this page mostly a sizing guide.
- A second adopter reports the opposite wrong turn — skipping versions where they were needed —
  because that would mean the table above is calibrated wrong.
