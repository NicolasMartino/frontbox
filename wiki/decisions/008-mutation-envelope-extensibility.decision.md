# The Mutation Envelope Is Extensible And Carries Optional Operation Metadata

Document Class: Decision
Status: Accepted
Date: 2026-08-26
Category: Public API Shape
Scope: The shape of the record callers enqueue, how it is constructed, how its JSON body is typed, and what identity it carries beyond `method` and `path`.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`, `raw/research/2026-08-25-prior-art-survey/sources/05-http-command-queues.md`, `raw/research/2026-08-25-prior-art-survey/sources/01-replicache-zero.md`, `raw/research/2026-08-25-prior-art-survey/sources/03-rxdb-watermelon.md`
Related: `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/references/prior-art-survey.reference.md`

## Decision

The enqueue input is a struct that can gain fields without a breaking change, its body is parsed
JSON rather than opaque text, and it carries optional structured operation metadata that core
stores but never interprets.

Three parts, all binding on the D1 public API:

**1. Records are extensible by construction.**

`OutboxRecord`, `DeadLetterRecord`, and `QuarantinedRecord` are `#[non_exhaustive]` and are built
through a constructor plus setters, never a struct literal and never a positional or fixed-arity
enqueue function.

```rust
#[non_exhaustive]
pub struct OutboxRecord {
    pub mutation_id: MutationId,
    pub method: String,
    pub path: String,
    pub body: serde_json::Value,
    pub created_at: i64,
    pub op: Option<OperationMeta>,
}

impl OutboxRecord {
    pub fn new(
        mutation_id: MutationId,
        method: impl Into<String>,
        path: impl Into<String>,
        body: serde_json::Value,
        created_at: i64,
    ) -> Self;

    pub fn with_op(self, op: OperationMeta) -> Self;
}
```

`#[non_exhaustive]` is what makes this decision cheap: outside the defining crate it blocks struct
literals, exhaustive destructuring, and functional update syntax, so every later field addition is
additive. It is also why the constructor is mandatory rather than stylistic.

**2. The body is `serde_json::Value`, not `String`.**

The source stores pre-serialized text (`persistence/types.rs:23`). Core parses at the enqueue
boundary instead. Backends remain free to persist text.

**3. Operation metadata is optional and structured.**

```rust
#[non_exhaustive]
pub struct OperationMeta {
    pub name: String,
    pub version: Option<String>,
}
```

Core stores it, returns it, and copies it across the outbox to dead-letter and quarantine
transitions. Core does not branch on it in D1: no routing, no deduplication, no ordering, no
retry policy keyed on `name`. Because the field is optional, any behavior core derived from it
would be behavior a caller could not opt out of.

## Why

**The envelope does not need a name to replay, so the usual argument does not apply.** The
replication cohort uses named mutators because the operation is a function and a function cannot
be persisted — TanStack Query states the constraint directly: "only the state of mutations is
persisted, as functions cannot be serialized." On resume those systems hold state and no function.
frontbox holds `{method, path, body}`, which is self-describing, and replay is an HTTP call. This
decision therefore does not adopt operation names as a replay mechanism, and D1 must not grow one.

Two narrower arguments do hold.

**Dead letters are unreadable without a label.** A dead letter reading
`POST /api/v1/sessions/3f2a-…/exercises` carries no intent. Dead letters are the surface a human
inspects after a server refusal, per decision 005, and the closest structural peer already labels
them: Redux Offline pairs an `effect: {url, method, json, headers}` with `commit` and `rollback`
action types, so the operation identity travels beside the HTTP envelope rather than being
reconstructed from the path.

**Schema drift across long offline windows is otherwise undetectable.** A body serialized by app
v1.2, queued for three weeks, replayed against a v2.0 server, fails as an opaque `Rejected` and
dead-letters with a server error that does not name the real cause. RxDB treats migration of
replication metadata as mandatory on schema change for this reason. `version` gives the caller a
place to record what wrote the body; D5 can act on it once durable storage exists.

**Parsing at enqueue moves corruption detection to the boundary.** With `body: String`, a
malformed body is only discovered at replay, after a reconnect, as a quarantine case under
decision 006. With `serde_json::Value`, it cannot enter the outbox at all. This narrows decision
006's surface; it does not remove it, since durable corruption still occurs after a successful
write.

## Consequences

- Every construction site — backends, tests, the migration trial — goes through `new()`. There is
  no struct-literal path from outside the crate.
- Backends must persist and return `op`. D1's in-memory backend gets this free; D5's SQLite and
  IndexedDB schemas must carry it from their first durable write, alongside the storage format
  version.
- Core must not compare `version` values. String ordering is wrong for semantic versions
  (`"1.10.0"` sorts below `"1.9.0"`). The field is caller-interpreted; core moves it and nothing
  else.
- Ordering remains `(created_at, mutation_id)` per the D1 plan. `op` plays no part in it.
- Decision 009 stamps a scope key on the same records. That addition is only non-breaking because
  of part 1 of this decision, so this decision lands first.
- `serde_json` becomes a public dependency of core types, not an internal detail. Its major
  version is part of the compatibility surface and belongs in `wiki/compatibility/` once the API
  is published.

## Amendment 2026-08-26: The Enqueue Input And The Stored Record Are Two Types

*The shape below is what shipped. `src/record/mod.rs` is now the authority on the exact signatures; the
rationale above and below is why they are what they are, and stays because a reader who wants to
change them needs it.*

D1 implementation found that the snippet above cannot hold as written alongside decision 009.

Decision 009 rule 2 requires the store to stamp its `ScopeKey` on every record, and
`wiki/plans/d1-core-cache-runtime.plan.md` shows `OutboxRecord` carrying a `pub scope: ScopeKey`
field. But `OutboxRecord::new(...)` above takes no scope, so a caller calling it would have to
fabricate one. All three cannot be true of a single type.

The resolution splits them, which is also the line the source itself draws between
`MutationIntentDto` and its own `OutboxRecord`:

- **`MutationIntent`** — what a caller enqueues. Carries this decision's constructor shape exactly:
  `new(mutation_id, method, path, body, created_at)` plus `with_op`, `#[non_exhaustive]`, parsed
  `serde_json::Value` body, no scope. It is also the wire type (decision 010).
- **`OutboxRecord`** — what a store returns. The same fields plus `scope: ScopeKey`, built only by
  `OutboxRecord::stamp(intent, scope)`.

Everything this decision actually binds is preserved: non-exhaustive, constructor-built, parsed
body, optional uninterpreted `OperationMeta`. What changes is which type the constructor belongs to.

The split is a strengthening rather than a workaround. A caller has no type in which to *put* a
scope, so `enqueue` is the only thing that can apply one and there is no forgeable path. Under the
single-type shape, a caller could have passed a scope and the store would have had to either trust
it or silently overwrite it.

`DeadLetterRecord` and `QuarantinedRecord` follow `OutboxRecord`: store-produced, scope-stamped, no
caller-facing constructor — consistent with `DeadLetterStore` exposing no `insert`.

## Revisit If

The D4 migration trial shows callers never populate `op`. Removing a public field is breaking
whether or not it is optional, so the window to drop it closes at the first stable release, not
at D5.

## Prior-Art Support

From D0a (`wiki/references/prior-art-survey.reference.md`).

**Named operations dominate the replication cohort, but for a reason frontbox does not share.**
Replicache and Zero invoke named mutators with JSON args; the name is the dispatch key on both
client and server. That is a property of function-replay systems. The HTTP command-queue cohort
splits: Redux Offline carries action types beside the effect, Workbox Background Sync persists the
`Request` object alone, and Amplify DataStore names its queue but not its individual operations.
The survey's original framing — that named operations are dominant and frontbox should follow —
overstated the transferable evidence, and this decision adopts the field on the two narrower
grounds above rather than on cohort weight.

**Serializability is the binding constraint on persisted mutation queues.** TanStack Query:
"only the state of mutations is persisted, as functions cannot be serialized." frontbox satisfies
this by construction, which is the reason its envelope is domain-neutral in the first place.

**Metadata versioning must exist before the first durable write.** RxDB documents migration of
replication metadata so clients need not restart replication after a schema change. Adding a
version stamp after durable data exists means the earliest records are the ones with no version,
which is precisely the population that needs it most.
