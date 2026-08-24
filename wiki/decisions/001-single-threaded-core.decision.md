# Single-Threaded Core, No `Send` Bounds

Document Class: Decision
Status: Accepted
Date: 2026-08-25
Category: API Shape
Scope: Whether the extracted core requires `Send` futures, and whether backend traits are usable as trait objects.
Sources: `raw/initial/2026-08-25T083750Z/sources/persistence/mutations.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/mod.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/web.rs`, `raw/initial/2026-08-25T083750Z/sources/persistence/native.rs`
Related: `wiki/specs/source-frontend-cache-architecture.spec.md`, `wiki/decisions/002-error-model.decision.md`

## Decision

The core targets single-threaded Rust frontend runtimes first.

- No `Send` bounds in the v0 public API.
- Traits use `async fn` in trait (AFIT, stabilized in Rust 1.75).
- Backends are generic type parameters, not trait objects.

## Why

The evidence is cross-target, not native-only.

The native SQLite backend is deliberately cloneable and internally synchronized:
`Database` wraps `Arc<tokio::sync::Mutex<rusqlite::Connection>>`
(`persistence/native.rs:19-21`). That means the native backend alone is not evidence that the
whole source system is single-threaded.

The decisive constraints come from the shared frontend handle and the web backend:

- `MutationStore` wraps `Rc<MutationStoreInner>` (`persistence/mutations.rs:242`), so the store
  handle is `!Send`.
- On `wasm32`, `rexie`/IndexedDB futures are `!Send`; they cannot be made `Send` by wrapping.
- The module split selects IndexedDB on web and SQLite on native (`persistence/mod.rs:23-50`), so
  the public abstraction must serve the web target without conditional trait signatures.
- Dioxus web and desktop drive UI state from a frontend executor; a `Send` requirement would mostly
  help hypothetical non-frontend consumers.

Adding `Send` bounds would exclude the primary target. Making the bounds conditional through
`trait-variant` or a custom `MaybeSend` layer would double the API surface and verification matrix
before the first real consumer exists.

## Consequences

- **No `Box<dyn OutboxStore>` in v0.** `async fn` in traits is not dyn-compatible without an
  adapter. Backends are threaded through concrete generic parameters such as
  `SyncRunner<S: OutboxStore, T: SyncTransport>`.
- **Multi-threaded native consumers are not served by the initial API.** If a real user needs a
  `Send` variant, the project can add one later with `trait-variant` or a boxed-future adapter.
- **Errors do not require `Send + Sync` sources.** This is important for wasm error values. See
  `wiki/decisions/002-error-model.decision.md`.
- Interior mutability in core should use single-threaded primitives such as `Cell` and `RefCell`
  unless a backend itself needs synchronization internally.

## Enforcement

The implementation verification set must include:

- `cargo build --target wasm32-unknown-unknown`
- a native build
- tests for in-memory behavior that do not require a threaded runtime

The wasm build is the guardrail that catches accidental `Send`, threaded-runtime, or native-only
dependencies.

## Revisit If

A concrete consumer needs frontbox on a multi-threaded runtime, or monomorphization cost becomes
measurable in a real application. Until then, preserving wasm/frontend compatibility is the
dominant constraint.
