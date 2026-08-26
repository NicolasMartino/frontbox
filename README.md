# frontbox

Offline-first cache and mutation outbox for Rust frontends.

`frontbox` is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. The library is being extracted from RepForge's Dioxus application into a
framework-neutral core with storage and framework adapters.

## Current Status

**D1, the core runtime, is implemented** (2026-08-26). The crate is a single package at the
repository root; it is not published and the version is `0.0.0`.

What works today: a durable-shaped mutation outbox with bounded batches, atomic outcome application,
dead letters for server refusals, quarantine for locally corrupt rows, required per-scope isolation,
and a sync runner. Storage is in-memory only — the SQLite and IndexedDB backends are D5.

Everything after D1 is still planning. See [the roadmap](wiki/roadmaps/extraction.roadmap.md).

```
./scripts/verify.sh     # every gate: fmt, clippy on native and wasm, tests, both wasm builds, coverage
cargo test --all-features
```

96 tests pass: 42 backend-agnostic conformance cases, a fault-injection case, unit tests, `!Send`
proofs, ports of the source system's own tests, and doctests. Coverage is gated at 80%.

The wire format's RFC 3339 timestamp is rendered by the crate itself, with no date library in the
runtime dependency graph. `chrono` is a dev-dependency the rendering is compared against, so the
bytes stay byte-compatible with the source server without putting a pre-1.0 crate in the semver
contract (decision 011).

## Example

Enqueue a write, then sync it. The transport is where an application puts its own HTTP client and
its own credentials — neither is the library's business, and no queued record ever carries a token.

```rust
use frontbox::{
    Clock, InMemoryBackend, ManualClock, MutationBatchRequest, MutationBatchResponse, MutationId,
    MutationIntent, MutationResult, MutationStatus, OperationMeta, OutboxStore, ScopeKey,
    SyncRunner, SyncTransport,
};

struct MyServer;

impl SyncTransport for MyServer {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, frontbox::Error> {
        // Resolve credentials here, per send — a queued mutation may be days old.
        // Return Error::Offline when no request could be attempted; Error::Transport when one was.
        Ok(MutationBatchResponse::new(
            request
                .mutations
                .iter()
                .map(|m| MutationResult::new(m.mutation_id, MutationStatus::Applied))
                .collect(),
        ))
    }
}

let clock = ManualClock::new(1_700_000_000_000);
let backend = InMemoryBackend::new(clock.clone());

// There is no unscoped constructor. Compose principal, tenant, and schema version yourself.
let store = backend.open(ScopeKey::new("user:alice@tenant:acme@v1")?);

store
    .enqueue(
        MutationIntent::new(
            // `MutationId::new()` needs the default `v4` feature. Supply your own id instead
            // where you have one — that is what makes a direct write and a queued retry the
            // same mutation.
            MutationId::new(),
            "POST",
            "/api/v1/sessions",
            serde_json::json!({ "name": "Morning" }),
            clock.now_ms(),
        )
        .with_op(OperationMeta::new("start_session").with_version("1.2.0")),
    )
    .await?;

let runner = SyncRunner::new(backend.open(store.scope().clone()), MyServer);
let report = runner.sync_once().await?;

assert_eq!(report.counts.applied, 1);
assert!(report.made_progress());          // false on an idle pass too; is_stalled() separates them
assert!(!report.is_stalled());            // sent work and drained none of it
assert_eq!(store.pending_count().await?, 0);
```

The same example, and a second scope proving isolation, run as a doctest in `src/lib.rs`. The
doctest passes a fixed id rather than calling `MutationId::new()`, so that it still compiles under
`--no-default-features`.

## Shape

Modules today, crates later. The split below is what the module boundaries are drawn for; the crate
split waits until the public API has survived the D4 migration trial.

- `frontbox-core`: mutation protocol types, sync runner, outbox/dead-letter/quarantine traits,
  cache versioning, invalidation model, and core errors.
- `frontbox-sqlite`: native SQLite persistence backend.
- `frontbox-indexeddb`: web IndexedDB persistence backend.
- `frontbox-dioxus`: Dioxus adapter for signals, hooks, providers, and event wiring.
- `examples/repforge-style`: a small CQRS-style example proving the API without RepForge domain
  coupling.

## Project Knowledge

- [Source architecture](wiki/specs/source-frontend-cache-architecture.spec.md)
- [Extraction boundary](wiki/proposals/extraction-boundary.proposal.md)
- [Extraction roadmap](wiki/roadmaps/extraction.roadmap.md)
- [D1 core runtime plan](wiki/plans/d1-core-cache-runtime.plan.md)
- [Prior-art survey plan](wiki/plans/prior-art-survey.plan.md)
- [Decisions](wiki/decisions/) — ten accepted, covering concurrency, errors, atomicity, auth,
  outcome policy, corrupt records, entity keys, envelope extensibility, scope identity, and the
  wire format
- [Wiki index](wiki/index.md)

## Development Notes

`raw/` contains immutable copied source material. `wiki/` contains the maintained project knowledge
base. Git state is user-owned; agents should edit files only and avoid staging or committing unless
explicitly asked.
