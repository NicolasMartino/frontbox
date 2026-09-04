//! The opaque identity a cache version carries.

use serde::{Deserialize, Serialize};

/// One entity's cache version, as the server rendered it.
///
/// Opaque and **compared by equality only**. Core never orders two of these, never parses one, and
/// has no opinion about what the application's server puts inside.
///
/// # Why this is not a number
///
/// D2 shipped this as a `u64` compared by magnitude, because the source system's server kept a
/// counter. Two things retired that (`wiki/decisions/021-cache-version-identity.decision.md`):
///
/// A counter is bumped by writes that change nothing. Under idempotent `PUT` with client-generated
/// ids — the shape this crate steers callers toward, and the one
/// [`MutationId`](crate::MutationId) exists to make possible — a replayed mutation rewrites the
/// same bytes and advances the counter, invalidating every other device for a change that did not
/// happen. A content hash is unmoved by a rewrite that changes no bytes.
///
/// And ordering bought less than it looked like it did. The only thing magnitude comparison could
/// express that equality cannot is "the server is *behind* me", which is not a state a client can
/// act on: the source's own handling treats it as corruption to reset from, not as information.
///
/// **So the absence of [`PartialOrd`] is the decision, not an omission.** It is not derived here
/// and must not be added by a later convenience: a type that can be compared with `<` will be, and
/// the first caller to do it will be comparing two hex hashes as text. The compiler refusing that
/// is the whole enforcement mechanism, since core cannot see what a caller stores inside.
///
/// # What a caller puts here
///
/// Whatever its server sends, rendered to a string: a hex set hash, a decimal counter, an ETag, a
/// revision id. An application that genuinely has an ordered scheme stores its rendering and
/// compares it by equality — losing only the "behind" case above.
///
/// The value is stored exactly as given. No trim, no case fold, no normalization, for the same
/// reason [`ScopeKey`](crate::ScopeKey) does none of those: every normalization is non-injective,
/// and collapsing two distinct server states onto one identity is the failure this type exists to
/// prevent.
///
/// # The empty string is a valid version
///
/// It is accepted, like any other string. Core never reads the contents, so rejecting a value it
/// does not interpret would make the constructor fallible for no safety gain — the same reasoning
/// [`OperationMeta::new`](crate::OperationMeta::new) applies to an empty operation name.
///
/// **"Not known" is [`None`], never a sentinel value.** That distinction is the point of the
/// [`Option`] in [`EntityState::version`](crate::EntityState::version) and is load-bearing: under
/// XOR set hashing the empty collection hashes to zero, so a client that has never synced and a
/// collection the server legitimately emptied would otherwise share one identity and compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheVersion(String);

impl CacheVersion {
    /// Record a version exactly as the server rendered it.
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    /// Borrow the version as the server rendered it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CacheVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CacheVersion {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for CacheVersion {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stored exactly as written, and equality is the whole comparison.
    #[test]
    fn identity_is_byte_exact() {
        assert_eq!(CacheVersion::new("A3F8"), CacheVersion::new("A3F8"));
        assert_ne!(
            CacheVersion::new("A3F8"),
            CacheVersion::new("a3f8"),
            "case folding would collapse two distinct server states"
        );
        assert_ne!(CacheVersion::new(" 5"), CacheVersion::new("5"));
        assert_eq!(CacheVersion::new("").as_str(), "");
    }

    /// The zero-valued identity is a real value, not an absence.
    ///
    /// This is the case that made the `u64` wrong. XOR's identity element is zero, so an empty
    /// collection hashes to `"0"` — a legitimate server answer that must not read as "never heard".
    #[test]
    fn zero_is_a_value_not_an_absence() {
        let empty_collection = Some(CacheVersion::new("0"));
        let never_heard: Option<CacheVersion> = None;
        assert_ne!(empty_collection, never_heard);
    }

    /// Round-trips as a bare string, so a stored version is readable in a database dump.
    #[test]
    fn serializes_transparently() {
        let json = serde_json::to_string(&CacheVersion::new("a3f8")).expect("serialize");
        assert_eq!(json, "\"a3f8\"");
        let back: CacheVersion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, CacheVersion::new("a3f8"));
    }
}
