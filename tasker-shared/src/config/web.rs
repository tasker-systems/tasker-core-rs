use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Import web config structs from V2
pub use crate::config::tasker::{
    CorsConfig, OrchestrationWebConfig, RateLimitingConfig, ResilienceConfig, RouteAuthConfig,
    TlsConfig, WebDatabasePoolsConfig,
};

// Type aliases for backward compatibility (legacy names → V2 names)
pub type WebConfig = OrchestrationWebConfig;
pub type WebTlsConfig = TlsConfig;
pub type WebCorsConfig = CorsConfig;
pub type WebRateLimitConfig = RateLimitingConfig;
pub type WebResilienceConfig = ResilienceConfig;

/// Web API authentication configuration
///
/// This is an adapter over V2's AuthConfig. While V2 uses `Vec<ProtectedRouteConfig>`
/// (TOML-friendly), this version uses `HashMap<String, RouteAuthConfig>` for
/// runtime-optimized route lookups.
///
/// **Pattern**: Vec (V2 TOML) → HashMap (runtime adapter)
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

    /// JWT private key
    pub jwt_private_key: String,

    /// JWT public key
    pub jwt_public_key: String,

    /// API key for testing (use env var WEB_API_KEY in production)
    pub api_key: String,

    /// API key header name
    pub api_key_header: String,

    /// Route-specific authentication configuration (HashMap for fast runtime lookups)
    #[serde(default)]
    pub protected_routes: HashMap<String, RouteAuthConfig>,
}

impl WebAuthConfig {
    /// Check if a route requires authentication
    pub fn route_requires_auth(&self, method: &str, path: &str) -> bool {
        if !self.enabled {
            return false;
        }

        let route_key = format!("{method} {path}");

        // Check exact match first
        if let Some(config) = self.protected_routes.get(&route_key) {
            return config.required;
        }

        // Check for pattern matches (basic support for path parameters)
        for (pattern, config) in &self.protected_routes {
            if config.required && self.route_matches_pattern(&route_key, pattern) {
                return true;
            }
        }

        false
    }

    /// Get authentication type for a route
    pub fn auth_type_for_route(&self, method: &str, path: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let route_key = format!("{method} {path}");

        // Check exact match first
        if let Some(config) = self.protected_routes.get(&route_key) {
            if config.required {
                return Some(config.auth_type.clone());
            }
        }

        // Check for pattern matches
        for (pattern, config) in &self.protected_routes {
            if config.required && self.route_matches_pattern(&route_key, pattern) {
                return Some(config.auth_type.clone());
            }
        }

        None
    }

    /// Simple pattern matching for route paths with parameters
    /// Supports basic {param} patterns like "/v1/tasks/{task_uuid}"
    fn route_matches_pattern(&self, route: &str, pattern: &str) -> bool {
        let route_parts: Vec<&str> = route.split_whitespace().collect();
        let pattern_parts: Vec<&str> = pattern.split_whitespace().collect();

        if route_parts.len() != 2 || pattern_parts.len() != 2 {
            return false;
        }

        // Method must match exactly
        if route_parts[0] != pattern_parts[0] {
            return false;
        }

        // Path matching with parameter support
        let route_path_segments: Vec<&str> = route_parts[1].split('/').collect();
        let pattern_path_segments: Vec<&str> = pattern_parts[1].split('/').collect();

        if route_path_segments.len() != pattern_path_segments.len() {
            return false;
        }

        for (route_segment, pattern_segment) in
            route_path_segments.iter().zip(pattern_path_segments.iter())
        {
            // If pattern segment is a parameter (starts and ends with {}), it matches any value
            if pattern_segment.starts_with('{') && pattern_segment.ends_with('}') {
                continue;
            }
            // Otherwise, segments must match exactly
            if route_segment != pattern_segment {
                return false;
            }
        }

        true
    }
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
            api_key: String::new(),
            api_key_header: "X-API-Key".to_string(),
            protected_routes: HashMap::new(),
        }
    }
}

// TAS-61 Phase 6C/6D: Conversion from V2 AuthConfig to legacy WebAuthConfig
impl From<crate::config::tasker::AuthConfig> for WebAuthConfig {
    fn from(v2: crate::config::tasker::AuthConfig) -> Self {
        // Convert Vec<ProtectedRouteConfig> to HashMap<String, RouteAuthConfig>
        let protected_routes = v2.routes_map();

        Self {
            enabled: v2.enabled,
            jwt_issuer: v2.jwt_issuer,
            jwt_audience: v2.jwt_audience,
            jwt_token_expiry_hours: v2.jwt_token_expiry_hours as u64,
            jwt_private_key: v2.jwt_private_key,
            jwt_public_key: v2.jwt_public_key,
            api_key: v2.api_key,
            api_key_header: v2.api_key_header,
            protected_routes,
        }
    }
}

// Note: WebConfig is now a type alias for OrchestrationWebConfig, so no From impl needed
// (From<T> for T is automatically implemented)
