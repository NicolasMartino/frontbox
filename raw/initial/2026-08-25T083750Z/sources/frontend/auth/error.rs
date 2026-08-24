//! Authentication errors.

use thiserror::Error;

/// Authentication and authorization errors.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing authorization header")]
    MissingToken,

    #[error("Invalid authorization header format")]
    InvalidHeader,

    #[error("Token validation failed: {0}")]
    ValidationFailed(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Failed to fetch JWKS: {0}")]
    JwksFetchError(String),

    #[error("Invalid issuer")]
    InvalidIssuer,

    #[error("Invalid audience")]
    InvalidAudience,
}
