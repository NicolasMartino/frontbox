//! Validation utilities
//!
//! Common validation functions for domain types and DTOs.

use crate::frontend::{Error, Result};

/// Validate a generic name/title field.
pub fn validate_name(name: &str, max_length: usize) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::invalid_input("Name cannot be empty"));
    }

    if name.len() > max_length {
        return Err(Error::invalid_input(format!(
            "Name cannot exceed {} characters",
            max_length
        )));
    }

    Ok(())
}

/// Validate a UUID format.
pub fn validate_uuid(id: &str) -> Result<()> {
    // Basic UUID format validation.
    if id.len() != 36 {
        return Err(Error::invalid_input("Invalid UUID format"));
    }

    let mut parts = id.split('-');
    let expected_lengths = [8_usize, 4, 4, 4, 12];

    for expected_len in expected_lengths {
        let part = parts
            .next()
            .ok_or_else(|| Error::invalid_input("Invalid UUID format"))?;

        if part.len() != expected_len || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::invalid_input("Invalid UUID format"));
        }
    }

    if parts.next().is_some() {
        return Err(Error::invalid_input("Invalid UUID format"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name("Valid name", 100).is_ok());
        assert!(validate_name("  Valid name with spaces  ", 100).is_ok());
        assert!(validate_name("A", 100).is_ok());
        assert!(validate_name(&"a".repeat(100), 100).is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(validate_name("", 100).is_err());
        assert!(validate_name("   ", 100).is_err());
        assert!(validate_name("\t\n", 100).is_err());
        assert!(validate_name(&"a".repeat(101), 100).is_err());
    }

    #[test]
    fn test_validate_uuid_valid() {
        let valid_uuid = "550e8400-e29b-41d4-a716-446655440000";
        assert!(validate_uuid(valid_uuid).is_ok());
    }

    #[test]
    fn test_validate_uuid_invalid() {
        // Wrong length.
        assert!(validate_uuid("invalid").is_err());

        // Missing parts.
        assert!(validate_uuid("550e8400e29b41d4a716446655440000").is_err());

        // Wrong part lengths.
        assert!(validate_uuid("550e84-e29b-41d4-a716-446655440000").is_err());

        // Invalid characters.
        assert!(validate_uuid("550e8400-e29b-41d4-a716-44665544zzzz").is_err());

        // Too many parts.
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000-extra").is_err());
    }

    #[test]
    fn test_validate_errors_preserve_messages() {
        let empty = validate_name("   ", 10).expect_err("blank names should fail");
        assert_eq!(empty.to_string(), "Invalid input: Name cannot be empty");

        let too_long = validate_name("abcdef", 3).expect_err("overlong names should fail");
        assert_eq!(
            too_long.to_string(),
            "Invalid input: Name cannot exceed 3 characters"
        );

        let invalid_uuid =
            validate_uuid("550e8400-e29b-41d4-a716-44665544zzzz").expect_err("bad uuid");
        assert_eq!(
            invalid_uuid.to_string(),
            "Invalid input: Invalid UUID format"
        );
    }
}
