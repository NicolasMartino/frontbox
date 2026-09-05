# Build D5: Durable Storage, On Both Platforms

Document Class: Plan
Status: **Completed 2026-08-30**; the last owed proof line met 2026-08-31 — see `## Outcome`
Date: 2026-08-30
Category: Storage
Scope: The execution plan the roadmap records as "Not created yet" — two backend crates, the obligations core already placed on them, the application-side persistence D4b needs, and the decisions that must be settled before the first durable row is written.
Sources: `src/store/mod.rs`, `src/testing/mod.rs`, `src/testing/macros/mod.rs`, `src/memory/`, `examples/todo-core/src/`, `wiki/specs/source-frontend-cache-architecture.spec.md`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/decisions/016-monotonic-enqueue-sequence.decision.md`, `wiki/decisions/017-bounded-retention.decision.md`, `wiki/decisions/023-read-model-boundary.decision.md`, `wiki/decisions/024-scope-storage-encoding.decision.md`, `wiki/decisions/025-quarantine-storage-shape.decision.md`, `wiki/decisions/031-cross-realm-single-flight.decision.md`, `wiki/references/open-decisions.reference.md`, `wiki/proposals/extraction-boundary.proposal.md`

## Context

**D5 is the deliverable everything else has been deferring to.** Four accepted decisions are built in
core and the in-memory backend and have never met durable storage; the D4a trial proved the API
against a queue that dies with the tab; and `wiki/proposals/browser-background-services.proposal.md`
established that the browser's storage panel is empty because there is nothing to show.

Scope confirmed with the user 2026-08-30: **both backends together**, not split web-first — so
cross-backend conformance catches divergence between the two implementations rather than deferring
it. And — revised later the same day by decision 032 — **frontbox itself stores the read model as
opaque blobs**; Track D inverts accordingly, from a second storage stack per application to the row
store this plan now builds.

**What is already settled, so the plan does not reopen it:**

| Obligation | Where it came from | What a backend must do |
| --- | --- | --- |
| Injective storage naming | Decision 024, case 49 | Two distinct `ScopeKey`s never reach one physical store. No encoder in core |
| Quarantine shape | Decision 025, case 50 | Four properties; table-or-status-column is the backend's choice |
| Durable monotonic `seq` | Decision 016 | Assigned inside the enqueue transaction, primary sort key |
| Durable `attempts` | Decision 017 | Incremented when a sent record stays queued, carried onto the dead letter |
| One textual `mutation_id` | Decisions 009, 024 | Exactly `MutationId::to_string`, binary collation |
| Cross-realm single-flight | Decision 031 | The in-realm half is built (case 61); the cross-realm half is D5's, **built 2026-08-30 and observed 2026-08-31** (`tests/cross_realm.rs`) |
| Opaque row store | Decision 032 | `(scope, entity, row_id) → (blob, stale)` since that decision's 2026-09-01 amendment; contents never read |

## Reoriented (2026-08-30) — The Target Is RepForge Adoption

The user's direction, later the same day: **speed up, and build toward RepForge actually starting
on frontbox.** The reframe this plan adopts:

- **"Production for RepForge" is an adoptable 0.x, not a release.** RepForge's own section-C answer
  was that they have no production either — neither side has queue depths. The target is a git
  dependency they can build on: durable backends, the row store, suite green, their blocking
  questions answered. `publish = false` stays until D6's remaining half.
- **Their blockers get decided now, not at publication.** Entry 12 joins Track E: under decision
  032 it is nearly free — outcomes know their `mutation_id`s and the binding maps ids to rows, so
  a report can name the bound rows that drained. That is simultaneously RepForge's read-model
  convergence need and the retirement of Finding 1's index rebuild. Entry 3 was always Track E's.
  Entry 6 needs its *shape* only. The `If-Match` question is theirs and is relayed, not designed
  around.
- **What slims:** D4b folds to the seam proof (observation 3 inverted, diff in two parts); D6's
  adoption half — compat notes, a consumption path, an adoption guide mapping their
  `persistence/*.rs` to frontbox APIs — pulls forward alongside the backends; the crates.io half
  waits. Wiki pages get shorter; the record stays.
- **What does not slim, stated so speed has a floor:** the conformance suite on both backends, and
  columns-before-rows. A schema change after RepForge writes rows onto user devices is a migration
  on their users. These are what make fast adoption safe rather than merely fast.

Build order under the reorientation — three lanes, first two concurrent:

1. **Core lane** (starts immediately, no backend needed): Track E decisions (3, 4-shape, 12, 18),
   then the 032 core slice — `RowStore` trait, the `MutationIntent` binding, drained-ids on the
   reports, in-memory implementations, conformance cases. Everything here is testable on the
   backend that exists.
2. **Backend lane**: Tracks A and B against the then-final column set, C behind B.
3. **Adoption lane** (as the backends stabilize): the adoption guide, compat notes, the git-dep
   consumption path, and the relayed `If-Match` question.

## Track A — `crates/frontbox-sqlite`

**`rusqlite`, not `sqlx`.** `sqlx` is async and built around `Send` futures, which the `no Send
bound` gate rejects and decision 001's `!Send` posture rules out on principle. `rusqlite` is
synchronous, so the `async fn`s in `OutboxStore` complete without ever yielding — honest for a local
file, and it keeps every future `!Send` without pretending otherwise.

**A divergence from the source, recorded rather than discovered.** The source wraps
`Arc<tokio::sync::Mutex<rusqlite::Connection>>`
(`wiki/specs/source-frontend-cache-architecture.spec.md:210`), because it assumed `Send`. frontbox
holds `Rc<RefCell<Connection>>`. Same structure, one fewer guarantee claimed.

Five tables: `outbox`, `dead_letters`, `quarantine`, `cache_versions`, and decision 032's `rows` —
`(scope, entity, row_id)` unique, blob stored and never parsed. Notes that are not
obvious:

- **`seq INTEGER PRIMARY KEY AUTOINCREMENT`** is what makes decision 016 free here — SQLite assigns
  it inside the insert, monotonically, and never reuses a value even after deletes. Plain
  `INTEGER PRIMARY KEY` does reuse, which would invert pairs; the keyword is load-bearing.
- **`mutation_id TEXT COLLATE BINARY`**, and the schema says so explicitly rather than relying on
  the default, because the default is what a later `ALTER` would silently change.
- **`apply_outcomes` is one `IMMEDIATE` transaction** spanning all three tables. Deferred would
  acquire the write lock late and could fail mid-way through work already applied.

## Track B — `crates/frontbox-indexeddb`

The harder half, and the reason the two backends are worth building together.

**Verify the crate choice first.** The source uses `rexie`
(`source-frontend-cache-architecture.spec.md:30`). It is the default, not a conclusion — confirm it
is maintained and that its futures are `!Send`-compatible before committing, and record the answer
either way. The alternative is `idb`.

**The constraint that shapes everything: an IndexedDB transaction closes when the event loop
yields.** A transaction stays live only while there is a pending request against it, so *nothing
external may be awaited inside one*. `apply_outcomes` must therefore do all of its reads and writes
against one transaction with no intervening await on anything that is not an IDB request. This is
the single most likely source of a bug that passes in-memory and fails in a browser, and it is why
the atomicity case matters more here than on SQLite.

- **Monotonic `seq`** comes from an object store with `autoIncrement: true`, which IndexedDB
  guarantees is monotonic per store and does not reuse after deletion. Since decision 024 already
  puts each scope in its own physically distinct store, per-store is per-scope.
- **Scope naming** is decision 024's injectivity applied to a database name. The recommendation
  there stands; the *encoder* is this crate's.
- **Corrupt-record visibility**: `insert_corrupt_row` must be implementable, which means the schema
  cannot make a malformed row unrepresentable. A row that cannot be written cannot be swept.

## Track C — Cross-realm single-flight (decision 031's other half)

Case 61 proved the in-realm half: two handles on one scope, in one process, do not drain together.
**The cross-realm half is untested and is what a browser makes reachable** — two tabs on one origin
are two realms over one database, and neither `Cell` nor a core-side scope registry spans them.

- **IndexedDB: Web Locks** (`navigator.locks`), lock name derived from the same injective encoding
  as the database name. Available in window *and* worker scopes, which is what would later let a
  service worker participate.
- **SQLite: nothing, usually** — a native app is one process, and the in-realm guard already covers
  it. If a second process opens the same file, `BEGIN IMMEDIATE` is the fallback and it degrades to
  a failed transaction rather than a double-send.

**The conformance case cannot be written in-process**, which is new for this suite: every other case
runs in one realm by construction. Budget a browser-only test that opens two contexts, and if that
proves impractical, say so in the plan's outcome rather than quietly shipping the obligation as
prose. **A gate that cannot fail is worse than an acknowledged gap.**

## Track D — The row store, the binding, and the merge

**Rewritten 2026-08-30 by decision 032.** The first draft of this track had `todo-core` building
its own durable store beside frontbox's, and the drafting of that track is what fired decision
023's revisit clause. The second storage stack is deleted; what this track builds instead:

- **The row store in core's contract** — a `RowStore` trait beside the other three, implemented by
  all three backends (in-memory included, so conformance has a reference), scope-stamped under
  decision 009's rule. Blobs in, blobs out, nothing parsed.
- **The binding on `MutationIntent`** — optional `(entity, row_id)`, `#[serde(skip)]` beside
  `traceparent` and `precondition`, so decision 010's wire payload is untouched. `todo-core` binds
  at every enqueue site and retires `row_id_of`'s parse.
- **The merge rule, in the library** — a hydration write skips rows with pending bound mutations.
  This settles register entry 19's shape; D4b exercises it. `TodoStore` keeps its in-memory
  `BTreeMap` and rebuilds it from the row store at startup instead of from nothing —
  `TodoApp::start`'s hydrate step becomes *fetch, write-through-merge, reindex*.

Conformance owed, all three with real content on the in-memory backend: a bound mutation's row
survives a hydration write; an unbound caller sees unchanged behaviour; a deleted row takes its
marker with it.

## Track E — Decisions owed before the first durable row

The register's sequencing note 3 requires these settled *in this plan*, so the column set is final
before anything writes a row.

| Entry | Question | Why it cannot wait |
| --- | --- | --- |
| **3** | Last-error metadata on the record | It is a column. Adding it later is a migration on user devices |
| **4** | Does core learn about service origins | Only its *shape* — enough to know whether it is a column |
| **12** | Whether a report names the drained mutations | **Moved up from the publication clock (2026-08-30):** RepForge's convergence needs it, and 032's binding makes it cheap — ids are known, the binding maps them to rows |
| **18** | Whether the outbox partitions below scope | Decides whether the primary key is `(seq)` or `(partition, seq)` |
| **19** | Replace or merge on hydration | **Settled by decision 032**: merge, enforced by the library's skip rule; D4b builds it |

Each gets a decision page if it changes the schema, and a recorded "no" in this plan if it does not.

## Track F — Gates

`scripts/verify.sh` gains, per backend: clippy at `-D warnings`, the conformance suite, and for the
web crate a `wasm32-unknown-unknown` build. **Coverage stays measured on `-p frontbox`** for the
reason the D3 plan gave — averaging a floor across core and a backend lets a regression in either
hide behind the other — and each backend crate gets its own floor rather than joining core's.

The `no Send bound` grep extends to both new crates. The `no date library` gate must keep passing
with `rusqlite` and the IndexedDB crate in the graph, which is not guaranteed and is worth checking
before either dependency is committed to.

## Order

A and B are independent and run together — that is the point of not splitting. C depends on B for
the mechanism and on nothing for the SQLite side. D depends on both backends existing but its
*design* is settled by Track E's entry 19, which happens first. E is written before any code.

**E, then A and B in parallel, then C, then D** — refined by the reorientation above into the
three lanes: the core lane needs no backend and starts first, the backend lane follows the final
column set, the adoption lane rides the backends.

## Verification

1. **The D1 conformance suite passes on all three backends** — in-memory, SQLite, IndexedDB —
   through `StoreFactory` and `FaultInjection`, with the browser runs going through
   `frontbox_conformance_tests_async!` under `#[wasm_bindgen_test]`.
2. **Cases 49 and 50 stop being vacuous.** They were written to pass trivially on in-memory and to
   have real content on a durable backend. If either still passes trivially, the backend has not
   done the work.
3. **The single-flight profile passes on both durable backends** at `batch_limit = 1`, where a
   violated order is observable at all.
4. **`apply_outcomes` is atomic under fault injection** on both, which is the case the in-memory
   backend's own docs admit it cannot really prove — it commits by replacing state in one
   assignment, so a genuinely torn write is a durable-backend concern.
5. **Observation 3 inverts**: same scope, new process, work still queued. It is the one observation
   D4a could not make, and it has a test today asserting the opposite.
6. **The browser storage panel shows the outbox**, with `seq`, `attempts` and a readable
   `mutation_id`. This is not decoration: it is the cheapest possible check that the textual form
   rule survived contact with a real backend.
7. **`bash scripts/verify.sh` reports ALL GATES PASSED** with the new gates included.

## Risks

- **IndexedDB transaction lifetime** is the one that will actually bite. A backend that awaits
  anything non-IDB inside `apply_outcomes` will pass every in-process test and lose atomicity in a
  browser. Mitigation: write the atomicity case first, run it in the browser early, and treat a pass
  before the case exists as meaningless.
- **The cross-realm case may not be automatable.** Named in Track C. The failure mode is shipping an
  obligation nothing enforces while the suite reports green.
- **The binding is caller-supplied and can be wrong.** A mutation bound to the wrong row poisons
  the merge for that row — decision 032's own revisit clause. The parse `row_id_of` retires is the
  natural cross-check if D4b shows the failure happening.

## Outcome

**Built 2026-08-30**: `crates/frontbox-sqlite` and `crates/frontbox-indexeddb`, both running the
same conformance cases as the in-memory backend — 68 as of 2026-08-31, the browser ones under
`wasm-bindgen-test` in headless Chrome.

**The last owed proof line was met 2026-08-31.** "A two-realm drain does not double-send" is now
observed, by `crates/frontbox-indexeddb/tests/cross_realm.rs`: a second realm on the origin's lock
manager, asserted in both directions — a realm that finds the scope locked reports `AlreadyRunning`,
sends nothing and leaves its queue intact, and a pass in flight refuses that realm the same lock.

**Track C's named risk was the real one.** "The cross-realm case may not be automatable" is listed
under `## Risks` above, with its failure mode written out: *shipping an obligation nothing enforces
while the suite reports green*. That is exactly the state the crate was in for a day. It was
automatable after all, and what made it so was giving up on the second realm being a second *tab*:
a dedicated worker is a separate agent on the same origin-wide lock manager, and reachable from
inside a single-page harness.

**And the fixture is known to be able to fail**, which is the part the risk entry was really about.
Three sabotage runs against `locks.rs` — grant unconditionally, drift the lock name, leak the lease
— each turned a test red, and the third turned exactly one test red, the one written for it. See
decision 031's `## The Two-Realm Observation` for the table.

**The gate moved too.** `scripts/verify.sh` used to carry the browser command as a comment; it now
runs the suite when `CHROMEDRIVER` names a driver and prints `1 GATE(S) SKIPPED` when it does not.
A proof nothing executes is how this plan came to owe a line for a day.

**What writing the second implementation found**, which is the argument for having written one at
all rather than shipping the first and calling the trait proven:

- **`EntityState` had no constructor for one of its four states.** *Stale with no version* is what
  `mark_all_stale` writes, and nothing outside the crate could reconstruct it — so neither backend
  could rebuild what it had itself stored. `EntityState::from_parts` closed it; case 67 pins it.
- **A dropped `IdbTransaction` commits**, where a dropped `rusqlite::Transaction` rolls back. Every
  `?` between the first write and the commit was a partial write that landed.
- **A bounded read that filters corrupt rows cannot use a fixed over-read.** SQLite asked for
  `limit * 4` and returned nothing when more than `3 * limit` corrupt rows sat at the head. Case 69.
- **An unstated `list` order is three orders.** Case 68.
- **One truncation rule written three times is a divergence waiting to happen.** `truncate_error`
  now ships from core beside `LAST_ERROR_MAX`.

The last three were found by a whole-worktree review on 2026-08-31, not by this plan's own
verification — which is worth recording, because all three were green on every gate at the time.
