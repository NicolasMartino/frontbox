//! Local store identity.

use crate::error::Error;

/// The identity of one local store, compared for equality and nothing else.
///
/// Every store is opened with a required, non-empty, caller-composed key
/// (`wiki/decisions/009-local-scope-identity.decision.md`). There is no unscoped constructor: the
/// source system this crate is extracted from ships a scoped *and* an unscoped constructor on both
/// backends, so its isolation property holds only where every call site remembered to pick the
/// right one. Removing the choice is the point.
///
/// # Core does not interpret the key
///
/// Composing principal, tenant, schema version, and read scope into one key is the caller's job.
/// Core never parses it, never orders by it, and never derives authorization from it. A caller
/// whose identity model has dimensions this crate has never heard of is served by the same type.
///
/// # Core does not normalize the key
///
/// [`ScopeKey::new`] rejects an empty key and otherwise stores exactly what it was given. It does
/// not trim, case-fold, or replace characters. Any normalization is non-injective, and collapsing
/// two distinct keys onto one identity is the precise failure this type exists to prevent — the
/// same failure the source's storage-name sanitization has, where `tenant/1` and `tenant_1` both
/// become `tenant_1`.
///
/// Durable backends face that hazard again when they turn a key into a file or database name.
/// They must encode reversibly or hash; they must not replace characters.
///
/// # Whitespace-only keys are valid
///
/// Only the empty key is rejected. `" "` and `"\n"` are accepted, and are distinct scopes from each
/// other and from `""`-adjacent variants. This is deliberate rather than an oversight: rejecting
/// them would mean core deciding which key *contents* are meaningful, which is the same
/// normalization judgment the section above rules out.
///
/// It would not buy much either. The realistic composition bug is `format!("user:{id}")` with an
/// empty `id`, which yields `"user:"` — non-empty, meaningful-looking, and equally wrong. A caller
/// that needs its keys validated should validate them where it knows what they mean, before
/// building a `ScopeKey`:
///
/// ```
/// use frontbox::{Error, ScopeKey};
///
/// fn scope_for(user_id: &str, tenant: &str) -> Result<ScopeKey, Error> {
///     // Core cannot make this check: it does not know the key has parts, which of them may be
///     // empty, or that `v1` is a schema version rather than a user named `v1`.
///     if user_id.trim().is_empty() || tenant.trim().is_empty() {
///         return Err(Error::invalid_scope_key("user and tenant are both required"));
///     }
///     ScopeKey::new(format!("user:{user_id}@tenant:{tenant}@v1"))
/// }
///
/// assert!(scope_for("alice", "acme").is_ok());
/// assert!(scope_for("", "acme").is_err(), "core would have accepted \"user:@tenant:acme@v1\"");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeKey(String);

impl ScopeKey {
    /// Build a scope key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidScopeKey`] if the key is empty.
    pub fn new(key: impl Into<String>) -> Result<Self, Error> {
        let key = key.into();
        if key.is_empty() {
            return Err(Error::invalid_scope_key("scope key must not be empty"));
        }
        Ok(Self(key))
    }

    /// Borrow the key as written by the caller.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ScopeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The empty key is the only rejection, and it is typed rather than a panic.
    #[test]
    fn only_the_empty_key_is_rejected() {
        match ScopeKey::new("") {
            Err(Error::InvalidScopeKey { reason }) => assert!(!reason.is_empty()),
            other => panic!("an empty key must be rejected, got {other:?}"),
        }

        for accepted in [" ", "\n", "\t", "user:", "user:alice@tenant:acme@v1", "0"] {
            assert!(
                ScopeKey::new(accepted).is_ok(),
                "{accepted:?} must be accepted"
            );
        }
    }

    /// Stored exactly as written: no trim, no case fold, no character replacement.
    ///
    /// This is the whole type. Any normalization is non-injective, and collapsing two distinct
    /// keys onto one identity is the failure `ScopeKey` exists to prevent.
    #[test]
    fn the_key_is_stored_verbatim() {
        for key in [
            "  padded  ",
            "MiXeD",
            "tenant/1",
            "tenant_1",
            "a\u{0301}",
            "á",
        ] {
            let scope = ScopeKey::new(key).expect("valid");
            assert_eq!(scope.as_str(), key);
            assert_eq!(scope.to_string(), key, "Display must not transform it");
        }
    }

    /// The pairs a normalizing implementation would merge stay distinct.
    ///
    /// `tenant/1` and `tenant_1` are the source system's actual collision: its storage-name
    /// sanitization replaces `/` with `_`, so two tenants share one local store. The Unicode pair
    /// is the same hazard from the other direction — visually identical, different bytes — and it
    /// is listed to make explicit that this type does not normalize those either.
    #[test]
    fn keys_a_normalizer_would_merge_stay_distinct() {
        let distinct = [
            "tenant/1",
            "tenant_1",
            "MiXeD",
            "mixed",
            " user",
            "user",
            "a\u{0301}",
            "\u{e1}",
        ];

        for (i, left) in distinct.iter().enumerate() {
            for right in &distinct[i + 1..] {
                assert_ne!(
                    ScopeKey::new(*left).expect("valid"),
                    ScopeKey::new(*right).expect("valid"),
                    "{left:?} and {right:?} must be different scopes"
                );
            }
        }
    }

    /// `Ord` exists so a backend can key a map or a table by scope. It follows the string.
    #[test]
    fn ordering_follows_the_key() {
        let mut scopes: Vec<ScopeKey> = ["c", "a", "b"]
            .into_iter()
            .map(|key| ScopeKey::new(key).expect("valid"))
            .collect();
        scopes.sort();

        let sorted: Vec<&str> = scopes.iter().map(ScopeKey::as_str).collect();
        assert_eq!(sorted, ["a", "b", "c"]);
    }
}
