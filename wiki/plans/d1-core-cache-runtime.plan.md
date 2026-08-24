# D1 Core Cache Runtime Plan

Document Class: Plan
Status: Active
Date: 2026-08-25
Category: Implementation Preparation
Scope: Prepare the first extraction slice: a framework-neutral outbox and sync runtime with an in-memory backend and conformance tests.
Sources: `raw/initial/2026-08-25T083750Z/sources`, `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/references/source-test-inventory.reference.md`
Related: `wiki/roadmaps/extraction.roadmap.md`, `wiki/decisions/001-single-threaded-core.decision.md`, `wiki/decisions/002-error-model.decision.md`, `wiki/decisions/003-atomic-outcome-application.decision.md`, `wiki/decisions/004-transport-auth-and-offline.decision.md`, `wiki/decisions/005-mutation-outcome-policy.decision.md`, `wiki/decisions/006-corrupt-record-policy.decision.md`, `wiki/plans/prior-art-survey.plan.md`

## Objective

Define the first implementation slice before code starts: a framework-neutral outbox and sync
runtime that keeps RepForge's useful protocol shapes while correcting the source defects found
during review.

Implementation has not started. Do not create `Cargo.toml`, `src/`, or tests until the user gives
explicit implementation authorization. D1 implementation is additionally gated on D0a, the
prior-art survey (`wiki/plans/prior-art-survey.plan.md`), because that survey may still change
public API shapes. Refining this plan is not gated.

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

## D1 Deliverables

- Core mutation id type.
- Generic mutation envelope: HTTP method, path, JSON body, and client timestamp.
- Core batch request and response types.
- Mutation result status enum.
- Core remote rejection payload, replacing the RepForge-only `ApiError` dependency.
- Structured, non-exhaustive core `Error`.
- Clock abstraction for deterministic timestamps, including `rejected_at`.
- Outbox persistence trait with bounded pending batches, total ordering, and atomic outcome
  application.
- Dead-letter read/count/purge trait.
- Corrupt-record quarantine path, including a backend-owned sweep for rows with no usable
  `MutationId`, and counts that keep quarantined records out of the pending total.
- Sync transport trait that distinguishes offline from attempted request failure.
- Sync runner that applies batch results according to decisions 003 and 005, and reports a
  no-progress sync when every record in a batch was retained.
- In-memory backend with injectable failures for conformance tests.
- Ported source-test oracle from `persistence/mutations.rs` before new behavior tests are added.

## Proposed Core Shapes

Revised per decisions 001-006:

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

```rust
pub struct RemoteRejection {
    pub code: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
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
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), Error>;

    /// Oldest-first by `(created_at, mutation_id)`. The compound key makes ordering
    /// total and reproducible without adding a schema column; see Ordering Policy.
    async fn pending_batch(&self, limit: usize) -> Result<Vec<OutboxRecord>, Error>;

    /// Count of decodable pending records only. Quarantined rows are excluded here and
    /// reported by `QuarantineStore::count`, so a wedged record can never hide inside a
    /// pending total.
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

If real causal ordering is required, the answer is a monotonic sequence number assigned at enqueue.
That remains open and belongs with the durable backends in D5, because it does add a column. D1
must not assert causal ordering it does not provide.

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

## Out Of Scope For D1

- Cache versioning and invalidation (`EntityCache`) - D2.
- Dioxus adapter - D3.
- RepForge migration trial - D4.
- SQLite and IndexedDB durable backends - D5.
- Retry/backoff policy - no source precedent beyond fixed loop timing; design after prior-art
  review or a concrete product requirement.
- Dead-letter recovery UI or retry workflows.
- Generic read-model storage beyond the mutation outbox/dead-letter/quarantine path.

## Verification Gates For Implementation

When implementation is authorized, D1 is not complete until:

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

## Constraints

- The user owns Git state. Do not stage, commit, reset, or otherwise write Git state.
- Do not modify `raw/`; it is immutable provenance.
- Do not begin implementation before explicit authorization.
- Treat `08-offline-sync.spec.md` as normative when it conflicts with `mutations.rs`.
- Keep all source divergences visible in the docs until implementation tests prove the new
  behavior.
