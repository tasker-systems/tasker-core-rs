//! # Web API Application State
//!
//! Defines the shared state for the web API including database pools,
//! configuration, and circuit breaker health monitoring.

use crate::config::loader::ConfigManager;
use crate::orchestration::core::OrchestrationCore;
use crate::orchestration::task_initializer::TaskInitializer;
use crate::orchestration::coordinator::operational_state::SystemOperationalState;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;
use crate::web::errors::{ApiError, ApiResult};
use parking_lot::RwLock;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Duration;
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
}

impl AppState {
    /// Create AppState from OrchestrationCore with dedicated web resources
    ///
    /// This method creates the web API's dedicated database pool while
    /// maintaining references to shared orchestration components.
    pub async fn from_orchestration_core(
        web_config: WebServerConfig,
        orchestration_core: &OrchestrationCore,
        config_manager: &crate::config::ConfigManager,
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
            .acquire_timeout(Duration::from_secs(pool_config.web_api_connection_timeout_seconds))
            .idle_timeout(Duration::from_secs(pool_config.web_api_idle_timeout_seconds))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| ApiError::database_error(format!("Failed to create web database pool: {e}")))?;

        // Create circuit breaker for web database health
        let circuit_breaker = if web_config.resilience.circuit_breaker_enabled {
            WebDatabaseCircuitBreaker::new(
                5,                          // failure_threshold
                Duration::from_secs(30),    // recovery_timeout  
                "web_database"              // component_name
            )
        } else {
            // Disabled circuit breaker (always closed)
            WebDatabaseCircuitBreaker::new(u32::MAX, Duration::from_secs(1), "disabled")
        };

        // Extract orchestration status from OrchestrationCore
        let operational_state = orchestration_core.operational_state().await;
        let database_pool_size = orchestration_core.database_pool().size();
        let environment = config_manager.environment().to_string();
        
        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: true,
            environment: environment.clone(),
            operational_state: operational_state.clone(),
            database_pool_size,
            last_health_check: std::time::Instant::now(),
        }));

        info!(
            web_pool_size = pool_config.web_api_max_connections,
            orchestration_pool_size = database_pool_size,
            circuit_breaker_enabled = web_config.resilience.circuit_breaker_enabled,
            environment = %environment,
            operational_state = ?operational_state,
            "Web API application state created successfully"
        );

        Ok(Self {
            config: Arc::new(web_config),
            web_db_pool: web_db_pool.clone(),
            orchestration_db_pool: orchestration_core.database_pool().clone(),
            web_db_circuit_breaker: circuit_breaker,
            task_initializer: orchestration_core.task_initializer.clone(),
            orchestration_status,
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

    /// Create AppState for testing without requiring OrchestrationCore
    #[cfg(feature = "test-utils")]
    pub async fn new_for_testing(tasker_config: crate::config::TaskerConfig) -> ApiResult<Self> {
        use crate::orchestration::task_initializer::TaskInitializer;
        
        info!("Creating test web API application state");
        
        // Get web configuration from TaskerConfig
        let web_config_toml = tasker_config.web_config();
        
        // Convert to WebServerConfig
        let web_config = WebServerConfig::from_toml_config(&web_config_toml)?;
        
        // Extract database URL from configuration
        let database_url = tasker_config.database_url();
        let pool_config = &web_config.database_pools;
        
        debug!(
            pool_size = pool_config.web_api_pool_size,
            max_connections = pool_config.web_api_max_connections,
            "Creating test web API database pool"
        );
        
        // Create simplified database pool for testing
        let web_db_pool = PgPoolOptions::new()
            .max_connections(pool_config.web_api_max_connections.min(5)) // Smaller for tests
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(60))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| ApiError::database_error(format!("Failed to create test web database pool: {e}")))?;

        // Create a second pool for orchestration simulation in tests
        let orchestration_db_pool = PgPoolOptions::new()
            .max_connections(5) // Small test pool
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .test_before_acquire(true)
            .connect(&database_url)
            .await
            .map_err(|e| ApiError::database_error(format!("Failed to create test orchestration database pool: {e}")))?;

        // Create disabled circuit breaker for testing
        let circuit_breaker = WebDatabaseCircuitBreaker::new(u32::MAX, Duration::from_secs(1), "test_disabled");

        // Create test task initializer
        let task_initializer = Arc::new(TaskInitializer::new(
            orchestration_db_pool.clone(),
        ));

        // Create test orchestration status
        let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
            running: true,
            environment: "test".to_string(),
            operational_state: SystemOperationalState::Normal,
            database_pool_size: 5,
            last_health_check: std::time::Instant::now(),
        }));

        info!(
            web_pool_size = pool_config.web_api_max_connections.min(5),
            orchestration_pool_size = 5,
            "Test web API application state created successfully"
        );

        Ok(Self {
            config: Arc::new(web_config),
            web_db_pool,
            orchestration_db_pool,
            web_db_circuit_breaker: circuit_breaker,
            task_initializer,
            orchestration_status,
        })
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

    /// Report web API database pool usage to health monitoring system (TAS-37 Web Integration)
    ///
    /// This method creates a pool usage report and sends it to the health monitoring system
    /// if pool monitoring is enabled in the web configuration.
    pub async fn report_pool_usage_to_health_monitor(
        &self,
        health_monitor: &crate::orchestration::coordinator::monitor::HealthMonitor,
        operational_state_manager: Option<&crate::orchestration::coordinator::operational_state::OperationalStateManager>,
    ) -> crate::error::Result<()> {
        use crate::orchestration::coordinator::monitor::WebPoolUsageReport;

        // Check if pool usage reporting is enabled
        // Note: This would ideally check config.resource_monitoring.report_pool_usage_to_health_monitor
        // For now, we'll always report since the health monitor can handle it gracefully

        // Get current pool usage statistics
        let web_pool = &self.web_db_pool;
        let current_size = web_pool.size();
        let max_size = self.config.database_pools.web_api_max_connections;

        // Calculate usage ratio
        let usage_ratio = if max_size > 0 {
            current_size as f64 / max_size as f64
        } else {
            0.0
        };

        // Create usage report from web configuration thresholds
        let pool_report = WebPoolUsageReport {
            usage_ratio,
            active_connections: current_size,
            max_connections: max_size,
            // Use thresholds from web.toml configuration
            warning_threshold: 0.75,  // Could be config.resource_monitoring.pool_usage_warning_threshold
            critical_threshold: 0.90, // Could be config.resource_monitoring.pool_usage_critical_threshold
            pool_name: "web_api_pool".to_string(),
        };

        debug!(
            "WEB_POOL: Reporting usage to health monitor - {:.1}% ({}/{} connections)",
            usage_ratio * 100.0,
            current_size,
            max_size
        );

        // Report to health monitoring system
        health_monitor
            .record_web_pool_usage(pool_report, operational_state_manager)
            .await
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
    pub fn from_toml_config(web_config: &crate::config::WebConfig) -> ApiResult<Self> {
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
                web_api_connection_timeout_seconds: web_config.database_pools.web_api_connection_timeout_seconds,
                web_api_idle_timeout_seconds: web_config.database_pools.web_api_idle_timeout_seconds,
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
    pub fn from_config_manager(config_manager: &ConfigManager) -> ApiResult<Option<Self>> {
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
                web_api_connection_timeout_seconds: web_config.database_pools.web_api_connection_timeout_seconds,
                web_api_idle_timeout_seconds: web_config.database_pools.web_api_idle_timeout_seconds,
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