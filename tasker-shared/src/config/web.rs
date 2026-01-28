use serde::{Deserialize, Serialize};

// Import web config structs from V2
pub use crate::config::tasker::{OrchestrationWebConfig, ResilienceConfig, WebDatabasePoolsConfig};

// Type aliases for backward compatibility (legacy names → V2 names)
pub type WebConfig = OrchestrationWebConfig;
// TAS-61: Removed WebTlsConfig - web servers run plain HTTP only
// TAS-61: Removed WebCorsConfig - CORS uses hardcoded values in middleware
// TAS-61: Removed WebRateLimitConfig - no rate limiting middleware implemented
pub type WebResilienceConfig = ResilienceConfig;

/// Web API authentication configuration
///
/// This is an adapter over V2's AuthConfig for runtime use.
///
/// TAS-177: Removed protected_routes - superseded by SecurityContext permissions.
/// Route protection is now handled via SecurityContext and permission checks in handlers.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebAuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,

    /// JWT issuer
    pub jwt_issuer: String,

    /// JWT audience
    pub jwt_audience: String,

    /// JWT token expiry in hours
    pub jwt_token_expiry_hours: u64,

    /// JWT private key (inline PEM)
    pub jwt_private_key: String,

    /// JWT public key (inline PEM)
    pub jwt_public_key: String,

    /// JWT verification method: "public_key" or "jwks"
    pub jwt_verification_method: String,

    /// Path to JWT public key file
    pub jwt_public_key_path: String,

    /// JWKS endpoint URL
    pub jwks_url: String,

    /// JWKS refresh interval in seconds
    pub jwks_refresh_interval_seconds: u32,

    /// JWT claim name containing permissions
    pub permissions_claim: String,

    /// Reject tokens with unknown permissions
    pub strict_validation: bool,

    /// Log unknown permissions
    pub log_unknown_permissions: bool,

    /// Pre-existing Bearer token for client-side use (e.g., from env var)
    #[serde(default)]
    pub bearer_token: String,

    /// Legacy single API key (backward compat)
    pub api_key: String,

    /// API key header name
    pub api_key_header: String,

    /// Enable multiple API key support
    pub api_keys_enabled: bool,

    /// Multiple API keys with per-key permissions
    #[serde(default)]
    pub api_keys: Vec<crate::config::tasker::ApiKeyConfig>,
}

impl Default for WebAuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_issuer: "tasker-core".to_string(),
            jwt_audience: "tasker-api".to_string(),
            jwt_token_expiry_hours: 24,
            jwt_private_key: String::new(),
            jwt_public_key: String::new(),
            jwt_verification_method: "public_key".to_string(),
            jwt_public_key_path: String::new(),
            jwks_url: String::new(),
            jwks_refresh_interval_seconds: 3600,
            permissions_claim: "permissions".to_string(),
            strict_validation: true,
            log_unknown_permissions: true,
            bearer_token: String::new(),
            api_key: String::new(),
            api_key_header: "X-API-Key".to_string(),
            api_keys_enabled: false,
            api_keys: Vec::new(),
        }
    }
}

// TAS-61 Phase 6C/6D: Conversion from V2 AuthConfig to legacy WebAuthConfig
impl From<crate::config::tasker::AuthConfig> for WebAuthConfig {
    fn from(v2: crate::config::tasker::AuthConfig) -> Self {
        Self {
            enabled: v2.enabled,
            jwt_issuer: v2.jwt_issuer,
            jwt_audience: v2.jwt_audience,
            jwt_token_expiry_hours: v2.jwt_token_expiry_hours as u64,
            jwt_private_key: v2.jwt_private_key,
            jwt_public_key: v2.jwt_public_key,
            jwt_verification_method: v2.jwt_verification_method,
            jwt_public_key_path: v2.jwt_public_key_path,
            jwks_url: v2.jwks_url,
            jwks_refresh_interval_seconds: v2.jwks_refresh_interval_seconds,
            permissions_claim: v2.permissions_claim,
            strict_validation: v2.strict_validation,
            log_unknown_permissions: v2.log_unknown_permissions,
            bearer_token: String::new(),
            api_key: v2.api_key,
            api_key_header: v2.api_key_header,
            api_keys_enabled: v2.api_keys_enabled,
            api_keys: v2.api_keys,
        }
    }
}

// Note: WebConfig is now a type alias for OrchestrationWebConfig, so no From impl needed
// (From<T> for T is automatically implemented)
