# D1 Core Cache Runtime Plan

Document Class: Plan
Status: Completed
Date: 2026-08-25
Category: Implementation Preparation
Scope: Prepare the first extraction slice: a framework-neutral outbox and sync runtime with an in-memory backend and conformance tests.
Sources: `raw/initial/2026-08-25T083750Z/sources`, `raw/research/2026-08-25-prior-art-survey`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/source-test-inventory.reference.md`, `wiki/references/prior-art-survey.reference.md`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/references/prior-art-survey.reference.md`, `wiki/decisions/010-batch-wire-format.decision.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/decisions/008-mutation-envelope-extensibility.decision.md`, `wiki/decisions/009-local-scope-identity.decision.md`, `wiki/plans/prior-art-survey.plan.md`

## Objective

Define the first implementation slice before code starts: a framework-neutral outbox and sync
runtime that keeps RepForge's useful protocol shapes while correcting the source defects found
during review.

**Implemented 2026-08-26** under explicit user authorization. The crate exists at the repository
root; see `## Implementation Outcome` at the end of this page for what was built, what the
implementation forced the design to change, and what it found that this plan had wrong.

D0a, the prior-art survey (`wiki/references/prior-art-survey.reference.md`), is complete. It did not
replace the D1 runtime shape, but it raised follow-up questions that remain open into D5.

## Inputs

- `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`
- `raw/initial/2026-08-25T083750Z/sources/persistence/types.rs`
- `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`
- `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`
- `raw/initial/2026-08-25T083750Z/sources/frontend/dto.rs`
- `raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md`
- `wiki/specs/source-frontend-cache-architecture.spec.md`
- `wiki/proposals/extraction-boundary.proposal.md`
- `wiki/references/source-test-inventory.reference.md`
- `wiki/references/prior-art-survey.reference.md`
- `raw/research/2026-08-25-prior-art-survey/sources/05-http-command-queues.md` - frontbox's nearest
  peer cohort; the closest structural matches are Redux Offline and Workbox Background Sync

## D1 Deliverables

- Core mutation id type.
- Generic mutation envelope: HTTP method, path, parsed JSON body, client timestamp, optional
  operation metadata, and the store's scope key (decisions 008 and 009).
- Core batch request and response types.
- Mutation result status enum.
- Core remote rejection payload, replacing the RepForge-only `ApiError` dependency.
- Structured, non-exhaustive core `Error`.
- Clock abstraction for deterministic timestamps, including `rejected_at`.
- Outbox persistence trait with bounded pending batches, total ordering, and atomic outcome
  application.
- Required scope key on every store constructor, stamped on records and enforced on read.
- Dead-letter read/count/purge trait.
- Corrupt-record quarantine path, including a backend-owned sweep for rows with no usable
  `MutationId`, and counts that keep quarantined records out of the pending total.
- Sync transport trait that distinguishes offline from attempted request failure.
- Sync runner that applies batch results according to decisions 003 and 005, and reports a
  no-progress sync when every record in a batch was retained.
- In-memory backend with injectable failures for conformance tests.
- Ported source-test oracle from `persistence/mutations.rs` before new behavior tests are added.

## Proposed Core Shapes

Revised per decisions 001-009:

- No `type Error` per trait; all traits use the core `Error`.
- No `Send` bounds; backends are generic parameters, never `dyn` trait objects in v0.
- `delete` plus `dead-letter insert` collapse into one transactional `apply_outcomes`.
- `Blocked` maps to `Retain`, not `DeadLetter`.
- Batching with a `limit` is new design, not source extraction.
- Remote rejection is a core payload, not RepForge `ApiError`.
- Timestamps come from an injected clock, not direct `Utc::now()` calls in constructors.
- Corrupt records have an explicit quarantine path, addressable and id-less.
- Pending ordering is total: `(created_at, mutation_id)`, not `created_at` alone.
- Retained work is observable: a sync that drains nothing says so.
- Records are `#[non_exhaustive]` and constructor-built, so fields can be added without a
  breaking change.
- The body is parsed `serde_json::Value`, so a malformed body cannot enter the outbox.
- Store identity is a required, caller-composed `ScopeKey`; there is no unscoped constructor.

```rust
pub struct RemoteRejection {
    pub code: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ScopeKey(String);

impl ScopeKey {
    /// Rejects an empty key with `Error::InvalidScopeKey`.
    pub fn new(key: impl Into<String>) -> Result<Self, Error>;
    pub fn as_str(&self) -> &str;
}

#[non_exhaustive]
pub struct OperationMeta {
    pub name: String,
    pub version: Option<String>,
}

/// What a caller enqueues, and the wire representation of it (decision 010).
#[non_exhaustive]
pub struct MutationIntent {
    pub mutation_id: MutationId,
    pub method: String,
    pub path: String,
    pub created_at: i64,   // serialized as `client_datetime`, RFC 3339
    pub body: serde_json::Value,
    pub op: Option<OperationMeta>,
}

/// What a store returns. Built only by `OutboxRecord::stamp(intent, scope)`, so the scope
/// cannot be supplied by a caller. See decision 008's 2026-08-26 amendment.
#[non_exhaustive]
pub struct OutboxRecord {
    pub mutation_id: MutationId,
    pub method: String,
    pub path: String,
    pub body: serde_json::Value,
    pub created_at: i64,
    pub op: Option<OperationMeta>,
    pub scope: ScopeKey,
}

pub trait Clock {
    fn now_ms(&self) -> i64;
}

pub enum Disposition {
    Delete,
    DeadLetter { error: Option<RemoteRejection> },
    Retain,
    Quarantine { reason: String },
}

pub struct Outcome {
    pub id: MutationId,
    pub disposition: Disposition,
}

pub trait OutboxStore {
    fn scope(&self) -> &ScopeKey;

    /// Stamps this store's scope on the record. A caller cannot supply one.
    async fn enqueue(&self, intent: MutationIntent) -> Result<(), Error>;

    /// Oldest-first by `(created_at, mutation_id)`. The compound key makes ordering
    /// total and reproducible without adding a schema column; see Ordering Policy.
    /// Records stamped with a different `ScopeKey` are never returned (decision 009).
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;

    /// Count of decodable pending records in this store's scope only. Quarantined rows
    /// are excluded here and reported by `QuarantineStore::count`, so a wedged record can
    /// never hide inside a pending total, and records under another `ScopeKey` are never
    /// counted here (decision 009).
    async fn pending_count(&self) -> Result<usize, Error>;

    /// Backend-owned scan for rows that cannot be decoded into an `OutboxRecord` or
    /// converted into a sync intent. Moves them to quarantine and returns how many were
    /// moved. This is the id-less path decision 006 requires: rows whose `mutation_id`
    /// is missing or unparseable cannot be addressed through `Outcome`, so only the
    /// backend can find them.
    async fn sweep_corrupt(&self) -> Result<usize, Error>;

    /// Apply every outcome atomically: deletions, dead-letter inserts, and quarantine
    /// transitions either all commit or none do.
    async fn apply_outcomes(&self, outcomes: &[Outcome]) -> Result<(), Error>;
}

pub trait DeadLetterStore {
    async fn list(&self, limit: usize) -> Result<Vec<DeadLetterRecord>, Error>;
    async fn count(&self) -> Result<usize, Error>;
    async fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize, Error>;
}

pub trait QuarantineStore {
    async fn list(&self, limit: usize) -> Result<Vec<QuarantinedRecord>, Error>;
    async fn count(&self) -> Result<usize, Error>;
}

pub trait SyncTransport {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error>;
}
```

`Disposition::Quarantine` applies when the backend can still identify the affected pending record.
Rows without a valid `MutationId` are handled by `OutboxStore::sweep_corrupt`, because they cannot
be addressed through `Outcome` at all.

`QuarantineStore` exposes no `insert` for the same reason `DeadLetterStore` does not: a quarantine
entry is a transition out of the outbox, produced either by `apply_outcomes` or by `sweep_corrupt`,
never as a free-standing write.

## Ordering Policy

D1 orders pending work oldest-first by the compound key `(created_at, mutation_id)`.

`created_at` alone is not a total order. It comes from the client clock
(`persistence/types.rs:38`), and neither backend breaks ties: native emits
`ORDER BY created_at ASC` with no secondary key (`persistence/native.rs:253`) and web sorts with
`sort_by_key(|r| r.created_at)` (`persistence/web.rs:115`). Two mutations enqueued in the same
millisecond can therefore come back in either order, and differently on each backend.

Adding `mutation_id` as the tie-break makes the order total, reproducible, and identical across
backends, with no new schema column and no clock dependency. It buys determinism, not causality:
`MutationId` is a random UUID, so same-millisecond ties resolve stably but arbitrarily.

**The tie-break constrains the durable schema, which this plan originally did not say.**
`MutationId: Ord` compares the UUID's 16 bytes, so a durable backend has to sort the same way.

Text ordering agrees with byte ordering as long as **every row uses the same textual form**. ASCII
puts digits below both letter cases, so consistently lowercase and consistently uppercase hex each
sort correctly, and the hyphens sit at fixed positions so they never separate two canonical strings.
A SQLite `ORDER BY created_at, mutation_id` over a `TEXT` column holding `MutationId::to_string`
output reproduces the in-memory order exactly, as does a `BLOB` column holding the UUID's big-endian
bytes.

What breaks it is *mixed* forms. One row uppercase and another lowercase reverses any pair straddling
the case boundary; a row written without hyphens sorts before a hyphenated one whenever their first
eight characters match, since `-` is `0x2D`, below every hex digit. Both are what a migration, a
second write path, or a hand-edited row produces. The D5 rule is therefore *store exactly what
`MutationId::to_string` returns and never transform it* — and the column must use a binary collation,
not a case-folding one.

An earlier version of this note claimed uppercase hex alone would invert the order. That was wrong,
and the unit tests in `src/id.rs` demonstrate what actually inverts it. Corrected 2026-08-26.

If real causal ordering is required, the answer is a monotonic sequence number assigned at enqueue.
That remains open. It was originally deferred to durable backends in D5 because it adds a column.
D0a raises its priority, but on narrower evidence than the first draft claimed: Replicache is the
only surveyed system that documents per-client causal operation order, where a mutation id
"describe[s] a causal order to mutations from this client, and that order is respected by the
server." CouchDB sequence IDs are database-scoped, RxDB checkpoints are resume tokens, and
PowerSync documents FIFO without a published causal operation ID. D1 must not assert causal
ordering it does not provide.

`DeadLetterStore::insert` remains absent from the public surface. A dead letter is a transition
from a known pending record after a server verdict, not a free-standing write.

## Test Oracle

Before writing new tests, inventory and port the 12 source tests in
`persistence/mutations.rs`. Those tests are not all accepted as final behavior, but they are the
cheapest behavioral oracle for detecting accidental drift while changing semantics deliberately.

The D1 test suite should then cover:

1. `Applied` mutation is removed from the outbox.
2. `Duplicate` mutation is removed from the outbox.
3. `Rejected` mutation is removed and inserted into dead letters.
4. `Blocked` mutation remains queued.
5. `Pending` mutation remains queued.
6. A mixed batch applies all five statuses correctly in one call.
7. Transport `Error::Offline` leaves pending work untouched and does not enter the failure path.
8. Transport `Error::Transport` leaves pending work untouched and is visible as an attempted send
   failure.
9. Failure partway through `apply_outcomes` leaves no partial state: a record is never both
   pending and dead-lettered.
10. `pending_batch(limit)` never returns more than `limit`.
11. Ordering is oldest-first for distinct `created_at` values.
12. Same-`created_at` records order stably by `mutation_id`, and repeated reads of the same queue
    return the same sequence. Assert determinism only, never causal order.
13. Dead-letter `rejected_at` comes from the injected clock.
14. Dead-letter purge is deterministic with an injected cutoff.
15. A corrupt pending record is quarantined and stops counting toward `pending_count`, while
    remaining visible through `QuarantineStore`.
16. A server result for an unknown mutation id is treated as a protocol anomaly and does not
    mutate unrelated records.
17. Remote rejection payloads round-trip through the persistence representation.
18. `sweep_corrupt` finds and quarantines a row whose `mutation_id` cannot be parsed, proving the
    id-less path works where `Outcome` cannot reach.
19. **`Blocked` clears on the next sync.** A batch where the head is `Rejected` and the tail is
    `Blocked` leaves the tail queued; the following sync, with the blocker dead-lettered and gone,
    applies the tail normally. This is the liveness guarantee in decision 005.
20. **A fully retained batch is reported as no-progress.** A sync where every result is `Pending`
    or `Blocked` produces no deletions and no dead letters, and the runner surfaces that so a
    caller can distinguish a stalled queue from an idle one.
21. A `Pending` prefix longer than `limit` keeps later records unsent, and this is observable
    through the no-progress signal rather than silent. Documents the head-of-line behavior instead
    of pretending it cannot happen.
22. **Cross-scope records are invisible.** A record enqueued under scope A is absent from scope
    B's `pending_batch`, absent from B's `pending_count`, dead-letter count, and quarantine count,
    and is still present and syncable when a store is reopened under A. Proves decision 009's
    enforcement rule and its retain-on-mismatch rule in one pass.
23. An empty `ScopeKey` is rejected at construction with `Error::InvalidScopeKey`.
24. `OperationMeta` survives every transition: a record enqueued with `op` set carries it into the
    dead letter on `Rejected` and into quarantine on a corrupt-record transition. Core never reads
    `name` to make a decision.
25. A body that is not valid JSON never enters the outbox. **Refined during implementation:** this
    is not a rejection at `enqueue`, because there is no code path to reject. `MutationIntent` holds
    a parsed `serde_json::Value`, so a malformed body is not a value that exists and the failure
    happens before an intent can be constructed. The residual corrupt-body surface is durable
    corruption after a successful write, which is cases 15 and 18.
26. A `created_at` outside the range the wire format can express is a corrupt record, not a panic
    and not a mid-batch serialization failure. New: added because decision 010 renders timestamps
    as RFC 3339. The bound tightened on 2026-08-27 to RFC 3339's four-digit year (decision 011);
    the fixture is `i64::MAX`, unrepresentable under either rule, so the case is unchanged.
27. An intent with no `op` serializes to exactly the five keys the source protocol defines, with
    `client_datetime` as the expected RFC 3339 string, and round-trips. Setting `op` adds the key
    and only then. New: this is the assertion that makes decision 010 checkable.
28. `sync_once` re-entered while a pass is in flight returns without sending, so a periodic loop
    overlapping a manual trigger cannot double-send a batch. New: ports the source's
    compare-exchange guard (`persistence/mutations.rs:612-621`) into a testable form.
29. Scope keys are compared exactly. Keys differing only by surrounding whitespace or case are
    distinct scopes and cannot see each other's records. New: normalization would be non-injective,
    which is the same collapse decision 009 flags for backend storage-name encoding.
30. A sync pass dropped mid-flight does not wedge the runner: the next pass runs normally rather
    than reporting `AlreadyRunning` forever. Added 2026-08-26 after review found the in-flight flag
    was released only on the success path, which a cancelled `use_future` would have poisoned.
31. An outcome set naming one record twice is rejected with `Error::Protocol` and commits nothing.
    Added 2026-08-26 after review found `apply_outcomes` would happily write two dead letters for
    one row.
32. A verdict repeated inside one server response is an anomaly and **neither verdict is applied**;
    the record stays queued. Added 2026-08-26 alongside case 31. **Corrected 2026-08-27:** the case
    originally blessed first-wins, which made JSON array position the tie-break between two verdicts
    the docs said there was no basis for choosing between. Review found the contradiction.
33. A mutation the server returned no verdict for stays queued and is counted in
    `SyncReport::retained`. Added 2026-08-27: the behaviour was already implemented and documented,
    but the runtime spec cited case 21 as its proof, and case 21 is about records that were never
    sent at all. This case sends the record and has the server answer the batch without mentioning
    it, which is the distinct thing.

## Out Of Scope For D1

- Cache versioning and invalidation (`EntityCache`) - D2.
- Dioxus adapter - D3.
- RepForge migration trial - D4.
- SQLite and IndexedDB durable backends - D5.
- Retry/backoff policy - no source precedent beyond fixed loop timing; design after prior-art
  review or a concrete product requirement.
- Dead-letter recovery UI or retry workflows.
- Generic read-model storage beyond the mutation outbox/dead-letter/quarantine path.

## D0a Follow-Up

The prior-art survey does not block D1 implementation. Its follow-ups split by *when they must be
settled*, which is not the same as when they are implemented. Two were shape decisions that a public
D1 API could foreclose; both were settled on 2026-08-26. Three can wait for durable backends.

### Settled Before The D1 Public API Freezes

Both shape questions a published D1 API could foreclose are now decided.

- **Caller-owned operation metadata** - `wiki/decisions/008-mutation-envelope-extensibility.decision.md`.
  Records are `#[non_exhaustive]` and constructor-built, the body is parsed `serde_json::Value`, and
  an optional `OperationMeta { name, version }` rides along. Core stores and moves it; core never
  branches on it in D1.
- **Local namespace/scope identity** - `wiki/decisions/009-local-scope-identity.decision.md`. Every
  store constructor takes a required, non-empty, caller-composed `ScopeKey`. There is no unscoped
  constructor. Records are stamped, reads verify, and a mismatch retains rather than discards.

Decision 008 lands before 009: stamping a scope key on records is additive only because the records
are already non-exhaustive.

### Can Wait For Durable Backends (D5)

- **Monotonic enqueue sequence.** Adds a column, so it belongs with durable storage. Evidence is
  one system: Replicache's causally ordered per-client mutation id. D1 must continue to not assert
  causal ordering it does not provide.
- **Attempt count, last-error, or aging metadata for retained work.** D1's no-progress signal is
  the minimum. Reference designs: Redux Offline's `retry() -> null` discard over a 1s-to-1h
  schedule, Workbox's `maxRetentionTime`, Amplify's `outboxStatus{isEmpty}`. If frontbox adopts a
  bound, it should dead-letter at the bound rather than discard, per decision 005.
- **Durable storage format versioning.** Applies once SQLite or IndexedDB backends exist, but the
  version must be written from the *first* durable write, not added afterwards. RxDB's migration of
  replication metadata so clients need not restart replication is the reference case.

## Verification Gates For Implementation

All gates below passed on 2026-08-26 via `scripts/verify.sh`. D1 was not complete until:

- the in-memory conformance tests pass,
- the adapted source tests from `persistence/mutations.rs` are represented,
- native build passes,
- `wasm32-unknown-unknown` build passes,
- `cargo fmt --check` passes,
- `cargo clippy --all-targets -- -D warnings` passes on native and for the wasm target,
- no Dioxus dependency appears in core,
- no RepForge entity names appear in core,
- no `Send` bound is required by public core traits,
- and the D1 outcome is promoted back into the spec and log.

The prose gates — no Dioxus, no RepForge entity names, no `Send` bound — are enforced by `grep` in
`scripts/verify.sh` rather than left as intentions. The `Send` gate is textual by necessity: a
type-level assertion can prove a bound is *satisfied*, never that it is *absent*. It is backed by
`tests/not_send.rs`, where `InMemoryStore` — which holds `Rc`s and is therefore `!Send` —
implements all three storage traits. That would not compile if any trait required `Send`.

## Constraints

- The user owns Git state. Do not stage, commit, reset, or otherwise write Git state.
- Do not modify `raw/`; it is immutable provenance.
- Do not begin implementation before explicit authorization. D1 was authorized 2026-08-26 and D2 on
  2026-08-27; this still binds D3 onward.
- Treat `08-offline-sync.spec.md` as normative when it conflicts with `mutations.rs`.
- Keep all source divergences visible in the docs until implementation tests prove the new
  behavior. Discharged for D1: every divergence now has a named conformance case, tabulated in
  `wiki/specs/source-frontend-cache-architecture.spec.md` under `## D1 Extraction Outcome`.

## Implementation Outcome

Implemented 2026-08-26. One crate, `frontbox`, at the repository root, with modules matching the
future crate split rather than a workspace, per the extraction-boundary proposal.

### What was built

`MutationId`, `ScopeKey`, `Error`, `Clock`/`SystemClock`/`ManualClock`, the record types, the batch
protocol, the three storage traits, `SyncTransport`, `SyncRunner`, a complete in-memory backend with
failure injection, and a backend-agnostic conformance suite behind a `testing` feature.

**The behaviour it actually has is recorded in `wiki/specs/frontbox-runtime.spec.md`**, which is the
durable page. This section keeps only what belongs to a plan: what the act of implementing changed
about the plan itself.

77 tests pass at the end of D1: 32 conformance cases, 1 fault-injection case, 29 unit tests (4
identifier-ordering, 5 clock, 5 error, 4 scope, 11 RFC 3339 rendering), 2 `!Send` proofs, 6
source-oracle ports from `persistence/mutations.rs`, 5 wire-format oracles from `frontend/dto.rs`,
and 2 doctests. Both wasm builds and clippy on both targets are clean, and coverage is 90% regions /
96% lines / 93% functions.

D2 later took the crate to 96 tests; see `wiki/plans/d2-cache-invalidation.plan.md`. The figures
above are the D1 state, kept as the record of what this plan delivered.

Counts as of 2026-08-27, after two rounds that day. The 2026-08-26 build was 50 tests. The first
round added case 33 and the review-driven unit tests for `clock`, `error`, and `scope`, plus one
doctest on `ScopeKey`, reaching 66. The second added `src/rfc3339.rs` and its chrono oracle
(decision 011), reaching 77.

### What implementation changed

- **The enqueue input and the stored record are two types.** Decision 008's single `OutboxRecord`
  could not also satisfy decision 009's stamping rule. Recorded as an amendment on decision 008.
- **A new decision, 010**, pinning the batch wire format to the source server's payload. It also
  pinned `chrono` into the compatibility surface — later reversed by decision 011, which kept the
  payload and dropped the dependency.
- **No date library in the runtime graph**, so `Utc::now()` does not compile in this crate. The
  rendering lives in `src/rfc3339.rs` and chrono survives only as a dev-dependency oracle, itself
  declared without the `clock` feature so the rule holds in tests too. Decision 002's
  clock-injection rule is enforced by the build rather than by review.
- **`MutationId: Ord` constrains the D5 durable schema.** See the Ordering Policy above.

### What implementation found that this plan had wrong

- **Case 25 was mis-specified.** It asked for a rejection at `enqueue`; a parsed body makes the
  malformed case unrepresentable, so there is nothing to reject. Corrected above.
- **"Port the 12 tests in `persistence/mutations.rs`" overstates what is portable.** Five of the
  twelve assert RepForge route construction and have no counterpart in a domain-neutral library;
  the other seven ported into six tests, since the two `SyncStatus` tests collapse into one.
  The `frontend/dto.rs` round-trip tests turned out to be the more valuable oracle, because they pin
  the wire format decision 010 commits to. Recorded in `tests/source_oracle.rs`, in
  `tests/dto_oracle.rs`, and in `wiki/references/source-test-inventory.reference.md`.
- **The source's `SyncStatus` does not transfer.** It is four sticky booleans read back in priority
  order, and the source's own test has to reach into private atomics to set them up — the status is
  not reachable through its public API. `SyncReport` reports per pass instead of holding sticky
  state.
- **The in-memory backend deliberately holds every scope in one store.** Scope enforcement has to
  hold when two scopes share physical storage, because that is exactly the D5 hazard decision 009
  names. A backend modelled as one-store-per-scope would have made case 22 pass for the wrong
  reason.

### Carried into D5

- Injective `ScopeKey` encoding for storage names, plus the `MutationId` textual-form constraint
  above.
- Durable storage format versioning, written from the first durable write.
- The conformance suite itself: `StoreFactory` and `FaultInjection` are the seams SQLite and
  IndexedDB implement to run these same 32 cases, plus the fault-injection case.
- **IndexedDB runs the suite through the async macros.** `frontbox_conformance_tests_async!` and
  `frontbox_fault_injection_tests_async!` emit `async fn` cases for a harness that drives them
  itself, because a browser has no `block_on` that an IndexedDB future can make progress under. Both
  emission shapes read one shared case list, so the two backends cannot drift into running different
  suites. Added 2026-08-27; the wasm example in the module docs had until then named a
  `wasm_bindgen_futures` function that does not exist.
