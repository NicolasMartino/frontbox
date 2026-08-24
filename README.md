# frontbox

Offline-first cache and mutation outbox for Rust frontends.

`frontbox` is the backend outbox pattern moved to the client: a durable local queue of writes that
survives restarts, replays when connectivity returns, and routes server refusals into dead letters
instead of losing them. The library is being extracted from RepForge's Dioxus application into a
framework-neutral core with storage and framework adapters.

## Current Status

This repository is in research and planning. There is no crate yet: no `Cargo.toml`, no `src/`, and
no installable package.

Implementation should start only after the planning documents under `wiki/` are accepted.

## Planned Shape

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
- [Wiki index](wiki/index.md)

## Development Notes

`raw/` contains immutable copied source material. `wiki/` contains the maintained project knowledge
base. Git state is user-owned; agents should edit files only and avoid staging or committing unless
explicitly asked.
