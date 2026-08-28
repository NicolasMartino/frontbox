# frontbox Runtime Behaviour

Document Class: Spec
Status: Active
Date: 2026-08-26
Category: Architecture
Scope: What the extracted library actually does, as built and tested, and how each behaviour relates to the source system it was extracted from.
Sources: `src/`, `tests/`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/decisions/`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/plans/d1-core-cache-runtime.plan.md`, `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/references/source-test-inventory.reference.md`

## Why This Page Is Separate

`wiki/specs/source-frontend-cache-architecture.spec.md` records what RepForge's cache does, verified
against the copied corpus. This page records what frontbox does, verified by its own test suite.
Keeping them apart preserves the distinction the whole extraction rests on: observed behaviour
versus decided behaviour. When the two pages disagree, that is the point — the disagreement is the
extraction.

## Scope

D1 and D2: the outbox, dead letters, quarantine, scope isolation, one sync pass, and the cache
version and invalidation runtime, with the D2 cache-version amendment of 2026-08-28 (decision 021)
applied. The Dioxus adapter (D3) and the durable backends (D5) are not
built. Storage is in-memory.

## Divergences From The Source

Every row is a deliberate change, and every one has a test that fails if it is undone.

| Source behaviour | frontbox D1 | Proof |
| --- | --- | --- |
| `Blocked` dead-lettered with `Rejected` (`mutations.rs:270`) | Retained; only `Rejected` dead-letters | Cases 4, 19; `blocked_is_retained_where_the_source_dead_letters_it` |
| Whole outbox loaded as one batch (`mutations.rs:513-514`) | Bounded `pending_batch(limit)` | Cases 10, 21 |
| Dead-letter insert then separate discarded delete (`mutations.rs:281,658`) | One atomic `apply_outcomes` | Case 9 |
| Malformed rows hidden by `filter_map(...ok())` (`native.rs:269`, `web.rs:112`) | Quarantine plus a backend-owned `sweep_corrupt` | Cases 15, 18, 26 |
| `pub type StoreError = String` (`types.rs:12`) | One non-exhaustive structured `Error` | Compile-time |
| `Utc::now()` inside record constructors (`types.rs:88`) | Injected `Clock`; no date library in the runtime graph, so `Utc::now()` does not compile | Case 13 |
| `ORDER BY created_at` with no tie-break (`native.rs:253`, `web.rs:115`) | Total `(created_at, mutation_id)` order | Cases 11, 12 |
| Scoped and unscoped constructors side by side (`native.rs:70`/`:86`, `web.rs:81`/`:89`) | Required `ScopeKey`, no unscoped constructor, enforced on every read, retained on mismatch | Cases 22, 23, 29 |
| Body stored as pre-serialized text (`types.rs:23`) | Parsed `serde_json::Value`, so a malformed body is unrepresentable | Case 25 |
| Pending count cached in an `AtomicUsize` needing explicit refresh | Read from storage per call | `pending_count_needs_no_refresh` |
| Sticky `SyncStatus` booleans read in priority order (`mutations.rs:61,334-345`) | Per-pass `SyncReport`, with an explicit no-progress signal | Cases 7, 20; `each_pass_reports_what_it_did` |
| `MutationIntentDto` wire shape (`dto.rs:96-108`) | Preserved byte for byte | Case 27; the `dto.rs` ports in `tests/dto_oracle.rs` |

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
| Reads skip undecodable rows; only `sweep_corrupt` surfaces them | Sweeping inside a read makes reads mutate storage; erroring on a read lets one bad row wedge a healthy queue. The window is bounded because the runner sweeps every pass | Case 15 |

## Verification

`scripts/verify.sh` runs the whole gate set: `cargo fmt --check`, clippy with `-D warnings` on
native and `wasm32-unknown-unknown`, the test suite, wasm builds with and without default features,
and a `cargo llvm-cov` floor of 80% lines and functions. It also runs three textual checks — no
Dioxus, no RepForge entity names, no `Send` bound — which are explicitly secondary to the compile
gates and documented as such in the script.

Coverage sat at 89% regions / 96% lines / 93% functions when the floor was added, so the gate
records a property that already held rather than one being aimed at. It is 90% / 96% / 93% after
decision 011 added `src/rfc3339.rs` and its oracle, and 89% / 95% / 93% across 44 conformance cases
after decision 021's amendment.

The conformance suite lives in the library behind a `testing` feature rather than in `tests/`, so
D5's SQLite and IndexedDB backends run the identical cases through `StoreFactory`,
`VersionStoreFactory`, and `FaultInjection`. The three are separate traits on purpose: a backend
that cannot yet do one of them does not implement it and does not invoke the matching macro, which
leaves the gap visible in its test file rather than hidden behind a runtime skip.

## Constraints This Places On D5

- **Scope keys must encode injectively into storage names.** Character replacement is not injective;
  see `wiki/decisions/009-local-scope-identity.decision.md`.
- **A second durable schema, for cache versions.** Roughly `(scope, entity, version, stale)` with a
  uniqueness constraint on `(scope, entity)`, written atomically. `entity` is stored in exactly one
  textual form — whatever `EntityKey::as_str` returns — for the same reason `mutation_id` is: mixed
  spellings give a backend two entities where the caller has one. Ordering is not required of entity
  keys, so no collation constraint follows, but case sensitivity does.
- **`CacheVersionStore` is not generic over the application's key type.** It names entities by
  string, because a store has no registry to reconstruct typed keys with and a durable store can
  outlive the build that wrote its rows.
- **`mutation_id` must be stored in one consistent textual form** — exactly `MutationId::to_string`
  output — under a binary collation, or as the UUID's big-endian bytes. Mixed forms invert the
  pending order. Demonstrated by the unit tests in `src/id.rs`.
- **A storage format version must be written from the first durable write**, not added later.
- **`op` must be persisted and returned** across every transition, per
  `wiki/decisions/008-mutation-envelope-extensibility.decision.md`.
- **Two further columns are decided but not built.** Added 2026-08-27 from RepForge's single-flight
  proposal. A durable, globally monotonic `seq` assigned inside the enqueue transaction and used as
  the primary sort key (decision 016) — global rather than per-scope, because reads are scope
  filtered already and an `autoIncrement` object store then supplies it with no extra round trip.
  And a durable `attempts` count, incremented when a sent record stays queued and carried onto the
  dead letter at a caller-set bound (decision 017). Both are listed here rather than in the tables
  above because **neither is implemented**: this page documents what is built and tested, and these
  are constraints on a deliverable that has not started. See
  `wiki/proposals/single-flight-drain.proposal.md`.
- **The IndexedDB backend runs the suite through `frontbox_conformance_tests_async!`.** The
  synchronous macro drives each case with a caller-supplied `block_on`, and no such executor exists
  in a browser: an IndexedDB future suspends on a JavaScript callback that cannot fire until the
  stack unwinds, so any `block_on` deadlocks. The async macro emits `async fn` cases instead and
  lets `#[wasm_bindgen_test]` drive them. Both macros share one case list, so the two backends
  cannot silently run different suites.
