use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Web API configuration for TAS-28 Axum Web API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebConfig {
    /// Whether the web API is enabled
    pub enabled: bool,

    /// Address to bind the web server to
    pub bind_address: String,

    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,

    /// TLS configuration
    pub tls: WebTlsConfig,

    /// Database pool configuration for web API
    pub database_pools: WebDatabasePoolsConfig,

    /// CORS configuration
    pub cors: WebCorsConfig,

    /// Authentication configuration
    pub auth: WebAuthConfig,

    /// Rate limiting configuration
    pub rate_limiting: WebRateLimitConfig,

    /// Resilience configuration
    pub resilience: WebResilienceConfig,
}

/// Web API TLS configuration
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WebTlsConfig {
    /// Whether TLS is enabled
    pub enabled: bool,

    /// Path to TLS certificate file
    pub cert_path: String,

    /// Path to TLS private key file
    pub key_path: String,
}

/// Web API database pools configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebDatabasePoolsConfig {
    /// Web API dedicated pool size
    pub web_api_pool_size: u32,

    /// Web API maximum connections
    pub web_api_max_connections: u32,

    /// Web API connection timeout in seconds
    pub web_api_connection_timeout_seconds: u64,

    /// Web API idle timeout in seconds
    pub web_api_idle_timeout_seconds: u64,

    /// Maximum total connections hint for resource coordination
    pub max_total_connections_hint: u32,
}

/// Web API CORS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebCorsConfig {
    /// Whether CORS is enabled
    pub enabled: bool,

    /// Allowed origins
    pub allowed_origins: Vec<String>,

    /// Allowed methods
    pub allowed_methods: Vec<String>,

    /// Allowed headers
    pub allowed_headers: Vec<String>,

    /// Max age in seconds
    #[serde(default = "default_cors_max_age")]
    pub max_age_seconds: u64,
}

fn default_cors_max_age() -> u64 {
    86400 // 24 hours
}

/// Web API authentication configuration
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

    /// Route-specific authentication configuration
    #[serde(default)]
    pub protected_routes: HashMap<String, RouteAuthConfig>,
}

/// Authentication configuration for a specific route
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteAuthConfig {
    /// Type of authentication required ("bearer", "api_key")
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}

/// Web API rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebRateLimitConfig {
    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Requests per minute
    pub requests_per_minute: u32,

    /// Burst size
    pub burst_size: u32,

    /// Whether to apply limits per client
    pub per_client_limit: bool,
}

/// Web API resilience configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebResilienceConfig {
    /// Whether circuit breaker is enabled
    pub circuit_breaker_enabled: bool,

    /// Request timeout in seconds
    pub request_timeout_seconds: u64,

    /// Maximum concurrent requests
    pub max_concurrent_requests: u32,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "0.0.0.0:8080".to_string(),
            request_timeout_ms: 30000,
            tls: WebTlsConfig::default(),
            database_pools: WebDatabasePoolsConfig::default(),
            cors: WebCorsConfig::default(),
            auth: WebAuthConfig::default(),
            rate_limiting: WebRateLimitConfig::default(),
            resilience: WebResilienceConfig::default(),
        }
    }
}

impl Default for WebDatabasePoolsConfig {
    fn default() -> Self {
        Self {
            web_api_pool_size: 10,
            web_api_max_connections: 15,
            web_api_connection_timeout_seconds: 30,
            web_api_idle_timeout_seconds: 600,
            max_total_connections_hint: 45,
        }
    }
}

impl Default for WebCorsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "PATCH".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec!["*".to_string()],
            max_age_seconds: 86400,
        }
    }
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

impl Default for WebRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_minute: 1000,
            burst_size: 100,
            per_client_limit: true,
        }
    }
}

impl Default for WebResilienceConfig {
    fn default() -> Self {
        Self {
            circuit_breaker_enabled: true,
            request_timeout_seconds: 30,
            max_concurrent_requests: 100,
        }
    }
}

// TAS-61 Phase 6C/6D: Conversion from V2 AuthConfig to legacy WebAuthConfig
impl From<crate::config::tasker::tasker_v2::AuthConfig> for WebAuthConfig {
    fn from(v2: crate::config::tasker::tasker_v2::AuthConfig) -> Self {
        Self {
            enabled: v2.enabled,
            jwt_issuer: v2.jwt_issuer,
            jwt_audience: v2.jwt_audience,
            jwt_token_expiry_hours: v2.jwt_token_expiry_hours as u64,
            jwt_private_key: v2.jwt_private_key,
            jwt_public_key: v2.jwt_public_key,
            api_key: v2.api_key,
            api_key_header: v2.api_key_header,
            protected_routes: std::collections::HashMap::new(), // V2 doesn't have protected_routes yet
        }
    }
}

// TAS-61 Phase 6C/6D: Conversion from V2 OrchestrationWebConfig to legacy WebConfig
impl From<crate::config::tasker::tasker_v2::OrchestrationWebConfig> for WebConfig {
    fn from(v2: crate::config::tasker::tasker_v2::OrchestrationWebConfig) -> Self {
        Self {
            enabled: v2.enabled,
            bind_address: v2.bind_address,
            request_timeout_ms: v2.request_timeout_ms as u64,
            tls: v2.tls.map(|t| WebTlsConfig {
                enabled: t.enabled,
                cert_path: t.cert_path,
                key_path: t.key_path,
            }).unwrap_or_default(),
            database_pools: WebDatabasePoolsConfig {
                web_api_pool_size: v2.database_pools.web_api_pool_size,
                web_api_max_connections: v2.database_pools.web_api_max_connections,
                web_api_connection_timeout_seconds: v2.database_pools.web_api_connection_timeout_seconds as u64,
                web_api_idle_timeout_seconds: v2.database_pools.web_api_idle_timeout_seconds as u64,
                max_total_connections_hint: v2.database_pools.max_total_connections_hint,
            },
            cors: v2.cors.map(|c| WebCorsConfig {
                enabled: c.enabled,
                allowed_origins: c.allowed_origins,
                allowed_methods: c.allowed_methods,
                allowed_headers: c.allowed_headers,
                max_age_seconds: c.max_age_seconds as u64,
            }).unwrap_or_default(),
            auth: v2.auth.map(|a| a.into()).unwrap_or_else(|| WebAuthConfig::default()),
            rate_limiting: v2.rate_limiting.map(|r| WebRateLimitConfig {
                enabled: r.enabled,
                requests_per_minute: r.requests_per_minute,
                burst_size: r.burst_size,
                per_client_limit: r.per_client_limit,
            }).unwrap_or_default(),
            resilience: v2.resilience.map(|r| WebResilienceConfig {
                circuit_breaker_enabled: r.circuit_breaker_enabled,
                request_timeout_seconds: r.request_timeout_seconds as u64,
                max_concurrent_requests: r.max_concurrent_requests,
            }).unwrap_or_default(),
        }
    }
}
