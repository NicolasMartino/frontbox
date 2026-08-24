//! Frontend-specific types and utilities.
//!
//! This module contains types that are primarily used by frontend applications,
//! including DTOs, error handling, and SSE events.

pub mod auth;
pub mod dto;
pub mod error;
pub mod sse;
pub mod validation;
pub mod workout;

// Re-export commonly used types
pub use auth::{AuthError, Claims};
pub use dto::{
    ApiError, MutationBatchRequest, MutationBatchResponse, MutationId, MutationIntentDto,
    MutationResult, MutationStatus,
};
pub use error::{Error, Result};
pub use sse::InvalidationEvent;
pub use validation::{validate_name, validate_uuid};
pub use workout::Template;
