//! A backend-agnostic conformance suite.
//!
//! Every storage backend must behave identically, so the behavioural tests live here rather than
//! next to any one backend. The in-memory backend runs them today; the durable SQLite and IndexedDB
//! backends will run the same functions, which is what makes "shared conformance tests pass on all
//! backends" a checkable claim rather than an aspiration.
//!
//! Enable the `testing` feature to use this module.
//!
//! ```ignore
//! frontbox::frontbox_conformance_tests! {
//!     #[test]
//!     factory: MyFactory::new(),
//!     block_on: pollster::block_on,
//! }
//! ```
//!
//! Multiple attributes work too — `#[test] #[ignore = "needs a server"]` — because the macros take
//! `$(#[$attr:meta])*`. One test function is emitted per case rather than one for the whole suite,
//! which is what `wasm_bindgen_test` needs and what makes a single failure name itself.
//!
//! # Two emission shapes
//!
//! [`frontbox_conformance_tests`](crate::frontbox_conformance_tests) emits synchronous `fn`s and
//! drives each case with the `block_on` you supply. That needs an executor which can run a future
//! to completion without returning to a host event loop — `pollster`, `futures::executor`, a Tokio
//! runtime handle.
//!
//! A browser has no such executor. An IndexedDB future suspends on a JavaScript callback, and the
//! callback cannot fire until the stack unwinds and the event loop runs, so any `block_on` either
//! deadlocks or panics. Blocking the main thread is not an implementation gap in
//! `wasm-bindgen-futures`; it is unavailable by construction.
//!
//! So a wasm backend uses
//! [`frontbox_conformance_tests_async`](crate::frontbox_conformance_tests_async), which emits
//! `async fn`s and awaits each case directly. There is no `block_on` argument, because the test
//! harness is what drives the future:
//!
//! ```ignore
//! frontbox::frontbox_conformance_tests_async! {
//!     #[wasm_bindgen_test::wasm_bindgen_test]
//!     factory: IndexedDbFactory::new(),
//! }
//! ```
//!
//! The cases themselves are identical either way — nothing in them is native-specific. Only the
//! wrapper differs. The async form needs an attribute that accepts an `async fn`, which
//! `#[wasm_bindgen_test]`, `#[tokio::test]`, and `#[async_std::test]` all do and plain `#[test]`
//! does not.

use crate::cache::CacheVersionStore;
use crate::clock::ManualClock;
use crate::error::Error;
use crate::id::MutationId;
use crate::protocol::RemoteRejection;
use crate::record::MutationIntent;
use crate::scope::ScopeKey;
use crate::store::{DeadLetterStore, OutboxStore, QuarantineStore, RowStore};

pub mod cases;
mod macros;
mod scripted;

pub use scripted::{Reply, ScriptedTransport};

/// A kind of durable corruption a backend must be able to reproduce.
///
/// Corruption cannot be created through [`OutboxStore::enqueue`] — a [`MutationIntent`] holds a
/// parsed body and a parsed identifier, so a malformed one is not representable. Corrupt rows come
/// from storage itself: a partial write, a schema change, an older version of the library. A
/// backend has to be able to manufacture one for the corrupt-record path to be testable at all.
#[derive(Debug, Clone, PartialEq)]
pub enum CorruptKind {
    /// A row whose identifier does not parse.
    ///
    /// This is the id-less path: nothing outside the backend can name this row, so only
    /// [`OutboxStore::sweep_corrupt`] can find it.
    UnparseableId,
    /// A row whose stored body is not valid JSON.
    InvalidBody {
        /// The row's identifier, which does parse.
        id: MutationId,
    },
    /// A row whose stored timestamp is outside the range the wire format can express.
    UnrepresentableCreatedAt {
        /// The row's identifier, which does parse.
        id: MutationId,
    },
}

/// Opens stores for the conformance suite.
///
/// # The sharing contract
///
/// Two stores opened from one factory under *different* scopes must share durable state. That is
/// what makes the cross-scope cases meaningful: they have to prove that scope enforcement holds
/// when both scopes live in the same physical store, not merely that separate files cannot see each
/// other. A durable factory honours this by pointing both stores at the same directory or origin.
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait StoreFactory {
    /// The backend under test.
    type Store: OutboxStore + DeadLetterStore + QuarantineStore;

    /// The clock the backend stamps `rejected_at` and `quarantined_at` from.
    ///
    /// Returned so a case can move time and assert the result, instead of asserting against
    /// whenever the test happened to run.
    fn clock(&self) -> ManualClock;

    /// Open a store on `scope`.
    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error>;

    /// Write a corrupt row directly into `scope`'s storage.
    async fn insert_corrupt_row(&self, scope: &ScopeKey, kind: CorruptKind) -> Result<(), Error>;
}

/// A factory that can also open cache version stores.
///
/// Split from [`StoreFactory`] for the same reason [`FaultInjection`] is: a backend that has no
/// version store yet does not implement this and does not invoke [`frontbox_cache_tests`], which
/// leaves the gap visible in its test file rather than hidden behind a runtime skip.
///
/// # The sharing contract, again
///
/// As with [`StoreFactory::open`], two version stores opened under *different* scopes must share
/// durable state, and a version store opened twice under the *same* scope must see what the first
/// wrote. The second half is what makes the restart case meaningful: reopening is how a test says
/// "the process died and came back" to a backend that has no process to kill.
///
/// [`frontbox_cache_tests`]: crate::frontbox_cache_tests
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait VersionStoreFactory: StoreFactory {
    /// The cache version store under test.
    type Versions: CacheVersionStore;

    /// Open a version store on `scope`.
    async fn open_versions(&self, scope: ScopeKey) -> Result<Self::Versions, Error>;
}

/// A factory whose stores also hold read-model rows.
///
/// Split from [`StoreFactory`] on the same principle as [`VersionStoreFactory`]: a backend that has
/// not built decision 032's row store yet does not implement this and does not invoke
/// [`frontbox_row_tests`], which leaves the gap visible in its test file rather than hidden.
///
/// # The rows and the queue must be the same store
///
/// [`RowStore::merge_rows`](crate::store::RowStore::merge_rows()) skips rows a *pending mutation* is
/// bound to, so the row half and the outbox half have to see one another. `Rows` is therefore
/// normally the same type as [`Store`](StoreFactory::Store) — the associated type exists so a
/// backend that separates them can, not to suggest it should.
///
/// [`frontbox_row_tests`]: crate::frontbox_row_tests
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait RowStoreFactory: StoreFactory {
    /// The row store under test.
    type Rows: RowStore;

    /// Open a row store on `scope`.
    async fn open_rows(&self, scope: ScopeKey) -> Result<Self::Rows, Error>;
}

/// A factory that can also make a specific store operation fail.
///
/// Split from [`StoreFactory`] on purpose. A backend that cannot inject faults simply does not
/// implement this and does not invoke [`frontbox_fault_injection_tests`], which leaves the gap
/// visible in its test file. Folding these cases into the main suite with a runtime "unsupported,
/// skipping" branch would let a backend report a full pass while never testing the atomicity
/// contract.
///
/// [`frontbox_fault_injection_tests`]: crate::frontbox_fault_injection_tests
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait FaultInjection: StoreFactory {
    /// Make the next `apply_outcomes` call fail.
    async fn fail_next_apply_outcomes(&self);
}

/// Build a scope key, panicking on an invalid one.
///
/// # Panics
///
/// If `key` is empty.
pub fn scope(key: &str) -> ScopeKey {
    ScopeKey::new(key).expect("conformance scope key must be valid")
}

/// A deterministic mutation id.
///
/// The suite never uses random identifiers: the ordering cases assert a tie-break *by*
/// `mutation_id`, so the identifiers have to be known and reproducible for the assertion to mean
/// anything.
pub fn id(n: u128) -> MutationId {
    MutationId::from_uuid(uuid::Uuid::from_u128(n))
}

/// The registry the cache cases run against.
///
/// Three entities, so that a case can invalidate one and assert the others were left alone, and
/// `"templates"` is deliberately never invalidated by most cases so "unchanged" has a witness.
pub fn registry() -> crate::entity::SliceRegistry<&'static str> {
    crate::entity::SliceRegistry::new(["exercises", "sessions", "templates"])
}

/// A mutation intent with a deterministic id and a trivial body.
pub fn intent(n: u128, created_at: i64) -> MutationIntent {
    MutationIntent::new(
        id(n),
        "POST",
        format!("/api/v1/things/{n}"),
        serde_json::json!({ "seq": n }),
        created_at,
    )
}

/// A rejection payload with every field populated, for round-trip assertions.
pub fn full_rejection() -> RemoteRejection {
    RemoteRejection::new("title must not be empty")
        .with_code("invalid_input")
        .with_details(serde_json::json!({ "field": "title", "limits": { "min": 1 } }))
}
