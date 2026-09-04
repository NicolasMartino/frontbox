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

mod last_error;
mod row;

pub use last_error::{truncate_error, LAST_ERROR_MAX};
pub use row::{RowRef, StoredRow};

use serde::{Deserialize, Serialize};

use crate::id::MutationId;
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
    ///
    /// An empty name is accepted and is exactly as useless as omitting the metadata entirely — a
    /// dead letter labelled `""` tells a human nothing. It is not rejected, because validating a
    /// field core never reads would make the constructor fallible for no safety gain, and because
    /// what counts as a meaningful name is the caller's judgment, not this crate's.
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
    /// Caller-supplied W3C trace context, captured at enqueue.
    ///
    /// # Not part of the payload
    ///
    /// `#[serde(skip)]` is deliberate. Trace context travels as a *header* on the drain request,
    /// not as a body field, so serializing it here would put it in the wrong place and change the
    /// payload decision 010 fixed. The transport reads it off the intent and sets the header.
    ///
    /// # Why the caller supplies it
    ///
    /// A W3C `traceparent` is mostly randomness, and core generates none — the `v4` feature is the
    /// only randomness in the crate and `scripts/verify.sh` builds without it to prove that path
    /// works. The same rule as [`MutationId`], for the same reason. An application already inside a
    /// span when the user acts has the *correct* parent; a fresh root minted here would discard the
    /// link to the user action, which is the link the field exists to preserve.
    ///
    /// Stamped at enqueue rather than at drain: the interval worth measuring is how long the write
    /// waited, and a span cannot stay open across a restart
    /// (`wiki/decisions/022-durable-trace-context.decision.md`).
    #[serde(skip)]
    pub traceparent: Option<String>,
    /// Caller-supplied request precondition, captured at enqueue.
    ///
    /// Opaque. Core does not know whether it says `If-Match: <hash>` or `If-None-Match: *`, and
    /// must not: a `PUT` does not reveal whether a given mutation is a create, and only the
    /// application does. The transport turns it into whatever header it means.
    ///
    /// # Captured at enqueue, not at drain
    ///
    /// A precondition means something only if it carries the state the user was looking at **when
    /// they enqueued**. Computed at drain from current local state it compares the client against
    /// itself and detects nothing — it matches whatever is there at that moment, and every conflict
    /// passes silently.
    ///
    /// # Why this is a named field and not one entry in a header map
    ///
    /// Two fields now share this shape, and the generalization is refused rather than overlooked.
    /// A map is the mechanism by which an `Authorization` header ends up durable on disk, which
    /// `wiki/decisions/004-transport-auth-and-offline.decision.md` forbids — not through misuse,
    /// but because it is the obvious thing a map is for. It would also flatten a field that is
    /// load-bearing for correctness and one that is diagnostic into two equally optional entries
    /// (`wiki/decisions/026-replayable-preconditions.decision.md`).
    ///
    /// `#[serde(skip)]` for the same reason as [`traceparent`](MutationIntent::traceparent): it is
    /// a header, not a body field.
    #[serde(skip)]
    pub precondition: Option<String>,
    /// Which read-model row this mutation is about, if the caller says.
    ///
    /// # What binding buys
    ///
    /// Two things that are otherwise guesswork. A hydration write can skip rows with queued work
    /// (`wiki/decisions/032-opaque-row-store.decision.md`), and a report can say which rows a
    /// drain settled (`wiki/decisions/035-reports-name-what-drained.decision.md`) instead of a
    /// caller rebuilding an index to find out.
    ///
    /// # Optional, and inert when absent
    ///
    /// A caller that binds nothing gets exactly today's behaviour: no rows skipped on its behalf,
    /// `row: None` on every [`Drained`](crate::runner::Drained). Nothing about the queue changes.
    ///
    /// `#[serde(skip)]` for the reason [`precondition`](MutationIntent::precondition) is: this is
    /// local bookkeeping about the caller's own storage, and the server has no use for it. The
    /// payload decision 010 fixed is untouched.
    #[serde(skip)]
    pub row: Option<RowRef>,
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
            traceparent: None,
            precondition: None,
            row: None,
        }
    }

    /// Attach caller-owned operation metadata.
    #[must_use]
    pub fn with_op(mut self, op: OperationMeta) -> Self {
        self.op = Some(op);
        self
    }

    /// Attach W3C trace context, as the caller's tracing layer rendered it.
    ///
    /// Core stores it and never parses it. An application propagating a different format stores
    /// that instead and core is none the wiser.
    #[must_use]
    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }

    /// Attach a request precondition, in whatever form the caller's server expects.
    ///
    /// Stored verbatim and never parsed. An empty string is accepted and is exactly as useful as
    /// omitting it, for the reason [`OperationMeta::new`] accepts an empty name.
    #[must_use]
    pub fn with_precondition(mut self, precondition: impl Into<String>) -> Self {
        self.precondition = Some(precondition.into());
        self
    }

    /// Say which read-model row this mutation is about.
    ///
    /// The caller knows this at enqueue and is the only thing that does. Core compares it for
    /// equality and never parses either half.
    #[must_use]
    pub fn with_row(mut self, row: RowRef) -> Self {
        self.row = Some(row);
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
    /// Caller-supplied W3C trace context, captured at enqueue and replayed on every send.
    pub traceparent: Option<String>,
    /// Caller-supplied request precondition, captured at enqueue and replayed on every send.
    ///
    /// Opaque to core. Dropping it does not fail loudly — it disables conflict detection with no
    /// symptom at all, which is why it is durable rather than recomputed
    /// (`wiki/decisions/026-replayable-preconditions.decision.md`).
    pub precondition: Option<String>,
    /// The store that wrote this record.
    pub scope: ScopeKey,
    /// Enqueue order, assigned by the store inside the insert's own transaction.
    ///
    /// The primary sort key. Globally monotonic rather than per-scope: reads are scope filtered
    /// already (decision 009), so a global sequence is monotonic within every scope for free — and
    /// an `autoIncrement` object store then supplies it with no extra round trip.
    ///
    /// Not on [`MutationIntent`], for the reason [`scope`](OutboxRecord::scope) is not: only the
    /// store knows what it has already issued, and a field the caller cannot write is a field the
    /// caller cannot forge (`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`).
    ///
    /// `u64` does not wrap in any realistic lifetime — one enqueue per nanosecond for five
    /// centuries stays inside it — so no wrap policy is specified.
    pub seq: u64,
    /// How many passes sent this record and left it queued.
    ///
    /// Incremented by the store when [`apply_outcomes`](crate::store::OutboxStore::apply_outcomes)
    /// applies a [`Retain`](crate::store::Disposition::Retain), which is what makes `Retain` a
    /// write rather than a no-op. Counts verdicts *received*, so an offline pass and an attempted
    /// transport that failed both leave it alone: neither produced a verdict this client can read
    /// (`wiki/decisions/017-bounded-retention.decision.md`).
    pub attempts: u32,
    /// Which read-model row this mutation is about, as the caller bound it at enqueue.
    pub row: Option<RowRef>,
    /// Why the most recent verdict left this record queued.
    ///
    /// The server's own words, truncated to [`LAST_ERROR_MAX`], never parsed. Written by the store
    /// in the same step that increments [`attempts`](OutboxRecord::attempts), so the two always
    /// describe the same verdict.
    ///
    /// `None` before any verdict, and after a verdict that carried no reason. It is overwritten
    /// rather than accumulated: one slot, latest wins
    /// (`wiki/decisions/033-last-error-on-the-record.decision.md`).
    pub last_error: Option<String>,
}

impl OutboxRecord {
    /// Stamp an intent with the writing store's scope.
    ///
    /// Backends call this; callers cannot, because they have no record type to hand to
    /// [`enqueue`](crate::store::OutboxStore::enqueue) other than a [`MutationIntent`].
    pub fn stamp(intent: MutationIntent, scope: ScopeKey, seq: u64) -> Self {
        Self {
            mutation_id: intent.mutation_id,
            method: intent.method,
            path: intent.path,
            body: intent.body,
            created_at: intent.created_at,
            op: intent.op,
            traceparent: intent.traceparent,
            precondition: intent.precondition,
            row: intent.row,
            scope,
            seq,
            attempts: 0,
            last_error: None,
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
            traceparent: self.traceparent.clone(),
            precondition: self.precondition.clone(),
            row: self.row.clone(),
        }
    }

    /// The key pending work is ordered by: [`seq`](OutboxRecord::seq), and nothing else.
    ///
    /// Assigned at enqueue and unique within a store — a counter in memory, an
    /// `INTEGER PRIMARY KEY AUTOINCREMENT` in SQLite, an `autoIncrement` generator in IndexedDB —
    /// so it is already a total order and needs no tiebreak.
    ///
    /// **It is also the only faithful one.** `created_at` is a client clock reading rather than
    /// enqueue order, and same-millisecond ties broke on a random [`MutationId`], so three rapid
    /// writes could drain in an order the application never wrote in — a set before the session it
    /// belongs to (`wiki/decisions/016-monotonic-enqueue-sequence.decision.md`).
    ///
    /// This buys faithfulness, not causality. It replays the order the caller enqueued in; it
    /// cannot make that order correct if the caller enqueued a child before its parent.
    pub fn order_key(&self) -> u64 {
        self.seq
    }
}

mod terminal;

pub use terminal::{DeadLetterReason, DeadLetterRecord, QuarantinedRecord};

/// Serializes an epoch-milliseconds `i64` as the source protocol's `client_datetime`.
///
/// The rendering lives in [`crate::rfc3339`], which is held to chrono's byte-for-byte output by an
/// oracle test rather than by depending on chrono (decision 011).
mod client_datetime {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(millis: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        let text = crate::rfc3339::format(*millis).ok_or_else(|| {
            serde::ser::Error::custom(format!("client timestamp out of range: {millis}"))
        })?;
        serializer.serialize_str(&text)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        // Owned rather than borrowed: a `&str` cannot be deserialized from a JSON string carrying
        // an escape, and refusing those would make the accepted grammar depend on the encoding.
        let text = String::deserialize(deserializer)?;
        crate::rfc3339::parse(&text).map_err(|reason| {
            serde::de::Error::custom(format!("invalid client_datetime {text:?}: {reason}"))
        })
    }
}
