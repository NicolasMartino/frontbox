//! Which durable backend this target uses, and nothing else.
//!
//! # D4b's whole storage seam
//!
//! The promise D4b was written to test is that swapping the backend touches only the place the
//! backend is named. This module *is* that place: SQLite where there is a filesystem, IndexedDB
//! where there is a browser, and every other file in this crate is identical on both.
//!
//! # What the swap did force, and what it did not
//!
//! It did not force a change to any read, any write, or any of the ten observations — those are
//! written against core's traits and the traits did not move.
//!
//! It did force **construction to become `async`**. Opening a browser database is asynchronous and
//! cannot be made otherwise, so `TodoApp::new` is now an `async fn` on both targets. That is a
//! real finding rather than an inconvenience: the seam held for everything a running application
//! does and gave way at the one moment an application starts. Keeping the signature uniform across
//! targets is deliberate — a `new` that is sync natively and async on web is two APIs wearing one
//! name, and every caller would have to know which it had.

use frontbox::{Clock, Error, ScopeKey};

#[cfg(not(target_arch = "wasm32"))]
pub use native::{open_backend, Backend, Store, VersionStore};

#[cfg(target_arch = "wasm32")]
pub use web::{open_backend, Backend, Store, VersionStore};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Clock, Error, ScopeKey};

    /// The durable backend on native targets.
    pub type Backend = frontbox_sqlite::SqliteBackend;
    /// One scope's handle on it.
    pub type Store = frontbox_sqlite::SqliteStore;
    /// One scope's handle on the durable cache versions.
    ///
    /// A second handle rather than a second trait on `Store`, because `CacheVersionStore` and
    /// `OutboxStore` both declare `scope()` and one type implementing both would make every
    /// existing `store.scope()` call ambiguous. Same database underneath.
    pub type VersionStore = frontbox_sqlite::SqliteVersionStore;

    /// Open the durable store this target uses.
    ///
    /// `scope` is unused here and used on web, which is the asymmetry a `ScopeKey`-per-database
    /// backend would care about. Decision 024 leaves that choice to the backend and both of these
    /// put the scope in a column or an index, so neither needs it — the parameter stays so the two
    /// signatures match.
    pub async fn open_backend(
        path: &str,
        _scope: &ScopeKey,
        clock: impl Clock + 'static,
    ) -> Result<Backend, Error> {
        frontbox_sqlite::SqliteBackend::open(path, clock)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Clock, Error, ScopeKey};

    /// The durable backend on web targets.
    pub type Backend = frontbox_indexeddb::IdbBackend;
    /// One scope's handle on it.
    pub type Store = frontbox_indexeddb::IdbStore;
    /// One scope's handle on the durable cache versions. See the native module's note.
    pub type VersionStore = frontbox_indexeddb::IdbVersionStore;

    /// Open the durable store this target uses.
    ///
    /// The `path` is a database name here rather than a file path. Same argument, different
    /// meaning, and that is as far as the difference goes.
    pub async fn open_backend(
        path: &str,
        _scope: &ScopeKey,
        clock: impl Clock + 'static,
    ) -> Result<Backend, Error> {
        frontbox_indexeddb::IdbBackend::open(path, clock).await
    }
}
