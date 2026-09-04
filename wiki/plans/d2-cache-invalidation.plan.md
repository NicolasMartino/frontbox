# D2 Cache Version And Invalidation Plan

Document Class: Plan
Status: Completed
Date: 2026-08-27
Category: Implementation Preparation
Scope: Prepare the second extraction slice: generic entity keys, cache version reconciliation, invalidation handling, and the conflict signal an application needs before it replaces local state.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/cache.rs`, `raw/initial/2026-08-25T083750Z/sources/listener.rs`, `raw/initial/2026-08-25T083750Z/sources/frontend/sse.rs`, `raw/initial/2026-08-25T083750Z/sources/cache_versions.rs`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/source-test-inventory.reference.md`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/decisions/013-unknown-entity-name.decision.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/decisions/012-unknown-mutation-status.decision.md`, `wiki/plans/d1-core-cache-runtime.plan.md`

## Objective

Define the D2 slice before code starts: a framework-neutral cache-version runtime that models
staleness over caller-owned entity keys, reconciles against server versions on reconnect, and tells
an application when refetching would destroy unsent local work.

**Implemented 2026-08-27** under explicit user authorization, in the same session that wrote this
plan and decisions 012-015. See `## Implementation Outcome` at the end for what was built and what
implementation changed.

D1's precedent is the reason for the order. Decisions 008 and 009 were written before D1 was
authorized and both turned out to constrain the public API in ways that would have been breaking to
find afterwards; decision 010 was written *during* D1 and cost an amendment three weeks later.

## Inputs

- `raw/initial/2026-08-25T083750Z/sources/persistence/cache.rs` — `EntityType`, `EntityCache`,
  `VersionUpdateResult`, and the 6 tests that are D2's oracle.
- `raw/initial/2026-08-25T083750Z/sources/listener.rs` — event handling, reconnect reconciliation,
  eager refetch, and 3 of its 9 tests.
- `raw/initial/2026-08-25T083750Z/sources/frontend/sse.rs` — the wire event shape.
- `raw/initial/2026-08-25T083750Z/sources/cache_versions.rs` — the server's own entity enum, for
  reference only.
- Decisions 007 (registry), 012 (unknown status), 013 (unknown entity), 014 (pull conflict),
  015 (version persistence).

## D2 Deliverables

Included:

- `EntityKey` and `EntityRegistry` traits, per decision 007's sketch.
- Version comparison yielding the source's three outcomes, with the `Option` removed (below).
- Invalidation application over one or many events, returning what changed and what was not
  understood (decision 013).
- Reconnect reconciliation against a server version map, reporting unknown names rather than
  dropping them.
- A durable `(version, stale)` store per scope, with an in-memory implementation (decision 015).
- The pending-write conflict signal and its optional caller-supplied classifier (decision 014).
- Conformance cases 34 onward, added to the shared list.

Excluded:

- The SSE transport, reconnect/backoff policy, and anything that decides *when* events arrive. The
  source's listener mixes these with cache semantics; 6 of its 9 tests are transport concerns and
  belong to D3.
- Read models, refetch, and `replace_all`-shaped operations. Whether read-model persistence belongs
  in core at all is still open (`wiki/proposals/extraction-boundary.proposal.md:262`) and D2 must
  not answer it by accident.
- Dioxus signals, RepForge entity names, server-side invalidation production.

## Proposed Core Shapes

Starting from decision 007, whose trait names are explicitly not final but whose registry obligation
is:

```rust
pub trait EntityKey: Clone + Eq + std::hash::Hash + 'static {
    fn as_str(&self) -> &str;
}

pub trait EntityRegistry<K: EntityKey> {
    fn all(&self) -> &[K];
    fn parse(&self, wire: &str) -> Option<K>;
}
```

`parse` returning `None` is a reportable outcome, not a discard (decision 013).

Version comparison keeps the source's vocabulary — `Updated`, `NoChange`, `NeedsReset`
(`persistence/cache.rs:56-63`) — because the three cases are real and the names are clear.

### Drop the `Option` on the server version

The source signature is `update_version(&mut self, entity, server_version: Option<u64>)` and treats
`None` as `0` (`persistence/cache.rs:96`), which reads as an anomaly for any client with a non-zero
local version and forces a spurious reset. Decisions 007 and the source spec both flag this as
behaviour to document.

**Checking the call sites changes the recommendation from "document it" to "delete it."** All eleven
callers in the corpus pass `Some(...)` — `listener.rs:466`, `persistence/cache.rs:132`,
`preferences.rs:754`, and eight in tests. Nothing has ever passed `None`. The one thing that might
have needed it, `LegacyInvalidationEvent`, carries no version and is **declared but never used
anywhere in the corpus** (`frontend/sse.rs:26-34`).

So the `Option` is not a feature with a hazard attached; it is unexercised surface whose only
behaviour is the hazard. D2 takes `u64`. An event genuinely carrying no version is a different
operation — "mark stale, version unknown" — and should be named as one if a server ever needs it.

### Reporting

Both the per-event path and reconnect reconciliation return a value rather than mutating in silence.
It carries, at minimum: what was marked stale, what needs reset, and which wire names were not
understood. Decision 014 requires that asking about staleness also surfaces pending-write conflict,
so the staleness query and the conflict query are not two independent calls a caller can get half
right.

The classifier for conflict attribution is caller-supplied, roughly
`fn(&OutboxRecord) -> Option<K>`, and its absence degrades to whole-outbox. The plan must state
what happens when it is inconsistent between calls — the safe reading is that core never caches its
answer.

## Ordering And Scope Policy

Version state is scoped exactly as records are (decision 009): stamped, verified on read, mismatch
unreadable. `(version, stale)` is one durable unit written atomically (decision 015, decision 003).

The entity key is stored in exactly one canonical textual form, `EntityKey::as_str`. This is the
`MutationId` lesson transferred: mixed forms give a backend two entities where the caller has one.
Ordering is not required of entity keys, so no collation constraint follows — but case sensitivity
does.

## Test Oracle

**Direct ports** — the 6 tests in `persistence/cache.rs:191-258`: `test_update_version_normal`,
`test_update_version_no_change`, `test_update_version_anomaly`, `test_reset_version`,
`test_mark_fresh`, `test_check_versions`.

**Adapted** — 3 of the 9 listener tests, the ones that are cache semantics rather than transport:
`handle_invalidation_event_updates_template_version_and_marks_fresh_after_refetch`
(`listener.rs:719`), `handle_invalidation_event_no_change_keeps_entity_fresh` (`listener.rs:738`),
and `handle_invalidation_event_ignores_unknown_entity` (`listener.rs:759`). The last is a direct
oracle for decision 013 — the source asserts the ignoring, and frontbox must additionally assert the
reporting.

**Not ported** — the other 6 listener tests (`listener.rs:616`, `:643`, `:666`, `:689`, `:775`,
`:802`) are unauthorized/timeout/stream-error backoff and reconnect delay. They are D3's, and the
inventory's "9 listener tests, D2/D3 evidence" line should be read with that split in mind.

**New cases, numbered from 34** — the D1 suite used 1–33. Every case must be added to the single
shared list in `__frontbox_conformance_suite!` (`src/testing/macros/mod.rs`), which is deliberately the
only place the list appears so the synchronous and `async` emissions cannot drift.

| # | Case | Proves |
| --- | --- | --- |
| 34 | An unknown status applies every known verdict in the batch and retains the unknown one, with the server's spelling intact | Decision 012 |
| 35 | An event naming an unregistered entity changes no cache state and appears in the report | Decision 013 |
| 36 | Reconnect reconciliation with an unknown name in the server map reconciles the rest and reports the remainder | Decision 013 |
| 37 | A server version below local yields `NeedsReset` and does not silently advance | `cache.rs` oracle |
| 38 | `mark_fresh` clears staleness without moving the version | `cache.rs` oracle |
| 39 | An entity invalidated, then the store reopened, is still stale | Decision 015 — the case a version-only implementation fails |
| 40 | Versions written under one scope are invisible to another sharing the same physical store | Decision 015 + 009 |
| 41 | An invalidation for an entity with a queued mutation reports stale *and* conflicting | Decision 014 |
| 42 | With a classifier, an unrelated queued mutation reports stale without conflict; without one, with conflict | Decision 014 |
| 43 | A permanently retained outbox record does not suppress staleness reporting | Decision 014 liveness |

## Out Of Scope For D2

- D3 Dioxus adapter, D4 migration trial, D5 durable backends.
- Retry/backoff, attempt tracking, and aging of retained work. Decision 012 makes this more urgent
  but it remains its own question.
- Deciding whether read-model persistence belongs in core.

## Verification Gates For Implementation

Inherited from D1 unchanged, and `./scripts/verify.sh` runs them:

- `cargo fmt --check`, clippy native and wasm with `-D warnings`, tests, both wasm builds.
- Coverage floor 80%, measured across the crate.
- Secondary greps: no Dioxus, no RepForge entity names, no `Send` bound, no date library in the
  runtime dependency graph.

D2-specific:

- The conformance suite passes on the in-memory backend through `StoreFactory`, including the new
  version store.
- No RepForge entity name appears in core — the `grep` gate already covers `Exercise`, `Session`,
  `Template`, `Preferences`, which is exactly the source enum D2 is generalizing away from.

## Constraints

- Files stay under ~400 lines; split into a directory with a facade `mod.rs` rather than deleting
  content, and public paths must not move.
- No `Send` bounds; `async fn` in traits, generic parameters not trait objects (decision 001).
- Time enters only through `Clock`. No date library in the runtime graph (decision 011).
- Public API additions need their own go-ahead, separately from the D2 authorization itself.

## Implementation Outcome

**What was built.** `src/entity.rs` (`EntityKey`, `EntityRegistry`, `SliceRegistry`), `src/cache/`
(`EntityState`, `InvalidationEvent`, `VersionUpdate`, `compare`, `CacheVersionStore`, and
`InvalidationRunner` with its report types), and `src/memory/versions.rs`. Decision 012's D1 change
shipped alongside: `MutationStatus::Unknown(String)`, `Anomaly`/`AnomalyKind`, and
`SyncOutcomeCounts::unknown_status`.

96 tests pass, up from 77. Coverage 89% regions / 95% lines / 93% functions against the 80% floor.
All `scripts/verify.sh` gates green.

### What implementation changed

- **`CacheVersionStore` speaks strings, not the application's key type.** The plan sketched it over
  `Self::Key`; that does not work. `all_states` has to reconstruct keys from storage, and a store
  has no registry — worse, a durable store outlives the build that wrote it, so it can legitimately
  hold a name the current registry no longer models. Making the seam speak
  `EntityKey::as_str` keeps that honest and stops D5's backends having to be generic over an
  application type they never interpret. `InvalidationRunner` owns the typed API, because it is the
  thing that holds the registry.
- **`EntityRegistry` uses an associated `Key` type**, not the generic parameter decision 007
  sketched. One registry serving several key types is not a thing anyone wants, and the associated
  form removes a parameter from every signature downstream.
- **A separate `VersionStoreFactory` trait and `frontbox_cache_tests!` macro**, rather than
  extending `StoreFactory`. This follows the existing `FaultInjection` precedent: a backend without
  a version store does not implement it and does not invoke the macro, which leaves the gap visible
  in its test file instead of hidden behind a runtime skip.
- **An incomplete conflict scan degrades to `Unattributed` rather than reporting a clean partial
  view.** The plan said the classifier scan is bounded; it did not say what a bounded scan should
  conclude. A false "nothing is queued" is the answer that loses data, so a scan that could not see
  the whole queue declines to give one. Case 42 covers it.
- **`InvalidationEvent` drops the source's `user_id`.** The source's own comment says it is for
  debugging only and must not be used for access control. Rather than carry a field whose
  documentation is a warning, isolation stays entirely with `ScopeKey`.
- **`src/cache/runner/mod.rs` reached 466 lines** and was split into a directory — `mod.rs`, `report.rs`,
  `conflict.rs` — per the code-shape rule. The conflict half is the natural seam: it is the only
  part that reads the *outbox* rather than the version store.

### What implementation found that this plan had wrong

- The plan proposed `fn(&OutboxRecord) -> Option<K>` as "roughly" the classifier signature; that is
  exactly what it is, and it needed no record changes at all. `OperationMeta` was not touched.
- The `no RepForge entity names in core` gate fired on `src/entity.rs` — prose *explaining* the
  source's closed enum still names its variants. Reworded rather than exempted: the gate is a smell
  test and weakening it to allow comments would blunt the thing it exists for.
