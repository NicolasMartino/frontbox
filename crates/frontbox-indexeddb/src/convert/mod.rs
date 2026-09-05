//! The stored shapes, and the conversions to and from core's records.
//!
//! Rows are stored as JSON objects through `serde`, then handed to IndexedDB as structured-clonable
//! values. The identifier and the body are kept as **text**, not parsed values, for the same reason
//! the SQLite schema does: a row that will not decode has to be representable, or decision 006's
//! position — that corruption is visible rather than silently dropped — cannot be tested here.
//!
//! # No `#[serde(default)]` on any field in this file
//!
//! Every write goes through [`to_js`], which serializes the whole struct, so a stored object always
//! carries every key. A `default` could therefore only ever absorb a key written by a *different*
//! schema — and this crate has never shipped one, so no such row exists anywhere.
//!
//! Removing them is not tidiness. **A `default` on a durable row turns a schema mismatch into
//! silent data loss**: rename `precondition` and every stored row quietly reads as having none, so
//! decision 026's replayable preconditions stop applying and nothing says so. Without it,
//! [`from_js`] returns `None`, the row is treated as corrupt, and it reaches quarantine — which is
//! the "corrupt-record visibility instead of silent row loss" D5 was built on.
//!
//! `seq` keeps its `skip_serializing_if`, because that is a live mechanism rather than a tolerance:
//! the key is omitted on insert so IndexedDB's `autoIncrement` generator supplies it, and it is
//! written back into the object since the store's key path is `seq`.
//!
//! # The one exception, and what bounds it
//!
//! `OutboxRow::transport_started` carries `#[serde(default = "assume_transport_started")]`, and it
//! is the only field in this crate that may.
//!
//! The rule above rests on a premise this field retired: "this crate has never shipped [a different
//! schema], so no such row exists anywhere". Adding a field to a durable row is what makes one
//! exist. A stored object written before this feature has no `transport_started` key, and it is on
//! a user's device already.
//!
//! **A `default` is admissible exactly when the defaulted value is the conservative one — the value
//! that turns a feature off.** This one defaults to `true`, meaning "assume the server may already
//! hold this identifier", so a row missing the key becomes non-coalescible while staying perfectly
//! resendable. That is degradation, and it is the same answer a future rename would produce.
//! Contrast `precondition`, the field the rule was written about: absent there means conflict
//! detection silently stops, which is loss.
//!
//! The next field addition has to make this argument again rather than cite the precedent. A
//! `default` whose value would *enable* something — `attempts: 0` on a row that has been retried,
//! `stale: false` on a row that is not — remains exactly as forbidden as before.
//!
//! No version bump goes with it. `VERSION` moves to create object stores inside `onupgradeneeded`
//! (`crates/frontbox-indexeddb/src/backend.rs`); this adds no store and no index, and rewriting
//! every stored object to add a key whose absence already reads correctly would be the expensive
//! kind of migration bought for nothing.

use frontbox::{
    MutationId, MutationIntent, OperationMeta, OutboxRecord, QuarantinedRecord, RowRef, ScopeKey,
};
use serde::{Deserialize, Serialize};

mod terminal;

pub(crate) use terminal::{DeadLetterRow, QuarantineRow, RowRecord};
use wasm_bindgen::JsValue;

use crate::request::js_error;

/// Epoch milliseconds, stored as text.
///
/// # Why not a number
///
/// Values cross into IndexedDB as structured-clonable JavaScript values, and every JavaScript
/// number is an `f64`. An `i64` outside ±2^53 therefore does **not** round-trip: it comes back
/// rounded, silently. Real timestamps are far inside that range, which is exactly what makes this
/// dangerous — it looks fine until a value that is not a real timestamp shows up, and the one
/// place that happens is a corrupt row, which is the thing the backend most needs to report
/// faithfully.
///
/// Text round-trips exactly, at the cost of a parse. Core's own rule that `mutation_id` is stored
/// in one textual form is the same trade for the same reason.
pub(super) mod i64_text {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// One queued mutation, as IndexedDB holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutboxRow {
    /// Absent on insert so the `autoIncrement` generator supplies it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
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
    pub row_entity: Option<String>,
    pub row_id: Option<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    /// Whether the server may already hold this row's identifier.
    ///
    /// Set by `read_for_send`, and by `apply_outcomes` applying a `Retain`. See the module docs for
    /// why this is the one field in the crate carrying a `serde` default, and what bounds that
    /// exception.
    #[serde(default = "assume_transport_started")]
    pub transport_started: bool,
}

/// What a stored row with no `transport_started` key means.
///
/// `true`: the row predates the field, and the only safe reading of "we cannot tell whether this was
/// sent" is that it was.
fn assume_transport_started() -> bool {
    true
}

impl OutboxRow {
    /// Build the row an enqueue writes.
    pub fn from_intent(intent: &MutationIntent, scope: &ScopeKey) -> Result<Self, frontbox::Error> {
        Ok(Self {
            seq: None,
            scope: scope.as_str().to_owned(),
            mutation_id: intent.mutation_id.to_string(),
            method: intent.method.clone(),
            path: intent.path.clone(),
            raw_body: serde_json::to_string(&intent.body)
                .map_err(frontbox::Error::serialization)?,
            created_at: intent.created_at,
            op_name: intent.op.as_ref().map(|op| op.name.clone()),
            op_version: intent.op.as_ref().and_then(|op| op.version.clone()),
            traceparent: intent.traceparent.clone(),
            precondition: intent.precondition.clone(),
            row_entity: intent.row.as_ref().map(|row| row.entity.clone()),
            row_id: intent.row.as_ref().map(|row| row.row_id.clone()),
            attempts: 0,
            last_error: None,
            transport_started: false,
        })
    }

    fn op(&self) -> Option<OperationMeta> {
        self.op_name.clone().map(|name| {
            let meta = OperationMeta::new(name);
            match self.op_version.clone() {
                Some(version) => meta.with_version(version),
                None => meta,
            }
        })
    }

    /// The row this mutation is bound to, when both halves are present.
    ///
    /// One half without the other is a row this backend wrote wrong; treating it as unbound is the
    /// reading that cannot corrupt a merge.
    pub fn bound_row(&self) -> Option<RowRef> {
        match (self.row_entity.clone(), self.row_id.clone()) {
            (Some(entity), Some(id)) => Some(RowRef::new(entity, id)),
            _ => None,
        }
    }

    /// Build the record a caller sees, or say why the row is unusable.
    pub fn decode(&self, scope: &ScopeKey) -> Result<OutboxRecord, String> {
        let mutation_id: MutationId = self
            .mutation_id
            .parse()
            .map_err(|_| format!("unparseable mutation_id: {:?}", self.mutation_id))?;
        let body: serde_json::Value = serde_json::from_str(&self.raw_body)
            .map_err(|failure| format!("body is not valid JSON: {failure}"))?;
        if !frontbox::timestamp_is_representable(self.created_at) {
            return Err(format!(
                "created_at outside representable range: {}",
                self.created_at
            ));
        }

        let mut intent =
            MutationIntent::new(mutation_id, &self.method, &self.path, body, self.created_at);
        if let Some(op) = self.op() {
            intent = intent.with_op(op);
        }
        if let Some(traceparent) = self.traceparent.clone() {
            intent = intent.with_traceparent(traceparent);
        }
        if let Some(precondition) = self.precondition.clone() {
            intent = intent.with_precondition(precondition);
        }
        if let Some(bound) = self.bound_row() {
            intent = intent.with_row(bound);
        }
        let mut record = OutboxRecord::stamp(intent, scope.clone(), self.seq.unwrap_or_default());
        record.attempts = self.attempts;
        record.last_error = self.last_error.clone();
        Ok(record)
    }

    /// Move an unusable row into the quarantine shape.
    pub fn quarantined(
        &self,
        scope: &ScopeKey,
        reason: String,
        quarantined_at: i64,
    ) -> QuarantinedRecord {
        let record = QuarantinedRecord::from_raw(
            self.mutation_id.clone(),
            self.method.clone(),
            self.path.clone(),
            self.raw_body.clone(),
            self.created_at,
            scope.clone(),
            reason,
            quarantined_at,
        );
        match self.op() {
            Some(op) => record.with_op(op),
            None => record,
        }
    }
}

/// Serialize a stored shape into a structured-clonable value.
pub(crate) fn to_js<T: Serialize>(value: &T) -> Result<JsValue, frontbox::Error> {
    let text = serde_json::to_string(value).map_err(frontbox::Error::serialization)?;
    js_sys::JSON::parse(&text).map_err(js_error)
}

/// Read a stored shape back out of one.
pub(crate) fn from_js<T: for<'de> Deserialize<'de>>(value: &JsValue) -> Option<T> {
    let text = js_sys::JSON::stringify(value).ok()?.as_string()?;
    serde_json::from_str(&text).ok()
}
