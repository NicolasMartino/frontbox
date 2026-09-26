//! Atomic application-owned storage format upgrades.

use crate::{DeadLetterRecord, Error, MutationId, OperationMeta, OutboxRecord, StoredRow};
use serde_json::Value;

/// The application-owned part of a mutation, in its new storage format.
///
/// Everything else stays in place: method, path, timestamps, trace, row binding,
/// attempts, last error, enqueue sequence, and the durable transport-started mark.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct MutationPayload {
    /// The new stored body, which may include local metadata interpreted by the transport.
    pub body: Value,
    /// The operation and format version that describe this body.
    pub op: Option<OperationMeta>,
    /// The guard associated with this body. The caller must preserve its meaning.
    pub precondition: Option<String>,
}

impl MutationPayload {
    /// Build a complete replacement for the application-owned fields.
    pub fn new(body: Value, op: Option<OperationMeta>, precondition: Option<String>) -> Self {
        Self {
            body,
            op,
            precondition,
        }
    }
}

/// A queued mutation's format upgrade, with an explicit idempotency decision.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PendingMigration {
    /// The upgraded application-owned fields.
    pub payload: MutationPayload,
    /// A fresh identifier when the outgoing request changes; otherwise the existing one survives.
    pub replacement_id: Option<MutationId>,
}

impl PendingMigration {
    /// Change local representation while preserving the request and its identifier.
    ///
    /// The caller must ensure the transport still sends the same request. For example,
    /// wrapping a body in a local envelope is safe when the transport unwraps it.
    /// Core cannot compare wire bodies: it deliberately does not interpret stored JSON.
    pub fn local(payload: MutationPayload) -> Self {
        Self {
            payload,
            replacement_id: None,
        }
    }

    /// Change the outgoing request, assigning a fresh idempotency key in the same queue slot.
    ///
    /// Derive the identifier deterministically from the old identifier and migration version.
    /// It must differ from every pending or rejected identifier in this scope, including
    /// identifiers replaced by this migration. A collision aborts the entire transaction.
    /// This also preserves the transport-started mark and attempt history conservatively.
    pub fn new_request(payload: MutationPayload, replacement_id: MutationId) -> Self {
        Self {
            payload,
            replacement_id: Some(replacement_id),
        }
    }
}

/// Pure, synchronous transformations of one scope's application-owned data.
///
/// Return `None` for current or unrelated records. Reject unknown versions with an error;
/// errors roll back the whole scope. Transformations must be deterministic and idempotent,
/// perform no external effects, and must not call the store recursively. They run inside
/// its transaction; awaiting arbitrary work would invalidate an IndexedDB transaction.
/// Quarantine and cache-version state are never passed to these callbacks or modified.
pub trait StorageMigration {
    /// Upgrade a queued mutation without changing its position or audit metadata.
    fn pending(&self, record: &OutboxRecord) -> Result<Option<PendingMigration>, Error>;

    /// Upgrade a rejected edit's local representation.
    ///
    /// Its original identifier, rejection time, reason and request audit metadata survive.
    /// This never requeues the edit. A later explicit retry needs a new mutation identifier.
    fn dead_letter(&self, record: &DeadLetterRecord) -> Result<Option<MutationPayload>, Error>;

    /// Upgrade a cached blob, preserving its row key and stale flag.
    fn row(&self, row: &StoredRow) -> Result<Option<Value>, Error>;
}

/// Counts of records upgraded in one committed transaction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct MigrationReport {
    /// Queued records upgraded in place.
    pub pending: usize,
    /// Rejected records upgraded in place.
    pub dead_letters: usize,
    /// Cached blobs upgraded in place.
    pub rows: usize,
}

/// A backend that can upgrade all application data in one scoped transaction.
#[allow(async_fn_in_trait)]
pub trait MigrationStore: super::OutboxStore {
    /// Claim the same exclusion as a drain, then migrate pending writes, rejected edits and rows.
    ///
    /// `None` means another drain or migration owns this scope; no callback runs and nothing
    /// changes. Otherwise all validation and writes share one transaction, serialized with
    /// other storage writers. Callback, decoding, identifier-collision or storage failures
    /// commit nothing. Undecodable records cause an error rather than being silently skipped;
    /// already quarantined records remain untouched. Cancellation before commit rolls back;
    /// cancellation during commit may leave either the complete old or complete new state.
    ///
    /// Backends must use [`super::DrainLease::claim`] for both in-realm and cross-realm
    /// exclusion. Applications must stop writers that still produce the old format before
    /// migrating: a transaction cannot prevent an old client from writing after it ends.
    async fn migrate(
        &self,
        migration: &impl StorageMigration,
    ) -> Result<Option<MigrationReport>, Error>;
}
