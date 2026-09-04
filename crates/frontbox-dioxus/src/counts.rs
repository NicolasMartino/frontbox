//! The three numbers a UI shows about a queue.

use frontbox::{DeadLetterStore, Error, OutboxStore, QuarantineStore};

/// How much work is sitting in each of the three stores, for one scope.
///
/// Read together and refreshed together, because they only mean something as a set: a pending
/// count that drops while dead letters rise is a queue draining into refusals, and either number
/// alone reads as progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct OutboxCounts {
    /// Decodable records waiting to be sent.
    pub pending: usize,
    /// Records that will not be sent again.
    pub dead_letters: usize,
    /// Rows that could not be decoded.
    pub quarantined: usize,
}

impl OutboxCounts {
    /// Read all three from one store.
    ///
    /// Not a snapshot: the three reads are separate, so a mutation enqueued between them is visible
    /// to one and not another. That is the same looseness a UI has anyway — it renders after the
    /// fact — and tightening it would mean a transaction spanning three traits for a progress
    /// indicator.
    ///
    /// # Errors
    ///
    /// A storage failure from any of the three reads.
    pub async fn read<S>(store: &S) -> Result<Self, Error>
    where
        S: OutboxStore + DeadLetterStore + QuarantineStore,
    {
        Ok(Self {
            pending: store.pending_count().await?,
            dead_letters: DeadLetterStore::count(store).await?,
            quarantined: QuarantineStore::count(store).await?,
        })
    }

    /// Whether anything at all is held for this scope.
    pub fn is_empty(&self) -> bool {
        self.pending == 0 && self.dead_letters == 0 && self.quarantined == 0
    }
}
