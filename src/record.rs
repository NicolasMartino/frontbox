//! The records this crate stores and moves.
//!
//! Per `wiki/decisions/008-mutation-envelope-extensibility.decision.md` every type here is
//! `#[non_exhaustive]` and built through a constructor. Outside this crate that blocks struct
//! literals, exhaustive destructuring, and functional-update syntax, which is what makes every
//! later field addition additive rather than breaking.
//!
//! # Why there are two record types
//!
//! [`MutationIntent`] is what a caller enqueues. [`OutboxRecord`] is what a store returns, and it
//! carries the [`ScopeKey`] of the store that wrote it
//! (`wiki/decisions/009-local-scope-identity.decision.md`). Splitting them is what makes the scope
//! stamp unforgeable: a caller has no type in which to put a scope, so the store is the only thing
//! that can apply one. The source system draws the same line between `MutationIntentDto` and its
//! own `OutboxRecord`.

use serde::{Deserialize, Serialize};

use crate::id::MutationId;
use crate::protocol::RemoteRejection;
use crate::scope::ScopeKey;

/// Caller-owned label for what a mutation *is*.
///
/// Core stores this, returns it, and copies it across every transition. Core never branches on it:
/// no routing, no deduplication, no ordering, no retry policy keyed on `name`. Because the field is
/// optional, any behavior core derived from it would be behavior a caller could not opt out of.
///
/// It exists for two reasons. A dead letter reading `POST /api/v1/sessions/3f2a-…/exercises`
/// carries no intent, and dead letters are the surface a human inspects after a refusal. And a body
/// written by one application version, queued for weeks, and replayed against a newer server fails
/// as an opaque rejection unless something recorded what wrote it.
///
/// # `version` is not compared
///
/// Core must never order or compare `version` values. String ordering is wrong for semantic
/// versions — `"1.10.0"` sorts below `"1.9.0"`. The field is caller-interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OperationMeta {
    /// What the operation is called, in the caller's own vocabulary.
    pub name: String,
    /// What wrote the body, in the caller's own versioning scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl OperationMeta {
    /// Name an operation.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
        }
    }

    /// Record what wrote the body.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// A mutation a caller wants replayed, and the wire representation of one.
///
/// Field order matches the source system's `MutationIntentDto` so the serialized payload is
/// byte-identical, and `created_at` serializes as `client_datetime` in RFC 3339. Internally it
/// stays an `i64` of epoch milliseconds — the same representation [`Clock::now_ms`] produces and
/// the records store — so there is exactly one notion of time in the crate.
///
/// [`Clock::now_ms`]: crate::clock::Clock::now_ms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MutationIntent {
    /// The idempotency key. Caller-supplied.
    pub mutation_id: MutationId,
    /// HTTP method to replay.
    pub method: String,
    /// Path to replay.
    pub path: String,
    /// Client timestamp, epoch milliseconds. Serialized as `client_datetime`.
    #[serde(rename = "client_datetime", with = "client_datetime")]
    pub created_at: i64,
    /// Parsed request body.
    ///
    /// Parsed rather than text, so a malformed body cannot enter the outbox at all. That moves
    /// corruption detection to the enqueue boundary instead of leaving it to be discovered at
    /// replay, after a reconnect.
    pub body: serde_json::Value,
    /// Optional caller-owned operation label.
    ///
    /// Skipped entirely when unset, so a caller who does not use it produces a payload with exactly
    /// the five fields the source protocol defines. A server that rejects unknown keys sees nothing
    /// new until the caller opts in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<OperationMeta>,
}

impl MutationIntent {
    /// Build a mutation intent.
    pub fn new(
        mutation_id: MutationId,
        method: impl Into<String>,
        path: impl Into<String>,
        body: serde_json::Value,
        created_at: i64,
    ) -> Self {
        Self {
            mutation_id,
            method: method.into(),
            path: path.into(),
            created_at,
            body,
            op: None,
        }
    }

    /// Attach caller-owned operation metadata.
    #[must_use]
    pub fn with_op(mut self, op: OperationMeta) -> Self {
        self.op = Some(op);
        self
    }
}

/// A pending mutation as a store holds it.
///
/// There is no way to build one without a [`ScopeKey`]. See the module docs for why that matters.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OutboxRecord {
    /// The idempotency key.
    pub mutation_id: MutationId,
    /// HTTP method to replay.
    pub method: String,
    /// Path to replay.
    pub path: String,
    /// Parsed request body.
    pub body: serde_json::Value,
    /// Client timestamp, epoch milliseconds.
    pub created_at: i64,
    /// Optional caller-owned operation label.
    pub op: Option<OperationMeta>,
    /// The store that wrote this record.
    pub scope: ScopeKey,
}

impl OutboxRecord {
    /// Stamp an intent with the writing store's scope.
    ///
    /// Backends call this; callers cannot, because they have no record type to hand to
    /// [`enqueue`](crate::store::OutboxStore::enqueue) other than a [`MutationIntent`].
    pub fn stamp(intent: MutationIntent, scope: ScopeKey) -> Self {
        Self {
            mutation_id: intent.mutation_id,
            method: intent.method,
            path: intent.path,
            body: intent.body,
            created_at: intent.created_at,
            op: intent.op,
            scope,
        }
    }

    /// Recover the intent for replay.
    ///
    /// Infallible, unlike the source's equivalent, because the body was parsed at enqueue and the
    /// identifier was parsed when the row was read. Nothing is left to fail here.
    pub fn to_intent(&self) -> MutationIntent {
        MutationIntent {
            mutation_id: self.mutation_id,
            method: self.method.clone(),
            path: self.path.clone(),
            created_at: self.created_at,
            body: self.body.clone(),
            op: self.op.clone(),
        }
    }

    /// The key pending work is ordered by: `(created_at, mutation_id)`.
    ///
    /// `created_at` alone is not a total order. It comes from the client clock, and two mutations
    /// enqueued in the same millisecond would otherwise come back in either order, differently on
    /// each backend. The tie-break buys determinism, not causality: a [`MutationId`] is random, so
    /// same-millisecond ties resolve stably but arbitrarily.
    pub fn order_key(&self) -> (i64, MutationId) {
        (self.created_at, self.mutation_id)
    }
}

/// A mutation the server terminally refused.
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
    /// The store that wrote this record.
    pub scope: ScopeKey,
    /// When the refusal was recorded, from the injected clock.
    pub rejected_at: i64,
    /// The server's verdict, when it supplied one.
    pub error: Option<RemoteRejection>,
}

impl DeadLetterRecord {
    /// Move a pending record into the dead-letter state.
    pub fn from_record(
        record: OutboxRecord,
        rejected_at: i64,
        error: Option<RemoteRejection>,
    ) -> Self {
        Self {
            mutation_id: record.mutation_id,
            method: record.method,
            path: record.path,
            body: record.body,
            created_at: record.created_at,
            op: record.op,
            scope: record.scope,
            rejected_at,
            error,
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

/// Serializes an epoch-milliseconds `i64` as the source protocol's `client_datetime`.
///
/// Delegating to chrono's own impls rather than formatting by hand is what guarantees the output
/// matches the existing server byte for byte.
mod client_datetime {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(millis: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        let dt = DateTime::<Utc>::from_timestamp_millis(*millis).ok_or_else(|| {
            serde::ser::Error::custom(format!("client timestamp out of range: {millis}"))
        })?;
        dt.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        Ok(DateTime::<Utc>::deserialize(deserializer)?.timestamp_millis())
    }
}
