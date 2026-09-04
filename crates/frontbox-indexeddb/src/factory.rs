//! The conformance factory, so this backend runs the same cases the others do.

use frontbox::testing::{
    CorruptKind, FaultInjection, RowStoreFactory, StoreFactory, VersionStoreFactory,
};
use frontbox::{Error, ManualClock, ScopeKey};
use web_sys::IdbTransactionMode;

use crate::backend::{IdbBackend, IdbStore, OUTBOX};
use crate::convert::{to_js, OutboxRow};
use crate::request::{await_request, js_error};
use crate::IdbVersionStore;

/// Opens scopes on one shared IndexedDB database.
///
/// Every scope shares one database, which the suite's sharing contract requires: the isolation
/// cases have to prove scope enforcement holds when both scopes live in one physical store, not
/// merely that two databases cannot see each other.
pub struct IdbFactory {
    backend: IdbBackend,
    clock: ManualClock,
    name: String,
}

impl Drop for IdbFactory {
    /// Close and delete the database this factory opened.
    ///
    /// # Why a test factory has to clean up after itself
    ///
    /// The blocking backends get per-test isolation for free: `InMemoryFactory::new` and
    /// `SqliteFactory::new` each build a fresh store that evaporates when the test ends. The
    /// IndexedDB equivalent of "a fresh store" is a real database in the browser profile, and it
    /// **outlives the process**. A suite of fifty-odd cases therefore left fifty-odd databases
    /// behind on every run, and after enough runs the profile carried thousands — at which point
    /// the renderer stopped responding partway through a suite that had passed the week before.
    ///
    /// The delete is fired and not awaited, because `Drop` cannot await. That is acceptable here
    /// and only here: this is test scaffolding, the browser will process the request on its own,
    /// and a cleanup that occasionally loses a race costs a stale database rather than a wrong
    /// result. The `close` first is not optional — `deleteDatabase` blocks on open connections.
    fn drop(&mut self) {
        self.backend.close_now();
        let _ = IdbBackend::delete_soon(&self.name);
    }
}

impl IdbFactory {
    /// Open a factory over a named database.
    ///
    /// The caller supplies the name so each test file can have its own and they do not interfere —
    /// IndexedDB is origin-scoped and persists between runs, so a fixed name would carry state from
    /// one `cargo test` into the next.
    ///
    /// # Errors
    ///
    /// The database cannot be opened.
    pub async fn open(name: &str) -> Result<Self, Error> {
        let clock = ManualClock::new(1_700_000_000_000);
        // A fresh database per factory, rather than deleting and reopening one.
        //
        // **`deleteDatabase` blocks while any connection is open**, and it signals that by firing
        // `onblocked` — an event neither `onsuccess` nor `onerror`, so a future waiting on those
        // two waits forever. Earlier tests in a run keep their connections alive for the life of
        // the page, so a delete-first factory deadlocked on the second test. The suite hung rather
        // than failed, which is the worst way for this to present and took a browser run to find.
        //
        // A unique name sidesteps it entirely: nothing to delete, nothing to block on, and each
        // suite still starts from an empty store even though IndexedDB outlives the process.
        let unique = format!("{name}-{}-{}", session(), next_suffix());
        let backend = IdbBackend::open(&unique, clock.clone()).await?;
        Ok(Self {
            backend,
            clock,
            name: unique,
        })
    }
}

/// A per-page stamp, so one run never inherits another's databases.
///
/// IndexedDB is origin-scoped and outlives the process, and a counter that restarts at zero every
/// run means run N opens run N-1's databases — with its rows still in them. The counter alone was
/// not enough: it made factories unique *within* a run and left every run reusing the same handful
/// of names.
fn session() -> u64 {
    thread_local! {
        static STAMP: u64 = js_sys::Date::now() as u64;
    }
    STAMP.with(|stamp| *stamp)
}

/// A per-page counter, so two factories in one run never share a database.
fn next_suffix() -> u64 {
    use std::cell::Cell;
    thread_local! {
        static NEXT: Cell<u64> = const { Cell::new(0) };
    }
    NEXT.with(|next| {
        let value = next.get();
        next.set(value + 1);
        value
    })
}

impl StoreFactory for IdbFactory {
    type Store = IdbStore;

    fn clock(&self) -> ManualClock {
        self.clock.clone()
    }

    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error> {
        Ok(self.backend.open_scope(scope))
    }

    async fn insert_corrupt_row(&self, scope: &ScopeKey, kind: CorruptKind) -> Result<(), Error> {
        let (raw_id, body, created_at) = match kind {
            CorruptKind::UnparseableId => ("not-a-uuid".to_owned(), r#"{"ok":true}"#, 1),
            CorruptKind::InvalidBody { id } => (id.to_string(), "{ not json", 1),
            CorruptKind::UnrepresentableCreatedAt { id } => {
                (id.to_string(), r#"{"ok":true}"#, i64::MAX)
            }
        };
        // Written through the same object store the real path uses, which is only possible because
        // the identifier and body are stored as text. A schema that stored them as structured
        // values could not hold a row that will not decode, and decision 006's position would be
        // untestable here.
        let row = OutboxRow {
            seq: None,
            scope: scope.as_str().to_owned(),
            mutation_id: raw_id,
            method: "POST".to_owned(),
            path: "/api/v1/things".to_owned(),
            raw_body: body.to_owned(),
            created_at,
            op_name: None,
            op_version: None,
            traceparent: None,
            precondition: None,
            row_entity: None,
            row_id: None,
            attempts: 0,
            last_error: None,
        };
        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        await_request(store.add(&to_js(&row)?).map_err(js_error)?).await?;
        transaction.commit().await
    }
}

impl IdbFactory {
    /// Write an object into the outbox that is not an [`OutboxRow`] at all.
    ///
    /// # Why this is here and not a [`CorruptKind`]
    ///
    /// The three shared variants all produce a row that *parses* into the backend's row type and
    /// then fails a semantic check — an unparseable identifier, a body that is not JSON, a
    /// timestamp outside the representable range. This is the case only an object store can have:
    /// a stored value whose **shape** does not match the row type, which is what an older schema
    /// version or a foreign writer leaves behind.
    ///
    /// SQLite cannot reach the same state through the same door — its rows are read column by
    /// column into a raw struct, so the columns are there or the schema is wrong — which is why
    /// this is an adapter fixture rather than a fourth variant every backend would have to
    /// pantomime.
    ///
    /// The object carries `scope` and nothing else, so the scope index still finds it. That is the
    /// whole hazard: it is indexed, it is real, and until this was fixed nothing could see it.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn insert_unreadable_row(&self, scope: &ScopeKey) -> Result<(), Error> {
        let row = js_sys::Object::new();
        js_sys::Reflect::set(
            &row,
            &wasm_bindgen::JsValue::from_str("scope"),
            &wasm_bindgen::JsValue::from_str(scope.as_str()),
        )
        .map_err(js_error)?;
        js_sys::Reflect::set(
            &row,
            &wasm_bindgen::JsValue::from_str("written_by"),
            &wasm_bindgen::JsValue::from_str("a schema this build does not know"),
        )
        .map_err(js_error)?;

        let transaction = self
            .backend
            .transaction(&[OUTBOX], IdbTransactionMode::Readwrite)?;
        let store = transaction.inner().object_store(OUTBOX).map_err(js_error)?;
        await_request(store.add(&row).map_err(js_error)?).await?;
        transaction.commit().await
    }
}

impl RowStoreFactory for IdbFactory {
    // The same type as `Store`: `merge_rows` reads the queue to know which rows it is protecting,
    // and both live in one database behind one connection.
    type Rows = IdbStore;

    async fn open_rows(&self, scope: ScopeKey) -> Result<Self::Rows, Error> {
        Ok(self.backend.open_scope(scope))
    }
}

impl VersionStoreFactory for IdbFactory {
    type Versions = IdbVersionStore;

    async fn open_versions(&self, scope: ScopeKey) -> Result<Self::Versions, Error> {
        Ok(self.backend.open_versions(scope))
    }
}

impl FaultInjection for IdbFactory {
    async fn fail_next_apply_outcomes(&self) {
        self.backend.arm_apply_failure();
    }
}
