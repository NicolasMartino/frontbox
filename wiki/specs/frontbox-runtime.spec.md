# frontbox Runtime Behaviour

Document Class: Spec
Status: Active
Date: 2026-08-26; refreshed 2026-08-31
Category: Architecture
Scope: What the extracted library actually does, as built and tested, and how each behaviour relates to the source system it was extracted from.
Sources: `src/`, `tests/`, `crates/`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/references/source-test-inventory.reference.md`

## Why This Page Is Separate

`wiki/specs/source-frontend-cache-architecture.spec.md` records what RepForge's cache does, verified
against the copied corpus. This page records what frontbox does, verified by its own test suite.
Keeping them apart preserves the distinction the whole extraction rests on: observed behaviour
versus decided behaviour. When the two pages disagree, that is the point — the disagreement is the
extraction.

## Scope

**D1 through D5**: the outbox, dead letters, quarantine, scope isolation, the opaque row store, one
sync pass, the loop that repeats it, the cache version and invalidation runtime, and the two durable
backends that implement all of it. With the D2 cache-version amendment of 2026-08-28 (decision 021),
the outbox column set of 2026-08-29 (decisions 016, 017, 022), the drain loop of 2026-08-29
(decisions 028, 029), and the row store, `last_error` and `drained` of 2026-08-30 (decisions 032,
033, 035) applied.

**Storage is durable on both targets.** `crates/frontbox-sqlite` and `crates/frontbox-indexeddb` run
the same conformance cases this page cites, which is what lets every row below claim three
implementations rather than one. This page said otherwise for two days after they shipped; the
section that used to be *Constraints This Places On D5* is now **What D5 Had To Satisfy, And Did**,
below.

`crates/frontbox-dioxus` (D3b) is out of scope for this page, which records behaviour the
conformance suite can prove; its own note is `wiki/compatibility/dioxus-adapter.compat.md`.

## Divergences From The Source

Every row is a deliberate change, and every one has a test that fails if it is undone.

| Source behaviour | frontbox | Proof |
| --- | --- | --- |
| `Blocked` dead-lettered with `Rejected` (`mutations.rs:270`) | Retained; only `Rejected` dead-letters | Cases 4, 19; `blocked_is_retained_where_the_source_dead_letters_it` |
| Whole outbox loaded as one batch (`mutations.rs:513-514`) | Bounded `pending_batch(limit)` | Cases 10, 21 |
| Dead-letter insert then separate discarded delete (`mutations.rs:281,658`) | One atomic `apply_outcomes` | Case 9 |
| Malformed rows hidden by `filter_map(...ok())` (`native.rs:269`, `web.rs:112`) | Quarantine plus a backend-owned `sweep_corrupt` | Cases 15, 18, 26 |
| `pub type StoreError = String` (`types.rs:12`) | One non-exhaustive structured `Error` | Compile-time |
| `Utc::now()` inside record constructors (`types.rs:88`) | Injected `Clock`; no date library in the runtime graph, so `Utc::now()` does not compile | Case 13 |
| `ORDER BY created_at` with no tie-break (`native.rs:253`, `web.rs:115`) | Enqueue order: a store-assigned monotonic `seq`, alone — unique in all three backends, so no tiebreak | Cases 11, 12, 46 |
| Scoped and unscoped constructors side by side (`native.rs:70`/`:86`, `web.rs:81`/`:89`) | Required `ScopeKey`, no unscoped constructor, enforced on every read, retained on mismatch | Cases 22, 23, 29 |
| Body stored as pre-serialized text (`types.rs:23`) | Parsed `serde_json::Value`, so a malformed body is unrepresentable | Case 25 |
| Retention is unbounded; a record the server never resolves stays queued forever | A caller-set retention bound dead-letters at the bound with no `RemoteRejection`; `Retain` becomes a durable increment | Cases 47, 48 |
| No trace context on the record | Caller-supplied W3C trace context, stamped at enqueue, replayed as a header, `#[serde(skip)]` so it never enters the payload | Case 51 |
| A parked record says only whether a server explained itself | A `DeadLetterReason` naming which of three ways it was parked: a server refusal, the retention bound, or the application's own word | Case 56 |
| No way to carry a write precondition | An opaque caller-supplied precondition, stamped at enqueue and replayed as a header. Losing it disables conflict detection silently, which is why it is durable rather than recomputed | Case 55 |
| The sync loop sleeps before every pass and never drains back to back (`mutations.rs:743-748`) | `SyncRunner::drain` runs passes back to back until the queue stops draining; only the cadence between drains is the host's | Cases 57, 60 |
| Pending count cached in an `AtomicUsize` needing explicit refresh | Read from storage per call | `pending_count_needs_no_refresh` |
| Sticky `SyncStatus` booleans read in priority order (`mutations.rs:61,334-345`) | Per-pass `SyncReport`, with an explicit no-progress signal | Cases 7, 20; `each_pass_reports_what_it_did` |
| `MutationIntentDto` wire shape (`dto.rs:96-108`) | Preserved byte for byte | Case 27; the `dto.rs` ports in `tests/dto_oracle.rs` |
| Read-model rows live in a second store the application opens beside the cache, and the hydration merge is re-derived per application | frontbox holds the rows as opaque blobs it never parses, a mutation binds to a row at enqueue, and `merge_rows` skips rows with queued work — the one thing only something seeing both the rows and the queue can decide | Cases 62-65; decision 032 |
| A retained record records that it failed, not what it was up against | `last_error` carries the server's own words, bounded at 512 bytes by `LAST_ERROR_MAX` and never parsed | Case 66; decision 033 |
| A report names a mutation only when something went wrong | `DrainReport::drained` names every record that left the queue, how, and the row it was bound to — so an application's pending index can decrement instead of being rebuilt | Cases 65, 66; decision 035 |
| Terminal reads take a limit and no stated order | `DeadLetterStore::list` and `QuarantineStore::list` state `(timestamp, id)` order, so a truncating read returns the same *set* on every backend | Case 68 |
| A bounded read that filters corrupt rows can come back short | `pending_batch` pages until it has `limit` decodable records or the scope is exhausted; no fixed over-read multiplier | Case 69 |

Two source behaviours were deliberately **not** carried and remain out of scope: typed per-route
enqueue helpers, and direct-dispatch-with-fallback, which stays application-owned.

One observation about the source's own tests belongs here. `sync_status_reports_priority_order`
sets up its scenario by writing directly to `store.inner.is_offline` and `store.inner.is_error` —
private atomics. A status that can only be arranged by reaching past the public API is a status the
public API does not expose, which is why `SyncStatus` did not transfer as a shape.

## Behaviours The Source Has No Position On

These are not divergences — the source never faced the question, usually because it was an
application rather than a library. They are recorded because each is a contract a backend or a
caller has to honour.

| Behaviour | Why it exists | Proof |
| --- | --- | --- |
| A cancelled sync pass releases the in-flight flag | Cancellation is routine on the target runtimes: a Dioxus `use_future` is dropped on every re-render. A flag released only on the success path would leave the runner reporting `AlreadyRunning` forever, with no error to explain it | Case 30 |
| `apply_outcomes` rejects an outcome set naming one id twice | Two outcomes resolve to one record, so a pair of `DeadLetter` dispositions would write two dead letters for one row and the count would stop matching reality | Case 31 |
| A verdict repeated within one server response applies neither verdict | The two may disagree and there is no basis for preferring either. Applying the first and reporting the rest would make arrival order the tie-break, which is the preference being declined; the record stays queued and is ruled on again, which is safe because `mutation_id` is the idempotency key | Case 32 |
| A mutation the server returned no verdict for stays queued and counts as retained | Silence is not a verdict; reading it as one would either drop work or invent a refusal | Case 33 |
| A status this crate has never heard of is retained and reported with the server's spelling | `Delete` would assume acceptance and `DeadLetter` would assume refusal; both lose data when wrong. Failing the response instead — the prior behaviour — discarded every *other* verdict in the batch too, permanently, since the next pass gets the same answer | Case 34 |
| An invalidation naming an unregistered entity changes nothing and is reported | The registry defines what the application models, so a name outside it names data this client does not hold. Erroring would break every client during a rolling deploy | Cases 35, 36 |
| Cache version is an opaque identity compared by equality; `NeedsReset` does not exist | Amended 2026-08-28 (decision 021). The source counts, and D2 shipped a `u64` compared by magnitude. A counter is bumped by idempotent `PUT` replays that change nothing, and `0` collides with the XOR hash of an empty collection, so a never-synced client compared equal to a collection the server had emptied | Cases 37, 44 |
| Cache version and staleness persist as one unit | The version advances at invalidation, not at refetch, so only the staleness flag remembers a refetch is owed. Persisting the version alone makes a restart report an invalidated entity as fresh — worse than losing both, which fails safe | Case 39 |
| Asking what is stale also reports what refetching would discard | Core does not perform the refetch, so it cannot gate one; a hard gate would also let a single permanently retained record freeze every entity's cache. Reporting the conflict at the moment staleness is read means an application must actively ignore it to clobber unsent work | Cases 41, 42, 43 |
| A drain stops at the first pass that drains nothing, not when the queue empties | A fruitless pass leaves the queue unchanged, so the next pass sends the identical request. Repeating it also spends a retention bound in milliseconds — the bound counts verdicts received, and it is meant to give the server chances spread over time, not over one loop. Stopping on no progress also supplies the termination argument: a continuing pass strictly shortens the queue | Case 58 |
| Reads skip undecodable rows; only `sweep_corrupt` surfaces them | Sweeping inside a read makes reads mutate storage; erroring on a read lets one bad row wedge a healthy queue. The window is bounded because the runner sweeps every pass | Case 15 |

## Verification

`scripts/verify.sh` runs the whole gate set: `cargo fmt --check`, clippy with `-D warnings` on
native and `wasm32-unknown-unknown`, the test suite, wasm builds with and without default features,
and a `cargo llvm-cov` floor of 80% lines and functions. It also builds the documentation with `-D
warnings`, which is what stops intra-doc links rotting:
eighteen had accumulated by 2026-08-30 because nothing ran `cargo doc`, and two of those were added
the same week they were noticed. And it runs three textual checks — no Dioxus, no RepForge entity
names, no `Send` bound — which are explicitly secondary to the compile gates and documented as such
in the script.

Coverage sat at 89% regions / 96% lines / 93% functions when the floor was added, so the gate
records a property that already held rather than one being aimed at. It has stayed there through
every deliverable since: 89.17% regions / 95.79% lines / 92.72% functions across 68 conformance
cases as of 2026-08-31. The figures move by fractions of a point because the suite grows with the
surface, which is the property worth having — a floor that only holds while nothing is added is not
a floor.

Since D3 made the repository a workspace, **every gate names the package it means**. A bare
`cargo clippy` or `cargo llvm-cov` in a workspace silently changes which crates it covers, and the
coverage floor in particular stays on `-p frontbox`: averaging one number across core and an
adapter the conformance suite cannot reach would let a regression in either hide behind the other.
The adapter gained its first unit tests on 2026-08-30 — `Wake` and `Sleeper::wakeable` are a latch
and a hand-written race, testable with a counting waker — and two gates that compile its `web`
feature for wasm. Its `pageshow` listener still needs a browser, and the coverage floor still does
not apply to it.
The `no dioxus` check moved from grepping `Cargo.toml` to asking
`cargo tree -p frontbox --edges normal`, because a workspace member named `frontbox-dioxus` matches
the grep and is not a dependency (`wiki/decisions/028-drain-loop-boundary.decision.md`).

A fifth suite macro, `frontbox_single_flight_tests!`, runs five cases at `batch_limit = 1`
(decisions 018, 029 and 031). It is a separate list rather than the outbox list re-emitted: four
outbox cases lose their subject entirely at a limit of one, and the macro's own documentation names
which and why, because four cases quietly absent from a green run is a coverage claim that is not
true.

The conformance suite lives in the library behind a `testing` feature rather than in `tests/`, so
the SQLite and IndexedDB backends run the identical cases through `StoreFactory`,
`VersionStoreFactory`, and `FaultInjection`. They do: 68 cases green on all three backends, the
browser ones under `wasm-bindgen-test` in headless Chrome. The three are separate traits on
purpose: a backend that cannot yet do one of them does not implement it and does not invoke the
matching macro, which leaves the gap visible in its test file rather than hidden behind a runtime
skip.

## What D5 Had To Satisfy, And Did

These were written as constraints on a deliverable that had not started. Both backends now exist, so
each line is followed by what discharged it. Kept rather than deleted, because a constraint with its
outcome beside it is the record of whether the constraint was the right one.

- **Scope keys must encode injectively into storage names.** Character replacement is not injective;
  see `wiki/decisions/009-local-scope-identity.decision.md`. **Discharged** by a column rather than
  an encoder in both backends — `scope TEXT NOT NULL COLLATE BINARY` in SQLite, a `scope` index in
  IndexedDB — which decision 024 explicitly permitted and which makes injectivity trivial: two
  different keys are two different strings. Case 49 runs on both.
- **Drain exclusion must survive a realm boundary.** Core claims the scope in-process, so two
  runners over two handles cannot overlap and case 61 says so. Two browser tabs are two wasm
  instances with their own memory and no shared claim, so a backend that can be opened from more
  than one realm owes exclusion of its own — Web Locks for IndexedDB, usually nothing for a
  one-process native application. **Half discharged.** `IdbStore::claim_drain` takes a Web Lock and
  releases it on drop, and a single-realm test proves both. **Nothing has watched two realms
  contend**, and the conformance suite cannot: a second realm is not something a case running in one
  process can open. This is D5's one unmet proof line
  (`wiki/decisions/031-cross-realm-single-flight.decision.md`).
- **A second durable schema, for cache versions.** Roughly `(scope, entity, version, stale)` with a
  uniqueness constraint on `(scope, entity)`, written atomically. `entity` is stored in exactly one
  textual form — whatever `EntityKey::as_str` returns — for the same reason `mutation_id` is: mixed
  spellings give a backend two entities where the caller has one. Ordering is not required of entity
  keys, so no collation constraint follows, but case sensitivity does. **Discharged** as
  `cache_versions` in SQLite and a `[scope, entity]`-keyed object store in IndexedDB, both writing
  version and staleness in one statement. **`version` is nullable on purpose**, which is the whole
  of decision 021: SQL `NULL` is "never heard a version" and `''` is "the server said the empty
  string", and a `NOT NULL DEFAULT ''` column collapses them so a never-synced client compares equal
  to a collection the server emptied. Case 44 is the one case that fails if it does.
- **`CacheVersionStore` is not generic over the application's key type.** It names entities by
  string, because a store has no registry to reconstruct typed keys with and a durable store can
  outlive the build that wrote its rows. **Held.** Both backends store strings and neither needed a
  registry.
- **`mutation_id` must be stored in one consistent textual form** — exactly `MutationId::to_string`
  output — under a binary collation, or as the UUID's big-endian bytes. Mixed forms invert the
  pending order. Demonstrated by the unit tests in `src/id.rs`. **Discharged**, and the collation is
  written out rather than left to the default so an `ALTER` cannot change it silently. It became
  load-bearing a second time when the terminal stores gained a stated order, since the tie-break is
  the identifier.
- **A storage format version must be written from the first durable write**, not added later.
  **Discharged**: `StoredRow::schema` carries it on the row, an idea taken from RxDB/WatermelonDB in
  the prior-art survey, and IndexedDB carries a database version that already had to be bumped once —
  from 1 to 2 — to add the versions store.
- **`op` must be persisted and returned** across every transition, per
  `wiki/decisions/008-mutation-envelope-extensibility.decision.md`. **Discharged**; case 24 asserts
  it on all three backends.
- **Three columns are built in core and the in-memory backend**, and D5 inherited them as schema:
  `seq` (016) unique and the primary sort key, assigned inside the insert's own transaction;
  `attempts` (017), incremented when a sent record stays queued; and `traceparent` (022), an opaque
  caller-supplied string. All three are carried onto the dead letter. **Discharged**, with `seq`
  taken from `AUTOINCREMENT` rather than plain `INTEGER PRIMARY KEY` — the plain form reuses the
  largest deleted rowid, which would reissue a sequence number and invert exactly the pairs decision
  016 exists to order.
- **The IndexedDB backend runs the suite through `frontbox_conformance_tests_async!`.** The
  synchronous macro drives each case with a caller-supplied `block_on`, and no such executor exists
  in a browser: an IndexedDB future suspends on a JavaScript callback that cannot fire until the
  stack unwinds, so any `block_on` deadlocks. The async macro emits `async fn` cases instead and
  lets `#[wasm_bindgen_test]` drive them. Both macros share one case list, so the two backends
  cannot silently run different suites. **Discharged**, and the shared list did its job: every case
  added since runs on both without anyone remembering to add it twice.

## What The Backends Found That The Constraints Did Not Predict

Each of these came from writing a second implementation, which is the argument for having written
one at all.

- **`EntityState` had no constructor for one of its four states.** *Stale with no version* is what
  `InvalidationRunner::mark_all_stale` writes for an entity nothing has ever invalidated, and
  nothing outside the crate could reconstruct it — so neither durable backend could rebuild what it
  had itself stored. `EntityState::from_parts` closed it; case 67 pins it.
- **A transaction handle that is dropped commits.** Dropping a `rusqlite::Transaction` rolls back;
  dropping an `IdbTransaction` does the opposite, because the browser owns it. Every `?` between the
  first write and the commit was therefore a partial write that landed. `Txn` now aborts on drop
  unless `commit` consumed it — the same device `DrainLease` uses in core.
- **A filtered bounded read needs a loop, not a multiplier.** See case 69's row above.
- **An unstated order is three orders.** See case 68's.
- **One rule written three times is a divergence waiting to happen.** The `last_error` truncation
  walk-back was copied into both backends byte-identically. `truncate_error` now ships from core
  beside `LAST_ERROR_MAX`, which is what `src/rfc3339.rs` already argued for in the general case.

