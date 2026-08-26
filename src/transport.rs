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
