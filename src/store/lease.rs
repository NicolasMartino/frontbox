//! Shared scope exclusion for drains and storage migrations.

use super::OutboxStore;
use crate::Error;

/// Exclusive drain rights for one scope, held for the length of one pass.
///
/// Released on drop, whatever ends the pass — returning, erroring, or being cancelled. Core never
/// looks inside: a backend puts whatever it needs to release in the closure, and a backend with
/// nothing to release supplies none.
pub struct DrainLease(Option<Box<dyn FnOnce()>>);

impl DrainLease {
    /// Claim both this realm's scope slot and the backend's cross-realm drain rights.
    ///
    /// Shared by drains and storage migrations. `None` means the scope is busy. Dropping
    /// this lease releases both claims, including when the enclosing future is cancelled.
    pub async fn claim(store: &impl OutboxStore) -> Result<Option<Self>, Error> {
        let Some(claim) = crate::runner::exclusion::DrainClaim::try_acquire(store.scope()) else {
            return Ok(None);
        };
        let Some(lease) = store.claim_drain().await? else {
            return Ok(None);
        };
        Ok(Some(Self::held(move || drop((lease, claim)))))
    }

    /// A lease with nothing to release.
    ///
    /// What the default [`claim_drain`](OutboxStore::claim_drain) hands back, and what a
    /// single-realm backend should return.
    #[must_use]
    pub fn granted() -> Self {
        Self(None)
    }

    /// A lease that runs `release` when it is dropped.
    #[must_use]
    pub fn held(release: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}

impl Drop for DrainLease {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}

impl std::fmt::Debug for DrainLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "DrainLease(held)",
            None => "DrainLease(granted)",
        })
    }
}
