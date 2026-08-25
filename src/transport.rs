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
