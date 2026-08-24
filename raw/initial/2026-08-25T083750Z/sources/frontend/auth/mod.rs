//! Authentication types for JWT token handling.
//!
//! This module contains wasm-compatible types for JWT claims and auth errors.
//! The actual token validation logic is in `shared-backend` (native-only).

mod claims;
mod error;

pub use claims::{Aud, Claims};
pub use error::AuthError;
