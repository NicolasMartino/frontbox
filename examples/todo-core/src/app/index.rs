//! What the pending-row index could not account for.
//!
//! Split out of the `app` module with `direct.rs`, back when it was a single file.
//! [`PendingIndexGap`] keeps its public path.
//!
//! # `row_id_of` used to live here
//!
//! It recovered which row a queued envelope was about by parsing the mutation's path — guesswork
//! dressed as a lookup, and it broke on any route it had not been taught. Decision 032 records the
//! answer at enqueue instead (`MutationIntent::with_row`), so the parse is gone and an envelope
//! that names no row now means a caller that did not bind one.

/// What a pending-index rebuild could not account for.
///
/// Neither field is an error, and that is the point. A rebuild that cannot see every queued record
/// still produces the best index it can; refusing to publish one would leave the UI showing an
/// older, *more* wrong answer, and failing the call would throw away a
/// [`DrainReport`](frontbox::DrainReport) for work the server already committed. So the gap is
/// reported alongside the index rather than instead of it.
///
/// It is also a small piece of evidence for register entry 12: the application has to discover both
/// numbers by scanning, because nothing in a report names the mutations that drained.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingIndexGap {
    /// Queued records the scan never reached, so their rows are missing from the index.
    pub beyond_scan_cap: usize,
    /// Queued envelopes whose todo row this application could not recover from the path or body.
    pub unrecoverable: usize,
}

impl PendingIndexGap {
    /// Whether the index accounts for every queued record.
    pub fn is_empty(&self) -> bool {
        self.beyond_scan_cap == 0 && self.unrecoverable == 0
    }
}
