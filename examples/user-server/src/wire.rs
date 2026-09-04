//! The sync protocol, written from the spec a second time.
//!
//! # Why this file is a near-copy of `examples/todo-server/src/wire.rs`
//!
//! Because sharing it would retire the property both files exist to hold. `todo-server` hand-writes
//! these shapes from
//! `raw/initial/2026-08-25T083750Z/sources/08-offline-sync.spec.md` and
//! `wiki/decisions/010-batch-wire-format.decision.md` rather than importing frontbox's, so that the
//! two ends of the sync *can* disagree and a test will say so. A second server that imported the
//! first server's types would be a second endpoint, not a second implementation: it would inherit
//! every reading `todo-server` made, including a wrong one.
//!
//! So the duplication is deliberate and it is load-bearing. What it buys is small and real — this
//! file is one more independent reading of the same paragraph of spec, and D4d's observations fail
//! if the two readings differ.
//!
//! What is *not* duplicated is the domain: users, not todos.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A batch of mutations the client is replaying, oldest first.
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchRequest {
    /// The queued mutations, in the order the client wants them evaluated.
    pub mutations: Vec<Mutation>,
}

/// One queued write, as the client persisted it.
///
/// Five keys and no more. `op` rides along when the client set it and is ignored here.
#[derive(Debug, Deserialize, ToSchema)]
pub struct Mutation {
    /// The idempotency key for this mutation.
    pub mutation_id: String,
    /// HTTP method from the original envelope.
    pub method: String,
    /// Request path from the original envelope.
    pub path: String,
    /// RFC 3339, from the *client's* clock. Recorded, never trusted for ordering.
    #[allow(dead_code)]
    pub client_datetime: String,
    /// JSON request body from the original envelope.
    #[schema(value_type = Object)]
    pub body: serde_json::Value,
}

/// One verdict per mutation ruled on.
///
/// A mutation the request contained and this list omits stays queued on the client.
#[derive(Debug, Serialize, ToSchema)]
pub struct BatchResponse {
    /// Verdicts for the mutations this server actually ruled on.
    pub results: Vec<MutationResult>,
}

/// How this server classified one mutation.
#[derive(Debug, Serialize, ToSchema)]
pub struct MutationResult {
    /// Which mutation this verdict is for.
    pub mutation_id: String,
    /// How the mutation was classified.
    pub status: Status,
    /// The refusal payload, when the status is `Rejected`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Rejection>,
}

impl MutationResult {
    /// Build a non-refusal verdict.
    pub fn new(mutation_id: impl Into<String>, status: Status) -> Self {
        Self {
            mutation_id: mutation_id.into(),
            status,
            error: None,
        }
    }

    /// Build a terminal refusal verdict.
    pub fn rejected(mutation_id: impl Into<String>, error: Rejection) -> Self {
        Self {
            mutation_id: mutation_id.into(),
            status: Status::Rejected,
            error: Some(error),
        }
    }
}

/// PascalCase on the wire, which is what the client parses.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub enum Status {
    /// The server applied the mutation.
    Applied,
    /// The mutation id had already been applied.
    Duplicate,
    /// The server terminally refused the mutation.
    Rejected,
    /// The server skipped this mutation behind another one.
    #[allow(dead_code)]
    Blocked,
    /// The server accepted the mutation but has not finished it.
    Pending,
}

/// The payload a refusal carries, for a human to read out of a dead letter.
#[derive(Debug, Serialize, ToSchema)]
pub struct Rejection {
    /// Machine-readable refusal code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Human-readable explanation.
    pub message: String,
    /// Field-level or domain-specific detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub details: Option<serde_json::Value>,
}

impl Rejection {
    /// Build a refusal payload.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: Some(code.to_owned()),
            message: message.into(),
            details: None,
        }
    }

    /// Attach structured detail.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// A user as the read endpoints return it.
///
/// # "User" means two things in this trial, and this is the server's one
///
/// This is the **user record**: a row on this server, created by a mutation, with an id the client
/// generated. It is not the [`ScopeKey`] — the local queue identity the client composes with no
/// server involved (`wiki/decisions/009-local-scope-identity.decision.md`). The whole point of
/// decision 009 is that the second does not wait for the first, which is why an offline signup can
/// queue work under a scope for a user this server has never heard of.
///
/// [`ScopeKey`]: https://docs.rs/frontbox
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct User {
    /// Client-generated row id.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// The opaque per-entity versions this server holds.
///
/// One call answers for every entity the server owns, which is why the client's staleness contract
/// is a *budget* per entity rather than a schedule per entity — see
/// `wiki/proposals/invalidation-delivery.proposal.md`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Versions {
    /// The version of the `user` entity. Opaque: compare for equality, never order.
    pub user: String,
}
