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
//! ## Example
//!
//! ```
//! use frontbox::{
//!     Clock, InMemoryBackend, ManualClock, MutationId, MutationIntent, OperationMeta,
//!     OutboxStore, ScopeKey,
//! };
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
//! assert_eq!(store.pending_count().await?, 1);
//! # Ok::<_, frontbox::Error>(())
//! # })
//! # }
//! ```
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod clock;
pub mod error;
pub mod id;
pub mod memory;
pub mod protocol;
pub mod record;
pub mod runner;
pub mod scope;
pub mod store;
pub mod transport;

#[cfg(feature = "testing")]
pub mod testing;

pub use clock::Clock;
#[cfg(not(target_arch = "wasm32"))]
pub use clock::SystemClock;
pub use error::Error;
pub use id::MutationId;
pub use memory::{InMemoryBackend, InMemoryStore, ManualClock};
pub use protocol::{
    MutationBatchRequest, MutationBatchResponse, MutationResult, MutationStatus, RemoteRejection,
};
pub use record::{
    DeadLetterRecord, MutationIntent, OperationMeta, OutboxRecord, QuarantinedRecord,
};
pub use runner::{SyncOutcomeCounts, SyncPass, SyncReport, SyncRunner};
pub use scope::ScopeKey;
pub use store::{DeadLetterStore, Disposition, OutboxStore, Outcome, QuarantineStore};
pub use transport::SyncTransport;
