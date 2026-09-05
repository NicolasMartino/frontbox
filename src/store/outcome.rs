//! The vocabulary `apply_outcomes` speaks: what should happen to a pending record, and to which.
//!
//! Split from `super` for length. The seam is a real one rather than a cut at a line number —
//! everything next door is a trait a backend implements, and these two are the values those traits
//! move. Public paths are unchanged.

use crate::id::MutationId;
use crate::record::DeadLetterReason;

/// What should happen to one pending record.
///
/// `#[non_exhaustive]`: a future disposition is additive, so implementors match with a wildcard.
/// A backend that meets an unrecognised disposition should return
/// [`Error::Protocol`](crate::Error::Protocol) and commit nothing, rather than guess.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Disposition {
    /// Remove it. The server applied it, or had already seen it.
    Delete,
    /// Move it to the dead-letter store. It will not be sent again.
    DeadLetter {
        /// Why, so a reader does not have to infer it from what is absent.
        ///
        /// Core produces two of the three kinds — a server verdict and its own retention bound.
        /// The third exists so a caller parking a record for a reason of its own can say what it
        /// was, rather than being forced into a silence indistinguishable from the bound
        /// (`wiki/decisions/027-dead-letter-reason.decision.md`).
        reason: DeadLetterReason,
    },
    /// Leave it queued, with one more attempt against it.
    ///
    /// Not a no-op: the store increments `attempts` and records `reason`, which is what makes
    /// decision 017's bound reachable and decision 033's diagnosis possible.
    ///
    /// It also marks the record **transport-started**, which is what
    /// [`enqueue_coalescing`](super::OutboxStore::enqueue_coalescing) tests before it rewrites a
    /// queued body. A `Retain` *is* a verdict, and a verdict cannot exist without a request, so by
    /// the time one is applied the server has seen the record — whether or not this process read it
    /// through [`read_for_send`](super::OutboxStore::read_for_send).
    ///
    /// This is the only disposition that needs to say so, because every other one removes the
    /// record from the outbox and there is then nothing left to coalesce into.
    Retain {
        /// Why this verdict left the record queued, in the server's own words.
        ///
        /// Truncated by the store to [`LAST_ERROR_MAX`](crate::record::LAST_ERROR_MAX) and never
        /// parsed. `None` when the caller has nothing to say — an offline pass produces no verdict
        /// at all and so never reaches here.
        reason: Option<String>,
    },
    /// Move it to quarantine. The record is identifiable but unusable.
    Quarantine {
        /// What made the record unusable.
        reason: String,
    },
}

/// One record's disposition, addressed by id.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Outcome {
    /// Which record this is about.
    pub id: MutationId,
    /// What should happen to it.
    pub disposition: Disposition,
}

impl Outcome {
    /// Pair a record with its disposition.
    pub fn new(id: MutationId, disposition: Disposition) -> Self {
        Self { id, disposition }
    }
}
