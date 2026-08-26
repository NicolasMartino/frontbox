//! The storage seam for cache version state.

use crate::cache::EntityState;
use crate::error::Error;
use crate::scope::ScopeKey;

/// Durable per-entity cache version state, scoped to one identity.
///
/// The third storage trait, alongside [`OutboxStore`](crate::store::OutboxStore) and its
/// companions, and scoped the same way: every read is filtered by the store's
/// [`ScopeKey`](crate::scope::ScopeKey), and two scopes sharing one physical store cannot observe
/// each other. That is not decoration. Cache versions are per-user server state, so a store that
/// leaked them across a user switch would leave the new user believing the previous user's data is
/// fresh (`wiki/decisions/009-local-scope-identity.decision.md`).
///
/// # Why this speaks strings, not entity keys
///
/// Entities are named here by [`EntityKey::as_str`](crate::entity::EntityKey::as_str), not by the
/// application's key type. A store cannot reconstruct a typed key from storage — it has no
/// registry, and a durable one reading rows written by an older build might find a name the current
/// registry no longer models. Making the seam speak the storage form keeps that honest, and keeps
/// D5's SQLite and IndexedDB backends from having to be generic over an application type they never
/// interpret. [`InvalidationRunner`](crate::cache::InvalidationRunner) owns the typed API and does
/// the parsing, because it is the thing that holds the registry.
///
/// # Durability
///
/// Implementations are expected to persist. An in-memory implementation is legitimate and
/// [`InMemoryBackend`](crate::memory::InMemoryBackend) provides one, but it is the exception:
/// losing this state means every entity reads as version zero and is refetched on every launch,
/// which for an offline-first application defeats the point of having kept the data
/// (`wiki/decisions/015-cache-version-persistence.decision.md`).
#[allow(async_fn_in_trait)] // See the note on `SyncTransport`; decision 001.
pub trait CacheVersionStore {
    /// The scope every read and write here is filtered by.
    fn scope(&self) -> &ScopeKey;

    /// What is known about one entity.
    ///
    /// An entity with no recorded state reads as [`EntityState::unknown`] rather than an error.
    /// Never having heard a version is a normal starting condition, not a fault.
    async fn state(&self, entity: &str) -> Result<EntityState, Error>;

    /// Every entity with recorded state under this scope, in no particular order.
    ///
    /// Entities the application models but has never recorded state for are absent, not present at
    /// [`EntityState::unknown`]. Entities the application no longer models may be present — a
    /// durable store outlives the build that wrote it.
    async fn all_states(&self) -> Result<Vec<(String, EntityState)>, Error>;

    /// Write version and staleness for one or more entities, atomically.
    ///
    /// # The atomicity requirement
    ///
    /// All of `updates` commit or none does, and within each entry `version` and `stale` commit
    /// together. This is the same rule as
    /// [`apply_outcomes`](crate::store::OutboxStore::apply_outcomes) and it exists for a sharper
    /// reason here: a crash between advancing a version and setting its staleness flag leaves the
    /// client believing an invalidated entity is fresh, permanently, with nothing to detect it.
    ///
    /// Naming one entity twice in a single call is rejected, for the same reason
    /// `apply_outcomes` rejects a repeated id: the two entries may disagree, and picking one would
    /// make argument order the arbiter.
    ///
    /// # Errors
    ///
    /// A storage failure, or a repeated entity name.
    async fn put(&self, updates: &[(String, EntityState)]) -> Result<(), Error>;
}
