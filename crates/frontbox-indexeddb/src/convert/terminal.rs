//! The stored shapes for everything that is not a queued mutation.
//!
//! Split from `super` when the outbox row grew its `transport_started` field and the file passed the
//! four-hundred-line cap `AGENTS.md` sets. The seam is the one the store traits already draw: the
//! outbox next door, and the two terminal stores plus the read model here.
//!
//! **`super`'s rule against `#[serde(default)]` holds in this file without exception.** Nothing here
//! has gained a field, so a missing key can still only mean a schema this crate never shipped.

use frontbox::{
    DeadLetterReason, DeadLetterRecord, MutationId, MutationIntent, OperationMeta, OutboxRecord,
    QuarantinedRecord, RowRef, ScopeKey, StoredRow,
};
use serde::{Deserialize, Serialize};

use super::i64_text;

/// One dead letter, as IndexedDB holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeadLetterRow {
    pub scope: String,
    pub mutation_id: String,
    pub method: String,
    pub path: String,
    pub raw_body: String,
    #[serde(with = "i64_text")]
    pub created_at: i64,
    pub op_name: Option<String>,
    pub op_version: Option<String>,
    pub traceparent: Option<String>,
    pub precondition: Option<String>,
    pub attempts: u32,
    #[serde(with = "i64_text")]
    pub rejected_at: i64,
    pub reason_kind: String,
    pub reason_body: Option<String>,
}

impl DeadLetterRow {
    pub fn from_record(record: &DeadLetterRecord) -> Result<Self, frontbox::Error> {
        let (kind, body) = match &record.reason {
            DeadLetterReason::Rejected { error } => (
                "rejected",
                error
                    .as_ref()
                    .and_then(|error| serde_json::to_string(error).ok()),
            ),
            DeadLetterReason::RetentionBound => ("retention_bound", None),
            DeadLetterReason::Caller(reason) => ("caller", Some(reason.clone())),
            // `#[non_exhaustive]`: a variant added later is stored under a discriminant saying so,
            // rather than flattened into one this backend happens to know.
            other => ("unknown", Some(format!("{other:?}"))),
        };
        Ok(Self {
            scope: record.scope.as_str().to_owned(),
            mutation_id: record.mutation_id.to_string(),
            method: record.method.clone(),
            path: record.path.clone(),
            raw_body: serde_json::to_string(&record.body)
                .map_err(frontbox::Error::serialization)?,
            created_at: record.created_at,
            op_name: record.op.as_ref().map(|op| op.name.clone()),
            op_version: record.op.as_ref().and_then(|op| op.version.clone()),
            traceparent: record.traceparent.clone(),
            precondition: record.precondition.clone(),
            attempts: record.attempts,
            rejected_at: record.rejected_at,
            reason_kind: kind.to_owned(),
            reason_body: body,
        })
    }

    /// Rebuild the record, through `stamp` and `from_record` because decision 008 blocks literals.
    pub fn decode(&self, scope: &ScopeKey) -> Option<DeadLetterRecord> {
        let mut intent = MutationIntent::new(
            self.mutation_id.parse::<MutationId>().ok()?,
            &self.method,
            &self.path,
            serde_json::from_str(&self.raw_body).ok()?,
            self.created_at,
        );
        if let Some(name) = self.op_name.clone() {
            let meta = OperationMeta::new(name);
            intent = intent.with_op(match self.op_version.clone() {
                Some(version) => meta.with_version(version),
                None => meta,
            });
        }
        if let Some(traceparent) = self.traceparent.clone() {
            intent = intent.with_traceparent(traceparent);
        }
        if let Some(precondition) = self.precondition.clone() {
            intent = intent.with_precondition(precondition);
        }
        let mut record = OutboxRecord::stamp(intent, scope.clone(), 0);
        record.attempts = self.attempts;

        let reason = match self.reason_kind.as_str() {
            "rejected" => DeadLetterReason::Rejected {
                error: self
                    .reason_body
                    .as_ref()
                    .and_then(|body| serde_json::from_str(body).ok()),
            },
            "retention_bound" => DeadLetterReason::RetentionBound,
            "caller" => DeadLetterReason::Caller(self.reason_body.clone().unwrap_or_default()),
            other => DeadLetterReason::Caller(format!("unrecognised reason {other:?}")),
        };
        Some(DeadLetterRecord::from_record(
            record,
            self.rejected_at,
            reason,
        ))
    }
}

/// One quarantined row, as IndexedDB holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QuarantineRow {
    pub scope: String,
    pub raw_mutation_id: String,
    pub method: String,
    pub path: String,
    pub raw_body: String,
    #[serde(with = "i64_text")]
    pub created_at: i64,
    pub op_name: Option<String>,
    pub op_version: Option<String>,
    pub reason: String,
    #[serde(with = "i64_text")]
    pub quarantined_at: i64,
}

impl QuarantineRow {
    pub fn from_record(record: &QuarantinedRecord) -> Self {
        Self {
            scope: record.scope.as_str().to_owned(),
            raw_mutation_id: record.raw_mutation_id.clone(),
            method: record.method.clone(),
            path: record.path.clone(),
            raw_body: record.raw_body.clone(),
            created_at: record.created_at,
            op_name: record.op.as_ref().map(|op| op.name.clone()),
            op_version: record.op.as_ref().and_then(|op| op.version.clone()),
            reason: record.reason.clone(),
            quarantined_at: record.quarantined_at,
        }
    }

    pub fn decode(&self, scope: &ScopeKey) -> QuarantinedRecord {
        let record = QuarantinedRecord::from_raw(
            self.raw_mutation_id.clone(),
            self.method.clone(),
            self.path.clone(),
            self.raw_body.clone(),
            self.created_at,
            scope.clone(),
            self.reason.clone(),
            self.quarantined_at,
        );
        match self.op_name.clone() {
            Some(name) => {
                let meta = OperationMeta::new(name);
                record.with_op(match self.op_version.clone() {
                    Some(version) => meta.with_version(version),
                    None => meta,
                })
            }
            None => record,
        }
    }
}

/// One read-model row, as IndexedDB holds it.
///
/// `blob` is stored as a JSON string rather than a structured value on purpose: core promises never
/// to read it, and a structured clone would let IndexedDB's own type rules reshape what the
/// application stored — a `Map` coming back where an object went in, say. Text round-trips exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RowRecord {
    pub scope: String,
    pub entity: String,
    pub row_id: String,
    pub blob: String,
    pub stale: bool,
}

impl RowRecord {
    pub fn from_stored(stored: &StoredRow, scope: &ScopeKey) -> Result<Self, frontbox::Error> {
        Ok(Self {
            scope: scope.as_str().to_owned(),
            entity: stored.row.entity.clone(),
            row_id: stored.row.row_id.clone(),
            blob: serde_json::to_string(&stored.blob).map_err(frontbox::Error::serialization)?,
            stale: stored.stale,
        })
    }

    pub fn decode(&self) -> StoredRow {
        let blob = serde_json::from_str(&self.blob).unwrap_or(serde_json::Value::Null);
        StoredRow::new(RowRef::new(&self.entity, &self.row_id), blob).with_stale(self.stale)
    }
}
