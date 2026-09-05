# frontbox

Offline-first cache and mutation outbox for Rust frontends.

`frontbox` is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. The library is being extracted from RepForge's Dioxus application into a
framework-neutral core with storage and framework adapters.

## Current Status

**D1 through D5 and D4d are built**: the core runtime (2026-08-26), cache versioning
(2026-08-27), the drain loop and Dioxus adapter plus the migration trial's API half (2026-08-29),
the trial's durability half, the four-platform build and both durable backends (2026-08-30), and
two domain servers behind one queue (2026-08-31). Nothing is published and every version is
`0.0.0`. The repository is a workspace: the root package is `frontbox`, with
`crates/frontbox-dioxus`, `crates/frontbox-sqlite`, `crates/frontbox-indexeddb` and the four trial
crates under `examples/` — `todo-core`, `todo-server`, `user-server` and `todo-app`.

What works today: a durable mutation outbox with bounded batches, atomic outcome application, dead
letters for server refusals, quarantine for locally corrupt rows, required per-scope isolation, a
sync runner, a drain loop that empties the queue in one call, an opaque read-model row store with a
hydration merge rule, and version-based cache invalidation. **Storage is durable on both targets** —
SQLite natively, IndexedDB in the browser — behind the same traits and the same conformance suite.

**D5's last owed proof line was met on 2026-08-31.** Two browser realms are now watched
contending for the cross-realm drain lock in `crates/frontbox-indexeddb/tests/cross_realm.rs`,
rather than argued from a single-realm test that cannot reach a second realm by construction. Every
deliverable with a plan is built, and none is carrying an unwitnessed promise.

**The trial is the evidence.** A Dioxus todo application over two axum/sqlx servers, with
observation and regression tests running over real HTTP — and neither server depends on `frontbox`,
deliberately, so those tests are the wire-format oracle rather than a serde round-trip. It runs on
web, macOS desktop, iOS and Android from one component tree.

**D4d is built too** (2026-08-31): two domain servers behind one queue, so cross-service ordering
and the inbound invalidation path are observed rather than asserted. **What is not built:** D6
(adoption docs), which has no plan. See [the roadmap](wiki/roadmaps/extraction.roadmap.md).

```
./scripts/verify.sh     # every gate: fmt, clippy, tests, wasm and desktop builds, coverage, docs
cargo test -p frontbox --all-features
```

**As of 2026-09-02, all 35 gates pass**: 136 tests on `-p frontbox --all-features` — 68
backend-agnostic conformance cases, fault injection, unit tests, `!Send` proofs, ports of the source
system's own tests, and doctests — plus 69 on SQLite running *the same cases*, 57 trial tests over
real HTTP, 13 adapter tests, every wasm and desktop build, clippy everywhere, rustdoc, and 89.79%
region / 96.88% line coverage against an 80% floor.

The IndexedDB backend is compiled but not run by `verify.sh`: the browser suite needs a version-
matched chromedriver, which a build gate cannot assume. The command is in `scripts/verify.sh`.

Those figures are dated because they are claims, not decoration. `./scripts/verify.sh` prints
today's, and is the only thing entitled to be believed about them. They were stale for two days
after D4b, D4c and D5 shipped, which is why the gate count is now something the script reports
rather than something this file remembers.

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

Every crate below is built. `src/` stays at the repository root because the wiki cites
`src/<file>.rs:<line>` in hundreds of places.

- `frontbox` (root package): mutation protocol types, sync runner and drain loop,
  outbox/dead-letter/quarantine traits, cache versioning, invalidation model, and core errors.
  Eventually `frontbox-core`.

  **Cache versioning is optional.** `CacheVersionStore` is a separate trait on a separate type with
  its own conformance suite; the outbox and the sync runtime are complete without it, and a plain
  periodic re-read is a legitimate invalidation strategy rather than a shortcut. See
  [when versions earn their keep](wiki/compatibility/cache-versions-are-optional.compat.md), which
  also covers the one thing skipping them costs.
- `crates/frontbox-dioxus`: **built**. Context provider, store counts and invalidation as signals,
  and the sync cadence. The loop itself is core's, because it needs no framework and the
  conformance suite cannot reach an adapter.
- `crates/frontbox-sqlite`: **built**. Native SQLite persistence — outbox, dead letters,
  quarantine, rows and cache versions, with `apply_outcomes` inside one `IMMEDIATE` transaction.
  `rusqlite` rather than `sqlx`, because `sqlx`'s futures are `Send`-shaped and the gate forbids it.
- `crates/frontbox-indexeddb`: **built**. The same traits over IndexedDB, plus the cross-realm drain
  lock from decision 031 over Web Locks. Runs the identical conformance cases in headless Chrome.
- `examples/todo-core`: **built**. The trial application — always-enqueue writes, an optimistic
  projection over durable rows, direct dispatch, ten original observation tests, and review
  regressions. No Dioxus, so an adapter leak would fail to compile.
- `examples/todo-server`: **built**. axum over sqlx/SQLite. Does not depend on `frontbox`, on
  purpose — see [the trial plan](wiki/plans/d4a-offline-todo-trial.plan.md).
- `examples/todo-app`: **built**. The Dioxus UI over `todo-core`, on **four platforms from one
  component tree** — web, macOS desktop, iOS and Android, with nothing below `main.rs`
  conditionally compiled. It drives the drain loop through `frontbox_dioxus::use_sync_loop`; it
  could not, until that hook learned to take closures instead of a `SyncRunner` — see the D4a plan's
  `## The UI, And What It Cost To Wire` and `examples/todo-app/src/sync.rs`.

## Running The D4a Trial

The automated trial runs from the workspace root:

```bash
cargo test -p todo-core -p todo-server
```

Run the full demo with Docker Compose:

```bash
docker compose up --build
```

That serves the UI at `http://127.0.0.1:8081`, the API at `http://127.0.0.1:3000`, Swagger UI at
`http://127.0.0.1:3000/swagger-ui/`, and the raw OpenAPI document at
`http://127.0.0.1:3000/api-docs/openapi.json`. `TODO_SERVER_URL` is compiled into the WASM bundle,
so set it before building the app image if the API is published on another host port.

Run the server and UI in separate shells:

```bash
cargo run -p todo-server
cargo install dioxus-cli   # once; `dx` is the Dioxus CLI
cd examples/todo-app
dx serve --web
```

The server defaults to `127.0.0.1:3000`. Use `TODO_SERVER_ADDR` for the server bind address and
`TODO_SERVER_URL` for the UI build if you need another port. The UI's `offline` checkbox exercises
the deterministic transport switch without killing the server.

## Project Knowledge

- [Source architecture](wiki/specs/source-frontend-cache-architecture.spec.md)
- [Extraction boundary](wiki/proposals/extraction-boundary.proposal.md)
- [Extraction roadmap](wiki/roadmaps/extraction.roadmap.md)
- [D1 core runtime plan](wiki/plans/d1-core-cache-runtime.plan.md)
- [D2 cache invalidation plan](wiki/plans/d2-cache-invalidation.plan.md)
- [D3 drain loop and Dioxus adapter plan](wiki/plans/d3-drain-and-dioxus-adapter.plan.md)
- [D4a offline todo trial plan](wiki/plans/d4a-offline-todo-trial.plan.md) — and its findings
- [Prior-art survey plan](wiki/plans/prior-art-survey.plan.md)
- [D4b/D4c/D5 are in the roadmap](wiki/roadmaps/extraction.roadmap.md) rather than in separate
  plans of their own; [D4d's plan](wiki/plans/d4d-multi-domain-trial.plan.md) is a draft
- [Decisions](wiki/decisions/) — thirty-eight accepted, covering concurrency, errors, atomicity,
  auth, outcome policy, corrupt records, entity keys, envelope extensibility, scope identity, the
  wire format, ordering and retention, cache identity, the drain loop, the opaque row store, and
  multi-service routing
- [Open decisions register](wiki/references/open-decisions.reference.md) — what is still unmade
- [Wiki index](wiki/index.md)

## Development Notes

`raw/` contains immutable copied source material. `wiki/` contains the maintained project knowledge
base. Git state is user-owned; agents should edit files only and avoid staging or committing unless
explicitly asked.
