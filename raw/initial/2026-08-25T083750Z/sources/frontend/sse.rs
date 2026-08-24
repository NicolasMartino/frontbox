//! Server-Sent Events types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// SSE invalidation event - notifies clients that entities have changed.
///
/// SECURITY NOTE: The `user_id` field is for DEBUGGING/LOGGING only.
/// Security isolation is enforced at the channel level:
/// - Each user has their own broadcast channel
/// - Auth middleware validates token before subscribing
/// - Users can only subscribe to their own channel
///
/// Do NOT rely on `user_id` in the event for access control.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvalidationEvent {
    /// Which entity type changed (e.g., "exercises", "sessions").
    pub entity: String,
    /// Cache version number for this entity type.
    /// Client compares this to its local version to determine staleness.
    pub version: u64,
    /// For debugging/logging only - NOT for security.
    pub user_id: Uuid,
}

/// Legacy invalidation event format for backward compatibility.
/// Used during migration period when some code still expects `entities` array.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyInvalidationEvent {
    /// Which entity types changed (e.g., ["exercises"]).
    pub entities: Vec<String>,
    /// For debugging/logging only - NOT for security.
    pub user_id: Uuid,
}
