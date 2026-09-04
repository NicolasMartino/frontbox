# Cache Version And Staleness Persist Together, Or Not At All

Document Class: Decision
Status: Accepted; implemented 2026-08-27
Date: 2026-08-27
Category: Cache Versioning
Scope: Whether entity cache-version state survives a restart, and what has to survive alongside it for the result to be correct.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/cache.rs`, `wiki/specs/source-frontend-cache-architecture.spec.md`
Related: `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/plans/d2-cache-invalidation.plan.md`, `wiki/roadmaps/extraction.roadmap.md`

## Decision

Cache-version state is **durable by default**, and the durable unit is the pair `(version, stale)`
per entity per scope. A backend stores both or neither. Storing the version without the staleness
flag is not a permitted implementation.

An in-memory implementation remains available and supported. It is not the default, and its
restart behaviour is documented as *"everything is stale"* rather than left to be discovered.

Versions are scoped exactly as records are, per decision 009: stamped with the store's `ScopeKey`,
verified on read, and a mismatch is not readable. A version means nothing outside the scope that
recorded it.

## Why

**Persisting the version without the staleness flag is worse than persisting nothing.** This is the
part that decides the shape, and it is easy to get wrong by doing the obvious thing.

The source's transition is (`persistence/cache.rs:91-116`):

```rust
if server_version > current {
    self.versions.insert(entity, server_version);   // local catches up
    self.stale.insert(entity);                      // and is marked for refetch
    Updated
}
```

The local version advances *at the moment of invalidation*, not at the moment of refetch —
`mark_fresh` clears staleness and leaves the version alone. So immediately after an invalidation,
local version already equals server version while the data is still unfetched. Staleness is the only
thing that remembers the refetch is owed.

Persist the version alone and restart there: local is `5`, the server says `5`, the comparison
yields `NoChange`, and the entity is considered **fresh without ever having been refetched**. The
client now serves data the server explicitly invalidated, and nothing will correct it until the next
invalidation happens to arrive. Memory-only state fails safe by comparison — losing everything means
local reads as `0`, every entity is marked stale, and the client over-fetches. Half-persistence
turns a safe failure into a silent correctness bug.

Hence the pair, and hence the prohibition rather than a recommendation.

**Durable is right because the interesting restart is an offline one.** In the source, state is a
`HashMap` and a `HashSet` held in memory (`persistence/cache.rs:69-71`); the source spec's `## Known
Gaps` flags that a reusable library has to decide this. Memory-only means every cold start marks
everything stale and refetches everything. For a network-first application that is merely wasteful.
For an offline-first one it is close to self-defeating: the case that matters is launching with no
connectivity, holding durable read models, and needing to know what in them can be trusted. A client
that considers all of its data stale and cannot refetch any of it has kept the data and thrown away
its meaning.

**The pair must be written atomically**, which is the same argument decision 003 makes about outcome
application. A crash between "version advanced" and "marked stale" reproduces exactly the
half-persisted state above. One transition, one write.

**Scoping is not optional here either.** Cache versions are per-user server state. Two scopes
sharing a physical store — the arrangement the conformance suite deliberately forces — must not read
each other's versions, or a user switch would leave the new user believing the previous user's
freshness. Decision 009 already settled the mechanism; this only names versions as a second thing it
covers.

## Consequences

- **D5 gains a second durable schema.** Roughly `(scope, entity_key, version, stale)` with a
  uniqueness constraint on `(scope, entity_key)`. It carries the lesson `MutationId` already taught:
  the key is stored in exactly one canonical textual form — whatever `EntityKey::as_str` returns —
  because a store that writes `Exercise` in one row and `exercise` in another has two entities where
  the caller has one. Ordering does not matter here, so no collation requirement follows, but case
  sensitivity does.
- **The in-memory backend needs a version store too**, so the conformance suite can assert both
  behaviours against something before D5 exists — the same reason `InMemoryBackend` exists at all.
- **A durable-versions conformance case is owed** in the shape D1 used for scope isolation: write a
  version under one scope, open another scope on the same physical store, and observe nothing.
- **The restart contract is assertable, and asserted.** Conformance case 39 invalidates an entity,
  reopens the store, and requires that it is *still stale* — precisely the case a version-only
  implementation fails. Case 40 covers scope isolation against a backend where both scopes share
  storage.
- **Implementation moved the seam to strings.** `CacheVersionStore` names entities by
  `EntityKey::as_str` rather than by the application's key type: a store has no registry to
  reconstruct typed keys with, and a durable one can hold names a newer build no longer models.
  D5's backends are therefore not generic over any application type.
- **`mark_fresh` becomes a durable write**, not a memory update. It is the caller's statement that a
  refetch completed, and it is what stops the client refetching the same entity on every launch.
- **This resolves the open item** *"Decide whether cache version state is persisted by default"*
  (`wiki/index.md`, Open Work).

## Amended 2026-08-28 By Decision 021

This page says "version" throughout and D2 built it as a `u64` compared by magnitude. Decision 021
makes it an **opaque identity compared by equality only**, after RepForge asked (§10 Q4) whether
this decision assumed ordering. It did.

Nothing in this page's actual holding changes: `(version, stale)` remains one atomically written
unit, half-persistence remains prohibited, and the argument for durability is untouched — an opaque
identity persisted without its staleness flag fails in exactly the way described above. What changes
is the type of one field and the fate of `VersionUpdate::NeedsReset`, which decision 021 carries.

## Built Durably 2026-08-30, And The Fourth State It Exposed

**This decision had one implementation for four days longer than the roadmap said it did.**
`InMemoryVersionStore` was the only `CacheVersionStore` until D5's cache half landed; SQLite and
IndexedDB now have one each, and `frontbox_cache_tests` runs on all three.

Both durable backends honour the atomicity this page requires the same way, for the same reason. In
SQLite, `version` and `stale` are two columns of one row, so a single `UPSERT` inside an `IMMEDIATE`
transaction writes both or neither. In IndexedDB they are two fields of one record, written by one
`put` inside one transaction. Neither could tear the pair without going out of its way to.

**Writing the second implementation exposed a state neither this page nor the type could express.**
`EntityState` is `#[non_exhaustive]` with three constructors, covering `(None, false)`,
`(Some, true)` and `(Some, false)`. The fourth — **stale with no version** — had no constructor, so
a backend outside the crate reading a null version had to answer `unknown()` and drop the staleness
on the floor.

That is this page's own failure mode, reached from the other direction. This decision exists because
persisting `version` without `stale` leaves a client believing invalidated data is fresh; a backend
that cannot *reconstruct* `stale` without a version does the same thing, on the entity set
`InvalidationRunner::mark_all_stale` touches — which is every registered entity no invalidation has
ever named, on a client that has just installed and just hit an error. The pair being one unit on
disk is not enough if the type cannot carry one of its four values back.

`EntityState::from_parts` closes it and case 67 holds it closed, asserting both that the staleness
survives a reopen and that the entity still appears in `all_states` — because `stale()` walks the
enumeration, and a backend that filtered null versions out of it would pass a state-by-name check
and still never refetch.

## Revisit If

An application appears whose entity data is cheap enough to refetch that durable freshness tracking
costs more than it saves — a small, always-online read model where the whole point is to re-pull on
launch. The in-memory implementation already serves that case; it would only need to become the
default if it turned out to be the common one, and D4's migration trial is the first place that
could be observed.
