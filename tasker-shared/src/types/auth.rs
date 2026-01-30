//! # JWT Authentication
//!
//! JWT-based authentication system for worker systems using RSA public/private key pairs.

use axum::http::HeaderValue;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    pkcs8::DecodePrivateKey,
    pkcs8::DecodePublicKey,
    RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, warn};

use crate::config::web::WebAuthConfig;

/// JWT authentication errors
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("RSA key parsing error: {0}")]
    KeyParsingError(String),

    #[error("Token validation error: {0}")]
    TokenValidationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("JWT processing error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Missing authorization header")]
    MissingAuthHeader,

    #[error("Invalid authorization header format")]
    InvalidAuthFormat,

    #[error("Insufficient permissions")]
    InsufficientPermissions,
}

/// JWT claims for authenticated requests.
///
/// Used for both worker and general API authentication.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenClaims {
    /// Subject (user/service identifier)
    pub sub: String,
    /// Worker authorized namespaces (optional for non-worker tokens)
    #[serde(default)]
    pub worker_namespaces: Vec<String>,
    /// Token issuer
    pub iss: String,
    /// Token audience
    pub aud: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at time (Unix timestamp)
    pub iat: i64,
    /// Permissions granted by this token
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Backward-compatible alias for `TokenClaims`.
pub type WorkerClaims = TokenClaims;

/// JWT authenticator for worker systems
#[derive(Clone)]
pub struct JwtAuthenticator {
    config: WebAuthConfig,
    encoding_key: Option<EncodingKey>,
    decoding_key: Option<DecodingKey>,
}

impl std::fmt::Debug for JwtAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtAuthenticator")
            .field("enabled", &self.config.enabled)
            .field("has_encoding_key", &self.encoding_key.is_some())
            .field("has_decoding_key", &self.decoding_key.is_some())
            .field("api_key_configured", &!self.config.api_key.is_empty())
            .finish()
    }
}

impl JwtAuthenticator {
    /// Create authenticator from configuration
    pub fn from_config(config: &WebAuthConfig) -> Result<Self, AuthError> {
        if !config.enabled {
            debug!("JWT authentication disabled");
            return Ok(Self {
                config: config.clone(),
                encoding_key: None,
                decoding_key: None,
            });
        }

        if config.jwt_public_key.is_empty() {
            return Err(AuthError::ConfigurationError(
                "JWT public key not configured".to_string(),
            ));
        }

        // Parse RSA keys (private key is optional - only needed for token generation)
        let encoding_key = if config.jwt_private_key.is_empty() {
            None
        } else {
            Some(Self::parse_private_key(&config.jwt_private_key)?)
        };
        let decoding_key = Some(Self::parse_public_key(&config.jwt_public_key)?);

        debug!("JWT authenticator configured with RSA keys");

        Ok(Self {
            config: config.clone(),
            encoding_key,
            decoding_key,
        })
    }

    /// Parse RSA private key from PEM string
    fn parse_private_key(pem_str: &str) -> Result<EncodingKey, AuthError> {
        // Try PKCS#8 format first (modern standard)
        if let Ok(key) = RsaPrivateKey::from_pkcs8_pem(pem_str) {
            let der = key.to_pkcs1_der().map_err(|e| {
                AuthError::KeyParsingError(format!("Failed to convert private key to DER: {e}"))
            })?;
            return Ok(EncodingKey::from_rsa_der(der.as_bytes()));
        }

        // Fall back to PKCS#1 format (legacy)
        if let Ok(key) = RsaPrivateKey::from_pkcs1_pem(pem_str) {
            let der = key.to_pkcs1_der().map_err(|e| {
                AuthError::KeyParsingError(format!("Failed to convert private key to DER: {e}"))
            })?;
            return Ok(EncodingKey::from_rsa_der(der.as_bytes()));
        }

        Err(AuthError::KeyParsingError(
            "Failed to parse RSA private key from PEM".to_string(),
        ))
    }

    /// Parse RSA public key from PEM string
    fn parse_public_key(pem_str: &str) -> Result<DecodingKey, AuthError> {
        // Try PKCS#8 format first (modern standard)
        if let Ok(key) = RsaPublicKey::from_public_key_pem(pem_str) {
            let der = key.to_pkcs1_der().map_err(|e| {
                AuthError::KeyParsingError(format!("Failed to convert public key to DER: {e}"))
            })?;
            return Ok(DecodingKey::from_rsa_der(der.as_bytes()));
        }

        // Fall back to PKCS#1 format (legacy)
        if let Ok(key) = RsaPublicKey::from_pkcs1_pem(pem_str) {
            let der = key.to_pkcs1_der().map_err(|e| {
                AuthError::KeyParsingError(format!("Failed to convert public key to DER: {e}"))
            })?;
            return Ok(DecodingKey::from_rsa_der(der.as_bytes()));
        }

        Err(AuthError::KeyParsingError(
            "Failed to parse RSA public key from PEM".to_string(),
        ))
    }

    /// Validate a JWT token and extract claims.
    pub fn validate_token(&self, token: &str) -> Result<TokenClaims, AuthError> {
        self.validate_worker_token(token)
    }

    /// Validate a worker JWT token
    pub fn validate_worker_token(&self, token: &str) -> Result<TokenClaims, AuthError> {
        if !self.config.enabled {
            // Return a default claims object when auth is disabled
            return Ok(WorkerClaims {
                sub: "test-worker".to_string(),
                worker_namespaces: vec!["*".to_string()],
                iss: self.config.jwt_issuer.clone(),
                aud: self.config.jwt_audience.clone(),
                exp: chrono::Utc::now().timestamp() + 3600,
                iat: chrono::Utc::now().timestamp(),
                permissions: vec!["*".to_string()],
            });
        }

        let decoding_key = self.decoding_key.as_ref().ok_or_else(|| {
            AuthError::ConfigurationError("Decoding key not configured".to_string())
        })?;

        debug!(token_length = token.len(), "Validating worker JWT token");

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.config.jwt_issuer]);
        validation.set_audience(&[&self.config.jwt_audience]);
        validation.validate_exp = true;
        validation.validate_nbf = false; // Not using 'not before' field

        let token_data = decode::<WorkerClaims>(token, decoding_key, &validation).map_err(|e| {
            warn!(error = %e, "JWT token validation failed");
            AuthError::JwtError(e)
        })?;

        debug!(
            worker_id = %token_data.claims.sub,
            namespaces = ?token_data.claims.worker_namespaces,
            permissions = ?token_data.claims.permissions,
            "Worker token validated successfully"
        );

        Ok(token_data.claims)
    }

    /// Generate a JWT token with the given subject, namespaces, and permissions.
    pub fn generate_token(
        &self,
        subject: &str,
        permissions: Vec<String>,
    ) -> Result<String, AuthError> {
        self.generate_worker_token(subject, vec![], permissions)
    }

    /// Generate a JWT token for a worker
    pub fn generate_worker_token(
        &self,
        worker_id: &str,
        namespaces: Vec<String>,
        permissions: Vec<String>,
    ) -> Result<String, AuthError> {
        if !self.config.enabled {
            return Ok(format!("test-token-{worker_id}"));
        }

        let encoding_key = self.encoding_key.as_ref().ok_or_else(|| {
            AuthError::ConfigurationError("Encoding key not configured".to_string())
        })?;

        let now = Utc::now();
        let expiry = now + Duration::hours(self.config.jwt_token_expiry_hours as i64);

        let claims = WorkerClaims {
            sub: worker_id.to_string(),
            worker_namespaces: namespaces,
            iss: self.config.jwt_issuer.clone(),
            aud: self.config.jwt_audience.clone(),
            exp: expiry.timestamp(),
            iat: now.timestamp(),
            permissions,
        };

        debug!(
            worker_id = %worker_id,
            expiry_timestamp = expiry.timestamp(),
            namespaces = ?claims.worker_namespaces,
            permissions = ?claims.permissions,
            "Generating JWT token for worker"
        );

        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, encoding_key).map_err(|e| {
            error!(error = %e, "Failed to generate JWT token");
            AuthError::JwtError(e)
        })?;

        debug!(
            token_length = token.len(),
            "JWT token generated successfully"
        );
        Ok(token)
    }

    /// Extract bearer token from Authorization header
    pub fn extract_bearer_token(auth_header: &HeaderValue) -> Result<&str, AuthError> {
        let auth_str = auth_header
            .to_str()
            .map_err(|_| AuthError::InvalidAuthFormat)?;

        if !auth_str.starts_with("Bearer ") {
            return Err(AuthError::InvalidAuthFormat);
        }

        Ok(&auth_str[7..]) // Remove "Bearer " prefix
    }

    /// Check if worker has permission to access a namespace
    pub fn has_namespace_access(&self, claims: &WorkerClaims, namespace: &str) -> bool {
        claims.worker_namespaces.contains(&namespace.to_string())
            || claims.worker_namespaces.contains(&"*".to_string()) // Wildcard access
    }

    /// Check if worker has a specific permission
    pub fn has_permission(&self, claims: &WorkerClaims, permission: &str) -> bool {
        claims.permissions.contains(&permission.to_string())
            || claims.permissions.contains(&"*".to_string()) // Admin access
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: build a disabled WebAuthConfig using Default
    // -----------------------------------------------------------------------
    fn disabled_auth_config() -> WebAuthConfig {
        WebAuthConfig::default() // enabled: false by default
    }

    // -----------------------------------------------------------------------
    // Helper: parse RSA keys from config/dotenv/test.env
    //
    // The keys in test.env are stored as single-line PEM strings. We need to
    // reformat the base64 body with newlines every 64 characters so the RSA
    // PEM parser can handle them.
    // -----------------------------------------------------------------------
    fn format_pem(raw: &str) -> String {
        // Strip surrounding quotes if present
        let raw = raw.trim().trim_matches('"');

        // Detect header/footer markers
        let (header, footer, body) = if raw.contains("-----BEGIN RSA PRIVATE KEY-----") {
            let h = "-----BEGIN RSA PRIVATE KEY-----";
            let f = "-----END RSA PRIVATE KEY-----";
            let body = raw.replace(h, "").replace(f, "").replace(['\n', '\r'], "");
            (h, f, body)
        } else if raw.contains("-----BEGIN PUBLIC KEY-----") {
            let h = "-----BEGIN PUBLIC KEY-----";
            let f = "-----END PUBLIC KEY-----";
            let body = raw.replace(h, "").replace(f, "").replace(['\n', '\r'], "");
            (h, f, body)
        } else {
            panic!("Unknown PEM format: {}", &raw[..40]);
        };

        // Insert newlines every 64 characters in the base64 body
        let wrapped: String = body
            .as_bytes()
            .chunks(64)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<&str>>()
            .join("\n");

        format!("{header}\n{wrapped}\n{footer}\n")
    }

    /// Read and parse RSA keys from config/dotenv/test.env
    fn load_test_rsa_keys() -> (String, String) {
        let env_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../config/dotenv/test.env");
        let content =
            std::fs::read_to_string(env_path).expect("Failed to read config/dotenv/test.env");

        let mut private_key = String::new();
        let mut public_key = String::new();

        for line in content.lines() {
            if let Some(val) = line.strip_prefix("JWT_PRIVATE_KEY=") {
                private_key = format_pem(val);
            } else if let Some(val) = line.strip_prefix("JWT_PUBLIC_KEY=") {
                public_key = format_pem(val);
            }
        }

        assert!(
            !private_key.is_empty(),
            "JWT_PRIVATE_KEY not found in test.env"
        );
        assert!(
            !public_key.is_empty(),
            "JWT_PUBLIC_KEY not found in test.env"
        );

        (private_key, public_key)
    }

    // ===================================================================
    // 1. AuthError Display variants
    // ===================================================================
    #[test]
    fn test_auth_error_display_key_parsing_error() {
        let e = AuthError::KeyParsingError("bad key".to_string());
        assert_eq!(e.to_string(), "RSA key parsing error: bad key");
    }

    #[test]
    fn test_auth_error_display_token_validation_error() {
        let e = AuthError::TokenValidationError("expired".to_string());
        assert_eq!(e.to_string(), "Token validation error: expired");
    }

    #[test]
    fn test_auth_error_display_configuration_error() {
        let e = AuthError::ConfigurationError("missing key".to_string());
        assert_eq!(e.to_string(), "Configuration error: missing key");
    }

    #[test]
    fn test_auth_error_display_jwt_error() {
        // Create a real jsonwebtoken error by decoding garbage
        let result = jsonwebtoken::decode::<TokenClaims>(
            "not-a-token",
            &jsonwebtoken::DecodingKey::from_secret(b"secret"),
            &jsonwebtoken::Validation::default(),
        );
        let jwt_err = result.unwrap_err();
        let e = AuthError::JwtError(jwt_err);
        assert!(e.to_string().starts_with("JWT processing error:"));
    }

    #[test]
    fn test_auth_error_display_invalid_token() {
        let e = AuthError::InvalidToken("corrupted".to_string());
        assert_eq!(e.to_string(), "Invalid token: corrupted");
    }

    #[test]
    fn test_auth_error_display_missing_auth_header() {
        let e = AuthError::MissingAuthHeader;
        assert_eq!(e.to_string(), "Missing authorization header");
    }

    #[test]
    fn test_auth_error_display_invalid_auth_format() {
        let e = AuthError::InvalidAuthFormat;
        assert_eq!(e.to_string(), "Invalid authorization header format");
    }

    #[test]
    fn test_auth_error_display_insufficient_permissions() {
        let e = AuthError::InsufficientPermissions;
        assert_eq!(e.to_string(), "Insufficient permissions");
    }

    // ===================================================================
    // 2. TokenClaims serde roundtrip
    // ===================================================================
    #[test]
    fn test_token_claims_serde_roundtrip() {
        let claims = TokenClaims {
            sub: "worker-42".to_string(),
            worker_namespaces: vec!["ns1".to_string(), "ns2".to_string()],
            iss: "tasker-core".to_string(),
            aud: "tasker-api".to_string(),
            exp: 1_700_000_000,
            iat: 1_699_990_000,
            permissions: vec!["tasks:read".to_string(), "tasks:create".to_string()],
        };

        let json = serde_json::to_string(&claims).expect("serialize");
        let restored: TokenClaims = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.sub, "worker-42");
        assert_eq!(restored.worker_namespaces, vec!["ns1", "ns2"]);
        assert_eq!(restored.iss, "tasker-core");
        assert_eq!(restored.aud, "tasker-api");
        assert_eq!(restored.exp, 1_700_000_000);
        assert_eq!(restored.iat, 1_699_990_000);
        assert_eq!(restored.permissions, vec!["tasks:read", "tasks:create"]);
    }

    // ===================================================================
    // 3. JwtAuthenticator::from_config() with disabled auth
    // ===================================================================
    #[test]
    fn test_from_config_disabled_auth_returns_ok_no_keys() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).expect("should succeed");

        // With disabled auth, no keys should be present
        assert!(auth.encoding_key.is_none());
        assert!(auth.decoding_key.is_none());
        assert!(!auth.config.enabled);
    }

    // ===================================================================
    // 4. JwtAuthenticator::from_config() enabled but no public key
    // ===================================================================
    #[test]
    fn test_from_config_enabled_no_public_key_returns_config_error() {
        let config = WebAuthConfig {
            enabled: true,
            jwt_public_key: String::new(),
            ..WebAuthConfig::default()
        };

        let result = JwtAuthenticator::from_config(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Configuration error"),
            "Expected ConfigurationError, got: {msg}"
        );
        assert!(
            msg.contains("JWT public key not configured"),
            "Unexpected message: {msg}"
        );
    }

    // ===================================================================
    // 5. JwtAuthenticator Debug impl
    // ===================================================================
    #[test]
    fn test_jwt_authenticator_debug_shows_enabled_and_has_keys_not_contents() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).expect("should succeed");

        let debug_str = format!("{auth:?}");

        assert!(
            debug_str.contains("enabled"),
            "Debug should contain 'enabled'"
        );
        assert!(
            debug_str.contains("has_encoding_key"),
            "Debug should contain 'has_encoding_key'"
        );
        assert!(
            debug_str.contains("has_decoding_key"),
            "Debug should contain 'has_decoding_key'"
        );
        // Must NOT leak actual key material
        assert!(
            !debug_str.contains("BEGIN"),
            "Debug must not contain PEM key contents"
        );
        assert!(
            !debug_str.contains("RSA"),
            "Debug must not contain RSA key material"
        );
    }

    // ===================================================================
    // 6. validate_worker_token() with disabled auth
    // ===================================================================
    #[test]
    fn test_validate_worker_token_disabled_returns_default_claims() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).expect("should succeed");

        let claims = auth
            .validate_worker_token("any-token-value")
            .expect("should succeed with disabled auth");

        assert_eq!(claims.sub, "test-worker");
        assert_eq!(claims.worker_namespaces, vec!["*"]);
        assert_eq!(claims.permissions, vec!["*"]);
        assert_eq!(claims.iss, "tasker-core"); // default issuer
        assert_eq!(claims.aud, "tasker-api"); // default audience
    }

    // ===================================================================
    // 7. generate_worker_token() with disabled auth
    // ===================================================================
    #[test]
    fn test_generate_worker_token_disabled_returns_test_token() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).expect("should succeed");

        let token = auth
            .generate_worker_token("worker-99", vec![], vec![])
            .expect("should succeed with disabled auth");

        assert_eq!(token, "test-token-worker-99");
    }

    // ===================================================================
    // 8. generate_worker_token() + validate_worker_token() roundtrip
    //    with real RSA keys from config/dotenv/test.env
    // ===================================================================
    #[test]
    fn test_generate_and_validate_token_roundtrip_with_real_keys() {
        let (private_key, public_key) = load_test_rsa_keys();

        let config = WebAuthConfig {
            enabled: true,
            jwt_private_key: private_key,
            jwt_public_key: public_key,
            jwt_issuer: "tasker-core-test".to_string(),
            jwt_audience: "tasker-api-test".to_string(),
            jwt_token_expiry_hours: 1,
            ..WebAuthConfig::default()
        };

        let auth = JwtAuthenticator::from_config(&config)
            .expect("should build authenticator with real RSA keys");

        let namespaces = vec!["production".to_string(), "staging".to_string()];
        let permissions = vec!["tasks:read".to_string(), "tasks:create".to_string()];

        let token = auth
            .generate_worker_token("worker-roundtrip", namespaces.clone(), permissions.clone())
            .expect("should generate token");

        assert!(!token.is_empty());
        assert!(token.contains('.'), "JWT should contain dot separators");

        let claims = auth
            .validate_worker_token(&token)
            .expect("should validate generated token");

        assert_eq!(claims.sub, "worker-roundtrip");
        assert_eq!(claims.worker_namespaces, namespaces);
        assert_eq!(claims.permissions, permissions);
        assert_eq!(claims.iss, "tasker-core-test");
        assert_eq!(claims.aud, "tasker-api-test");
        assert!(claims.exp > claims.iat, "exp should be after iat");
    }

    // ===================================================================
    // 9. extract_bearer_token()
    // ===================================================================
    #[test]
    fn test_extract_bearer_token_valid() {
        let header = HeaderValue::from_static("Bearer my-secret-token");
        let token = JwtAuthenticator::extract_bearer_token(&header).expect("should extract");
        assert_eq!(token, "my-secret-token");
    }

    #[test]
    fn test_extract_bearer_token_missing_bearer_prefix() {
        let header = HeaderValue::from_static("Token my-secret-token");
        let result = JwtAuthenticator::extract_bearer_token(&header);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AuthError::InvalidAuthFormat),
            "Expected InvalidAuthFormat, got: {err}"
        );
    }

    #[test]
    fn test_extract_bearer_token_basic_auth_rejected() {
        let header = HeaderValue::from_static("Basic dXNlcjpwYXNz");
        let result = JwtAuthenticator::extract_bearer_token(&header);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::InvalidAuthFormat));
    }

    // ===================================================================
    // 10. has_namespace_access()
    // ===================================================================
    #[test]
    fn test_has_namespace_access_exact_match() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec!["production".to_string(), "staging".to_string()],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec![],
        };

        assert!(auth.has_namespace_access(&claims, "production"));
        assert!(auth.has_namespace_access(&claims, "staging"));
    }

    #[test]
    fn test_has_namespace_access_wildcard() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec!["*".to_string()],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec![],
        };

        assert!(auth.has_namespace_access(&claims, "any-namespace"));
        assert!(auth.has_namespace_access(&claims, "production"));
    }

    #[test]
    fn test_has_namespace_access_no_match() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec!["production".to_string()],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec![],
        };

        assert!(!auth.has_namespace_access(&claims, "staging"));
        assert!(!auth.has_namespace_access(&claims, "development"));
    }

    // ===================================================================
    // 11. has_permission()
    // ===================================================================
    #[test]
    fn test_has_permission_exact_match() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec![],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec!["tasks:read".to_string(), "tasks:create".to_string()],
        };

        assert!(auth.has_permission(&claims, "tasks:read"));
        assert!(auth.has_permission(&claims, "tasks:create"));
    }

    #[test]
    fn test_has_permission_wildcard() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec![],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec!["*".to_string()],
        };

        assert!(auth.has_permission(&claims, "tasks:read"));
        assert!(auth.has_permission(&claims, "dlq:update"));
        assert!(auth.has_permission(&claims, "anything"));
    }

    #[test]
    fn test_has_permission_no_match() {
        let config = disabled_auth_config();
        let auth = JwtAuthenticator::from_config(&config).unwrap();

        let claims = TokenClaims {
            sub: "worker-1".to_string(),
            worker_namespaces: vec![],
            iss: "test".to_string(),
            aud: "test".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec!["tasks:read".to_string()],
        };

        assert!(!auth.has_permission(&claims, "tasks:create"));
        assert!(!auth.has_permission(&claims, "dlq:read"));
    }
}
