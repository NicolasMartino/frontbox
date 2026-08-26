//! Cache version tracking and invalidation.
//!
//! The server keeps a version number per entity per user and bumps it when that entity changes. A
//! client compares its own version against the server's to decide what it needs to refetch. This
//! module holds that comparison, the state it reads and writes, and the report a caller acts on.
//!
//! # What is not here
//!
//! The refetch. Core marks an entity stale; it does not know how to fetch one, and it does not own
//! the read model that would receive the result. That boundary is why
//! [`InvalidationRunner::stale`] reports a *conflict* rather than blocking a pull —
//! `wiki/decisions/014-pull-gating.decision.md` has the reasoning, and the short version is that
//! gating an operation core does not perform is not a policy core can implement.
//!
//! Also not here: how invalidation events arrive. Server-sent events, websockets, and polling are
//! transport concerns, and the source system's listener mixes them with cache semantics in a way
//! this deliberately does not.

use serde::{Deserialize, Serialize};

mod runner;
mod store;

pub use runner::{
    ClassifiedConflict, InvalidationReport, InvalidationRunner, PendingConflict, StaleEntity,
};
pub use store::CacheVersionStore;

/// What a client knows about one entity.
///
/// # Why both fields travel together
///
/// The version advances at the moment of *invalidation*, not at the moment of refetch: an
/// invalidation sets `version` to the server's and `stale` to true, and only a successful refetch
/// clears `stale`. So immediately after an invalidation the local version already equals the
/// server's while the data is still unfetched, and `stale` is the only thing that remembers the
/// refetch is owed.
///
/// A backend that persisted `version` alone would therefore restart, compare equal, report no
/// change, and serve data the server explicitly invalidated. Losing *both* is safe by comparison —
/// everything reads as version zero and is refetched. That asymmetry is why
/// `wiki/decisions/015-cache-version-persistence.decision.md` makes the pair one atomically written
/// unit rather than two fields that happen to sit near each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EntityState {
    /// The highest server version this client has been told about.
    pub version: u64,
    /// Whether a refetch is owed.
    pub stale: bool,
}

impl EntityState {
    /// State for an entity nothing is known about: version zero, not stale.
    ///
    /// This is what an unrecorded entity reads as, and it is deliberately *not* stale. An
    /// application that has never heard of a version has also never been told its data is wrong.
    pub const fn unknown() -> Self {
        Self {
            version: 0,
            stale: false,
        }
    }

    /// State at `version`, with a refetch owed.
    pub const fn stale_at(version: u64) -> Self {
        Self {
            version,
            stale: true,
        }
    }

    /// State at `version`, with nothing owed.
    pub const fn fresh_at(version: u64) -> Self {
        Self {
            version,
            stale: false,
        }
    }
}

/// A server's notice that one entity changed.
///
/// # No user identifier
///
/// The source's event carries a `user_id` and documents it as debugging-only, with a note that
/// security isolation happens at the channel level and that the field must not be used for access
/// control. Rather than carry a field whose documentation is a warning, frontbox leaves it out:
/// isolation is [`ScopeKey`](crate::scope::ScopeKey), which every store enforces on every read
/// (`wiki/decisions/009-local-scope-identity.decision.md`). A transport that receives the source's
/// payload drops the field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct InvalidationEvent {
    /// Which entity changed, in the server's spelling.
    pub entity: String,
    /// The server's version for that entity after the change.
    pub version: u64,
}

impl InvalidationEvent {
    /// Build an event.
    pub fn new(entity: impl Into<String>, version: u64) -> Self {
        Self {
            entity: entity.into(),
            version,
        }
    }
}

/// What comparing one server version against the local one implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionUpdate {
    /// The server is ahead. The local version advances and a refetch is owed.
    Updated,
    /// The versions agree. Nothing to do, and an already-owed refetch stays owed.
    NoChange,
    /// The server is *behind* the local version, which cannot happen in normal operation.
    ///
    /// It means the server's counter restarted — a database restore, a re-provisioned user, a
    /// migration that reset the table. Advancing the local version to the smaller number would
    /// leave the client believing it is ahead of the server forever, so the answer is to reset to
    /// zero and refetch, which is the only state both ends can agree on.
    NeedsReset,
}

/// Compare a server version against local state.
///
/// # Why this takes `u64` and not `Option<u64>`
///
/// The source signature is `update_version(entity, server_version: Option<u64>)` and maps `None`
/// onto zero, which reads as [`NeedsReset`](VersionUpdate::NeedsReset) for any client with a
/// non-zero local version — a spurious full reset triggered by a missing field.
///
/// Checking the source's own call sites is what settled this: **all eleven pass `Some`**. The one
/// event shape that carries no version, `LegacyInvalidationEvent`, is declared and never used
/// anywhere in the copied corpus. The `Option` was not a feature with a hazard attached; it was
/// unexercised surface whose only behaviour was the hazard. A server that genuinely needs to say
/// "this changed, version unknown" is asking for a different operation, and it should be named as
/// one rather than smuggled in as a `None`.
pub fn compare(local: EntityState, server_version: u64) -> VersionUpdate {
    if server_version > local.version {
        VersionUpdate::Updated
    } else if server_version < local.version {
        VersionUpdate::NeedsReset
    } else {
        VersionUpdate::NoChange
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three cases, which are the source's six `persistence/cache.rs` tests in one place.
    #[test]
    fn comparison_covers_ahead_equal_and_behind() {
        assert_eq!(
            compare(EntityState::unknown(), 5),
            VersionUpdate::Updated,
            "a first invalidation from version zero"
        );
        assert_eq!(
            compare(EntityState::fresh_at(5), 5),
            VersionUpdate::NoChange
        );
        assert_eq!(
            compare(EntityState::fresh_at(10), 5),
            VersionUpdate::NeedsReset,
            "a server behind the client means the server's counter restarted"
        );
    }

    /// Zero from zero is agreement, not an update. Otherwise a server that has never bumped an
    /// entity would invalidate it on every reconnect forever.
    #[test]
    fn zero_against_zero_is_no_change() {
        assert_eq!(compare(EntityState::unknown(), 0), VersionUpdate::NoChange);
    }

    /// An entity nothing is known about is not stale. Never having heard a version is not the same
    /// as having been told the data is wrong.
    #[test]
    fn unknown_state_is_not_stale() {
        assert_eq!(EntityState::unknown().version, 0);
        assert!(!EntityState::unknown().stale);
        assert!(EntityState::stale_at(3).stale);
        assert!(!EntityState::fresh_at(3).stale);
    }

    /// Comparison reads the version only. An entity can be stale at a version the server agrees
    /// with — that is exactly the state a refetch has not yet cleared, and the case that makes
    /// `stale` worth persisting.
    #[test]
    fn staleness_does_not_affect_comparison() {
        assert_eq!(
            compare(EntityState::stale_at(5), 5),
            VersionUpdate::NoChange
        );
        assert_eq!(compare(EntityState::stale_at(5), 6), VersionUpdate::Updated);
    }

    #[test]
    fn an_event_round_trips_without_a_user_id() {
        let event = InvalidationEvent::new("exercises", 7);
        let json = serde_json::to_value(&event).expect("serializes");

        assert_eq!(
            json,
            serde_json::json!({ "entity": "exercises", "version": 7 })
        );
        assert_eq!(
            serde_json::from_value::<InvalidationEvent>(json).expect("round trips"),
            event
        );
    }
}
