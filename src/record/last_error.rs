//! The bound on a stored `last_error`, and the one rule for applying it.
//!
//! A file of its own because the constant and the function are a pair, and separating them is how
//! three backends came to hold three copies of the walk-back loop. Whatever cites the bound now
//! finds the rule next to it.

/// The longest `last_error` a store may keep.
///
/// Core states the bound; the store truncates to it, and a conformance case checks that it did —
/// the shape decisions 024, 025 and 031 all use. Without a bound, a server that answers with a
/// stack trace grows a durable row every time it is retried
/// (`wiki/decisions/033-last-error-on-the-record.decision.md`).
pub const LAST_ERROR_MAX: usize = 512;

/// Cut a reason down to [`LAST_ERROR_MAX`], on a character boundary.
///
/// Bytes rather than characters is the bound core states, and slicing a `String` at a byte index
/// mid-character panics — so the cut walks back to the nearest boundary rather than trusting the
/// number. A backend that got this wrong would panic only on a multi-byte reason at exactly the
/// wrong length, which is the kind of bug that ships.
///
/// # Why core exports this rather than stating it
///
/// It was stated, and three backends then wrote it three times — byte-identical, which is the
/// coincidence that hides a divergence rather than the proof there is none. `rfc3339.rs` makes the
/// argument for the general case: *two implementations of one rule is exactly the divergence the
/// conformance suite exists to prevent.* The suite can only catch a divergence it has a case for,
/// and a fourth backend written from the prose is free to truncate at the byte index and panic on
/// the input that reaches it.
///
/// So the bound and the rule ship together. A backend calls this; nothing has to re-derive the
/// walk-back. Where a rule is genuinely a *policy* — which scope encoding, which quarantine table
/// — core states it and the backend chooses, because there the answers legitimately differ.
/// Truncation has one right answer.
#[must_use]
pub fn truncate_error(reason: &str) -> String {
    if reason.len() <= LAST_ERROR_MAX {
        return reason.to_owned();
    }
    let mut end = LAST_ERROR_MAX;
    while end > 0 && !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_owned()
}
