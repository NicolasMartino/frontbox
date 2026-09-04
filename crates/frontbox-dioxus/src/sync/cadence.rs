//! How long to wait before draining again, per outcome.

use frontbox::DrainEnd;

/// How long to wait before draining again, per outcome.
///
/// A drain returns as soon as it stops draining, so this is the only thing deciding how hard a
/// client pushes. The defaults come from the source's own loop — five seconds normally, thirty on
/// error (`persistence/mutations.rs:731`) — with the two endings the source had no concept of given
/// their own values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyncCadence {
    /// Before the first drain. Zero drains immediately on mount.
    pub first_ms: u32,
    /// After a drain that emptied the queue.
    pub idle_ms: u32,
    /// After a drain that sent work and drained none of it.
    ///
    /// Longer than `idle_ms` on purpose. A stalled queue means the server is not resolving what it
    /// was given, and asking again sooner sends the identical request — and each round burns one
    /// attempt against any [retention bound](frontbox::SyncRunner::with_retention_bound), so this
    /// interval is what spreads those attempts over time rather than over milliseconds.
    pub stalled_ms: u32,
    /// After a drain that could not reach the network.
    pub offline_ms: u32,
    /// After a pass that failed.
    pub error_ms: u32,
}

impl Default for SyncCadence {
    fn default() -> Self {
        Self {
            first_ms: 0,
            idle_ms: 5_000,
            stalled_ms: 30_000,
            offline_ms: 15_000,
            error_ms: 30_000,
        }
    }
}

impl SyncCadence {
    /// How long to wait after a drain that ended this way.
    pub fn after(&self, ended: DrainEnd) -> u32 {
        match ended {
            DrainEnd::Drained => self.idle_ms,
            DrainEnd::Stalled => self.stalled_ms,
            DrainEnd::Offline => self.offline_ms,
            // Another drain holds the claim, so this one did nothing and there is nothing to back
            // off from.
            DrainEnd::AlreadyRunning => self.idle_ms,
            // `DrainEnd` is `#[non_exhaustive]`, so a future variant lands here. Backing off is the
            // safe reading of an ending this build does not recognise: whatever it means, this
            // client cannot act on it, and pushing harder on something it does not understand is
            // the one response that can make things worse.
            _ => self.stalled_ms,
        }
    }
}
