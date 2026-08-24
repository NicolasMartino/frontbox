//! Data Transfer Objects
//!
//! Request/response types for public mutation APIs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::frontend::Error;

/// A stable API error DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        let (code, message) = match err {
            Error::NotFound { message } => ("not_found", message),
            Error::InvalidInput { message } => ("invalid_input", message),
            Error::Internal { message } => ("internal", message),
        };

        Self {
            code: code.to_string(),
            message,
        }
    }
}

// --- Mutation ingestion DTOs ---

/// Client-provided idempotency key for mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct MutationId(Uuid);

impl MutationId {
    /// Create a new `MutationId` with a random UUID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a `MutationId` from an existing UUID string.
    pub fn from_string(id: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(id)?))
    }

    /// Create a `MutationId` from a UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get the string representation.
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for MutationId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for MutationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for MutationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MutationId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// A resource-oriented mutation intent for offline replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MutationIntentDto {
    /// Client-generated mutation ID for idempotency.
    pub mutation_id: MutationId,
    /// Public HTTP method for the mutation.
    pub method: String,
    /// Public BFF path for the mutation.
    pub path: String,
    /// Client timestamp when the mutation was created.
    pub client_datetime: DateTime<Utc>,
    /// Mutation body payload.
    #[schema(value_type = Object)]
    pub body: serde_json::Value,
}

impl MutationIntentDto {
    /// Create a new mutation intent with the current timestamp.
    pub fn new(
        mutation_id: MutationId,
        method: impl Into<String>,
        path: impl Into<String>,
        body: serde_json::Value,
    ) -> Self {
        Self {
            mutation_id,
            method: method.into(),
            path: path.into(),
            client_datetime: Utc::now(),
            body,
        }
    }
}

/// Request to push a batch of mutation intents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MutationBatchRequest {
    pub mutations: Vec<MutationIntentDto>,
}

/// Result classification for a single mutation submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum MutationStatus {
    Applied,
    Duplicate,
    Rejected,
    Blocked,
    Pending,
}

/// Response item for one mutation in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MutationResult {
    pub mutation_id: MutationId,
    pub status: MutationStatus,
    pub error: Option<ApiError>,
}

/// Response returned after pushing a batch of mutation intents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MutationBatchResponse {
    pub results: Vec<MutationResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_from_error() {
        let err = Error::invalid_input("Title is required");
        let api: ApiError = err.into();

        assert_eq!(api.code, "invalid_input");
        assert_eq!(api.message, "Title is required");
    }

    #[test]
    fn test_mutation_id_serialization_roundtrip() {
        let id = MutationId::new();

        let json = serde_json::to_string(&id).unwrap();
        let deserialized: MutationId = serde_json::from_str(&json).unwrap();

        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_mutation_intent_roundtrip() {
        let mutation_id = MutationId::new();
        let intent = MutationIntentDto::new(
            mutation_id,
            "PATCH",
            "/api/v1/exercises/123",
            serde_json::json!({"name": "Bench Press"}),
        );

        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: MutationIntentDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mutation_id, mutation_id);
        assert_eq!(deserialized.method, "PATCH");
        assert_eq!(deserialized.path, "/api/v1/exercises/123");
        assert_eq!(
            deserialized.body,
            serde_json::json!({"name": "Bench Press"})
        );
    }

    #[test]
    fn test_mutation_batch_request_and_response_roundtrip() {
        let mutation_id = MutationId::new();

        let request = MutationBatchRequest {
            mutations: vec![MutationIntentDto::new(
                mutation_id,
                "PUT",
                "/api/v1/me/preferences",
                serde_json::json!({"default_rest_seconds": 120}),
            )],
        };

        let request_json = serde_json::to_string(&request).unwrap();
        let deserialized_request: MutationBatchRequest =
            serde_json::from_str(&request_json).unwrap();
        assert_eq!(request, deserialized_request);

        let response = MutationBatchResponse {
            results: vec![MutationResult {
                mutation_id,
                status: MutationStatus::Applied,
                error: None,
            }],
        };

        let response_json = serde_json::to_string(&response).unwrap();
        let deserialized_response: MutationBatchResponse =
            serde_json::from_str(&response_json).unwrap();
        assert_eq!(response, deserialized_response);
    }

    #[test]
    fn test_mutation_batch_request_new_sets_client_timestamp() {
        let before = Utc::now();
        let mutation = MutationIntentDto::new(
            MutationId::new(),
            "POST",
            "/api/v1/exercises",
            serde_json::json!({"name": "Morning Workout"}),
        );

        assert!(mutation.client_datetime >= before);
    }
}
