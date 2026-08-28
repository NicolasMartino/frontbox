//! The in-memory [`CacheVersionStore`] implementation.

use super::{InMemoryBackend, StoreOp};
use crate::cache::{CacheVersionStore, EntityState};
use crate::error::Error;
use crate::scope::ScopeKey;

/// A handle on one scope's cache version state.
///
/// Separate from [`InMemoryStore`](crate::memory::InMemoryStore) because the outbox and the version
/// map are different stores with different keys — the outbox holds raw HTTP envelopes and knows
/// nothing about entities. Both read the same [`InMemoryBackend`], so the scope enforcement they
/// share is one implementation rather than two that have to agree.
#[derive(Debug, Clone)]
pub struct InMemoryVersionStore {
    backend: InMemoryBackend,
    scope: ScopeKey,
}

impl InMemoryVersionStore {
    pub(super) fn new(backend: InMemoryBackend, scope: ScopeKey) -> Self {
        Self { backend, scope }
    }

    /// The backend behind this handle.
    pub fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }
}

impl CacheVersionStore for InMemoryVersionStore {
    fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    async fn state(&self, entity: &str) -> Result<EntityState, Error> {
        self.backend.check(StoreOp::CacheState)?;
        Ok(self
            .backend
            .state
            .borrow()
            .versions
            .get(&(self.scope.clone(), entity.to_string()))
            .cloned()
            // Not an error: never having heard a version is a normal starting condition.
            .unwrap_or_else(EntityState::unknown))
    }

    async fn all_states(&self) -> Result<Vec<(String, EntityState)>, Error> {
        self.backend.check(StoreOp::CacheAllStates)?;
        // Rows under another scope are not merely filtered out of the result — the scope is half of
        // the key, so they are unreachable. Same rule as the outbox, which is what lets two scopes
        // share one backend in the conformance suite and still prove isolation.
        Ok(self
            .backend
            .state
            .borrow()
            .versions
            .iter()
            .filter(|((scope, _), _)| *scope == self.scope)
            .map(|((_, entity), state)| (entity.clone(), state.clone()))
            .collect())
    }

    async fn put(&self, updates: &[(String, EntityState)]) -> Result<(), Error> {
        self.backend.check(StoreOp::CachePut)?;

        // Validated and staged fully before anything commits, so a rejected batch leaves no partial
        // write. This backend cannot tear a write — it commits by mutating one map under one
        // borrow — so what the staging proves is the *contract*. Real transactional rollback is a
        // durable backend's problem.
        let mut staged: Vec<((ScopeKey, String), EntityState)> = Vec::with_capacity(updates.len());
        for (entity, state) in updates {
            let key = (self.scope.clone(), entity.clone());
            if staged.iter().any(|(seen, _)| *seen == key) {
                return Err(Error::protocol(format!(
                    "one entity named twice in a single put: {entity:?}"
                )));
            }
            staged.push((key, state.clone()));
        }

        let mut state = self.backend.state.borrow_mut();
        for (key, value) in staged {
            state.versions.insert(key, value);
        }
        Ok(())
    }
}
