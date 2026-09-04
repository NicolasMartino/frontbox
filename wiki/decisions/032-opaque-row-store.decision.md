# frontbox Stores The Rows It Does Not Read

Document Class: Decision
Status: Accepted 2026-08-30; supersedes decision 023 in part; built in D5; **amended 2026-09-01** — the row shape lost `version` and `schema`, see `## Amendment`
Date: 2026-08-30
Category: Storage Contract
Scope: frontbox durably stores read-model rows as opaque blobs beside its staleness markers, mutations bind to rows at enqueue, and the hydration merge becomes library orchestration. What stays the application's: serialization, meaning, indexes, queries, and the pull itself.
Sources: `wiki/references/prior-art-survey.reference.md`, `examples/todo-core/src/app/index.rs`, `examples/todo-core/src/store.rs`, `src/record/mod.rs`, `Cargo.toml`
Related: `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/014-pull-gating.decision.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/007-generic-entity-key-registry.decision.md`, `wiki/plans/d5-persistence-backends.plan.md`, `wiki/references/open-decisions.reference.md`

## Decision

**frontbox durably stores read-model rows as opaque blobs.** The store is
~~`(scope, entity, row_id) → (blob, version, stale, schema)`~~ **`(scope, entity, row_id) →
(blob, stale)` since 2026-09-01** — the marker table decision 023 already committed to D5, with the
blob beside the flag. frontbox never deserializes, never hashes, and never indexes the contents: it
returns what it was given. The two stamps this line originally carried are struck out above and the
reason is in `## Amendment`.

Three parts, each with its own boundary:

- **The row store.** Written by the application — after its own fetch, or as an optimistic
  projection. Read by the application at startup instead of arriving empty. Scope-stamped under the
  same rule as every other durable schema (decision 009).
- **The row binding.** `MutationIntent` gains an optional `(entity, row_id)` pair, caller-supplied
  at enqueue, `#[serde(skip)]` like `traceparent` and `precondition` — storage-only, never on the
  wire, so decision 010's payload is untouched and a caller who does not bind sees nothing change.
- **The merge rule.** A hydration write into the row store *skips rows with pending bound
  mutations.* frontbox knows which rows those are — that is what the binding is for — so the rule
  the application cannot get wrong is the rule the library enforces, and the application supplies
  only what it alone knows: the fetch, and the projection.

**Superseded in 023:** the sentence "frontbox does not store read-model rows," and the
consequence that markers outlive their rows. **Survives from 023, intact:** markers are real and
row-addressable; the application owns meaning, serialization, and its query layer; frontbox takes
no position on canonical bytes; and decision 014's boundary stands — frontbox still never performs
the pull.

## Why 023 Is Superseded Rather Than Defended

023 was a scope defense against RepForge's §5b.3, which proposed a *hash-bearing, indexed* row
schema. Its three arguments were sound against that proposal and two of them do not touch this one:

- *"Not holding the rows means frontbox cannot be wrong about their bytes."* A store that never
  compares, hashes, or canonicalizes bytes cannot be wrong about them either. The hash-skip stays
  refused; the blob is not evidence for it.
- *Indexes and queries.* Still refused. The application's in-memory index over durable blobs is the
  model — `TodoStore`'s `BTreeMap` survives unchanged and is simply rebuilt from storage instead of
  from nothing.
- *The algorithm needs markers, not blobs.* True, and the markers stay. The blob rides on the table
  the markers already required.

What forced the supersession is that **023's own `## Revisit If` clause fired before a line of D5
was written.** It said to revisit if applications routinely reimplement the same blob store above
frontbox; the *first* application to want offline reads — the D5 plan's Track D, 2026-08-30 —
had to plan exactly that: a second durable store, its own schema, in every consumer. Three more
costs traced to the same line:

- **The merge was every consumer's homework.** Register entry 19: hydrating over a durable queue
  must not clobber optimistic rows, and under 023 each application derives and enforces that rule
  alone. Replicache solves it in the library — rebase over canonical state — and can *only* because
  it holds both the data and the queue.
- **Markers outlive their rows by construction.** 023 admits markers accumulate for rows the
  application deleted "because it does not hold the rows." A marker beside its row dies with it.
  The unbounded-growth position 023 owed becomes unnecessary.
- **`row_id_of` is the binding, reverse-engineered.** The application knows which row a write
  touches at enqueue, discards that, and re-derives it by parsing paths
  (`examples/todo-core/src/app/index.rs`) — the parse exists only because nothing recorded the
  answer. Finding 1's full-index rebuild is the same missing fact.

## The Prior Art, Which Is Where The Line Comes From

The survey's own cohort split is the design space. Pure queue: **Workbox** — the survey's
counterexample, whose missing terminal/transient split validates decision 005. Local-first
database: **Replicache, PowerSync, RxDB, WatermelonDB** — they own storage, queries, and indexes.
Between them, **TanStack Query and Redux Offline both persist application data as opaque blobs**:
TanStack the query cache itself, keyed and versioned (`buster`), contents never interpreted; Redux
Offline the whole store via redux-persist, outbox inside it.

frontbox's declared peer group is that middle row, and **two of its three peers persist app data.
The point 023 defended is occupied only by the cohort's counterexample.** This decision moves
frontbox to where its peers already are — and stops short of the database cohort, deliberately:
a query layer means competing with RxDB and PowerSync on their ground, and the actual consumer
already owns a read model it would have to delete.

It also makes `Cargo.toml`'s one-liner true. "Offline-first cache and mutation outbox" described a
library that cached nothing; after this, it describes the library.

## Versioning, And Encryption

- ~~**Blob versioning ships with the store.** Each row carries a caller-supplied schema stamp;
  reads surface it, and the migration function is the application's — RxDB's model.~~ **Retired
  2026-09-01; see `## Amendment`.** frontbox owns migrations of its *own* schemas regardless (the
  D1 plan's format-versioning item, due at D5).
- **Encryption is far-future, by explicit choice (user, 2026-08-30).** Owning storage is what makes
  at-rest encryption *possible* — under 023 there was nothing to encrypt — but key custody deserves
  its own decision. D5 leaves a seam: blobs pass through one read path and one write path per
  backend, so a cipher has a single place to sit. Nothing more.

## Consequences

- **D5's third schema grows one column** (blob; the schema stamp went in 2026-09-01) instead of frontbox growing a fourth
  schema per application. Track D of the D5 plan inverts: `todo-core` deletes its planned second
  storage stack and writes through frontbox's row store.
- **Register entry 19 is settled in shape**: merge, enforced by the library's skip rule; D4b builds
  it. Entry 12 gains a second piece of evidence — bound row ids are exactly what a drain report
  could name.
- **The pending index becomes a query.** `row_id_of` and its parse retire once the binding exists;
  the application that binds gets `is_saving` from storage instead of from path-parsing.
- **`MutationIntent` changes shape** — one optional field, `#[non_exhaustive]` absorbing it, wire
  format untouched. Conformance owes: a bound mutation's rows are skipped by hydration writes; an
  unbound caller's behaviour is unchanged; markers die with their rows.
- **023 stays load-bearing for everything not named superseded**, and its page is amended to say
  which sentence fell and why, rather than rewritten.

## Revisit If

- An application needs indexed queries over rows too large to hold in memory. That is the database
  cohort's problem; the answer would be an *adapter* over a real embedded database, not indexes in
  frontbox.
- The binding turns out to be wrong often enough to matter — a caller binding the wrong row poisons
  the merge for that row. If D4b shows this, the parse-based fallback deserves a second look as a
  cross-check rather than a replacement.

## Amendment (2026-09-01): the row shape loses `version` and `schema`

**Both were written by the trial and read by nothing**, in the sweep that followed D4d. The row is
now `(blob, stale)`.

**The argument against them is this decision's own.** The thing that makes an opaque row store
defensible — and that dissolved decision 023's objection that owning rows means owning their
canonical bytes — is that *core never deserializes the blob*. A caller-supplied stamp sitting
**beside** a blob core cannot interpret duplicates something the blob can already say. The bytes are
wholly the application's, so a shape number or a content version belongs inside them, next to the
code that understands both.

**Neither stamp bought a query.** Nothing filters on them, and `list_rows` returns whole rows — so a
caller reading a stamp always had the blob in hand anyway. The lazy-migration story that justifies
RxDB's version (look at the shape without paying to decode it) needs a read path that returns the
stamp *without* the blob, and this store has never had one.

**The evidence is the trial's diff.** `examples/todo-core` set `.with_schema(1)` at both of its
write sites and never read it back at either read site — across D4b, D4c and D4d, the three
deliverables that were supposed to exercise the store. `version` was never set outside the
conformance case that asserted it round-trips.

**`stale` stays, and the contrast is the rule this amendment establishes.** Core writes it
(`RowStore::set_stale`), core reads it, and it means the same thing to core as to the application.
That is what earns a field a place beside an opaque blob: **core has to act on it.** A number core
carries and hands back is the application's to store in its own bytes.

**What a caller loses, stated plainly:** nothing it cannot have for one line. An application that
wants blob versioning adds a field to its own struct — `examples/todo-core/src/app/rows.rs` says
where. What it gains is that the row store no longer offers a second, weaker place to put it.

**Case 62 changed rather than shrank.** It used to assert the two stamps round-trip. It now round-
trips a nested, mixed-typed blob — an object, an array, a number, a bool — which is a stronger claim
about the promise that survived: a backend that reshaped a nested object or narrowed a number passed
the old flat fixture and fails this one.
