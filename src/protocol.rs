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
#[non_exhaustive]
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
/// | `Unknown`   | A verdict this crate cannot act on                             | `Retain`     |
///
/// `Rejected` is the only status that produces a dead letter. `Blocked` is retained, which diverges
/// from the source: a blocked mutation was never evaluated on its own merits, so dead-lettering it
/// would discard valid work for the sole reason that it followed a failed record in the same batch.
///
/// This is a server *verdict*, not an HTTP status class. Nothing here should ever be derived from a
/// response code — a `401` is a 4xx that means "retry with a fresh token", not "terminally
/// refused".
///
/// # Evolution
///
/// `#[non_exhaustive]`, so a future status is an additive change and external matches carry a
/// wildcard. [`Unknown`](MutationStatus::Unknown) covers the other direction: a status string this
/// crate has never heard of deserializes into it, carrying the server's own spelling, rather than
/// failing the whole response and discarding every verdict alongside it
/// (`wiki/decisions/012-unknown-mutation-status.decision.md`).
///
/// Serialization is by name and round-trips exactly, `Unknown` included: `"Throttled"` deserializes
/// to `Unknown("Throttled".into())` and serializes back to `"Throttled"`, never to a nested
/// `{"Unknown": …}`. Carrying the string is the point — a status you can neither act on nor name is
/// a stall with no diagnosis. It is also why this type is not `Copy`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
    /// A verdict this crate cannot act on, as the server spelled it.
    ///
    /// Retained, never guessed at. `Delete` would assume the server accepted the mutation and
    /// `DeadLetter` would assume it refused; both lose data when wrong. The record stays queued and
    /// the pass reports an
    /// [`AnomalyKind::UnknownStatus`](crate::runner::AnomalyKind::UnknownStatus) carrying this
    /// string.
    Unknown(String),
}

impl MutationStatus {
    /// Whether this verdict removes the record from the queue, one way or another.
    ///
    /// True for `Applied`, `Duplicate`, and `Rejected`. This is what "progress" means for a sync:
    /// a batch of nothing but `Blocked`, `Pending`, and `Unknown` leaves the queue exactly as it
    /// was.
    pub const fn drains(&self) -> bool {
        matches!(self, Self::Applied | Self::Duplicate | Self::Rejected)
    }

    /// The wire spelling of this status.
    ///
    /// For a known status this is the variant name; for [`Unknown`](MutationStatus::Unknown) it is
    /// whatever the server sent, which is what makes the round trip lossless.
    fn as_wire(&self) -> &str {
        match self {
            Self::Applied => "Applied",
            Self::Duplicate => "Duplicate",
            Self::Rejected => "Rejected",
            Self::Blocked => "Blocked",
            Self::Pending => "Pending",
            Self::Unknown(raw) => raw,
        }
    }
}

// Hand-written rather than derived so that `Unknown` is transparent on the wire. The derive would
// emit `{"Unknown": "Throttled"}` and would refuse to parse a name it did not recognise, which is
// the behaviour decision 012 exists to remove.
impl Serialize for MutationStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for MutationStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Owned, because an unrecognised name has to be kept and a borrowed `&str` cannot outlive
        // a deserializer that unescaped it.
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "Applied" => Self::Applied,
            "Duplicate" => Self::Duplicate,
            "Rejected" => Self::Rejected,
            "Blocked" => Self::Blocked,
            "Pending" => Self::Pending,
            _ => Self::Unknown(raw),
        })
    }
}

/// The server's verdict on one mutation in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct MutationBatchRequest {
    /// The mutations to replay, oldest first.
    pub mutations: Vec<MutationIntent>,
}

impl MutationBatchRequest {
    /// Build a batch request.
    pub fn new(mutations: Vec<MutationIntent>) -> Self {
        Self { mutations }
    }
}

/// The server's verdicts on a pushed batch.
///
/// The server is not obliged to return a result for every mutation sent. A mutation with no result
/// stays queued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MutationBatchResponse {
    /// One verdict per mutation the server ruled on.
    ///
    /// A mutation the batch contained but this list omits stays queued. The runner counts it in
    /// [`SyncReport::retained`](crate::runner::SyncReport::retained) rather than inventing a
    /// verdict for it.
    pub results: Vec<MutationResult>,
}

impl MutationBatchResponse {
    /// Build a batch response.
    pub fn new(results: Vec<MutationResult>) -> Self {
        Self { results }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant survives a round trip, spelled the way the server spelled it.
    ///
    /// The `Serialize`/`Deserialize` pair here is hand-written rather than derived, precisely so
    /// `Unknown` can be transparent — `"Throttled"` in, `"Throttled"` out, never a nested
    /// `{"Unknown": …}`. A derive would produce the nested form and the wire would stop matching
    /// the source protocol for exactly the case decision 012 exists to handle.
    ///
    /// Enumerated rather than sampled, because the failure this catches is a *missing arm*: the
    /// deserializer's match has one line per known status, and a status omitted from it does not
    /// fail — it silently becomes `Unknown("Applied")`, which retains a record the server applied
    /// and keeps re-sending it forever.
    #[test]
    fn every_status_round_trips_through_its_wire_name() {
        let cases = [
            (MutationStatus::Applied, "\"Applied\""),
            (MutationStatus::Duplicate, "\"Duplicate\""),
            (MutationStatus::Rejected, "\"Rejected\""),
            (MutationStatus::Blocked, "\"Blocked\""),
            (MutationStatus::Pending, "\"Pending\""),
            (MutationStatus::Unknown("Throttled".into()), "\"Throttled\""),
        ];

        for (status, wire) in cases {
            let rendered = serde_json::to_string(&status).expect("serialize");
            assert_eq!(rendered, wire, "{status:?} must render as its bare name");

            let parsed: MutationStatus = serde_json::from_str(wire).expect("deserialize");
            assert_eq!(parsed, status, "{wire} must parse back to what wrote it");
        }
    }

    /// An unrecognised status keeps the server's exact bytes, including ones a match arm cannot.
    ///
    /// Case, whitespace and non-ASCII are all preserved, because the string exists to be read by a
    /// human diagnosing a stall. Normalising it would make two different server states look like
    /// one, which is the same argument `ScopeKey` and `CacheVersion` both make.
    #[test]
    fn an_unknown_status_is_kept_verbatim() {
        for raw in ["applied", "APPLIED", "rate limited", "refusé", ""] {
            let wire = serde_json::to_string(raw).expect("quote");
            let parsed: MutationStatus = serde_json::from_str(&wire).expect("deserialize");
            assert_eq!(
                parsed,
                MutationStatus::Unknown(raw.to_owned()),
                "{raw:?} is not a status this crate knows, so it must survive unchanged"
            );
            assert_eq!(serde_json::to_string(&parsed).expect("serialize"), wire);
        }
    }

    /// A response may rule on fewer mutations than were sent, and that must parse.
    ///
    /// The omission is meaningful — the runner retains what the server stayed silent about — so a
    /// response with an empty `results` is well-formed rather than an error.
    #[test]
    fn a_response_may_omit_verdicts() {
        let empty: MutationBatchResponse =
            serde_json::from_str(r#"{"results":[]}"#).expect("deserialize");
        assert!(empty.results.is_empty());
    }
}
