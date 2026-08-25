//! Client-generated mutation identity.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client-provided idempotency key for a mutation.
///
/// The caller owns generation. Core never mints a `MutationId` on the caller's behalf, which is
/// what lets an application attempt a direct write and then enqueue under the *same* id if that
/// write fails — the direct-dispatch pattern that
/// `wiki/proposals/extraction-boundary.proposal.md` keeps out of core.
///
/// # Ordering
///
/// `Ord` compares the underlying UUID's 16 bytes. This is load-bearing: pending work is ordered by
/// the compound key `(created_at, mutation_id)`, and a durable backend must reproduce that order
/// exactly. Lowercase canonical hex compares in the same sequence as the raw bytes, so a SQLite
/// `ORDER BY created_at, mutation_id` over a `TEXT` column holding [`MutationId::to_string`] output
/// yields the identical order. Storing the id in any other textual form breaks that guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MutationId(Uuid);

impl MutationId {
    /// Wrap an existing UUID.
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Borrow the underlying UUID.
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Generate a random v4 identifier.
    ///
    /// Requires the `v4` feature, which pulls in a randomness backend. Core itself never calls
    /// this; it exists so callers that have no id of their own do not have to depend on `uuid`
    /// directly.
    #[cfg(feature = "v4")]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[cfg(feature = "v4")]
impl Default for MutationId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for MutationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for MutationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Lowercase hyphenated form. See the ordering note on the type.
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MutationId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}
