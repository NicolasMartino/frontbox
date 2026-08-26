//! The conformance-suite factory over [`InMemoryBackend`](super::InMemoryBackend).
//!
//! Behind the `testing` feature in its entirety, so nothing here is compiled into an application.

use crate::error::Error;
use crate::scope::ScopeKey;

use super::{InMemoryBackend, InMemoryStore, InMemoryVersionStore, ManualClock, StoreOp};

/// A [`StoreFactory`](crate::testing::StoreFactory) over [`InMemoryBackend`].
///
/// One factory is one backend, so every scope it opens shares durable state — which is exactly the
/// sharing contract the conformance suite needs.
#[derive(Clone, Debug)]
pub struct InMemoryFactory {
    backend: InMemoryBackend,
    clock: ManualClock,
}

impl Default for InMemoryFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryFactory {
    /// Build a factory over a fresh backend and a clock at the epoch.
    pub fn new() -> Self {
        let clock = ManualClock::new(0);
        Self {
            backend: InMemoryBackend::new(clock.clone()),
            clock,
        }
    }

    /// The backend every store from this factory shares.
    pub fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }
}

impl crate::testing::StoreFactory for InMemoryFactory {
    type Store = InMemoryStore;

    fn clock(&self) -> ManualClock {
        self.clock.clone()
    }

    async fn open(&self, scope: ScopeKey) -> Result<Self::Store, Error> {
        Ok(self.backend.open(scope))
    }

    async fn insert_corrupt_row(
        &self,
        scope: &ScopeKey,
        kind: crate::testing::CorruptKind,
    ) -> Result<(), Error> {
        use crate::testing::CorruptKind;
        match kind {
            CorruptKind::UnparseableId => self.backend.insert_raw_row(
                scope,
                "not-a-uuid",
                "POST",
                "/api/v1/things",
                r#"{"ok":true}"#,
                1,
            ),
            CorruptKind::InvalidBody { id } => self.backend.insert_raw_row(
                scope,
                id.to_string(),
                "POST",
                "/api/v1/things",
                "{ not json",
                1,
            ),
            CorruptKind::UnrepresentableCreatedAt { id } => self.backend.insert_raw_row(
                scope,
                id.to_string(),
                "POST",
                "/api/v1/things",
                r#"{"ok":true}"#,
                i64::MAX,
            ),
        }
        Ok(())
    }
}

impl crate::testing::VersionStoreFactory for InMemoryFactory {
    type Versions = InMemoryVersionStore;

    async fn open_versions(&self, scope: ScopeKey) -> Result<Self::Versions, Error> {
        Ok(self.backend.open_versions(scope))
    }
}

impl crate::testing::FaultInjection for InMemoryFactory {
    async fn fail_next_apply_outcomes(&self) {
        self.backend.fail_next(StoreOp::ApplyOutcomes);
    }
}
