# frontbox Tracks What Is Stale, Not What The Data Is

Document Class: Decision
Status: Accepted 2026-08-28; implementation not authorized
Date: 2026-08-28
Category: Cache Versioning
Scope: Whether frontbox stores read-model rows, at what granularity staleness is tracked, and who owns the verification pass that clears it.
Sources: `src/cache/store.rs`, `src/cache/runner/report.rs`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/references/repforge-single-flight-proposal.reference.md`
Related: `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/015-cache-version-persistence.decision.md`, `wiki/decisions/021-cache-version-identity.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/proposals/repforge-read-model-convergence.proposal.md`

## Decision

**frontbox does not store read-model rows.** The application owns the data; frontbox owns the
bookkeeping about whether that data can be trusted.

RepForge's §5b.3 proposes a row shape of `(seq, id, entity_type, blob, hash, stale, …indexed
columns)` and §5b.4 an algorithm that skips and flags *rows*. Read literally that makes frontbox a
read-model store, which D5's scope excludes in one line — "App-specific read-model stores unless they
are examples." The proposal does not flag this as a scope change and it is the largest one in the
document.

The split this decision draws instead:

- **Staleness markers become row-addressable.** frontbox stores `(entity, row_id, stale)` — an
  opaque row identifier and a flag, durable and scope-stamped, and **never the row's contents.**
- **The application keeps the blob**, its indexes, and its query layer.
- **frontbox signals the verification trigger** rather than performing the refetch, consistent with
  decision 014.

## Why

**The algorithm needs durable per-row markers; it does not need frontbox to hold the rows.** §5b.4's
own justification is precisely about the marker, not the data: "The `stale` flag must be persisted,
or a restart between skip and drain loses the marker silently." That is exactly the argument
decision 015 already made about `(version, stale)`, applied one level down. Granting it costs a
narrow table; granting the row shape around it costs frontbox its scope boundary.

Today the tracking is entity-granular. `CacheVersionStore::state(entity: &str) -> EntityState` and
`PendingConflict::ForEntity { pending }` both answer questions about an entity type, not a row. So
§5b.4 is a real extension and not a misreading — it just does not have to drag the blob along.

**Row identifiers can be opaque for the same reason entity keys are.** Decision 007 made the entity
key an application type reached through `as_str`; a row id is the same problem one level down. Core
compares row ids for equality and stores them; it never parses one, and a `String` keeps the
IndexedDB and SQLite backends from needing to agree on anything but bytes.

**The data hole §5b.4 identifies is real, and decision 017 makes it wider.** Their chain:

> row has pending mutation → pull skips it → mutation is `Rejected` → dead-lettered → server state
> never changed → NO second invalidation ever fires → local row keeps the optimistic value the
> server refused — forever

Decision 017 opens a second mouth on the same hole: a record dead-lettered at the *attempt bound*
also terminates without the server state ever changing, and it does so for records the server never
refused at all. So the trigger has to be what RepForge says it is — the outbox emptying **however it
emptied** — and 017 makes that requirement stronger rather than incidental. frontbox is the only
component that can observe the condition, since it owns `pending_count`.

**But frontbox reports the trigger; it does not act on it.** Decision 014 settled this shape:
asking what is stale also reports what refetching would discard, and core does not perform the
refetch so it cannot gate one. A verification pass is a refetch. The same reasoning that kept
frontbox out of the pull keeps it out of the verification, and the same reasoning that made
`PendingConflict` a *report* makes this one too.

**This is also what keeps §5b.2's skip honest.** RepForge is candid that the hash-match skip "can
only help; it should not be designed around" and will fire rarely at first. A design where frontbox
owned the rows would have to take a position on byte-exact canonical serialization of the
application's DTOs — the thing §5b.3 puts in a shared `contracts` crate precisely because getting it
wrong fails silently. Not holding the rows means frontbox cannot be wrong about their bytes.

## Consequences

- **This resolves the open item** *"Decide whether local read-model persistence belongs in core or
  app-owned companion traits"* (`wiki/index.md`, Open Work): app-owned, with core holding markers.
- **A third durable schema for D5**, after the outbox and the version store: `(scope, entity,
  row_id, stale)`, uniqueness on `(scope, entity, row_id)`. Smaller than the other two and subject
  to the same scope-stamping rule as decision 009.
- **`InvalidationReport` grows a row dimension.** `StaleEntity` currently answers "is this entity
  stale and would refetching discard work"; the row-level answer is "which rows within it are
  flagged". The entity-level answer stays — it is what an application without row tracking uses.
- **A verification-trigger signal is owed**, and its shape is a reporting one: the outbox for this
  scope reached zero, with the counts that got it there. A caller that ignores it is in exactly the
  position decision 014 describes — it must *actively* ignore a reported conflict to clobber unsent
  work.
- **Unbounded growth needs a position.** Markers accumulate for rows the application deleted, and
  frontbox cannot know it deleted them because it does not hold the rows. Clearing on verification
  covers the normal path; a `sweep` the application drives covers the rest. This is the cost of the
  split and it should be stated rather than discovered.
- **Conformance cases are owed** in decision 015's shape: flag a row, reopen the store, observe the
  flag survives; and a second scope on the same physical store sees none of it.

## Revisit If

D4 shows applications routinely reimplementing the same blob store above frontbox, with the same
bugs. That would be evidence the boundary is drawn in the wrong place — though the remedy would be
an *example* backend rather than core scope, which is what D5's exclusion already leaves room for.
