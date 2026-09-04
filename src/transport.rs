//! The seam between this crate and the network.

use crate::error::Error;
use crate::protocol::{MutationBatchRequest, MutationBatchResponse};

/// Sends a batch of mutations to a server.
///
/// # Authentication is not part of this trait
///
/// There is no bearer-token type, header map, cookie model, or credential snapshot here, and no
/// record in this crate carries auth data. Implementors close over whatever mechanism their
/// application uses, but they must evaluate it **per send**: a queued mutation may sit in the
/// outbox for days, and capturing one token at construction time is not equivalent to consulting a
/// token provider immediately before sending
/// (`wiki/decisions/004-transport-auth-and-offline.decision.md`).
///
/// A record that carries no credential cannot leak one, and token refresh works without any change
/// to this API.
///
/// # Offline is not a transport failure
///
/// Return [`Error::Offline`] when no request could reasonably complete — the network is
/// unreachable, DNS fails, a browser `fetch` fails for lack of connectivity. Return
/// [`Error::Transport`] when a request was attempted and failed, and [`Error::Protocol`] when the
/// server answered with something the protocol does not allow.
///
/// The runner treats these differently: offline keeps work queued without counting a failed
/// attempt, because being offline is an expected operating mode for this crate rather than an
/// error. Collapsing the two produces misleading status and retry behavior.
///
/// Missing credentials are neither. An application may decline to sync without them, or its
/// transport may return a typed failure, but this crate does not invent credentials and does not
/// treat their absence as network loss.
///
/// # Partial and surplus responses
///
/// A response need not carry one result per mutation sent, and an implementor should not pad,
/// reorder, or invent results to make it look like it does. Pass through exactly what the server
/// said. The runner handles the three cases:
///
/// - **A mutation with no result** stays queued and is counted in
///   [`SyncReport::retained`](crate::runner::SyncReport::retained). Silence is not a verdict, and
///   reading it as one would either drop work or invent a refusal.
/// - **A result for a mutation this batch did not contain** is reported in
///   [`SyncReport::anomalies`](crate::runner::SyncReport::anomalies) and changes nothing. Guessing
///   which record the server meant risks mutating an unrelated one.
/// - **More than one result for the same mutation** makes *every* one of them an anomaly, not just
///   the later ones. The verdicts may disagree and there is no basis for preferring either, so none
///   is applied and the record stays queued for the next pass. Applying the first and reporting the
///   rest would make arrival order the tie-break, which is the preference this crate is declining to
///   make; leaving the record queued costs one resend, and `mutation_id` is the idempotency key.
/// - **A status this crate does not recognise** deserializes to
///   [`MutationStatus::Unknown`](crate::protocol::MutationStatus::Unknown) carrying the server's own
///   spelling. The record is retained and the pass reports an
///   [`AnomalyKind::UnknownStatus`](crate::runner::AnomalyKind::UnknownStatus). A transport does not
///   need to filter these out: every other verdict in the same response is still applied, which is
///   the point — one unrecognised word must not discard the batch.
///
/// Results may arrive in any order; the runner matches on `mutation_id`, not position.
///
/// # Headers the record carries
///
/// Two fields on [`MutationIntent`](crate::record::MutationIntent) are stored at enqueue, replayed
/// unchanged on every send, and never interpreted by core, because both are headers rather than
/// payload: `traceparent` (decision 022) and `precondition` (decision 026). A transport that uses
/// them reads them off the intent and sets whatever headers they mean; one that does not is
/// unaffected, because both are `None` unless a caller supplied them.
///
/// The precondition is opaque on purpose. A `PUT` does not reveal whether a mutation creates or
/// updates, so core cannot know whether the value is a base hash or a wildcard, and does not try.
///
/// **Dropping the precondition fails silently.** The write succeeds, a concurrent edit is
/// clobbered, and nothing anywhere reports it — unlike a dropped `traceparent`, which costs only
/// diagnostics. A transport that cannot attach the header should refuse to send and let the record
/// dead-letter, which turns a lost update into something a human can see.
///
/// # Synthesizing a verdict the server did not author
///
/// Everything above assumes a server that returns a [`MutationBatchResponse`] — a status per
/// mutation, decided by something that knows what it did. Not every deployment has one. Against a
/// per-resource `PUT` API behind a gateway that only routes, **the transport authors the verdicts**,
/// and the judgment moves from the server, which knew whether it had evaluated a mutation, to the
/// client, which sees a status code.
///
/// Core does not classify HTTP responses and will not, for the reason it does not interpret a body
/// (decision 008) or attribute mutations to entities without a caller-supplied classifier
/// (decision 014): a `409` is a version conflict on one service and a uniqueness violation on
/// another, a `422` is terminal for a malformed body and transient for a reference that has not
/// replicated yet, and a default here would be policy a caller could not opt out of.
///
/// A transport that synthesizes therefore owes three things
/// (`wiki/decisions/019-verdict-synthesis.decision.md`):
///
/// - **[`Rejected`](crate::protocol::MutationStatus::Rejected) means the server terminally refused
///   *this mutation on its merits*.** It is not a status class. Do not map 4xx onto it as a rule.
///   The cost of getting this wrong is asymmetric: a wrongly-terminal verdict dead-letters work
///   that would have succeeded, and no retry ever revisits it.
/// - **A response you cannot confidently classify is
///   [`Unknown`](crate::protocol::MutationStatus::Unknown), carrying what the server actually
///   said** — not `Rejected`, not `Applied`. The record is retained, the pass reports it with the
///   server's own spelling, and a human can see the vocabulary that defeated the mapping.
/// - **A missing prerequisite is transient.** A `404` or `409` produced because a parent resource
///   has not been created yet resolves when the predecessor lands. Dead-lettering it discards valid
///   work for a reason that is about to stop being true, and at `batch_limit = 1` the predecessor
///   is often the very next record in the queue.
///
/// ## Mapping a structured terminality signal
///
/// A server that says which of these applies makes the obligation a lookup rather than a judgment,
/// and it is worth asking for. Where an error body carries a field distinguishing terminal from
/// transient — RepForge's services carry `retry`, valued `terminal`, `transient`, or `conflict` —
/// the mapping is:
///
/// | Signal | Status | Disposition | Why |
/// | --- | --- | --- | --- |
/// | `terminal` | `Rejected` | `DeadLetter` | Refused on the merits; it will be refused again unchanged |
/// | `transient` | a retaining status — see below | `Retain` | The condition stops being true without the user acting |
/// | `conflict` | `Rejected`, with the reason in [`RemoteRejection::code`](crate::protocol::RemoteRejection::code) | `DeadLetter` | Terminal for this attempt, but resolvable by a human rather than by a retry |
///
/// Two properties of that arrangement are worth relying on. **An unrecognised signal value must
/// degrade to terminal and round-trip verbatim**, so a client compiled against an older contract
/// keeps the error rather than failing the parse and does not retry on a word it cannot reason
/// about. And **the status line survives as a documented fallback**: a gateway routing failure or a
/// transport error never reaches a service and so carries no error body at all, which makes
/// body-absence the discriminator between "a service ruled on this" and "this never got there".
/// Satisfy yourself that the discriminator holds in your deployment — a proxy that returns a
/// parseable error body in front of the gateway would break it, and a rate limiter read as a
/// service verdict dead-letters work that would have succeeded.
///
/// ## The vocabulary has no word for "transient", and that is deliberate rather than settled
///
/// `MutationStatus` offers three statuses whose disposition is `Retain`, and none of them means
/// exactly "retry this later":
///
/// - [`Blocked`](crate::protocol::MutationStatus::Blocked) is the closest fit for a **missing
///   prerequisite**, since it already means an earlier *ordered* mutation did not land. It says
///   nothing about a dependency being down or a lock being held elsewhere.
/// - [`Pending`](crate::protocol::MutationStatus::Pending) is wrong for every transient case: it
///   asserts the server **accepted** the mutation and has not finished, and a transient refusal is
///   not an acceptance.
/// - [`Unknown`](crate::protocol::MutationStatus::Unknown) is honest and carries the server's own
///   word, at the cost of reporting a routine, expected condition through the anomaly channel that
///   exists to surface vocabulary mismatches.
///
/// **The binding obligation is the disposition, not the spelling: a transient condition must
/// retain.** Which of the three carries it is a diagnostic choice, and a transport should pick the
/// one whose report reads truthfully to whoever is on call. If this proves to matter in practice,
/// the fix is a variant that means what it says rather than a stretched existing one — which is a
/// public API change and is not made here.
///
/// ## What the conformance suite does not cover
///
/// None of this. Every conformance case drives an in-memory transport that returns whatever the
/// case hands it, so the suite tests what the *runner* does with a verdict and never how a real
/// adapter arrived at one. This section is enforced by review, which is weaker than the standard
/// the rest of the crate holds itself to, and saying so is better than letting it be discovered.
#[allow(async_fn_in_trait)]
// The lint fires because callers cannot add a `Send` bound to the returned future. That is the
// intended property, not a defect: `wiki/decisions/001-single-threaded-core.decision.md` targets
// single-threaded frontend runtimes, where IndexedDB futures are `!Send` and cannot be made `Send`
// by wrapping. Adding the bound would exclude the primary target.
pub trait SyncTransport {
    /// Push a batch and return the server's verdicts.
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error>;
}
