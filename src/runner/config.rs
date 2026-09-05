//! The runner's constructor and its two knobs.
//!
//! Split from `super` for length only. `SyncRunner` is one type whose inherent methods are
//! implemented across two files; nothing here is reachable by a path that did not exist before.

use crate::store::OutboxStore;
use crate::transport::SyncTransport;

use super::{SyncRunner, DEFAULT_BATCH_LIMIT};

impl<S, T> SyncRunner<S, T>
where
    S: OutboxStore,
    T: SyncTransport,
{
    /// Build a runner with the default batch limit.
    pub fn new(store: S, transport: T) -> Self {
        Self {
            store,
            transport,
            batch_limit: DEFAULT_BATCH_LIMIT,
            retention_bound: None,
        }
    }

    /// Set how many records one pass sends.
    ///
    /// A limit of zero would send nothing forever, so it is clamped to one.
    #[must_use]
    pub fn with_batch_limit(mut self, limit: usize) -> Self {
        self.batch_limit = limit.max(1);
        self
    }

    /// Dead-letter a record once it has been sent and retained this many times.
    ///
    /// **There is no default.** Without a bound, retention is unbounded and a record the server
    /// never resolves stays queued forever — which is what D1 shipped and what
    /// `wiki/decisions/005-mutation-outcome-policy.decision.md` argued for at the time. Turning
    /// this on silently would move work out of the send path without the caller asking.
    ///
    /// # When a bound is not optional
    ///
    /// At `batch_limit = 1` the window *is* the head, so one permanently retained record freezes
    /// the whole queue rather than starving a window. Single-flight without a bound is a
    /// configuration with **no liveness argument**
    /// (`wiki/decisions/017-bounded-retention.decision.md`).
    ///
    /// # What happens at the bound
    ///
    /// The record becomes a dead letter carrying **no**
    /// [`RemoteRejection`](crate::protocol::RemoteRejection), because no server refused it —
    /// synthesising one would put words in the server's mouth. Its
    /// [`attempts`](crate::record::DeadLetterRecord::attempts) count is what makes the reason
    /// legible. The record survives for inspection and requeue; it is never discarded.
    ///
    /// A bound of zero would dead-letter on the first retention, so it is clamped to one.
    #[must_use]
    pub fn with_retention_bound(mut self, bound: u32) -> Self {
        self.retention_bound = Some(bound.max(1));
        self
    }

    /// The store this runner drives.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The transport this runner sends through.
    pub fn transport(&self) -> &T {
        &self.transport
    }
}
