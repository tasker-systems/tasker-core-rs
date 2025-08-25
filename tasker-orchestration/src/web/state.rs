//! # Web API Application State
//!
//! Defines the shared state for the web API including database pools,
//! configuration, and circuit breaker health monitoring.

// TAS-40: Simplified operational state for web API compatibility with command pattern
#[derive(Debug, Clone, PartialEq)]
pub enum SystemOperationalState {
    Normal,
    GracefulShutdown,
    Emergency,
    Stopped,
    Startup,
}
use crate::orchestration::core::{OrchestrationCore, OrchestrationCoreStatus};
use crate::orchestration::lifecycle::task_initializer::TaskInitializer;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use crate::web::response_types::{ApiError, ApiResult};
use parking_lot::RwLock;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tasker_shared::config::{ConfigManager, WebConfig};
use tasker_shared::TaskerResult;
use tracing::{debug, info};

/// Database operation types for smart pool selection
#[derive(Debug, Clone, Copy)]
pub enum DbOperationType {
    /// Write operations that need dedicated pool (task creation, cancellation)
    WebWrite,
    /// High-priority web operations requiring dedicated resources
    WebCritical,
    /// Read-only operations that can use shared orchestration pool
    ReadOnly,
    /// Analytics and reporting queries
    Analytics,
}

/// Web server configuration from TOML components
#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub max_request_size_mb: u64,

    pub database_pools: DatabasePoolConfig,
    pub cors: CorsConfig,
    pub auth: AuthConfig,
    pub rate_limiting: RateLimitConfig,
    pub resilience: ResilienceConfig,
}

/// Database pool configuration for web operations
#[derive(Debug, Clone)]
pub struct DatabasePoolConfig {
    pub web_api_pool_size: u32,
    pub web_api_max_connections: u32,
    pub web_api_connection_timeout_seconds: u64,
    pub web_api_idle_timeout_seconds: u64,
}

/// CORS configuration
#[derive(Debug, Clone)]
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub enabled: bool,
    pub jwt_private_key: String,
    pub jwt_public_key: String,
    pub jwt_token_expiry_hours: u64,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub api_key_header: String,
    pub protected_routes: std::collections::HashMap<String, RouteAuthConfig>,
}

/// Authentication configuration for a specific route
#[derive(Debug, Clone)]
pub struct RouteAuthConfig {
    /// Type of authentication required ("bearer", "api_key")
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}

impl AuthConfig {
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

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub per_client_limit: bool,
}

/// Resilience configuration
#[derive(Debug, Clone)]
pub struct ResilienceConfig {
    pub circuit_breaker_enabled: bool,
    pub request_timeout_seconds: u64,
    pub max_concurrent_requests: u32,
}

/// Operational status tracking for web API integration
#[derive(Debug, Clone)]
pub struct OrchestrationStatus {
    pub running: bool,
    pub environment: String,
    pub operational_state: SystemOperationalState,
    pub database_pool_size: u32,
    pub last_health_check: std::time::Instant,
}

/// Database pool usage statistics for monitoring (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct DatabasePoolUsageStats {
    pub pool_name: String,
    pub active_connections: u32,
    pub max_connections: u32,
    pub usage_ratio: f64,
    pub is_healthy: bool,
}

/// Shared application state for the web API
///
/// This state is shared across all request handlers and contains:
/// - Dedicated database pool for web operations
/// - Shared orchestration database pool reference
/// - Circuit breaker for web database health
/// - Configuration and orchestration status
#[derive(Clone)]
pub struct AppState {
    /// Web server configuration
    pub config: Arc<WebServerConfig>,

    /// Dedicated database pool for web API operations
    pub web_db_pool: PgPool,

    /// Shared orchestration database pool (for read operations)
    pub orchestration_db_pool: PgPool,

    /// Circuit breaker for web database health monitoring
    pub web_db_circuit_breaker: WebDatabaseCircuitBreaker,

    /// Shared task initializer component
    pub task_initializer: Arc<TaskInitializer>,

    /// Orchestration system operational status
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,

    pub orchestration_core: Arc<OrchestrationCore>,
}

impl AppState {
    /// Create AppState from OrchestrationCore with dedicated web resources
    ///
    /// This method creates the web API's dedicated database pool while
    /// maintaining references to shared orchestration components.
    pub async fn from_orchestration_core(
        web_config: WebServerConfig,
        orchestration_core: Arc<OrchestrationCore>,
        config_manager: Arc<ConfigManager>,
    ) -> ApiResult<Self> {
        info!("Creating web API application state with dedicated database pool");

        // Extract database URL from orchestration configuration
        let database_url = config_manager.config().database_url();
        let pool_config = &web_config.database_pools;

        debug!(
            pool_size = pool_config.web_api_pool_size,
            max_connections = pool_config.web_api_max_connections,
            connection_timeout = pool_config.web_api_connection_timeout_seconds,
            idle_timeout = pool_config.web_api_idle_timeout_seconds,
            "Creating dedicated web API database pool"
        );

        let web_db_pool = PgPoolOptions::new()
            .max_connections(pool_config.web_api_max_connections)
            .min_connections(pool_config.web_api_pool_size / 2)
            .acquire_timeout(Duration::from_secs(
                pool_config.web_api_connection_timeout_seconds,
            ))
            .idle_timeout(Duration::from_secs(
                pool_config.web_api_idle_timeout_seconds,
            ))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| {
                ApiError::database_error(format!("Failed to create web database pool: {e}"))
            })?;

        // Create circuit breaker for web database health
        let circuit_breaker = if web_config.resilience.circuit_breaker_enabled {
            WebDatabaseCircuitBreaker::new(
                5,                       // failure_threshold
                Duration::from_secs(30), // recovery_timeout
                "web_database",          // component_name
            )
        } else {
            // Disabled circuit breaker (always closed)
            WebDatabaseCircuitBreaker::new(u32::MAX, Duration::from_secs(1), "disabled")
        };

        // Extract orchestration status from OrchestrationCore (TAS-40 simplified)
        let database_pool_size = orchestration_core.context.database_pool().size();
        let environment = config_manager.environment().to_string();
        let core_status = orchestration_core.status();

        // Convert OrchestrationCoreStatus to SystemOperationalState
        let operational_state = match core_status {
            OrchestrationCoreStatus::Running => SystemOperationalState::Normal,
            OrchestrationCoreStatus::Starting => SystemOperationalState::Startup,
            OrchestrationCoreStatus::Stopping => SystemOperationalState::GracefulShutdown,
            OrchestrationCoreStatus::Stopped => SystemOperationalState::Stopped,
            OrchestrationCoreStatus::Error(_) => SystemOperationalState::Emergency,
            OrchestrationCoreStatus::Created => SystemOperationalState::Startup,
        };

        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: matches!(core_status, OrchestrationCoreStatus::Running),
            environment: environment.clone(),
            operational_state,
            database_pool_size,
            last_health_check: std::time::Instant::now(),
        }));

        // Create TaskInitializer from orchestration context
        let task_initializer = Arc::new(TaskInitializer::new(
            orchestration_core.context.database_pool().clone(),
        ));

        info!(
            web_pool_size = pool_config.web_api_max_connections,
            orchestration_pool_size = database_pool_size,
            circuit_breaker_enabled = web_config.resilience.circuit_breaker_enabled,
            environment = %environment,
            orchestration_status = ?orchestration_status,
            "Web API application state created successfully"
        );

        Ok(Self {
            config: Arc::new(web_config),
            web_db_pool: web_db_pool.clone(),
            orchestration_db_pool: orchestration_core.context.database_pool().clone(),
            web_db_circuit_breaker: circuit_breaker,
            task_initializer,
            orchestration_status,
            orchestration_core,
        })
    }

    /// Select the appropriate database pool based on operation type
    ///
    /// This implements the smart pool selection strategy:
    /// - Write operations use dedicated web pool
    /// - Read operations can use shared orchestration pool
    pub fn select_db_pool(&self, operation_type: DbOperationType) -> &PgPool {
        match operation_type {
            DbOperationType::WebWrite | DbOperationType::WebCritical => &self.web_db_pool,
            DbOperationType::ReadOnly | DbOperationType::Analytics => &self.orchestration_db_pool,
        }
    }

    /// Check if web database circuit breaker is healthy
    pub fn is_database_healthy(&self) -> bool {
        !self.web_db_circuit_breaker.is_circuit_open()
    }

    /// Record a successful database operation for circuit breaker
    pub fn record_database_success(&self) {
        self.web_db_circuit_breaker.record_success();
    }

    /// Record a failed database operation for circuit breaker
    pub fn record_database_failure(&self) {
        self.web_db_circuit_breaker.record_failure();
    }

    /// Update orchestration status (called periodically by health check)
    pub fn update_orchestration_status(&self, new_status: OrchestrationStatus) {
        let mut status = self.orchestration_status.write();
        *status = new_status;
    }

    /// Get current orchestration operational state
    pub fn operational_state(&self) -> SystemOperationalState {
        self.orchestration_status.read().operational_state.clone()
    }

    /// Report web API database pool usage to health monitoring system (TAS-40 Web Integration)
    ///
    /// This method creates a pool usage report for external health monitoring.
    /// Uses TAS-40 command pattern architecture with simplified operational state.
    pub async fn report_pool_usage_stats(&self) -> TaskerResult<DatabasePoolUsageStats> {
        // Get current pool usage statistics (simplified for TAS-40)
        Ok(self.get_pool_usage_stats())
    }

    /// Get web API database pool usage statistics for external monitoring
    pub fn get_pool_usage_stats(&self) -> DatabasePoolUsageStats {
        let web_pool = &self.web_db_pool;
        let current_size = web_pool.size();
        let max_size = self.config.database_pools.web_api_max_connections;

        let usage_ratio = if max_size > 0 {
            current_size as f64 / max_size as f64
        } else {
            0.0
        };

        DatabasePoolUsageStats {
            pool_name: "web_api_pool".to_string(),
            active_connections: current_size,
            max_connections: max_size,
            usage_ratio,
            is_healthy: usage_ratio <= 0.75, // Using default warning threshold
        }
    }
}

impl WebServerConfig {
    /// Create WebServerConfig directly from TOML config for testing
    #[cfg(feature = "test-utils")]
    pub fn from_toml_config(web_config: &WebConfig) -> ApiResult<Self> {
        info!(
            "Loading web server configuration from TOML: enabled={}, bind_address={}",
            web_config.enabled, web_config.bind_address
        );

        Ok(WebServerConfig {
            enabled: web_config.enabled,
            bind_address: web_config.bind_address.clone(),
            request_timeout_ms: web_config.request_timeout_ms,
            max_request_size_mb: web_config.max_request_size_mb,
            database_pools: DatabasePoolConfig {
                web_api_pool_size: web_config.database_pools.web_api_pool_size,
                web_api_max_connections: web_config.database_pools.web_api_max_connections,
                web_api_connection_timeout_seconds: web_config
                    .database_pools
                    .web_api_connection_timeout_seconds,
                web_api_idle_timeout_seconds: web_config
                    .database_pools
                    .web_api_idle_timeout_seconds,
            },
            cors: CorsConfig {
                enabled: web_config.cors.enabled,
                allowed_origins: web_config.cors.allowed_origins.clone(),
                allowed_methods: web_config.cors.allowed_methods.clone(),
                allowed_headers: web_config.cors.allowed_headers.clone(),
            },
            auth: AuthConfig {
                enabled: web_config.auth.enabled,
                jwt_issuer: web_config.auth.jwt_issuer.clone(),
                jwt_audience: web_config.auth.jwt_audience.clone(),
                jwt_token_expiry_hours: web_config.auth.jwt_token_expiry_hours,
                jwt_private_key: web_config.auth.jwt_private_key.clone(),
                jwt_public_key: web_config.auth.jwt_public_key.clone(),
                api_key_header: web_config.auth.api_key_header.clone(),
                protected_routes: web_config
                    .auth
                    .protected_routes
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            RouteAuthConfig {
                                auth_type: v.auth_type.clone(),
                                required: v.required,
                            },
                        )
                    })
                    .collect(),
            },
            rate_limiting: RateLimitConfig {
                enabled: web_config.rate_limiting.enabled,
                requests_per_minute: web_config.rate_limiting.requests_per_minute,
                burst_size: web_config.rate_limiting.burst_size,
                per_client_limit: web_config.rate_limiting.per_client_limit,
            },
            resilience: ResilienceConfig {
                circuit_breaker_enabled: web_config.resilience.circuit_breaker_enabled,
                request_timeout_seconds: web_config.resilience.request_timeout_seconds,
                max_concurrent_requests: web_config.resilience.max_concurrent_requests,
            },
        })
    }

    /// Create WebServerConfig from ConfigManager TOML configuration
    ///
    /// Loads configuration from the component-based TOML system with
    /// environment-specific overrides.
    pub fn from_config_manager(config_manager: &ConfigManager) -> TaskerResult<Option<Self>> {
        let config = config_manager.config();

        // Check if web API is enabled
        if !config.web_enabled() {
            debug!("Web API is disabled in configuration");
            return Ok(None);
        }

        let web_config = config.web_config();

        info!(
            "Loading web server configuration from TOML: enabled={}, bind_address={}",
            web_config.enabled, web_config.bind_address
        );

        Ok(Some(WebServerConfig {
            enabled: web_config.enabled,
            bind_address: web_config.bind_address,
            request_timeout_ms: web_config.request_timeout_ms,
            max_request_size_mb: web_config.max_request_size_mb,
            database_pools: DatabasePoolConfig {
                web_api_pool_size: web_config.database_pools.web_api_pool_size,
                web_api_max_connections: web_config.database_pools.web_api_max_connections,
                web_api_connection_timeout_seconds: web_config
                    .database_pools
                    .web_api_connection_timeout_seconds,
                web_api_idle_timeout_seconds: web_config
                    .database_pools
                    .web_api_idle_timeout_seconds,
            },
            cors: CorsConfig {
                enabled: web_config.cors.enabled,
                allowed_origins: web_config.cors.allowed_origins,
                allowed_methods: web_config.cors.allowed_methods,
                allowed_headers: web_config.cors.allowed_headers,
            },
            auth: AuthConfig {
                enabled: web_config.auth.enabled,
                jwt_issuer: web_config.auth.jwt_issuer,
                jwt_audience: web_config.auth.jwt_audience,
                jwt_token_expiry_hours: web_config.auth.jwt_token_expiry_hours,
                jwt_private_key: web_config.auth.jwt_private_key,
                jwt_public_key: web_config.auth.jwt_public_key,
                api_key_header: web_config.auth.api_key_header,
                protected_routes: web_config
                    .auth
                    .protected_routes
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            RouteAuthConfig {
                                auth_type: v.auth_type.clone(),
                                required: v.required,
                            },
                        )
                    })
                    .collect(),
            },
            rate_limiting: RateLimitConfig {
                enabled: web_config.rate_limiting.enabled,
                requests_per_minute: web_config.rate_limiting.requests_per_minute,
                burst_size: web_config.rate_limiting.burst_size,
                per_client_limit: web_config.rate_limiting.per_client_limit,
            },
            resilience: ResilienceConfig {
                circuit_breaker_enabled: web_config.resilience.circuit_breaker_enabled,
                request_timeout_seconds: web_config.resilience.request_timeout_seconds,
                max_concurrent_requests: web_config.resilience.max_concurrent_requests,
            },
        }))
    }
}
