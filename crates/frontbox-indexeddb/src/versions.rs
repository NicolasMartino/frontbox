//! Decision 015's cache version state over IndexedDB.
//!
//! # A separate handle rather than another `impl` on `IdbStore`
//!
//! [`CacheVersionStore`] and [`OutboxStore`](frontbox::OutboxStore) both declare `scope()`, so one
//! type implementing both would make every existing `store.scope()` call ambiguous. The SQLite and
//! in-memory backends split the two the same way, and the split states something true besides: the
//! outbox holds raw HTTP envelopes and knows nothing about entities, while this store holds
//! entities and knows nothing about HTTP.

use frontbox::{CacheVersion, CacheVersionStore, EntityState, Error, ScopeKey};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

use crate::backend::{IdbBackend, VERSIONS};
use crate::convert::{from_js, to_js};
use crate::request::{await_request, js_error};
use crate::scan::all_in_scope;

/// One entity's cache state as it sits in the object store.
///
/// # `version` is `Option<String>`, and the `null` matters
///
/// Serialized through `JSON.parse`, [`None`] becomes JavaScript `null` and `Some("")` becomes the
/// empty string. They round-trip as different values and compare unequal, which is the distinction
/// decision 021 turns on: under XOR set hashing an emptied collection hashes to zero, so "never
/// heard a version" and "the server said this" must not share a representation. A field that
/// defaulted a missing value to `""` would collapse them silently.
#[derive(Serialize, Deserialize)]
pub(crate) struct VersionRecord {
    scope: String,
    entity: String,
    version: Option<String>,
    stale: bool,
}

/// A handle on one scope's cache version state.
#[derive(Clone)]
pub struct IdbVersionStore {
    backend: IdbBackend,
    scope: ScopeKey,
}

impl std::fmt::Debug for IdbVersionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdbVersionStore")
            .field("scope", &self.scope.as_str())
            .finish()
    }
}

impl IdbVersionStore {
    pub(crate) fn new(backend: IdbBackend, scope: ScopeKey) -> Self {
        Self { backend, scope }
    }

    /// The composite key this store is keyed on.
    fn key(&self, entity: &str) -> js_sys::Array {
        let key = js_sys::Array::new();
        key.push(&JsValue::from_str(self.scope.as_str()));
        key.push(&JsValue::from_str(entity));
        key
    }
}

impl CacheVersionStore for IdbVersionStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn state(&self, entity: &str) -> Result<EntityState, Error> {
        let transaction = self
            .backend
            .transaction(&[VERSIONS], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(VERSIONS)
            .map_err(js_error)?;
        let value = await_request(store.get(&self.key(entity)).map_err(js_error)?).await?;
        // Absent is not an error. Never having heard a version is a normal starting condition, and
        // the trait says so explicitly.
        Ok(from_js::<VersionRecord>(&value)
            .map(|record| record.decode())
            .unwrap_or_else(EntityState::unknown))
    }

    async fn all_states(&self) -> Result<Vec<(String, EntityState)>, Error> {
        let transaction = self
            .backend
            .transaction(&[VERSIONS], IdbTransactionMode::Readonly)?;
        let store = transaction
            .inner()
            .object_store(VERSIONS)
            .map_err(js_error)?;
        // An unreadable version record reads as "no version known", which costs one refetch and
        // loses nothing: cache versions are derived state the server can always restate.
        let records: Vec<VersionRecord> = all_in_scope(&store, &self.scope).await?.decodable();
        Ok(records
            .into_iter()
            .map(|record| (record.entity.clone(), record.decode()))
            .collect())
    }

    async fn put(&self, updates: &[(String, EntityState)]) -> Result<(), Error> {
        // Rejected before the transaction opens, so a duplicate leaves the store untouched rather
        // than half-applied. The trait rejects a repeated name because the two entries may
        // disagree and picking one would make argument order the arbiter.
        for (index, (entity, _)) in updates.iter().enumerate() {
            if updates[..index].iter().any(|(seen, _)| seen == entity) {
                return Err(Error::protocol(format!(
                    "one entity named twice in a single put: {entity:?}"
                )));
            }
        }

        // One transaction for the whole batch, and every await inside it is an IDB request — the
        // rule the `request` module exists to keep followable. Decision 015 makes version and
        // staleness one atomically written unit; a batch that committed halfway would leave some
        // entities advanced and others not.
        let transaction = self
            .backend
            .transaction(&[VERSIONS], IdbTransactionMode::Readwrite)?;
        let store = transaction
            .inner()
            .object_store(VERSIONS)
            .map_err(js_error)?;
        for (entity, state) in updates {
            let record = VersionRecord {
                scope: self.scope.as_str().to_owned(),
                entity: entity.clone(),
                version: state.version.as_ref().map(|v| v.as_str().to_owned()),
                stale: state.stale,
            };
            await_request(store.put(&to_js(&record)?).map_err(js_error)?).await?;
        }
        transaction.commit().await
    }
}

impl VersionRecord {
    /// # Why this is `from_parts` and not a match on the three named constructors
    ///
    /// A `null` version with `stale: true` is a real record —
    /// [`mark_all_stale`](frontbox::InvalidationRunner::mark_all_stale) writes one for every
    /// registered entity no invalidation has ever named. Rebuilding through
    /// `unknown()`/`stale_at`/`fresh_at` covers the other three combinations and would have to
    /// answer `unknown()` here, dropping the staleness and cancelling the refetch the caller asked
    /// for. Case 67 is what fails when it does.
    fn decode(&self) -> EntityState {
        EntityState::from_parts(self.version.clone().map(CacheVersion::new), self.stale)
    }
}
