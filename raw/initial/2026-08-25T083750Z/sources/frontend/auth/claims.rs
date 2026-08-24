//! JWT claims extracted from Keycloak access tokens.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims extracted from Keycloak access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID in Keycloak) - stored as String, looks like UUID.
    /// Keycloak returns this as a string, safer to not assume format.
    pub sub: String,
    /// Preferred username.
    pub preferred_username: Option<String>,
    /// Email.
    pub email: Option<String>,
    /// Token expiration (Unix timestamp).
    pub exp: u64,
    /// Token issued at (Unix timestamp).
    pub iat: u64,
    /// Issuer (Keycloak realm URL).
    pub iss: String,
    /// Audience (client ID) - optional as some tokens may not have it.
    #[serde(default)]
    pub aud: Option<Aud>,
}

/// Audience claim - can be single string or array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Aud {
    Single(String),
    Multiple(Vec<String>),
}

impl Claims {
    /// Create new claims (primarily for testing).
    pub fn new(user_id: Uuid, username: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: user_id.to_string(),
            preferred_username: Some(username),
            email: None,
            exp: now + 3600, // 1 hour from now
            iat: now,
            iss: "test".to_string(),
            aud: Some(Aud::Single("test".to_string())),
        }
    }

    /// Parse sub as UUID for use as owner_id.
    /// Panics if sub is not a valid UUID (shouldn't happen with Keycloak).
    pub fn user_id(&self) -> Uuid {
        Uuid::parse_str(&self.sub).expect("Keycloak sub should be valid UUID")
    }

    /// Get sub as string (for logging, DB keys, etc.).
    pub fn sub_str(&self) -> &str {
        &self.sub
    }

    /// Get preferred username if available.
    pub fn username(&self) -> Option<&str> {
        self.preferred_username.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_new_populates_expected_defaults() {
        let user_id = Uuid::new_v4();
        let claims = Claims::new(user_id, "nicolas".to_string());

        assert_eq!(claims.user_id(), user_id);
        assert_eq!(claims.sub_str(), user_id.to_string());
        assert_eq!(claims.username(), Some("nicolas"));
        assert_eq!(claims.email, None);
        assert_eq!(claims.iss, "test");
        assert!(claims.exp >= claims.iat);
        assert_eq!(claims.exp - claims.iat, 3600);
        assert!(matches!(claims.aud, Some(Aud::Single(ref aud)) if aud == "test"));
    }

    #[test]
    fn claims_support_multiple_audiences_and_optional_username() {
        let user_id = Uuid::new_v4();
        let claims: Claims = serde_json::from_value(serde_json::json!({
            "sub": user_id,
            "preferred_username": null,
            "email": "user@example.com",
            "exp": 42,
            "iat": 21,
            "iss": "http://issuer/realm",
            "aud": ["web", "mobile"]
        }))
        .expect("claims should deserialize");

        assert_eq!(claims.user_id(), user_id);
        assert_eq!(claims.username(), None);
        assert_eq!(claims.sub_str(), user_id.to_string());
        assert!(matches!(
            claims.aud,
            Some(Aud::Multiple(ref audiences)) if audiences == &vec!["web".to_string(), "mobile".to_string()]
        ));
    }
}
