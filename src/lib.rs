//! # frontbox
//!
//! An offline-first cache and mutation outbox for Rust frontends: a durable local queue of writes
//! that survives restarts, replays when connectivity returns, and routes server refusals into dead
//! letters instead of losing them.
//!
//! This is D1, the framework-neutral core. It has no Dioxus dependency and no dependency on any
//! particular application's domain types. Cache versioning and invalidation (D2), the Dioxus
//! adapter (D3), and the durable SQLite and IndexedDB backends (D5) are separate deliverables.
//!
//! ## What is here
//!
//! - [`MutationIntent`] — what a caller enqueues, and the wire representation of it.
//! - [`OutboxStore`], [`DeadLetterStore`], [`QuarantineStore`] — the storage seam.
//! - [`SyncTransport`] — the network seam.
//! - [`SyncRunner`] — one sync pass: read a bounded batch, send it, apply the verdicts atomically.
//!   Each pass returns a [`SyncReport`], whose [`made_progress`](SyncReport::made_progress) and
//!   [`is_stalled`](SyncReport::is_stalled) separate a queue that drained nothing from an empty one.
//! - [`InMemoryBackend`] — a complete backend, with failure injection, for tests and examples.
//!
//! ## Shape
//!
//! The public API has no `Send` bounds and uses `async fn` in traits. Backends are generic
//! parameters, never trait objects. This targets single-threaded frontend runtimes, where
//! IndexedDB futures are `!Send` and cannot be made `Send` by wrapping; adding the bound would
//! exclude the primary target.
//!
//! Every store is opened under a required [`ScopeKey`], and every read is filtered by it.
//!
//! Records are `#[non_exhaustive]` and constructor-built, so fields can be added without a breaking
//! change.
//!
//! ## Features
//!
//! - `v4` *(default)* — [`MutationId::new`] for callers with no identifier of their own. It pulls
//!   in a randomness backend, which core itself never needs: the caller supplies the id, and that
//!   is what lets an application try a direct write and enqueue under the same id if it fails.
//!   Build `--no-default-features` for a core with no randomness dependency at all.
//! - `testing` — the backend-agnostic conformance suite in [`testing`], plus [`InMemoryFactory`].
//!   Off by default so the scaffolding is not compiled into applications.
//!
//! ## Example
//!
//! Enqueue a write, then sync it. The transport is where an application puts its own HTTP client
//! and its own credentials — neither is core's business.
//!
//! ```
//! use frontbox::{
//!     Clock, InMemoryBackend, ManualClock, MutationBatchRequest, MutationBatchResponse,
//!     MutationId, MutationIntent, MutationResult, MutationStatus, OperationMeta, OutboxStore,
//!     ScopeKey, SyncRunner, SyncTransport,
//! };
//!
//! struct FakeServer;
//!
//! impl SyncTransport for FakeServer {
//!     async fn send_batch(
//!         &self,
//!         request: MutationBatchRequest,
//!     ) -> Result<MutationBatchResponse, frontbox::Error> {
//!         // A real transport resolves credentials here, per send, and posts the batch.
//!         Ok(MutationBatchResponse::new(
//!             request
//!                 .mutations
//!                 .iter()
//!                 .map(|m| MutationResult::new(m.mutation_id, MutationStatus::Applied))
//!                 .collect(),
//!         ))
//!     }
//! }
//!
//! # fn main() -> Result<(), frontbox::Error> {
//! # pollster::block_on(async {
//! let clock = ManualClock::new(1_700_000_000_000);
//! let backend = InMemoryBackend::new(clock.clone());
//! let store = backend.open(ScopeKey::new("user:alice@tenant:acme")?);
//!
//! store
//!     .enqueue(
//!         MutationIntent::new(
//!             MutationId::from_uuid(uuid::Uuid::from_u128(1)),
//!             "POST",
//!             "/api/v1/sessions",
//!             serde_json::json!({ "name": "Morning" }),
//!             clock.now_ms(),
//!         )
//!         .with_op(OperationMeta::new("start_session").with_version("1.2.0")),
//!     )
//!     .await?;
//!
//! assert_eq!(store.pending_count().await?, 1);
//!
//! // A different scope cannot see it, and does not delete it either.
//! let other = backend.open(ScopeKey::new("user:bob@tenant:acme")?);
//! assert_eq!(other.pending_count().await?, 0);
//!
//! // Sync drains it.
//! let runner = SyncRunner::new(backend.open(store.scope().clone()), FakeServer);
//! let report = runner.sync_once().await?;
//!
//! assert_eq!(report.counts.applied, 1);
//!
//! // `made_progress` is false for an idle pass as well as a stalled one. Use `is_stalled` to
//! // tell a queue that sent work and drained none of it from a queue with nothing to send.
//! assert!(report.made_progress());
//! assert!(!report.is_stalled());
//! assert_eq!(store.pending_count().await?, 0);
//! # Ok::<_, frontbox::Error>(())
//! # })
//! # }
//! ```
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod cache;
pub mod clock;
pub mod entity;
pub mod error;
pub mod id;
pub mod memory;
pub mod protocol;
pub mod record;
/// The wire format's timestamp rendering. Private: `client_datetime` is a serialization detail of
/// [`record`], not a public time API (decision 011).
mod rfc3339;
pub mod runner;
pub mod scope;
pub mod store;
pub mod transport;

#[cfg(feature = "testing")]
pub mod testing;

pub use cache::{
    CacheVersionStore, EntityState, InvalidationEvent, InvalidationReport, InvalidationRunner,
    PendingConflict, StaleEntity, VersionUpdate,
};
pub use clock::Clock;
#[cfg(not(target_arch = "wasm32"))]
pub use clock::SystemClock;
pub use entity::{EntityKey, EntityRegistry, SliceRegistry};
pub use error::Error;
pub use id::MutationId;
#[cfg(feature = "testing")]
pub use memory::InMemoryFactory;
pub use memory::{InMemoryBackend, InMemoryStore, InMemoryVersionStore, ManualClock};
pub use protocol::{
    MutationBatchRequest, MutationBatchResponse, MutationResult, MutationStatus, RemoteRejection,
};
pub use record::{
    DeadLetterRecord, MutationIntent, OperationMeta, OutboxRecord, QuarantinedRecord,
};
pub use runner::{Anomaly, AnomalyKind, SyncOutcomeCounts, SyncPass, SyncReport, SyncRunner};
pub use scope::ScopeKey;
pub use store::{DeadLetterStore, Disposition, OutboxStore, Outcome, QuarantineStore};
pub use transport::SyncTransport;
