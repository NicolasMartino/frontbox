//! The batch push protocol.
//!
//! These types serialize to the wire format the source system's server already speaks, so a
//! transport for that server is a passthrough. The shapes are protocol, not storage: a durable
//! backend defines its own row types and maps to these explicitly.

use serde::{Deserialize, Serialize};

use crate::id::MutationId;
use crate::record::MutationIntent;

/// A server's verdict on one mutation.
///
/// This is not an [`Error`](crate::error::Error). A remote rejection is a payload the server
/// attached to a result and that a dead letter carries for a human to read; the error type is for
/// failures of *this* crate's own operations.
///
/// `code` is optional even though the source server always sends one, because a server that
/// refuses without a machine-readable code is a server this crate should still be able to talk to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRejection {
    /// Machine-readable refusal code, when the server supplied one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Human-readable explanation.
    pub message: String,
    /// Anything further the server attached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl RemoteRejection {
    /// Build a rejection from a message alone.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            details: None,
        }
    }

    /// Attach a machine-readable code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Attach further detail.
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// How the server classified one submitted mutation.
///
/// The local disposition each of these maps to is fixed by
/// `wiki/decisions/005-mutation-outcome-policy.decision.md`:
///
/// | Status      | Meaning                                                      | Disposition  |
/// | ----------- | ------------------------------------------------------------ | ------------ |
/// | `Applied`   | Server applied it                                              | `Delete`     |
/// | `Duplicate` | Server had already seen this id                                | `Delete`     |
/// | `Rejected`  | Server terminally refused it                                   | `DeadLetter` |
/// | `Blocked`   | Skipped because an earlier ordered mutation failed terminally  | `Retain`     |
/// | `Pending`   | Server did not finish processing it                            | `Retain`     |
///
/// `Rejected` is the only status that produces a dead letter. `Blocked` is retained, which diverges
/// from the source: a blocked mutation was never evaluated on its own merits, so dead-lettering it
/// would discard valid work for the sole reason that it followed a failed record in the same batch.
///
/// This is a server *verdict*, not an HTTP status class. Nothing here should ever be derived from a
/// response code — a `401` is a 4xx that means "retry with a fresh token", not "terminally
/// refused".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MutationStatus {
    /// The server applied the mutation.
    Applied,
    /// The server had already processed this mutation id.
    Duplicate,
    /// The server terminally refused the mutation.
    Rejected,
    /// The server skipped it because an earlier ordered mutation failed terminally.
    Blocked,
    /// The server accepted it but has not finished processing it.
    Pending,
}

impl MutationStatus {
    /// Whether this verdict removes the record from the queue, one way or another.
    ///
    /// True for `Applied`, `Duplicate`, and `Rejected`. This is what "progress" means for a sync:
    /// a batch of nothing but `Blocked` and `Pending` leaves the queue exactly as it was.
    pub const fn drains(&self) -> bool {
        matches!(self, Self::Applied | Self::Duplicate | Self::Rejected)
    }
}

/// The server's verdict on one mutation in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationResult {
    /// Which mutation this is about.
    pub mutation_id: MutationId,
    /// How the server classified it.
    pub status: MutationStatus,
    /// The refusal payload, when the status carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RemoteRejection>,
}

impl MutationResult {
    /// Build a result.
    pub fn new(mutation_id: MutationId, status: MutationStatus) -> Self {
        Self {
            mutation_id,
            status,
            error: None,
        }
    }

    /// Attach the server's refusal payload.
    #[must_use]
    pub fn with_error(mut self, error: RemoteRejection) -> Self {
        self.error = Some(error);
        self
    }
}

/// A batch of mutations pushed to the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationBatchRequest {
    /// The mutations to replay, oldest first.
    pub mutations: Vec<MutationIntent>,
}

/// The server's verdicts on a pushed batch.
///
/// The server is not obliged to return a result for every mutation sent. A mutation with no result
/// stays queued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationBatchResponse {
    /// One verdict per mutation the server ruled on.
    pub results: Vec<MutationResult>,
}
