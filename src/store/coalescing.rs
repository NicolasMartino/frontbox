//! The opt-in coalescing enqueue's policy, outcome, and refusal types.
//!
//! Split from `super` for length; the public paths are unchanged, so `crate::store::CoalescingPolicy`
//! is where it has always been.
//!
//! The design these express is argued in `wiki/proposals/queued-write-coalescing.proposal.md`. The
//! short version: a queued write may have its body replaced only while frontbox can still prove the
//! transport has never seen it, and that proof is a durable fact written *before* the request rather
//! than inferred afterwards from how the request failed.

use crate::id::MutationId;

/// What [`enqueue_coalescing`](super::OutboxStore::enqueue_coalescing) should do when it cannot
/// replace.
///
/// # Choosing between the two
///
/// The question is not "how strict do I want to be". It is **can this caller compute a precondition
/// valid for a freshly appended write?** If yes, appending is always available and
/// `AppendIfMissing` is right. If no, appending would mean guessing a guard, and `RequireExisting`
/// exists so the caller is told rather than guessed for.
///
/// | | `AppendIfMissing` | `RequireExisting` |
/// | --- | --- | --- |
/// | One safe match | replaces | replaces |
/// | No row binding, no match, several matches, or a started match | **appends** | [`NotQueued`](CoalescingEnqueue::NotQueued) |
/// | Can return `NotQueued`? | never | yes, and that is its purpose |
/// | The caller must be able to | supply a precondition valid now | handle a refusal without retrying through [`enqueue`](super::OutboxStore::enqueue) |
///
/// Both discard the queued body on a replacement, so both carry the same full-row-state obligation
/// — see [`enqueue_coalescing`](super::OutboxStore::enqueue_coalescing).
///
/// `#[non_exhaustive]`: a future policy is additive, so implementors match with a wildcard. A
/// backend that meets an unrecognised policy should return [`Error::Protocol`](crate::Error::Protocol)
/// and commit nothing, rather than guess — the same rule as [`Disposition`](super::Disposition), and
/// for a sharper reason. The two policies here are opposite instructions about the caller's write,
/// so a wrong guess either drops a write the caller expected to be queued or queues one it expected
/// to be refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoalescingPolicy {
    /// Replace the one safe match, or queue the intent as an ordinary enqueue.
    ///
    /// **This policy never returns [`CoalescingEnqueue::NotQueued`].** Every case that is not a
    /// replacement — no [`RowRef`](crate::record::RowRef) on the intent, no match, several matches,
    /// or a match the transport may already have seen — appends instead.
    ///
    /// That is not leniency, it is the only safe reading of what the caller said. A caller reaching
    /// for this policy has stated it holds a precondition valid right now; appending with it is
    /// exactly what [`enqueue`](super::OutboxStore::enqueue) would have done, and is never worse.
    /// Returning "not queued" for, say, a forgotten
    /// [`with_row`](crate::record::MutationIntent::with_row) would hand back an `Ok` for a write
    /// that was silently dropped — the failure
    /// `wiki/decisions/006-corrupt-record-policy.decision.md` exists to refuse.
    AppendIfMissing,
    /// Replace the one safe match, or queue nothing at all.
    ///
    /// The path for a caller that **cannot** compute a valid precondition for a second write —
    /// because the row's hash covers a server-assigned field, so the state write 1 will produce is
    /// unknowable until write 1 drains. Appending would mean guessing, and a wrong precondition
    /// either fails a write that should have succeeded or, if it is omitted entirely, silently
    /// disables the conflict detection it existed to provide.
    ///
    /// A [`NotQueued`](CoalescingEnqueue::NotQueued) here means the caller must fall back to its own
    /// refresh-or-refuse behaviour. It must **not** retry through `enqueue` with a guessed
    /// precondition.
    RequireExisting,
}

/// What [`enqueue_coalescing`](super::OutboxStore::enqueue_coalescing) did.
///
/// `#[non_exhaustive]`: a future outcome is additive.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoalescingEnqueue {
    /// No replacement was made and the intent was queued as a new record.
    Appended {
        /// The queued record's identifier — the one the caller supplied.
        mutation_id: MutationId,
    },
    /// One queued record's body was replaced in place.
    Replaced {
        /// The queued record that survived, keeping its position, identifier, and precondition.
        kept: MutationId,
        /// The identifier the caller minted for this intent, which was **never queued**.
        ///
        /// It will therefore appear in no [`SyncReport`](crate::runner::SyncReport) and no
        /// [`Drained`](crate::runner::Drained). An application correlating its own writes against
        /// drain reports has to map this id onto `kept`.
        discarded: MutationId,
    },
    /// Nothing was written. Only [`CoalescingPolicy::RequireExisting`] produces this.
    NotQueued {
        /// The identifier the caller minted, which was not queued.
        mutation_id: MutationId,
        /// Why nothing was written.
        reason: CoalescingRefusal,
    },
}

/// Why [`CoalescingPolicy::RequireExisting`] queued nothing.
///
/// `#[non_exhaustive]`: a future refusal is additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoalescingRefusal {
    /// The intent carried no [`RowRef`](crate::record::RowRef), so there was nothing to match on.
    ///
    /// Coalescing is row-scoped by construction: without a row binding there is no way to say which
    /// queued record this intent is a newer version *of*.
    Unbound,
    /// No pending record in this scope matches the intent's row, method, and path.
    MissingMatch,
    /// More than one pending record matches.
    ///
    /// Refused rather than resolved. Picking the newest would be defensible and picking the oldest
    /// would not, and a store that guesses here is deciding which of the caller's writes to discard
    /// on the caller's behalf.
    AmbiguousMatch,
    /// The one match is transport-started, so the server may already hold its identifier.
    ///
    /// Set either by [`read_for_send`](super::OutboxStore::read_for_send), which marks what it hands
    /// to the transport, or by [`apply_outcomes`](super::OutboxStore::apply_outcomes) applying a
    /// [`Disposition::Retain`](super::Disposition::Retain) — a verdict cannot exist without a
    /// request, so a retained record has been seen whether or not this process is what sent it.
    ///
    /// Replacing its body would mean sending new content under an id the server can dedupe against,
    /// which deletes the new body without ever applying it.
    TransportStarted,
}
