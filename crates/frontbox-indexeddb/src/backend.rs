//! Opening the database, and the handle a scope is reached through.

use std::rc::Rc;

use frontbox::{Clock, Error, ScopeKey};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{IdbDatabase, IdbObjectStoreParameters, IdbTransactionMode};

use crate::request::{await_request, js_error, Txn};

/// Object store names, in one place so a typo cannot make a read and a write disagree.
pub(crate) const OUTBOX: &str = "outbox";
pub(crate) const DEAD_LETTERS: &str = "dead_letters";
pub(crate) const QUARANTINE: &str = "quarantine";
pub(crate) const ROWS: &str = "rows";
pub(crate) const VERSIONS: &str = "versions";

/// The schema version. Bumping it runs `onupgradeneeded` against an existing database.
///
/// # Why this is 2, and what that means for a database already on disk
///
/// Version 1 had four stores; `versions` is the fifth, added when D5's cache half landed. A new
/// object store can only be created inside a version-change transaction, so adding one *requires*
/// this number to move — a build that added the store and left the version alone would open the
/// old database, find no `versions` store, and throw `NotFoundError` on the first cache read.
///
/// **This is the first real migration this crate has run**, and it is the cheap kind: nothing is
/// rewritten, no existing store is touched, and a browser holding a version-1 database gets the
/// new store added on next open with every queued mutation and stored row intact. The expensive
/// kind — reshaping data already written to user devices — is what
/// `wiki/decisions/024-scope-storage-encoding.decision.md` and the D5 plan keep warning about, and
/// it is why the schema is worth getting right before a consumer ships rather than after.
const VERSION: u32 = 2;

/// An open IndexedDB database holding every scope's storage.
///
/// Cloning shares the connection, so two clones are two handles on the same data — the contract
/// every backend owes, and what the conformance suite's sharing rule needs.
#[derive(Clone)]
pub struct IdbBackend {
    database: Rc<Handle>,
    clock: Rc<dyn Clock>,
    /// Shared with every clone, so a factory can arm it and a store opened later still sees it.
    fail_next_apply: Rc<std::cell::Cell<bool>>,
    #[cfg(feature = "testing")]
    fail_next_migration: Rc<std::cell::Cell<bool>>,
}

impl std::fmt::Debug for IdbBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IdbBackend(..)")
    }
}

impl IdbBackend {
    /// Open a database by name, creating the object stores if they are not there.
    ///
    /// # The name is the caller's, and injectivity is theirs to keep
    ///
    /// Decision 024 requires that two distinct `ScopeKey`s never reach one physical store, and
    /// leaves the mechanism open. This backend puts every scope in **one** database with the scope
    /// as an indexed field, which satisfies injectivity trivially — two distinct keys are two
    /// distinct strings, and no sanitiser is in a position to collapse them. A caller wanting
    /// physical separation per user passes a different `name`, which is a different question.
    ///
    /// # Errors
    ///
    /// No global with an `indexedDB` (neither a window nor a worker), or the open is refused —
    /// which in a browser most often means private browsing, and says so.
    pub async fn open(name: &str, clock: impl Clock + 'static) -> Result<Self, Error> {
        let factory = factory()?;
        let request = factory.open_with_u32(name, VERSION).map_err(js_error)?;

        // Creating the stores has to happen inside `onupgradeneeded`, synchronously, before the
        // open request completes. A closure rather than an awaited step for exactly that reason:
        // there is no point at which this could be awaited without the version-change transaction
        // having already ended.
        let upgrade = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let Some(target) = event.target() else { return };
            let Ok(request) = target.dyn_into::<web_sys::IdbOpenDbRequest>() else {
                return;
            };
            let Ok(result) = request.result() else { return };
            let Ok(database) = result.dyn_into::<IdbDatabase>() else {
                return;
            };
            create_stores(&database);
        });
        request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));

        let value = await_request(request.unchecked_into()).await?;
        drop(upgrade);
        let database: IdbDatabase = value.dyn_into().map_err(js_error)?;

        Ok(Self {
            database: Rc::new(Handle(database)),
            clock: Rc::new(clock),
            fail_next_apply: Rc::new(std::cell::Cell::new(false)),
            #[cfg(feature = "testing")]
            fail_next_migration: Rc::new(std::cell::Cell::new(false)),
        })
    }

    /// Delete a database entirely.
    ///
    /// # It blocks on open connections, and this future does not handle that
    ///
    /// `deleteDatabase` fires `onblocked` — neither `onsuccess` nor `onerror` — when any other
    /// connection to the database is still open, and waits for them to close. A future awaiting
    /// only the other two events therefore waits forever. **Call this only when nothing else holds
    /// the database open**, which in practice means at startup, before anything has opened it.
    ///
    /// The conformance factory deliberately does *not* use it, for exactly this reason: it gives
    /// each factory its own database name instead.
    ///
    /// # Errors
    ///
    /// No global with an `indexedDB`, or the delete is refused.
    pub async fn delete(name: &str) -> Result<(), Error> {
        let request = factory()?.delete_database(name).map_err(js_error)?;
        await_request(request.unchecked_into()).await?;
        Ok(())
    }

    /// Ask the browser to delete a database, without waiting for it.
    ///
    /// The awaited [`delete`](IdbBackend::delete) is the one to use when the outcome matters. This
    /// exists for `Drop`, which cannot await — see [`IdbFactory`](crate::IdbFactory)'s.
    ///
    /// # Errors
    ///
    /// No global with an `indexedDB`, or the request could not be issued.
    /// Only the conformance factory's `Drop` calls this. Gated so a consumer without `testing` carries no test scaffolding.
    #[cfg(feature = "testing")]
    pub(crate) fn delete_soon(name: &str) -> Result<(), Error> {
        factory()?.delete_database(name).map_err(js_error)?;
        Ok(())
    }

    /// Open a handle scoped to `scope`.
    pub fn open_scope(&self, scope: ScopeKey) -> IdbStore {
        IdbStore {
            backend: self.clone(),
            scope,
        }
    }

    /// Open a cache version handle scoped to `scope`.
    ///
    /// Separate from [`open_scope`](Self::open_scope) because `CacheVersionStore` and `OutboxStore`
    /// both declare `scope()`; one type implementing both would make every existing `store.scope()`
    /// call ambiguous. Same database underneath, so the two handles are two views of one store.
    pub fn open_versions(&self, scope: ScopeKey) -> crate::IdbVersionStore {
        crate::IdbVersionStore::new(self.clone(), scope)
    }

    /// Start a transaction over one or more stores.
    ///
    /// Every method that must be atomic opens exactly one of these and issues every request
    /// against it. A method that opened two would have no atomicity at all, which is the failure
    /// mode this helper exists to make hard to write by accident.
    pub(crate) fn transaction(
        &self,
        stores: &[&str],
        mode: IdbTransactionMode,
    ) -> Result<Txn, Error> {
        let names = js_sys::Array::new();
        for store in stores {
            names.push(&JsValue::from_str(store));
        }
        // Wrapped immediately, so the completion handlers are attached before any request is
        // issued. See `Txn` for why attaching them later is a hang rather than an error.
        self.database
            .0
            .transaction_with_str_sequence_and_mode(&names, mode)
            .map(|transaction| Txn::new(transaction, mode))
            .map_err(js_error)
    }

    /// Close the connection now, without waiting for the last handle to drop.
    ///
    /// Needed because a `Drop` implementation cannot await, and deleting a database requires every
    /// connection to it to be closed first.
    /// Only the conformance factory's `Drop` calls this. Gated so a consumer without `testing` carries no test scaffolding.
    #[cfg(feature = "testing")]
    pub(crate) fn close_now(&self) {
        self.database.0.close();
    }

    pub(crate) fn now(&self) -> i64 {
        self.clock.now_ms()
    }

    /// Arm a single `apply_outcomes` failure.
    ///
    /// Only the conformance factory calls this, so it is gated: a consumer that depends on this
    /// crate without `testing` should not carry fault-injection scaffolding, and without the gate
    /// it is dead code in every real build.
    #[cfg(feature = "testing")]
    pub(crate) fn arm_apply_failure(&self) {
        self.fail_next_apply.set(true);
    }

    #[cfg(feature = "testing")]
    pub(crate) fn arm_migration_failure(&self) {
        self.fail_next_migration.set(true);
    }

    #[cfg(feature = "testing")]
    pub(crate) fn take_migration_failure(&self) -> bool {
        self.fail_next_migration.replace(false)
    }

    /// Consume the armed failure, if there is one.
    pub(crate) fn take_apply_failure(&self) -> bool {
        self.fail_next_apply.replace(false)
    }
}

/// An open database that closes itself when the last handle goes.
///
/// **Not hygiene for its own sake.** A connection left open holds the database against
/// `deleteDatabase` and against any later `onupgradeneeded`, and a page that accumulates them —
/// a conformance run opening one per case, say — stalls rather than fails. `close()` is
/// asynchronous in the sense that it takes effect once outstanding transactions finish, which is
/// exactly the semantics wanted here: drop the handle, let the work already in flight land.
struct Handle(IdbDatabase);

impl Drop for Handle {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// A store scoped to one [`ScopeKey`].
#[derive(Clone)]
pub struct IdbStore {
    pub(crate) backend: IdbBackend,
    pub(crate) scope: ScopeKey,
}

impl std::fmt::Debug for IdbStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdbStore")
            .field("scope", &self.scope.as_str())
            .finish()
    }
}

/// The `indexedDB` factory, from a window or a worker.
///
/// Both are checked because a service worker has no `window`, and the whole reason
/// `wiki/proposals/browser-background-services.proposal.md` treats Background Sync as reachable
/// later is that this backend can be opened from a worker realm.
fn factory() -> Result<web_sys::IdbFactory, Error> {
    let global = js_sys::global();
    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        return window
            .indexed_db()
            .map_err(js_error)?
            .ok_or_else(|| Error::storage_message("indexedDB is unavailable in this window"));
    }
    if let Some(worker) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        return worker
            .indexed_db()
            .map_err(js_error)?
            .ok_or_else(|| Error::storage_message("indexedDB is unavailable in this worker"));
    }
    Err(Error::storage_message(
        "no global with an indexedDB: not a window and not a worker",
    ))
}

/// Create every object store and index. Runs inside `onupgradeneeded`.
///
/// # Not "runs once"
///
/// It runs on every version bump, including against a database that already has some of these
/// stores. `createObjectStore` throws `ConstraintError` for a name that already exists, so each
/// creation is guarded by [`has`] rather than by ignoring the error — an ignored throw is
/// indistinguishable from a store that failed to be created for a reason that mattered, and this
/// function has no way to report either.
fn create_stores(database: &IdbDatabase) {
    // `autoIncrement` is what supplies decision 016's monotonic `seq`: IndexedDB guarantees the
    // generator never reuses a key, so a queue that drains to empty and is written to again cannot
    // issue a number that would sort a later write ahead of an earlier one.
    let outbox_params = IdbObjectStoreParameters::new();
    outbox_params.set_key_path(&JsValue::from_str("seq"));
    outbox_params.set_auto_increment(true);
    if let Ok(store) = create(database, OUTBOX, &outbox_params) {
        // Reads are scope-filtered, so the scope index is what makes them cheap rather than a full
        // scan of every user's queue.
        index(&store, "scope");
        index(&store, "mutation_id");
    }

    let auto = IdbObjectStoreParameters::new();
    auto.set_auto_increment(true);
    for name in [DEAD_LETTERS, QUARANTINE] {
        if let Ok(store) = create(database, name, &auto) {
            index(&store, "scope");
        }
    }

    // Keyed by the triple rather than auto-incremented: a row has an identity of its own, and a
    // `put` under the same key must replace rather than accumulate.
    let rows_params = IdbObjectStoreParameters::new();
    let key_path = js_sys::Array::new();
    key_path.push(&JsValue::from_str("scope"));
    key_path.push(&JsValue::from_str("entity"));
    key_path.push(&JsValue::from_str("row_id"));
    rows_params.set_key_path(&key_path);
    if let Ok(store) = create(database, ROWS, &rows_params) {
        // Named `scope` like every other store's, because `all_in_scope` looks it up by that
        // name. A store whose index was called something else returned an error from every read —
        // and the failing read left its transaction unresolved, so the suite hung instead of
        // failing. Found in the browser; no compile gate could see it.
        //
        // Worth being precise about, because it says what a check here would and would not buy:
        // **that creation succeeded**. An index called `scopes` is created perfectly happily; it is
        // the later `store.index("scope")` that fails. So surfacing errors from `index` below would
        // not have caught it, and the name being written twice in this file — once here, once in
        // `all_in_scope` — is the actual exposure.
        index(&store, "scope");
    }

    // Keyed by `[scope, entity]` for the same reason the rows are keyed by their triple: an entity
    // has an identity, and a `put` under the same key has to replace rather than accumulate.
    let version_params = IdbObjectStoreParameters::new();
    let version_key = js_sys::Array::new();
    version_key.push(&JsValue::from_str("scope"));
    version_key.push(&JsValue::from_str("entity"));
    version_params.set_key_path(&version_key);
    if let Ok(store) = create(database, VERSIONS, &version_params) {
        index(&store, "scope");
    }
}

/// Add one index to a store this function has just created.
///
/// # Why the result is dropped
///
/// It is dropped deliberately, and the alternatives are worse rather than merely more work. This
/// runs inside `onupgradeneeded`, which is a callback with nowhere to return an error to: the
/// `open` request has not resolved yet, and panicking across the JS boundary aborts the module
/// rather than failing the open.
///
/// What can actually fail here is narrow. The store was created two lines up, so it exists and is
/// empty; `createIndex` throws only for a name already present on it — which would mean this
/// function's own literal list repeats a name, a mistake visible by reading it. It is not the
/// failure mode this crate has actually met; see the note on the `ROWS` index above.
fn index(store: &web_sys::IdbObjectStore, name: &str) {
    let _ = store.create_index_with_str(name, name);
}

/// Create one object store, or report that it is already there.
fn create(
    database: &IdbDatabase,
    name: &str,
    params: &IdbObjectStoreParameters,
) -> Result<web_sys::IdbObjectStore, ()> {
    if has(database, name) {
        return Err(());
    }
    database
        .create_object_store_with_optional_parameters(name, params)
        .map_err(|_| ())
}

/// Whether a database already holds an object store by this name.
fn has(database: &IdbDatabase, name: &str) -> bool {
    database.object_store_names().contains(name)
}
