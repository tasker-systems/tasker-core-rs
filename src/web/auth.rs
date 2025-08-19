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

use crate::web::state::AuthConfig;

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

/// JWT claims for worker authentication
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkerClaims {
    /// Subject (worker identifier)
    pub sub: String,
    /// Worker authorized namespaces
    pub worker_namespaces: Vec<String>,
    /// Token issuer
    pub iss: String,
    /// Token audience
    pub aud: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at time (Unix timestamp)
    pub iat: i64,
    /// Worker permissions
    pub permissions: Vec<String>,
}

/// JWT authenticator for worker systems
#[derive(Clone)]
pub struct JwtAuthenticator {
    config: AuthConfig,
    encoding_key: Option<EncodingKey>,
    decoding_key: Option<DecodingKey>,
}

impl JwtAuthenticator {
    /// Create authenticator from configuration
    pub fn from_config(config: &AuthConfig) -> Result<Self, AuthError> {
        if !config.enabled {
            debug!("JWT authentication disabled");
            return Ok(Self {
                config: config.clone(),
                encoding_key: None,
                decoding_key: None,
            });
        }

        if config.jwt_private_key.is_empty() {
            return Err(AuthError::ConfigurationError(
                "JWT private key not configured".to_string(),
            ));
        }
        if config.jwt_public_key.is_empty() {
            return Err(AuthError::ConfigurationError(
                "JWT public key not configured".to_string(),
            ));
        }

        // Parse RSA keys
        let encoding_key = Some(Self::parse_private_key(&config.jwt_private_key)?);
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

    /// Validate a worker JWT token
    pub fn validate_worker_token(&self, token: &str) -> Result<WorkerClaims, AuthError> {
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
