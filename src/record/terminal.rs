//! The two ways a record leaves the outbox.
//!
//! Split from the pending path because these are terminal states: a dead letter is a server verdict
//! or an exhausted client, quarantine is a local integrity failure. Neither is ever written
//! directly — both are produced by an outcome transition, which is what makes decision 003's
//! atomicity requirement expressible at all.
use crate::id::MutationId;
use crate::protocol::RemoteRejection;
use crate::record::{OperationMeta, OutboxRecord};
use crate::scope::ScopeKey;
use serde::{Deserialize, Serialize};
/// Why a record was parked.
///
/// # Why this is not `Option<RemoteRejection>`
///
/// It was, until 2026-08-30, and that field answered two questions at once: *did a server refuse
/// this* and *did it explain itself*. `Some` meant a refusal and `None` meant the client had
/// reached its retention bound — which held exactly as long as those were the only two ways a
/// record could be parked.
///
/// They never were. [`apply_outcomes`](crate::store::OutboxStore::apply_outcomes) is public, so an
/// application has always been able to park a record for a reason of its own, and it produced the
/// same `None`. [`attempts`](DeadLetterRecord::attempts) narrows that and does not settle it: a
/// caller-driven dead letter can carry any count, including one equal to the bound.
///
/// The [`Option`] now sits inside [`Rejected`](DeadLetterReason::Rejected), where it means *the
/// server refused and may or may not have said why* — a real distinction, since a `Rejected` status
/// with no payload is a well-formed server answer (`wiki/decisions/027-dead-letter-reason.decision.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeadLetterReason {
    /// A server refused this mutation on its merits.
    Rejected {
        /// What the server said, when it said anything.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<RemoteRejection>,
    },
    /// The client reached its retention bound. **No server ever refused this.**
    ///
    /// The record was sent [`attempts`](DeadLetterRecord::attempts) times and stayed queued every
    /// time. Synthesising a rejection here would put words in the server's mouth, which is what
    /// `wiki/decisions/017-bounded-retention.decision.md` refuses.
    RetentionBound,
    /// The application parked it, in its own words.
    ///
    /// Core never reads the string and never enumerates application reasons — a wall-clock watchdog
    /// is one, a user cancelling a queued write is another, and neither is core's to name. Empty is
    /// accepted and is exactly as useful as omitting the variant, for the reason
    /// [`OperationMeta::new`] accepts an empty name.
    Caller(String),
}
/// A mutation that will not be sent again.
///
/// Produced only by an outcome transition, never as a free-standing write: a dead letter is a
/// server verdict on a record the store was holding, and the transition out of the outbox has to
/// commit with it (`wiki/decisions/003-atomic-outcome-application.decision.md`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DeadLetterRecord {
    /// The idempotency key.
    pub mutation_id: MutationId,
    /// HTTP method that was replayed.
    pub method: String,
    /// Path that was replayed.
    pub path: String,
    /// Parsed request body.
    pub body: serde_json::Value,
    /// Client timestamp, epoch milliseconds.
    pub created_at: i64,
    /// Caller-owned operation label, carried across the transition.
    pub op: Option<OperationMeta>,
    /// Trace context, carried across the transition.
    ///
    /// Without it a parked mutation cannot be joined to the request that parked it, which is the
    /// one moment anybody goes looking.
    pub traceparent: Option<String>,
    /// The precondition the send carried, carried across the transition.
    ///
    /// A dead letter that failed on a conflict is far easier to act on when it says what state it
    /// expected, and requeueing one without its precondition would re-run exactly the unchecked
    /// write it exists to prevent.
    pub precondition: Option<String>,
    /// The store that wrote this record.
    pub scope: ScopeKey,
    /// How many passes had sent this record and left it queued.
    ///
    /// Carried across because a human triaging a dead letter wants to know how many times a server
    /// was asked, not only that it was parked. It is **not** the discriminator between the ways a
    /// record can be parked — it was pressed into that role while the reason was encoded as an
    /// absent rejection, and [`reason`](DeadLetterRecord::reason) says it outright now
    /// (`wiki/decisions/027-dead-letter-reason.decision.md`).
    pub attempts: u32,
    /// When the record was parked, from the injected clock.
    pub rejected_at: i64,
    /// Why it was parked.
    pub reason: DeadLetterReason,
}
impl DeadLetterRecord {
    /// Move a pending record into the dead-letter state.
    pub fn from_record(record: OutboxRecord, rejected_at: i64, reason: DeadLetterReason) -> Self {
        Self {
            mutation_id: record.mutation_id,
            method: record.method,
            path: record.path,
            body: record.body,
            created_at: record.created_at,
            op: record.op,
            traceparent: record.traceparent,
            precondition: record.precondition,
            scope: record.scope,
            attempts: record.attempts,
            rejected_at,
            reason,
        }
    }
    /// The server's refusal payload, if a server refused this and explained itself.
    ///
    /// A convenience for callers that only want the payload. `None` covers three different facts —
    /// a refusal with no explanation, a retention bound, and a caller-driven parking — so a caller
    /// that needs to tell them apart matches on [`reason`](DeadLetterRecord::reason) instead.
    pub fn rejection(&self) -> Option<&RemoteRejection> {
        match &self.reason {
            DeadLetterReason::Rejected { error } => error.as_ref(),
            _ => None,
        }
    }
}
/// A locally stored row that could not be used.
///
/// Distinct from a dead letter, and the distinction is load-bearing: a dead letter is a server
/// verdict on a well-formed record, quarantine is a local integrity failure on a record the server
/// never saw. Collapsing them would misreport local corruption as server refusal.
///
/// Fields that may have been unreadable are kept in their raw form, so a quarantined row stays
/// diagnosable rather than becoming an unexplained gap in a pending count.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct QuarantinedRecord {
    /// The identifier, when it parsed.
    ///
    /// `None` is the id-less path: a row whose identifier is unreadable cannot be named by an
    /// [`Outcome`](crate::store::Outcome) at all, so only
    /// [`sweep_corrupt`](crate::store::OutboxStore::sweep_corrupt) can reach it.
    pub mutation_id: Option<MutationId>,
    /// The identifier exactly as stored.
    pub raw_mutation_id: String,
    /// HTTP method as stored.
    pub method: String,
    /// Path as stored.
    pub path: String,
    /// The body exactly as stored, which may not be valid JSON.
    pub raw_body: String,
    /// Client timestamp as stored, which may be outside the representable range.
    pub created_at: i64,
    /// Caller-owned operation label, carried across the transition.
    pub op: Option<OperationMeta>,
    /// The store that wrote this record.
    pub scope: ScopeKey,
    /// What made the row unusable.
    pub reason: String,
    /// When the row was quarantined, from the injected clock.
    pub quarantined_at: i64,
}
impl QuarantinedRecord {
    /// Build a quarantine entry from a row that would not decode.
    ///
    /// # Why a backend needs this
    ///
    /// Decision 008 marks every record `#[non_exhaustive]` so later fields are additive, which
    /// blocks struct literals outside this crate. That is right for *readers*. A storage backend is
    /// a **writer** of these types, and until `frontbox-sqlite` was built every backend lived
    /// inside this crate and could use a literal — so nothing was in a position to notice that an
    /// out-of-tree backend had no way to produce one at all.
    ///
    /// [`DeadLetterRecord::from_record`] already covered the other terminal transition. This is its
    /// counterpart for the id-less path, where there is no `OutboxRecord` to move because the row
    /// is precisely the kind that will not become one.
    ///
    /// `mutation_id` is derived from `raw_mutation_id` rather than taken: an entry whose parsed and
    /// raw identifiers disagreed would be a lie no caller could detect.
    ///
    /// Eight arguments, and the lint is allowed rather than satisfied: the record genuinely has
    /// eight independent fields a backend must supply, and the usual remedy — a parameter struct —
    /// would be a second public type existing only to construct the first, which is worse for
    /// exactly the audience this constructor exists for.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        raw_mutation_id: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
        raw_body: impl Into<String>,
        created_at: i64,
        scope: ScopeKey,
        reason: impl Into<String>,
        quarantined_at: i64,
    ) -> Self {
        let raw_mutation_id = raw_mutation_id.into();
        Self {
            mutation_id: raw_mutation_id.parse().ok(),
            raw_mutation_id,
            method: method.into(),
            path: path.into(),
            raw_body: raw_body.into(),
            created_at,
            op: None,
            scope,
            reason: reason.into(),
            quarantined_at,
        }
    }
    /// Carry the caller's operation label across the transition.
    #[must_use]
    pub fn with_op(mut self, op: OperationMeta) -> Self {
        self.op = Some(op);
        self
    }
}

#[cfg(test)]
mod tests;
