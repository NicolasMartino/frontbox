//! What the invalidation runner hands back.

use crate::cache::CacheVersion;

/// What applying invalidations changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InvalidationReport<K> {
    /// Entities whose server identity differs from the local one. A refetch is now owed for each.
    ///
    /// D2 reported a second list, `needs_reset`, for entities where the server's version was
    /// *lower* than the local one. It went with the ordering that produced it
    /// (`wiki/decisions/021-cache-version-identity.decision.md`): a differing identity is a
    /// differing identity, and there is nothing for a separate list to mean.
    pub marked_stale: Vec<K>,
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
            unchanged: Vec::new(),
            unknown: Vec::new(),
        }
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
    /// The server version this client has been told about, or [`None`] if it never has been.
    ///
    /// An entity can be stale with no version. [`InvalidationRunner::mark_all_stale`] flags
    /// everything the registry models, including entities no invalidation has ever named.
    ///
    /// [`InvalidationRunner::mark_all_stale`]: crate::cache::InvalidationRunner::mark_all_stale
    pub version: Option<CacheVersion>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// **`Unattributed` is possible, not absent.** This is the whole point of the predicate.
    ///
    /// A conflict core could not attribute is unproven rather than disproven, and a false "nothing
    /// is queued" is the answer that loses data — so `is_possible` is true for it, exactly as it is
    /// for a conflict named against a specific entity. Only `None` means the outbox is genuinely
    /// empty (`wiki/decisions/014-pull-gating.decision.md`).
    #[test]
    fn only_an_empty_outbox_reads_as_no_possible_conflict() {
        assert!(!PendingConflict::None.is_possible());

        for possible in [
            PendingConflict::ForEntity { pending: 1 },
            PendingConflict::Unattributed { pending: 1 },
            PendingConflict::Unattributed { pending: 250 },
        ] {
            assert!(
                possible.is_possible(),
                "{possible:?} might discard unsent work, and must not read as safe"
            );
        }
    }

    /// A classified scan reports what it saw *and* whether it saw everything.
    ///
    /// `complete` is not a detail: an incomplete scan degrades every entity to `Unattributed`
    /// rather than reporting a clean partial view, so a consumer that ignored this field would
    /// turn a capped read into a false all-clear.
    #[test]
    fn a_classification_says_whether_it_saw_the_whole_queue() {
        let whole = ClassifiedConflict {
            attributed: vec![("exercises", 2)],
            unattributed: 0,
            complete: true,
        };
        assert!(whole.complete);
        assert_eq!(whole.attributed, vec![("exercises", 2)]);

        let capped = ClassifiedConflict {
            attributed: Vec::<(&'static str, usize)>::new(),
            unattributed: 7,
            complete: false,
        };
        assert!(!capped.complete);
        assert_eq!(
            capped.unattributed, 7,
            "an incomplete scan reports the whole queue as unattributed rather than claiming any \
             entity is clean"
        );
    }

    /// A stale entity can have no version at all, and the type has to admit it.
    ///
    /// `mark_all_stale` flags every registered entity, including ones no invalidation has ever
    /// named — so `(None, stale)` is reached by the most ordinary recovery path there is, not by a
    /// corner case (`wiki/decisions/021-cache-version-identity.decision.md`).
    #[test]
    fn a_stale_entity_may_carry_no_version() {
        let never_heard = StaleEntity {
            key: "exercises",
            version: None,
            conflict: PendingConflict::None,
        };
        assert_eq!(never_heard.version, None);
        assert!(!never_heard.conflict.is_possible());

        let known = StaleEntity {
            key: "sessions",
            version: Some(CacheVersion::new("a3f8")),
            conflict: PendingConflict::ForEntity { pending: 1 },
        };
        assert_eq!(
            known.version.as_ref().map(CacheVersion::as_str),
            Some("a3f8")
        );
        assert!(known.conflict.is_possible());
    }
}
