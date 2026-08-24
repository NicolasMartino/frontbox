//! Error types
//!
//! Shared error structures for API surface and internal error handling.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Shared error type for API responses and internal error handling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub enum Error {
    /// Resource not found.
    NotFound { message: String },
    /// Invalid input data.
    InvalidInput { message: String },
    /// Internal server error.
    Internal { message: String },
}

impl Error {
    /// Create a new not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    /// Create a new invalid input error.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    /// Create a new internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Get the error message.
    pub fn message(&self) -> &str {
        match self {
            Error::NotFound { message } => message,
            Error::InvalidInput { message } => message,
            Error::Internal { message } => message,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound { message } => write!(f, "Not found: {}", message),
            Error::InvalidInput { message } => write!(f, "Invalid input: {}", message),
            Error::Internal { message } => write!(f, "Internal error: {}", message),
        }
    }
}

impl std::error::Error for Error {}

/// Support conversion from string slices for backward compatibility.
///
/// This maintains the existing `Err("...".into())` pattern.
impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Self::internal(msg)
    }
}

/// Support conversion from `String` for convenience.
impl From<String> for Error {
    fn from(msg: String) -> Self {
        Self::internal(msg)
    }
}

/// Result type alias for convenience.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let not_found = Error::not_found("Todo not found");
        assert_eq!(
            not_found,
            Error::NotFound {
                message: "Todo not found".to_string()
            }
        );

        let invalid = Error::invalid_input("Title is required");
        assert_eq!(
            invalid,
            Error::InvalidInput {
                message: "Title is required".to_string()
            }
        );

        let internal = Error::internal("Database connection failed");
        assert_eq!(
            internal,
            Error::Internal {
                message: "Database connection failed".to_string()
            }
        );
    }

    #[test]
    fn test_error_from_str() {
        let error: Error = "Something went wrong".into();
        assert_eq!(
            error,
            Error::Internal {
                message: "Something went wrong".to_string()
            }
        );
    }

    #[test]
    fn test_error_from_string() {
        let error: Error = "Something went wrong".to_string().into();
        assert_eq!(
            error,
            Error::Internal {
                message: "Something went wrong".to_string()
            }
        );
    }

    #[test]
    fn test_error_display() {
        let not_found = Error::not_found("Todo not found");
        assert_eq!(not_found.to_string(), "Not found: Todo not found");

        let invalid = Error::invalid_input("Title is required");
        assert_eq!(invalid.to_string(), "Invalid input: Title is required");

        let internal = Error::internal("Database failed");
        assert_eq!(internal.to_string(), "Internal error: Database failed");
    }

    #[test]
    fn test_error_serialization() {
        let error = Error::not_found("Todo not found");
        let json = serde_json::to_string(&error).unwrap();
        let deserialized: Error = serde_json::from_str(&json).unwrap();
        assert_eq!(error, deserialized);
    }

    #[test]
    fn test_error_message() {
        let error = Error::invalid_input("Title cannot be empty");
        assert_eq!(error.message(), "Title cannot be empty");
    }
}
