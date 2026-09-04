//! The direct-dispatch outcome, and the classification core does not do for you.
//!
//! Split out of the `app` module when it passed the four-hundred-line mark and became a
//! directory. Nothing here moved in the public path: [`Direct`] is still `todo_core::app::Direct`
//! and `todo_core::Direct`.

use frontbox::{Error, RemoteRejection};

/// How a direct write ended, and whether the queue should pick it up.
///
/// # The classification core does not do for you
///
/// `wiki/proposals/extraction-boundary.proposal.md` keeps direct-dispatch-then-fallback out of core
/// on the bet that a caller-supplied [`MutationId`](frontbox::MutationId) is enough. The bet holds
/// — the id is generated once and reused, so a direct write and its queued replay are the same
/// mutation to the server. What the bet does not cover is this enum: deciding *whether* to fall
/// back means deciding whether the failure was terminal, and that is
/// `wiki/decisions/019-verdict-synthesis.decision.md`'s problem sitting in an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direct {
    /// The server took it. Nothing is queued.
    Applied,
    /// The server refused it terminally. Queueing it would replay a refusal forever.
    Refused(RemoteRejection),
    /// Nothing was decided. Queued under the same id, so the resend is not a second write.
    Queued,
}

/// What [`classify`] made of a direct attempt, before the application decides what to do about it.
pub(super) enum Verdict {
    Applied,
    Refused(RemoteRejection),
    Undecided,
}

/// Decide whether a failed direct write should fall back to the queue.
///
/// **This function is the finding.** It is short, it is obvious, and every application attempting
/// direct dispatch has to write it — differently. It is the terminal-versus-retryable split
/// `wiki/decisions/019-verdict-synthesis.decision.md` describes for a transport, relocated into an
/// application, and the rule it encodes is the same one 019 insists on: `4xx` is not automatically
/// terminal, because `401` and `429` are both retryable.
pub(super) async fn classify(attempt: Result<reqwest::Response, Error>) -> Verdict {
    let Ok(response) = attempt else {
        // Never reached the server, or the answer never came back. Undecided is the only safe
        // reading: the write may have landed, which is exactly why the queued replay reuses the id.
        return Verdict::Undecided;
    };
    let status = response.status();
    if status.is_success() {
        return Verdict::Applied;
    }
    if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        return Verdict::Undecided;
    }
    Verdict::Refused(
        response
            .json::<RemoteRejection>()
            .await
            .unwrap_or_else(|_| RemoteRejection::new(format!("server answered {status}"))),
    )
}
