//! What the invalidation runner hands back.

/// What applying invalidations changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InvalidationReport<K> {
    /// Entities the server is ahead on. A refetch is now owed for each.
    pub marked_stale: Vec<K>,
    /// Entities whose local version was reset to zero because the server was behind.
    ///
    /// These are also stale — a reset means refetching from nothing — but they are listed
    /// separately because a reset is a server-side anomaly worth surfacing, not routine
    /// invalidation.
    pub needs_reset: Vec<K>,
    /// Entities already in step with the server.
    pub unchanged: Vec<K>,
    /// Wire names the registry does not model, in the server's spelling.
    ///
    /// Not an error and not a discard. A server that adds an entity type starts naming it to
    /// clients built before it existed, so failing here would break every client during a rolling
    /// deploy. Nothing is marked stale for these — the application does not hold the data — but a
    /// caller that wants to alert on a registry falling behind the server has the fact
    /// (`wiki/decisions/013-unknown-entity-name.decision.md`).
    pub unknown: Vec<String>,
}

impl<K> Default for InvalidationReport<K> {
    fn default() -> Self {
        Self {
            marked_stale: Vec::new(),
            needs_reset: Vec::new(),
            unchanged: Vec::new(),
            unknown: Vec::new(),
        }
    }
}

impl<K> InvalidationReport<K> {
    /// Whether anything now owes a refetch.
    pub fn any_stale(&self) -> bool {
        !self.marked_stale.is_empty() || !self.needs_reset.is_empty()
    }
}

/// Whether refetching an entity would discard unsent local work.
///
/// Reported rather than enforced. Core does not perform the refetch — the endpoints and the read
/// model belong to the application — so it cannot gate one, and a hard gate would be the wrong
/// answer anyway: one permanently retained record would freeze every entity's cache forever, which
/// is the failure `wiki/decisions/005-mutation-outcome-policy.decision.md` spent its liveness
/// argument avoiding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PendingConflict {
    /// The outbox holds no unsent work. Replacing local state discards nothing.
    None,
    /// Unsent work the classifier attributed to this entity.
    ///
    /// Refetching now would overwrite a local projection the server has not accepted yet.
    ForEntity {
        /// How many pending records were attributed to it.
        pending: usize,
    },
    /// The outbox holds unsent work that was not attributed to any entity.
    ///
    /// Either no classifier was supplied, or there was more pending work than the scan would read.
    /// Deliberately conservative: this says "something is queued and it might be this", because a
    /// false "nothing is queued" is the answer that loses data.
    Unattributed {
        /// How many records are pending across the whole outbox.
        pending: usize,
    },
}

impl PendingConflict {
    /// Whether a refetch might discard something.
    ///
    /// True for everything except [`None`](PendingConflict::None) — including
    /// [`Unattributed`](PendingConflict::Unattributed), because an unattributed conflict is
    /// unproven, not absent.
    pub fn is_possible(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A stale entity and what refetching it would cost.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StaleEntity<K> {
    /// Which entity.
    pub key: K,
    /// The server version this client has been told about.
    pub version: u64,
    /// Whether refetching would discard unsent local work.
    pub conflict: PendingConflict,
}

/// Classification of the outbox by entity, as one caller-supplied function saw it.
///
/// Produced internally by the classified query. Exposed so that a caller can reuse the same
/// attribution for its own reporting without scanning the outbox twice.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClassifiedConflict<K> {
    /// Entity and how many pending records were attributed to it.
    pub attributed: Vec<(K, usize)>,
    /// Pending records the classifier returned `None` for.
    pub unattributed: usize,
    /// Whether the scan saw the whole queue.
    ///
    /// False when there was more pending work than the scan limit, in which case every entity's
    /// conflict degrades to [`Unattributed`](PendingConflict::Unattributed).
    pub complete: bool,
}
