//! An opaque version derived from this server's own state.
//!
//! # Not shared with `user-server`, and not because of the wire-format argument
//!
//! `wire.rs` is duplicated between the two servers to keep them independent readings of one
//! protocol. This file is duplicated for a different and simpler reason: **a version is opaque, so
//! the two servers are not implementing one thing at all.** No client ever computes a version, and
//! nothing compares one server's version to another's. The two are free to differ, and a shared
//! crate would create exactly the coupling both manifests refuse.

/// Fold rows into an opaque version string.
///
/// # Derived, not counted
///
/// A monotonic counter bumped on every write would be simpler and would be wrong in the case that
/// matters: a mutation that changes nothing — a rename to the same title, a replayed `Duplicate` —
/// would bump it and every client would refetch identical data. A version computed from the rows
/// changes exactly when the rows do.
///
/// # Opaque, and the client is held to that
///
/// FNV-1a rendered as hex: not ordered, not a timestamp, not a row count. Decision 012 and the
/// cache runtime compare versions for **equality** and never for order, and picking a value nobody
/// could subtract is cheaper than documenting that nobody may.
///
/// Not a cryptographic digest: this defends against accidental staleness, not against a server
/// choosing to lie about its own state.
pub fn version_of<'a>(fields: impl Iterator<Item = &'a str>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for field in fields {
        for b in field.as_bytes() {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        // A separator that cannot occur in a field, so `["ab", "c"]` and `["a", "bc"]` are
        // different inputs rather than the same concatenation.
        hash ^= 0;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property that makes this usable as an invalidation signal at all.
    #[test]
    fn a_version_tracks_the_state_and_nothing_else() {
        let empty = version_of(std::iter::empty());
        let one = version_of(["t1", "Buy milk", "0", "u1"].into_iter());
        let done = version_of(["t1", "Buy milk", "1", "u1"].into_iter());

        assert_ne!(empty, one, "adding a todo changes the version");
        assert_ne!(one, done, "completing a todo changes the version");
        assert_eq!(
            one,
            version_of(["t1", "Buy milk", "0", "u1"].into_iter()),
            "identical state is identical version — this is what stops a no-op write from \
             invalidating every client's cache"
        );
    }

    /// The separator earns itself: without it these two states hash alike.
    #[test]
    fn field_boundaries_are_part_of_the_input() {
        assert_ne!(
            version_of(["ab", "c"].into_iter()),
            version_of(["a", "bc"].into_iter())
        );
    }
}
