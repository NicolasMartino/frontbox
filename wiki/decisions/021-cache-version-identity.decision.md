# A Cache Version Is An Opaque Identity, Not An Ordered Counter

Document Class: Decision
Status: Accepted and implemented 2026-08-28 — this one changed shipped D2 code
Date: 2026-08-28
Category: Cache Versioning
Scope: Whether an entity's cache version is comparable by magnitude or only by equality, and what that costs the D2 types that are already built.
Sources: `src/cache/mod.rs`, `src/cache/runner/report.rs`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/decisions/013-unknown-entity-name.decision.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`

## Decision

A cache version is an **opaque identity compared by equality only**. It is not a number, it does not
order, and "the server is behind" stops being a state this crate can express.

RepForge asked (§10 Q4) whether decision 015 assumes ordering. **It does, everywhere, and it is
shipped.** This decision answers the question by conceding it.

- **`EntityState::version` becomes an opaque token**, not `u64`. Equality is the only operation.
- **`VersionUpdate::NeedsReset` loses its producer** and must be reconsidered rather than quietly
  kept, exactly as decision 005's `Blocked` was.
- **Zero stops being a safe sentinel.** `EntityState::unknown()` cannot keep meaning "version 0".
- **`(version, stale)` stays one atomically written unit.** Decision 015's core holding is untouched
  and is if anything strengthened.

## Why

**The built code assumed ordering in five places, and two of them existed only for it.**

```rust
pub fn compare(local: EntityState, server_version: u64) -> VersionUpdate {
    if server_version > local.version {
        VersionUpdate::Updated
    } else if server_version < local.version {
        VersionUpdate::NeedsReset
    } else {
        VersionUpdate::NoChange
    }
}
```
(`src/cache/mod.rs:142-149`)

`EntityState { version: u64, stale: bool }`, `InvalidationEvent { entity: String, version: u64 }`,
and `StaleEntity { key, version: u64, conflict }` all carry the number.

A fifth site was found during implementation and is not in the first draft of this page:
`reconcile_pairs` collapsed repeated events for one entity with `(*current).max(version)`. Under
opaque identities "highest" has no meaning, so the collapse became **last-wins**. That is the more
faithful rule independently of hashing — the final event is the server's current answer, and under
`max` an entity that changed and then changed *back* would have kept the intermediate version
forever. But `compare` is where the
assumption is load-bearing: **`NeedsReset` is reachable only through `<`.** Its documentation says
so — "the server is *behind* the local version, which cannot happen in normal operation... the
server's counter restarted". Under a set hash there is no behind. A hash is either the one you hold
or a different one, and a different one means refetch. So the three-way answer collapses to two, and
the variant that exists to describe a restored database has nothing left to describe.

That is not a small edit. `VersionUpdate` is public and `#[non_exhaustive]`, so *adding* variants is
additive but removing one is breaking, and its removal changes what `check_versions` can report.

**Zero is not a safe sentinel under XOR set hashing, and this is the sharp one.**
`EntityState::unknown()` is documented as "version zero, not stale — an application that has never
heard of a version has also never been told its data is wrong". Under a counter that is sound.
Under RepForge's scheme:

> `set_hash ^= old_row_hash; set_hash ^= new_row_hash`

**XOR's identity element is zero, so the hash of an empty set is zero.** "I have never heard of this
entity" and "the server says this entity holds no rows" become the same stored value. A client that
has never synced and a client whose collection was legitimately emptied are indistinguishable, and
`compare` answers `NoChange` to both. The sentinel has to become explicit — `Option<Version>`, or a
token type where "unknown" is a distinct variant rather than a magic value.

This is the kind of defect that never fails loudly. It produces a client that quietly does not
refetch, which is precisely the failure mode RepForge names for the counter (§5b.1: a deleted row
"is never invalidated and stays cached indefinitely"). Their argument against `max(updated_at)`
applies to a zero sentinel under their own scheme.

**Their reasons for hashing over counting are good, and independent of RepForge.** With `PUT` and a
replaying outbox, an idempotent rewrite bumps a counter and invalidates every device for a change
that changed nothing. That is not a RepForge quirk; it is what happens to any counter behind an
idempotent write path, which is the path decision 016 and structural idempotency are steering
frontbox's consumers toward. A crate that recommends client-generated ids and replayable `PUT`s
should not also assume a counter that every replay perturbs.

**Opaque is the right shape regardless of which scheme wins.** frontbox does not compute the version
and has no business knowing whether it is a counter, a hash, an ETag, or a vector clock. Making it
opaque is the same move decision 007 made for entity keys and decision 014 made for conflict
attribution: core defines where a value is stored and when it is compared, the application defines
what it means. RepForge's §10 Q5 assumes exactly this split, and the assumption is correct.

An opaque token also keeps a counter *expressible*. An application that genuinely has an ordered
version stores its rendering and compares it by equality; it loses `NeedsReset`, which was only ever
reachable through a server-side restore.

## Consequences

- **This modifies shipped D2 code**, which no decision since D1 has done. `EntityState`,
  `InvalidationEvent`, `StaleEntity`, `VersionUpdate`, and `compare` are all public and all affected.
  It is cheap only because `publish = false`; after publication it is a major version.
- **`VersionUpdate::NeedsReset` was removed.** This decision originally named three options and
  leaned to the 005 precedent — keep the variant unreachable, as `MutationStatus::Blocked` is kept
  — "absent a reason". Implementing it produced the reason, and it is that the analogy does not
  hold. `Blocked` is kept because *a server can still send it*: the variant has an external
  producer this crate does not control. `VersionUpdate` is produced by `compare`, which is ours and
  nobody else's, so keeping the variant would have meant a public enum arm no code path can reach
  and an `InvalidationReport::needs_reset` field permanently empty — an API that reports a category
  that cannot occur. `VersionUpdate` is `#[non_exhaustive]`, so restoring it later is additive,
  whereas a false surface is not free. The reset *capability* is not lost: `mark_all_stale` is what
  the reset path did anyway.
- **Conformance cases changed, not just types.** Case 37 was
  `case_37_a_server_behind_local_needs_reset`; a smaller version has no special meaning now, so the
  case was rewritten in place as `case_37_a_differing_identity_is_an_update_not_a_reset`, keeping
  its number because a numerically smaller identity is exactly where the old rule and the new one
  disagree. **Case 44 is new** and pins the zero-sentinel bug: an entity the server says is empty
  must still be fetched once by a client that has never synced. The suite is 44 cases, and
  `src/testing/cases/invalidation.rs` was split — cases 41-43 moved to `pull_conflict.rs` — because
  the additions pushed it past the 400-line cap.
- **`EntityState::version` is `Option<CacheVersion>`**, with `None` for "never heard". `EntityState`
  loses `Copy` as a result, the same trade decision 012 made for `MutationStatus`.
- **`CacheVersion` is a new public type** in `src/cache/version.rs`: a newtype over `String`,
  serialized transparently, stored byte-exact with no trim or case fold — for the reason `ScopeKey`
  does none of those either. The empty string is a valid version, because core never reads the
  contents and a constructor should not be fallible over a field it does not interpret
  (decision 008's reasoning about an empty `OperationMeta` name).
- **D5's version schema stays `(scope, entity, version, stale)`** with `version` widening from an
  integer to a bounded opaque blob. Decision 015's atomicity requirement is unchanged.
- **The equality-only comparison makes the hash-carrying invalidation of §5b.2 free.** Comparing the
  event's identity against the stored one *is* the skip. No extra mechanism is needed for it.

## Revisit If

An application appears that needs to know whether it is behind or ahead rather than merely
different — a client reconciling against an append-only log, where "ahead" means it holds writes the
server has not seen and the remedy is to push rather than pull. That is a real shape and this
decision cannot express it. It is not RepForge's shape, and it is not the source's either: the
source's own `NeedsReset` handling treats "behind" as corruption to be reset from, not as
information to act on.
