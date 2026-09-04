//! Transport error classification.

use frontbox::Error;

/// Decide whether a failed send means "offline" or "the attempt failed".
///
/// The distinction is not cosmetic. [`Error::Offline`] produces `SyncPass::Offline` — a clean `Ok`
/// with the queue untouched — while any other error propagates as `Err` and *aborts a drain*,
/// discarding the aggregate report for every pass that already succeeded.
///
/// # The two platforms name the same condition differently
///
/// Native `reqwest` has `is_connect`: a connection that was never established. Its wasm backend
/// does not — `is_connect` is `#[cfg(not(target_arch = "wasm32"))]`, absent from the type rather
/// than merely unhelpful, because a browser reports every network-layer `fetch` failure as one
/// opaque `TypeError` on purpose, so that a page cannot probe the user's network topology by
/// timing the difference.
///
/// What survives on both is the *layer* the failure came from. In reqwest's wasm client the fetch
/// itself yields `Kind::Request` while URL and header construction yield `Kind::Builder`, so
/// `is_request` names the same condition on the web that `is_connect` names natively: the request
/// left the builder intact and the network refused it.
///
/// Mapping every wasm send failure to `Offline` was the first thing tried here and it is wrong. It
/// cannot tell a wrong endpoint from an unreachable one, so a typo in a base URL would present as
/// a permanent, silent offline state — the queue retained forever, no error ever surfaced, and the
/// one condition a developer most needs to see rendered as the one condition the UI is designed to
/// treat as normal.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn map_send_error(source: reqwest::Error) -> Error {
    if source.is_connect() || source.is_timeout() {
        Error::Offline
    } else {
        Error::transport(source)
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn map_send_error(source: reqwest::Error) -> Error {
    // `is_request` rather than `is_connect`; see the native arm for why they name one condition.
    if source.is_request() || source.is_timeout() {
        Error::Offline
    } else {
        Error::transport(source)
    }
}
