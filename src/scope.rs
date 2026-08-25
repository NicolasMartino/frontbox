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
