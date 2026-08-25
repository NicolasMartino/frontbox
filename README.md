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
./scripts/verify.sh     # every gate: fmt, clippy on native and wasm, tests, both wasm builds
cargo test --all-features
```

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
