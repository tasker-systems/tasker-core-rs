//! # Security Service
//!
//! Unified authentication service combining JWT (static key + JWKS) and API key
//! authentication methods. Provides a single entry point for request authentication.

use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, warn};

use crate::config::tasker::AuthConfig;
use crate::types::auth::{AuthError, JwtAuthenticator, TokenClaims};
use crate::types::jwks::{JwksConfig, JwksKeyStore};
use crate::types::permissions::validate_permissions;
use crate::types::security::{AuthMethod, SecurityContext};

use crate::types::api_key_auth::ApiKeyRegistry;

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

            match JwtAuthenticator::from_config(&crate::config::web::WebAuthConfig::from(
                resolved_config,
            )) {
                Ok(auth) => Some(auth),
                Err(e) => {
                    warn!(error = %e, "JWT authenticator init failed (may be OK if using JWKS)");
                    None
                }
            }
        } else {
            None
        };

        // Resolve JWKS store with hardening config
        let jwks_store = if config.jwt_verification_method == "jwks" && !config.jwks_url.is_empty()
        {
            let jwks_config = JwksConfig {
                url: config.jwks_url.clone(),
                refresh_interval: Duration::from_secs(config.jwks_refresh_interval_seconds as u64),
                max_stale: Duration::from_secs(config.jwks_max_stale_seconds as u64),
                allow_http: config.jwks_url_allow_http,
                allowed_algorithms: config.jwt_allowed_algorithms.clone(),
            };
            match JwksKeyStore::with_config(jwks_config).await {
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
    ///
    /// Uses the configured algorithm allowlist rather than trusting the token's
    /// `alg` header, preventing algorithm confusion attacks.
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

        // Use algorithm allowlist from config (not the token's alg header)
        let allowed_algs: Vec<jsonwebtoken::Algorithm> = self
            .config
            .jwt_allowed_algorithms
            .iter()
            .filter_map(|a| Self::parse_algorithm(a))
            .collect();

        if allowed_algs.is_empty() {
            return Err(AuthError::ConfigurationError(
                "No valid algorithms configured in jwt_allowed_algorithms".to_string(),
            ));
        }

        let mut validation = jsonwebtoken::Validation::new(allowed_algs[0]);
        validation.algorithms = allowed_algs;
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

    /// Parse an algorithm string into a jsonwebtoken Algorithm enum.
    fn parse_algorithm(alg: &str) -> Option<jsonwebtoken::Algorithm> {
        match alg {
            "RS256" => Some(jsonwebtoken::Algorithm::RS256),
            "RS384" => Some(jsonwebtoken::Algorithm::RS384),
            "RS512" => Some(jsonwebtoken::Algorithm::RS512),
            "PS256" => Some(jsonwebtoken::Algorithm::PS256),
            "PS384" => Some(jsonwebtoken::Algorithm::PS384),
            "PS512" => Some(jsonwebtoken::Algorithm::PS512),
            "ES256" => Some(jsonwebtoken::Algorithm::ES256),
            "ES384" => Some(jsonwebtoken::Algorithm::ES384),
            _ => {
                warn!(algorithm = %alg, "Unknown JWT algorithm in config, skipping");
                None
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // 1. parse_algorithm() — all supported algorithms + unknown
    // ===================================================================
    #[test]
    fn test_parse_algorithm_rs256() {
        let alg = SecurityService::parse_algorithm("RS256");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::RS256));
    }

    #[test]
    fn test_parse_algorithm_rs384() {
        let alg = SecurityService::parse_algorithm("RS384");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::RS384));
    }

    #[test]
    fn test_parse_algorithm_rs512() {
        let alg = SecurityService::parse_algorithm("RS512");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::RS512));
    }

    #[test]
    fn test_parse_algorithm_es256() {
        let alg = SecurityService::parse_algorithm("ES256");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::ES256));
    }

    #[test]
    fn test_parse_algorithm_es384() {
        let alg = SecurityService::parse_algorithm("ES384");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::ES384));
    }

    #[test]
    fn test_parse_algorithm_ps256() {
        let alg = SecurityService::parse_algorithm("PS256");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::PS256));
    }

    #[test]
    fn test_parse_algorithm_ps384() {
        let alg = SecurityService::parse_algorithm("PS384");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::PS384));
    }

    #[test]
    fn test_parse_algorithm_ps512() {
        let alg = SecurityService::parse_algorithm("PS512");
        assert_eq!(alg, Some(jsonwebtoken::Algorithm::PS512));
    }

    #[test]
    fn test_parse_algorithm_unknown_returns_none() {
        assert_eq!(SecurityService::parse_algorithm("HS256"), None);
        assert_eq!(SecurityService::parse_algorithm("HS384"), None);
        assert_eq!(SecurityService::parse_algorithm("none"), None);
        assert_eq!(SecurityService::parse_algorithm(""), None);
    }

    // ===================================================================
    // 2. claims_to_context() — maps TokenClaims to SecurityContext
    // ===================================================================
    #[test]
    fn test_claims_to_context_maps_fields_correctly() {
        let claims = TokenClaims {
            sub: "worker-42".to_string(),
            worker_namespaces: vec!["production".to_string()],
            iss: "tasker-core-test".to_string(),
            aud: "tasker-api-test".to_string(),
            exp: 1_700_000_000,
            iat: 1_699_990_000,
            permissions: vec!["tasks:read".to_string(), "tasks:create".to_string()],
        };

        let ctx = SecurityService::claims_to_context(claims);

        assert_eq!(ctx.subject, "worker-42");
        assert_eq!(ctx.auth_method, AuthMethod::Jwt);
        assert_eq!(ctx.permissions, vec!["tasks:read", "tasks:create"]);
        assert_eq!(ctx.issuer, Some("tasker-core-test".to_string()));
        assert_eq!(ctx.expires_at, Some(1_700_000_000));
    }

    #[test]
    fn test_claims_to_context_empty_permissions() {
        let claims = TokenClaims {
            sub: "svc-empty".to_string(),
            worker_namespaces: vec![],
            iss: "issuer".to_string(),
            aud: "audience".to_string(),
            exp: 0,
            iat: 0,
            permissions: vec![],
        };

        let ctx = SecurityService::claims_to_context(claims);

        assert_eq!(ctx.subject, "svc-empty");
        assert!(ctx.permissions.is_empty());
        assert_eq!(ctx.auth_method, AuthMethod::Jwt);
    }

    // ===================================================================
    // 3. Disabled auth from_config()
    // ===================================================================
    #[tokio::test]
    async fn test_from_config_disabled_returns_service_not_enabled() {
        let config = AuthConfig::builder().enabled(false).build();

        let service = SecurityService::from_config(&config)
            .await
            .expect("should build disabled service");

        assert!(!service.is_enabled());
        assert!(service.jwt_authenticator.is_none());
        assert!(service.jwks_store.is_none());
        assert!(service.api_key_registry.is_none());
    }

    // ===================================================================
    // 4. Getters: is_enabled(), strict_validation(), api_key_header()
    // ===================================================================
    #[tokio::test]
    async fn test_is_enabled_reflects_config() {
        let disabled = AuthConfig::builder().enabled(false).build();
        let service = SecurityService::from_config(&disabled)
            .await
            .expect("disabled service");
        assert!(!service.is_enabled());
    }

    #[tokio::test]
    async fn test_strict_validation_reflects_config() {
        let strict_on = AuthConfig::builder()
            .enabled(false)
            .strict_validation(true)
            .build();
        let service = SecurityService::from_config(&strict_on)
            .await
            .expect("service");
        assert!(service.strict_validation());

        let strict_off = AuthConfig::builder()
            .enabled(false)
            .strict_validation(false)
            .build();
        let service2 = SecurityService::from_config(&strict_off)
            .await
            .expect("service");
        assert!(!service2.strict_validation());
    }

    #[tokio::test]
    async fn test_api_key_header_reflects_config() {
        let config = AuthConfig::builder()
            .enabled(false)
            .api_key_header("X-Custom-Key".to_string())
            .build();
        let service = SecurityService::from_config(&config)
            .await
            .expect("service");
        assert_eq!(service.api_key_header(), "X-Custom-Key");
    }

    #[tokio::test]
    async fn test_api_key_header_default_value() {
        let config = AuthConfig::builder().enabled(false).build();
        let service = SecurityService::from_config(&config)
            .await
            .expect("service");
        assert_eq!(service.api_key_header(), "X-API-Key");
    }
}
