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
/// exactly.
///
/// Text ordering agrees with byte ordering as long as **every row uses the same textual form** and
/// the column carries a **binary collation**. ASCII puts digits below both letter cases, so
/// consistently lowercase and consistently uppercase hex each sort correctly, and the hyphens sit
/// at fixed positions so they never separate two canonical strings. A SQLite
/// `ORDER BY created_at, mutation_id` over a `TEXT` column holding [`MutationId::to_string`] output
/// therefore yields the identical order — as does a `BLOB` column holding `Uuid::as_bytes`, which
/// SQLite compares bytewise.
///
/// The collation is a separate requirement from the form, and easier to lose: SQLite's default for
/// `TEXT` is `BINARY` and satisfies this, but a column declared `COLLATE NOCASE` compares
/// case-insensitively and reorders nothing visibly — the rows still look canonical. IndexedDB has
/// no collation choice to make; its keys compare as UTF-16 code units, which for this alphabet is
/// the same order.
///
/// What breaks it is *mixed* forms. One row uppercase and another lowercase reverses any pair that
/// straddles the case boundary, and a row written without hyphens sorts before a hyphenated one
/// whenever their first eight characters match — `-` is `0x2D`, below every hex digit. Both are
/// what a migration, a second write path, or a hand-edited row produces. The safe rule for a
/// backend is to store exactly what [`MutationId::to_string`] returns and never transform it; this
/// module's tests demonstrate both inversions.
///
/// `wiki/specs/frontbox-runtime.spec.md` carries this as a D5 constraint, because it is a schema
/// decision a durable backend has to make before its first write rather than a bug it can fix
/// afterwards: correcting the form or the collation later means rewriting every stored row.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> MutationId {
        MutationId::from_uuid(Uuid::from_u128(n))
    }

    /// The D5 durable-schema constraint, made checkable.
    ///
    /// A backend storing `mutation_id` as text and ordering by it reproduces core's order only if
    /// the text sorts the way the bytes do. It does — for the form [`MutationId::to_string`]
    /// produces — across the whole nibble range and both boundaries that could plausibly diverge.
    #[test]
    fn canonical_text_sorts_the_same_way_the_bytes_do() {
        let mut ids: Vec<MutationId> = (0u128..256).map(|n| id((n << 120) | n)).collect();
        ids.extend((0u128..64).map(|n| id(u128::MAX - n)));
        ids.extend((0u128..64).map(id));

        let mut by_bytes = ids.clone();
        by_bytes.sort();

        let mut by_text = ids.clone();
        by_text.sort_by_key(MutationId::to_string);

        assert_eq!(
            by_bytes, by_text,
            "text ordering must match byte ordering, or a text-column backend reorders the queue"
        );
    }

    /// Mixed case inverts the order. This is the hazard, not uppercase itself.
    ///
    /// Consistent uppercase would in fact be fine — ASCII puts digits below both letter cases, so
    /// `0..9 < A..F` and `0..9 < a..f` order alike. What breaks is two rows written in *different*
    /// forms, which is what a migration, a second write path, or a hand-edited row produces.
    #[test]
    fn mixed_case_text_inverts_the_order() {
        let lower = id(0xa0 << 120);
        let upper = id(0xb0 << 120);

        assert!(lower < upper, "by bytes, 0xa0 precedes 0xb0");
        assert!(
            lower.to_string() < upper.to_string(),
            "consistently lowercase text agrees"
        );
        assert!(
            lower.to_string().to_uppercase() < upper.to_string().to_uppercase(),
            "consistently uppercase text also agrees"
        );
        assert!(
            upper.to_string().to_uppercase() < lower.to_string(),
            "but one row uppercase and one lowercase reverses the pair: 'B' < 'a'"
        );
    }

    /// Dropping the hyphens inverts the order against rows that kept them.
    ///
    /// `-` is `0x2D`, below every hex digit, so a hyphenated string sorts before an unhyphenated one
    /// the moment their first eight characters match — regardless of what the bytes say.
    #[test]
    fn dropping_hyphens_inverts_the_order() {
        let smaller = id(1);
        let larger = id(0x0000_0000_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF);

        assert!(smaller < larger, "by bytes");
        assert!(
            smaller.to_string() < larger.to_string(),
            "consistently hyphenated text agrees"
        );
        assert!(
            larger.to_string() < smaller.to_string().replace('-', ""),
            "but a hyphenated row sorts before an unhyphenated one once the first eight characters \
             match, reversing the pair"
        );
    }

    #[test]
    fn text_round_trips_through_display_and_from_str() {
        let id = id(0x0123_4567_89AB_CDEF);
        let text = id.to_string();
        assert_eq!(text, text.to_lowercase(), "canonical form is lowercase");
        assert_eq!(text.parse::<MutationId>().expect("parse"), id);
    }
}
