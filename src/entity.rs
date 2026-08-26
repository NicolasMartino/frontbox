//! Caller-owned entity keys.
//!
//! The source system has a closed `EntityType` enum on both the client and the server, naming four
//! of its own domain types, while the wire carries a bare string. A reusable library cannot have
//! that enum, so the application owns the key type and supplies a registry for the two operations
//! that need to enumerate or parse them
//! (`wiki/decisions/007-generic-entity-key-registry.decision.md`).

use std::hash::Hash;

/// An application's name for one kind of cached data.
///
/// Implement this on whatever the application already uses — usually its own enum. The string form
/// is the wire form: it is what a server names in an invalidation event and what a durable backend
/// stores.
///
/// # One textual form
///
/// [`as_str`](EntityKey::as_str) must be stable and must be the *only* spelling a key ever has. This
/// is the lesson [`MutationId`](crate::id::MutationId) already carries into D5: a backend that
/// writes `Widget` in one row and `widget` in another has two entities where the caller has one,
/// and the second will never be found by a lookup for the first. Ordering is not required of entity
/// keys, so no collation constraint follows — but case does.
pub trait EntityKey: Clone + Eq + Hash + 'static {
    /// The wire and storage spelling of this key.
    fn as_str(&self) -> &str;
}

impl EntityKey for String {
    fn as_str(&self) -> &str {
        self
    }
}

impl EntityKey for &'static str {
    fn as_str(&self) -> &str {
        self
    }
}

/// The application's set of known entity keys, and how to parse one off the wire.
///
/// Two operations need this and neither can be expressed over a bare string.
/// [`all`](EntityRegistry::all) is what makes "mark everything stale" meaningful — the source spells
/// it `EntityType::all()` and a generic key has no equivalent. [`parse`](EntityRegistry::parse) is
/// what turns a wire name into a key, and its `None` is a reportable outcome rather than a discard
/// (`wiki/decisions/013-unknown-entity-name.decision.md`).
pub trait EntityRegistry {
    /// The application's key type.
    type Key: EntityKey;

    /// Every entity this application models.
    fn all(&self) -> &[Self::Key];

    /// Turn a wire name into a key, or `None` if this application does not model it.
    ///
    /// Returning `None` is not an error. A server that adds an entity type starts naming it
    /// immediately, to clients built before it existed; treating that as a failure would break
    /// every client during a rolling deploy. The caller learns about it from
    /// [`InvalidationReport::unknown`](crate::cache::InvalidationReport::unknown) instead.
    fn parse(&self, wire: &str) -> Option<Self::Key>;
}

/// A registry over a fixed list of keys.
///
/// Matches by [`EntityKey::as_str`], which is the definition of the wire form, so an application
/// whose keys are already strings or a `&'static str` enum needs nothing else. An application with
/// aliases, plurals, or a legacy spelling implements [`EntityRegistry`] itself.
#[derive(Debug, Clone)]
pub struct SliceRegistry<K> {
    keys: Vec<K>,
}

impl<K: EntityKey> SliceRegistry<K> {
    /// Build a registry over `keys`.
    ///
    /// Duplicate keys are accepted and are harmless: `parse` finds the first and `all` is only ever
    /// used to iterate. Rejecting them would make the constructor fallible for a caller mistake
    /// that cannot corrupt anything.
    pub fn new(keys: impl IntoIterator<Item = K>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
        }
    }
}

impl<K: EntityKey> EntityRegistry for SliceRegistry<K> {
    type Key = K;

    fn all(&self) -> &[K] {
        &self.keys
    }

    fn parse(&self, wire: &str) -> Option<K> {
        self.keys.iter().find(|key| key.as_str() == wire).cloned()
    }
}

impl<R: EntityRegistry + ?Sized> EntityRegistry for &R {
    type Key = R::Key;

    fn all(&self) -> &[R::Key] {
        (**self).all()
    }

    fn parse(&self, wire: &str) -> Option<R::Key> {
        (**self).parse(wire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slice_registry_parses_by_wire_form() {
        let registry = SliceRegistry::new(["exercises", "sessions"]);

        assert_eq!(registry.parse("exercises"), Some("exercises"));
        assert_eq!(registry.parse("sessions"), Some("sessions"));
        assert_eq!(registry.all().len(), 2);
    }

    /// The case decision 013 exists for: an unmodelled name is `None`, not an error and not a guess.
    #[test]
    fn an_unmodelled_name_does_not_parse() {
        let registry = SliceRegistry::new(["exercises"]);

        assert_eq!(registry.parse("templates"), None);
        assert_eq!(registry.parse(""), None);
    }

    /// Matching is exact. A registry that quietly accepted `Exercises` would give a backend two
    /// spellings for one entity, which is the failure `EntityKey::as_str` is documented against.
    #[test]
    fn parsing_is_case_sensitive() {
        let registry = SliceRegistry::new(["exercises"]);

        assert_eq!(registry.parse("Exercises"), None);
        assert_eq!(registry.parse("EXERCISES"), None);
    }

    #[test]
    fn owned_and_borrowed_keys_both_work() {
        let owned = SliceRegistry::new([String::from("exercises")]);
        assert_eq!(owned.parse("exercises"), Some(String::from("exercises")));

        // The blanket impl over `&R` is what lets a runner borrow a registry rather than own it.
        let registry = SliceRegistry::new(["sessions"]);
        fn parse_with(registry: impl EntityRegistry<Key = &'static str>) -> Option<&'static str> {
            registry.parse("sessions")
        }
        assert_eq!(parse_with(&registry), Some("sessions"));
    }
}
