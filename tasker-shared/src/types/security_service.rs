//! # Security Service
//!
//! Unified authentication service combining JWT (static key + JWKS) and API key
//! authentication methods. Provides a single entry point for request authentication.

use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, warn};

use crate::config::tasker::AuthConfig;
use crate::types::auth::{AuthError, JwtAuthenticator, TokenClaims};
use crate::types::jwks::JwksKeyStore;
use crate::types::permissions::validate_permissions;
use crate::types::security::{AuthMethod, SecurityContext};

use super::api_key_auth::ApiKeyRegistry;

/// Unified authentication service.
///
/// Combines JWT (static key or JWKS) and API key authentication into a single
/// service. Constructed from `AuthConfig` at startup.
#[derive(Debug)]
pub struct SecurityService {
    jwt_authenticator: Option<JwtAuthenticator>,
    jwks_store: Option<Arc<JwksKeyStore>>,
    api_key_registry: Option<ApiKeyRegistry>,
    config: AuthConfig,
}

impl SecurityService {
    /// Build the service from configuration.
    ///
    /// Initializes whichever auth backends are configured:
    /// - Static JWT key (from inline key, file path, or env)
    /// - JWKS endpoint
    /// - API key registry
    pub async fn from_config(config: &AuthConfig) -> Result<Self, AuthError> {
        if !config.enabled {
            debug!("Security service: authentication disabled");
            return Ok(Self {
                jwt_authenticator: None,
                jwks_store: None,
                api_key_registry: None,
                config: config.clone(),
            });
        }

        // Resolve JWT authenticator (static key mode)
        let jwt_authenticator = if config.jwt_verification_method == "public_key" {
            let mut resolved_config = config.clone();
            Self::resolve_public_key(&mut resolved_config)?;

            match JwtAuthenticator::from_config(
                &crate::config::web::WebAuthConfig::from(resolved_config),
            ) {
                Ok(auth) => Some(auth),
                Err(e) => {
                    warn!(error = %e, "JWT authenticator init failed (may be OK if using JWKS)");
                    None
                }
            }
        } else {
            None
        };

        // Resolve JWKS store
        let jwks_store = if config.jwt_verification_method == "jwks" && !config.jwks_url.is_empty()
        {
            let refresh = Duration::from_secs(config.jwks_refresh_interval_seconds as u64);
            match JwksKeyStore::new(config.jwks_url.clone(), refresh).await {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    warn!(error = %e, "JWKS store init failed");
                    None
                }
            }
        } else {
            None
        };

        // Build API key registry
        let api_key_registry = if config.api_keys_enabled && !config.api_keys.is_empty() {
            Some(ApiKeyRegistry::from_config(&config.api_keys))
        } else if !config.api_key.is_empty() {
            // Legacy single API key support
            let legacy = crate::config::tasker::ApiKeyConfig {
                key: config.api_key.clone(),
                permissions: vec!["*".to_string()],
                description: "Legacy API key".to_string(),
            };
            Some(ApiKeyRegistry::from_config(&[legacy]))
        } else {
            None
        };

        debug!(
            jwt = jwt_authenticator.is_some(),
            jwks = jwks_store.is_some(),
            api_keys = api_key_registry.is_some(),
            "Security service initialized"
        );

        Ok(Self {
            jwt_authenticator,
            jwks_store,
            api_key_registry,
            config: config.clone(),
        })
    }

    /// Authenticate a bearer token (JWT).
    ///
    /// Tries static key first, then JWKS if configured.
    pub async fn authenticate_bearer(&self, token: &str) -> Result<SecurityContext, AuthError> {
        // Try static JWT authenticator
        if let Some(auth) = &self.jwt_authenticator {
            let claims = auth.validate_token(token)?;
            let ctx = Self::claims_to_context(claims);
            self.validate_token_permissions(&ctx)?;
            return Ok(ctx);
        }

        // Try JWKS
        if let Some(jwks) = &self.jwks_store {
            let ctx = self.validate_with_jwks(jwks, token).await?;
            self.validate_token_permissions(&ctx)?;
            return Ok(ctx);
        }

        Err(AuthError::ConfigurationError(
            "No JWT verification method configured".to_string(),
        ))
    }

    /// Authenticate an API key.
    pub fn authenticate_api_key(&self, key: &str) -> Result<SecurityContext, AuthError> {
        match &self.api_key_registry {
            Some(registry) => {
                let ctx = registry.validate_key(key)?;
                self.validate_token_permissions(&ctx)?;
                Ok(ctx)
            }
            None => Err(AuthError::ConfigurationError(
                "API key authentication not configured".to_string(),
            )),
        }
    }

    /// Whether authentication is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Whether strict permission validation is enabled.
    pub fn strict_validation(&self) -> bool {
        self.config.strict_validation
    }

    /// The configured API key header name.
    pub fn api_key_header(&self) -> &str {
        &self.config.api_key_header
    }

    /// Resolve the public key from file path or inline config value.
    ///
    /// Priority: file path > inline value.
    /// Environment variables should be handled via config loader interpolation
    /// (e.g., `jwt_public_key = "${TASKER_JWT_PUBLIC_KEY:-}"` in TOML).
    fn resolve_public_key(config: &mut AuthConfig) -> Result<(), AuthError> {
        if !config.jwt_public_key_path.is_empty() {
            match std::fs::read_to_string(&config.jwt_public_key_path) {
                Ok(contents) => {
                    config.jwt_public_key = contents;
                    return Ok(());
                }
                Err(e) => {
                    return Err(AuthError::ConfigurationError(format!(
                        "Failed to read public key from '{}': {e}",
                        config.jwt_public_key_path
                    )));
                }
            }
        }

        // jwt_public_key may already be set via config loader env var interpolation
        // (TASKER_JWT_PUBLIC_KEY). Not an error if empty when JWKS is configured.
        Ok(())
    }

    /// Validate token permissions against the known vocabulary (strict mode).
    fn validate_token_permissions(&self, ctx: &SecurityContext) -> Result<(), AuthError> {
        let unknown = validate_permissions(&ctx.permissions);
        if !unknown.is_empty() {
            if self.config.log_unknown_permissions {
                warn!(
                    subject = %ctx.subject,
                    unknown = ?unknown,
                    "Token contains unknown permissions"
                );
            }
            if self.config.strict_validation {
                return Err(AuthError::InvalidToken(format!(
                    "Unknown permissions: {}",
                    unknown.join(", ")
                )));
            }
        }
        Ok(())
    }

    /// Validate a JWT using JWKS key resolution.
    async fn validate_with_jwks(
        &self,
        jwks: &JwksKeyStore,
        token: &str,
    ) -> Result<SecurityContext, AuthError> {
        // Decode header to get kid
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("Invalid JWT header: {e}")))?;

        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken("JWT missing 'kid' header".to_string()))?;

        let decoding_key = jwks.get_key(&kid).await?;

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&[&self.config.jwt_issuer]);
        validation.set_audience(&[&self.config.jwt_audience]);
        validation.validate_exp = true;

        let token_data = jsonwebtoken::decode::<TokenClaims>(token, &decoding_key, &validation)
            .map_err(|e| {
                warn!(error = %e, "JWKS token validation failed");
                AuthError::TokenValidationError(format!("JWT validation failed: {e}"))
            })?;

        Ok(Self::claims_to_context(token_data.claims))
    }

    /// Convert validated token claims to a SecurityContext.
    fn claims_to_context(claims: TokenClaims) -> SecurityContext {
        SecurityContext {
            subject: claims.sub,
            auth_method: AuthMethod::Jwt,
            permissions: claims.permissions,
            issuer: Some(claims.iss),
            expires_at: Some(claims.exp),
        }
    }
}
