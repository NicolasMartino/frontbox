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
mod version;

pub use runner::{
    ClassifiedConflict, InvalidationReport, InvalidationRunner, PendingConflict, StaleEntity,
};
pub use store::CacheVersionStore;
pub use version::CacheVersion;

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
/// everything reads as unknown and is refetched. That asymmetry is why
/// `wiki/decisions/015-cache-version-persistence.decision.md` makes the pair one atomically written
/// unit rather than two fields that happen to sit near each other.
///
/// # `version` is an `Option`, and that is load-bearing
///
/// [`None`] means *nothing has ever been heard about this entity*. It is not a version, and no
/// version compares equal to it. D2 shipped this as `0` and that was a latent bug under any
/// content-addressed scheme: XOR's identity element is zero, so an empty collection hashes to
/// zero, and a never-synced client would have compared equal to a collection the server had
/// legitimately emptied — reporting no change and never refetching
/// (`wiki/decisions/021-cache-version-identity.decision.md`).
///
/// This type is not `Copy`, because [`CacheVersion`] owns a `String`. Clone it explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EntityState {
    /// The server version this client has been told about, or [`None`] if it never has been.
    pub version: Option<CacheVersion>,
    /// Whether a refetch is owed.
    pub stale: bool,
}

impl EntityState {
    /// State for an entity nothing is known about: no version, not stale.
    ///
    /// This is what an unrecorded entity reads as, and it is deliberately *not* stale. An
    /// application that has never heard of a version has also never been told its data is wrong.
    pub const fn unknown() -> Self {
        Self {
            version: None,
            stale: false,
        }
    }

    /// State at `version`, with a refetch owed.
    pub fn stale_at(version: impl Into<CacheVersion>) -> Self {
        Self {
            version: Some(version.into()),
            stale: true,
        }
    }

    /// State at `version`, with nothing owed.
    pub fn fresh_at(version: impl Into<CacheVersion>) -> Self {
        Self {
            version: Some(version.into()),
            stale: false,
        }
    }

    /// Rebuild a state from what a store has on disk.
    ///
    /// # Why this exists, and why the three named constructors are not enough
    ///
    /// This type is `#[non_exhaustive]`, so a backend outside this crate cannot write the struct
    /// literal. The three constructors above cover *three* of the four combinations —
    /// [`unknown`](Self::unknown) is `(None, false)`, [`stale_at`](Self::stale_at) is
    /// `(Some, true)`, [`fresh_at`](Self::fresh_at) is `(Some, false)`. The fourth, **stale with no
    /// version**, had no constructor at all.
    ///
    /// It is not a corner. [`InvalidationRunner::mark_all_stale`] flags every entity the registry
    /// models, including ones no invalidation has ever named — and for those, `state()` reads back
    /// `unknown()`, so the write is `(None, true)`. That is the error-recovery hammer applied to a
    /// client that has just installed, which is the *most* likely time to reach for it.
    /// [`StaleEntity::version`](crate::cache::StaleEntity::version) already documented the state as
    /// legitimate; nothing outside this crate could reconstruct it.
    ///
    /// Found by writing D5's second version store rather than by reading the type, which is the
    /// same way [`QuarantinedRecord::from_raw`](crate::QuarantinedRecord::from_raw) was found: a
    /// `#[non_exhaustive]` record is only as reconstructible as its constructors, and the in-tree
    /// backend never noticed because it builds the literal directly.
    ///
    /// [`InvalidationRunner::mark_all_stale`]: crate::cache::InvalidationRunner::mark_all_stale
    pub fn from_parts(version: Option<CacheVersion>, stale: bool) -> Self {
        Self { version, stale }
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
    pub version: CacheVersion,
}

impl InvalidationEvent {
    /// Build an event.
    pub fn new(entity: impl Into<String>, version: impl Into<CacheVersion>) -> Self {
        Self {
            entity: entity.into(),
            version: version.into(),
        }
    }
}

/// What comparing one server version against the local one implies.
///
/// # There is no "behind"
///
/// D2 shipped a third variant, `NeedsReset`, for a server version *lower* than the local one. It
/// was removed by `wiki/decisions/021-cache-version-identity.decision.md` along with the ordering
/// that produced it: a [`CacheVersion`] is compared by equality, so an identity is either the one
/// this client holds or a different one, and a different one means refetch. Nothing could produce
/// the variant any more, and a public variant nothing produces — with a report field that would
/// always be empty — is worse than an absent one. `VersionUpdate` is `#[non_exhaustive]`, so
/// restoring it later is additive.
///
/// An application that needs "my stored identity is unusable, start over" has
/// [`InvalidationRunner::mark_all_stale`], which is what the reset path did anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionUpdate {
    /// The server's identity differs from the local one. The local version advances and a refetch
    /// is owed.
    ///
    /// This is also the answer for an entity nothing is known about, since [`None`] is not a
    /// version and cannot compare equal to one.
    Updated,
    /// The identities agree. Nothing to do, and an already-owed refetch stays owed.
    NoChange,
}

/// Compare a server version against local state.
///
/// Equality only. Core does not know whether the application's server counts, hashes, or issues
/// ETags, and it does not need to: the question a client actually asks is "is what I hold still
/// what the server has", which equality answers for every one of those schemes.
///
/// # Why this takes a `CacheVersion` and not an `Option<CacheVersion>`
///
/// A server saying "this changed, version unknown" is asking for a different operation and should
/// name it as one. The source's signature was `update_version(entity, Option<u64>)` mapping `None`
/// onto zero — a spurious full reset triggered by a missing field — and checking its call sites is
/// what settled this: **all eleven pass `Some`**. The one event shape carrying no version,
/// `LegacyInvalidationEvent`, is declared and never used anywhere in the copied corpus.
pub fn compare(local: &EntityState, server_version: &CacheVersion) -> VersionUpdate {
    match &local.version {
        Some(local_version) if local_version == server_version => VersionUpdate::NoChange,
        _ => VersionUpdate::Updated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equality is the whole comparison, and magnitude is not consulted.
    ///
    /// The source's six `persistence/cache.rs` tests reduce to this plus the case below. Its third
    /// case — a server *behind* the client — has no equivalent: there is no behind
    /// (`wiki/decisions/021-cache-version-identity.decision.md`).
    #[test]
    fn comparison_is_equality_not_order() {
        let five = CacheVersion::new("5");
        assert_eq!(
            compare(&EntityState::unknown(), &five),
            VersionUpdate::Updated,
            "a first invalidation, from nothing known"
        );
        assert_eq!(
            compare(&EntityState::fresh_at("5"), &five),
            VersionUpdate::NoChange
        );
        assert_eq!(
            compare(&EntityState::fresh_at("10"), &five),
            VersionUpdate::Updated,
            "a smaller identity is a different identity, not a reset"
        );
        assert_eq!(
            compare(&EntityState::fresh_at("a3f8"), &CacheVersion::new("b104")),
            VersionUpdate::Updated,
            "identities need not be numeric at all"
        );
    }

    /// A zero-valued identity is a version; never having heard one is not.
    ///
    /// This is the case the shipped `u64` got wrong. Under XOR set hashing an empty collection
    /// hashes to zero, so "the server says this holds nothing" and "I have never synced" would
    /// have compared equal and the client would never have refetched.
    #[test]
    fn a_zero_identity_is_not_an_absent_one() {
        let zero = CacheVersion::new("0");
        assert_eq!(
            compare(&EntityState::unknown(), &zero),
            VersionUpdate::Updated,
            "an empty collection is news to a client that has never synced"
        );
        assert_eq!(
            compare(&EntityState::fresh_at("0"), &zero),
            VersionUpdate::NoChange,
            "and having already been told it is empty is not news again"
        );
    }

    /// An entity nothing is known about is not stale. Never having heard a version is not the same
    /// as having been told the data is wrong.
    #[test]
    fn unknown_state_is_not_stale() {
        assert_eq!(EntityState::unknown().version, None);
        assert!(!EntityState::unknown().stale);
        assert!(EntityState::stale_at("3").stale);
        assert!(!EntityState::fresh_at("3").stale);
    }

    /// Comparison reads the version only. An entity can be stale at a version the server agrees
    /// with — that is exactly the state a refetch has not yet cleared, and the case that makes
    /// `stale` worth persisting.
    #[test]
    fn staleness_does_not_affect_comparison() {
        assert_eq!(
            compare(&EntityState::stale_at("5"), &CacheVersion::new("5")),
            VersionUpdate::NoChange
        );
        assert_eq!(
            compare(&EntityState::stale_at("5"), &CacheVersion::new("6")),
            VersionUpdate::Updated
        );
    }

    #[test]
    fn an_event_round_trips_without_a_user_id() {
        let event = InvalidationEvent::new("exercises", "7");
        let json = serde_json::to_value(&event).expect("serializes");

        assert_eq!(
            json,
            serde_json::json!({ "entity": "exercises", "version": "7" }),
            "the version travels as the server rendered it, not as a number"
        );

        let back: InvalidationEvent = serde_json::from_value(json).expect("deserializes");
        assert_eq!(back, event);
    }

    /// `EntityState` round-trips as a unit, which is what decision 015 requires of a backend.
    #[test]
    fn entity_state_round_trips_as_a_pair() {
        for state in [
            EntityState::unknown(),
            EntityState::stale_at("a3f8"),
            EntityState::fresh_at(""),
        ] {
            let json = serde_json::to_string(&state).expect("serializes");
            let back: EntityState = serde_json::from_str(&json).expect("deserializes");
            assert_eq!(back, state);
        }
    }
}
