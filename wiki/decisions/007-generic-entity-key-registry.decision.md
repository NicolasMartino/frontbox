# Generic Entity Keys Need A Caller-Supplied Registry

Document Class: Decision
Status: Accepted; amended 2026-08-27 by decision 013
Date: 2026-08-25
Category: Cache Versioning
Scope: How frontbox generalizes RepForge's closed entity enums for cache versioning and invalidation.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/cache.rs`, `raw/initial/2026-08-25T083750Z/sources/cache_versions.rs`, `raw/initial/2026-08-25T083750Z/sources/frontend/sse.rs`, `raw/initial/2026-08-25T083750Z/sources/listener.rs`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/013-unknown-entity-name.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/plans/d2-cache-invalidation.plan.md`

## Decision

The cache-version layer cannot hard-code entity names. A generic frontbox cache needs caller-owned
entity keys plus a caller-supplied registry for operations that require enumerating all known
entities.

D2 should model this as a bounded generic or equivalent trait, not as `String` alone:

```rust
pub trait EntityKey: Clone + Eq + std::hash::Hash + 'static {
    fn as_str(&self) -> &str;
}

pub trait EntityRegistry<K: EntityKey> {
    fn all(&self) -> &[K];
    fn parse(&self, wire: &str) -> Option<K>;
}
```

Exact trait names are not final, but the registry obligation is.

## Why

RepForge has closed, domain-specific enums on both sides:

- frontend `EntityType` has `Exercise`, `Session`, `Template`, `UserPreferences`
  (`persistence/cache.rs:14-40`)
- server `EntityType` has plural variants (`cache_versions.rs:17-43`)
- the wire SSE payload uses `entity: String` (`frontend/sse.rs:18`)

The source can call `EntityType::all()` to mark all known entities stale
(`persistence/cache.rs:183-186`). A generic `String` key has no equivalent operation. If frontbox
needs `mark_all_stale`, reconnect reconciliation, or typed stale callbacks, the application must
provide the known key set and parser.

Unknown wire entities are currently ignored after logging (`listener.rs:414,465-505`). A reusable
library should make that behavior explicit so apps decide whether unknown entities are ignored,
reported, or treated as protocol errors.

## Consequences

- D2 is not a straight port of `EntityType`; it is a redesign around app-owned keys.
- ~~The API must specify what happens to unknown wire entity names.~~ **Discharged 2026-08-27 by
  decision 013:** ignored for cache purposes, reported to the caller. That decision also *amends*
  this one — the suggestion above that apps choose between "ignored, reported, or treated as
  protocol errors" is declined in favour of a single behaviour that lets an application implement
  any of the three itself.
- ~~Version comparisons must document the current source behavior where `update_version(entity,
  None)` treats the server version as `0`~~ (`persistence/cache.rs:96`). **Superseded 2026-08-27:**
  the D2 plan drops the `Option` instead of documenting it. All eleven callers in the corpus pass
  `Some(...)`, and `LegacyInvalidationEvent` — the one shape that carries no version — is declared
  and never used. It was unexercised surface whose only behaviour was the hazard.
- Read-model cache validity predicates, such as RepForge's safe exercise fallback rule, belong next
  to the caller's entity/query model rather than in generic core.

## Revisit If

D2 removes all operations that require enumerating or parsing entity keys. As long as those
operations exist, a registry or equivalent caller hook is required.
