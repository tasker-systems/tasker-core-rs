//! # TaskerCore Configuration System
//!
//! This module provides comprehensive configuration management that mirrors the Ruby side's
//! YAML-based configuration approach. It eliminates hardcoded fallbacks and environment
//! variable dependencies in favor of explicit, validated configuration loading.
//!
//! ## Architecture
//!
//! - **Single Source of Truth**: All configuration comes from YAML files
//! - **Environment Awareness**: Supports development/test/production overrides
//! - **Explicit Validation**: No silent fallbacks or data corruption
//! - **Ruby Parity**: Mirrors Ruby side configuration structure exactly
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::config::ConfigManager;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration (environment auto-detected)
//! let config = ConfigManager::load()?;
//!
//! // Access configuration values
//! let database_url = config.config().database_url();
//! let pool_size = config.config().database.pool;
//! let timeout = config.config().execution.step_execution_timeout_seconds;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod loader;
pub mod query_cache_config;

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub use error::ConfigurationError;
pub use loader::ConfigManager;
pub use query_cache_config::{CacheTypeConfig, QueryCacheConfig, QueryCacheConfigLoader};

/// Custom deserializer for pool configuration that can handle both simple integer
/// and structured hash formats for maximum compatibility
fn deserialize_pool_config<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use serde_json::Value;

    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        // Simple integer format: pool: 25
        Value::Number(n) => {
            if let Some(i) = n.as_u64() {
                i.try_into().map_err(|_| {
                    D::Error::custom("Pool size exceeds maximum allowed value (u32::MAX)")
                })
            } else {
                Err(D::Error::custom("Pool value must be a positive integer"))
            }
        }
        // Structured format: pool: { max_connections: 25, ... }
        Value::Object(obj) => {
            if let Some(max_conn) = obj.get("max_connections") {
                if let Some(max_conn_num) = max_conn.as_u64() {
                    max_conn_num.try_into().map_err(|_| {
                        D::Error::custom("max_connections exceeds maximum allowed value (u32::MAX)")
                    })
                } else {
                    Err(D::Error::custom("max_connections must be a number"))
                }
            } else {
                Err(D::Error::custom(
                    "Structured pool format requires max_connections field",
                ))
            }
        }
        _ => Err(D::Error::custom(
            "Pool must be either an integer or an object with max_connections",
        )),
    }
}

/// Root configuration structure mirroring tasker-config.yaml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskerConfig {
    /// Authentication and authorization settings
    pub auth: AuthConfig,

    /// Database connection and pooling configuration
    pub database: DatabaseConfig,

    /// Telemetry and monitoring settings
    pub telemetry: TelemetryConfig,

    /// Task processing engine configuration
    pub engine: EngineConfig,

    /// TaskTemplate discovery configuration
    pub task_templates: TaskTemplatesConfig,

    /// Health monitoring configuration
    pub health: HealthConfig,

    /// Dependency graph processing settings
    pub dependency_graph: DependencyGraphConfig,

    /// System-wide settings
    pub system: SystemConfig,

    /// Backoff and retry configuration
    pub backoff: BackoffConfig,

    /// Task execution settings
    pub execution: ExecutionConfig,

    /// Task reenqueue configuration
    pub reenqueue: ReenqueueConfig,

    /// Event processing configuration
    pub events: EventsConfig,

    /// Caching configuration
    pub cache: CacheConfig,

    /// Query caching configuration - reuse existing QueryCacheConfig
    pub query_cache: QueryCacheConfig,

    /// PGMQ (PostgreSQL Message Queue) configuration
    pub pgmq: PgmqConfig,

    /// Orchestration system configuration
    pub orchestration: OrchestrationConfig,
    
    /// Circuit breaker configuration for resilience patterns
    pub circuit_breakers: CircuitBreakerConfig,
}

/// Authentication and authorization configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub authentication_enabled: bool,
    pub strategy: String,
    pub current_user_method: String,
    pub authenticate_user_method: String,
    pub authorization_enabled: bool,
    pub authorization_coordinator_class: String,
}

/// Database connection and pooling configuration
///
/// ## Architecture: Worker-Optimized Configuration
///
/// This configuration format is designed for **Ruby workers and ActiveRecord integration**.
/// Uses a simple integer `pool` value that maps directly to ActiveRecord's connection pool size.
///
/// **Use this configuration for:**
/// - Ruby worker processes
/// - ActiveRecord database connections  
/// - Simple connection pool requirements
/// - Integration with Rails/Sinatra applications
///
/// **For high-performance orchestration, see:** `orchestration::config::DatabaseConfig`
/// which provides structured pool configuration with timeouts and lifecycle management.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub enable_secondary_database: bool,
    pub url: Option<String>,
    pub adapter: String,
    pub encoding: String,
    pub host: String,
    pub username: String,
    pub password: String,
    /// Simple pool size for ActiveRecord compatibility (use max_connections from structured format)
    #[serde(deserialize_with = "deserialize_pool_config")]
    pub pool: u32,
    pub variables: DatabaseVariables,
    pub checkout_timeout: u64,
    pub reaping_frequency: u64,
    /// Environment-specific database name override
    pub database: Option<String>,
    /// Skip migration check on startup (useful for development/testing)
    #[serde(default)]
    pub skip_migration_check: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseVariables {
    pub statement_timeout: u64,
}

impl DatabaseConfig {
    /// Get database name for the current environment
    pub fn database_name(&self, environment: &str) -> String {
        // Use explicit database name if provided (from environment overrides)
        if let Some(db_name) = &self.database {
            return db_name.clone();
        }

        // Otherwise use environment-based naming convention
        match environment {
            "development" => "tasker_rust_development".to_string(),
            "test" => "tasker_rust_test".to_string(),
            "production" => {
                std::env::var("POSTGRES_DB").unwrap_or_else(|_| "tasker_production".to_string())
            }
            _ => format!("tasker_rust_{environment}"),
        }
    }

    /// Build complete database URL from configuration
    pub fn database_url(&self, environment: &str) -> String {
        // If URL is explicitly provided (with ${DATABASE_URL} expansion), use it
        if let Some(url) = &self.url {
            if url.starts_with("${DATABASE_URL}") {
                if let Ok(env_url) = std::env::var("DATABASE_URL") {
                    return env_url;
                }
            } else if !url.is_empty() && url != "${DATABASE_URL}" {
                return url.clone();
            }
        }

        // Build URL from components
        let port = std::env::var("DATABASE_PORT").unwrap_or_else(|_| "5432".to_string());

        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username,
            self.password,
            self.host,
            port,
            self.database_name(environment)
        )
    }
}

/// Telemetry and monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub service_name: String,
    pub sample_rate: f64,
}

/// Task processing engine configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EngineConfig {
    pub task_handler_directory: String,
    pub task_config_directory: String,
    pub identity_strategy: String,
    pub custom_events_directories: Vec<String>,
}

/// TaskTemplate discovery configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskTemplatesConfig {
    pub search_paths: Vec<String>,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthConfig {
    pub enabled: bool,
    pub check_interval_seconds: u64,
    pub alert_thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlertThresholds {
    pub error_rate: f64,
    pub queue_depth: f64,
}

/// Dependency graph processing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependencyGraphConfig {
    pub max_depth: u32,
    pub cycle_detection_enabled: bool,
    pub optimization_enabled: bool,
}

/// System-wide configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub default_dependent_system: String,
    pub default_queue_name: String,
    pub version: String, // Updated to match Rust TASKER_CORE_VERSION
    // New constants unification fields
    pub max_recursion_depth: u32, // Replaces hardcoded recursion limits
}

/// Backoff and retry configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackoffConfig {
    pub default_backoff_seconds: Vec<u64>,
    pub max_backoff_seconds: u64,
    pub backoff_multiplier: f64,
    pub jitter_enabled: bool,
    pub jitter_max_percentage: f64,
    pub reenqueue_delays: ReenqueueDelays,
    pub default_reenqueue_delay: u64,
    pub buffer_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReenqueueDelays {
    pub has_ready_steps: u64,
    pub waiting_for_dependencies: u64,
    pub processing: u64,
}

/// Task execution configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    pub processing_mode: String,
    pub max_concurrent_tasks: u32,
    pub max_concurrent_steps: u32,
    pub default_timeout_seconds: u64,
    pub step_execution_timeout_seconds: u64,
    pub environment: String,
    pub max_discovery_attempts: u32,
    pub step_batch_size: u32,
    // New constants unification fields
    pub max_retries: u32,                // Replaces Ruby FALLBACK_MAX_RETRIES
    pub max_workflow_steps: u32,         // Replaces Rust constants::system::MAX_WORKFLOW_STEPS
    pub connection_timeout_seconds: u64, // Replaces Ruby hardcoded API timeouts
}

impl ExecutionConfig {
    /// Get step execution timeout as Duration
    pub fn step_execution_timeout(&self) -> Duration {
        Duration::from_secs(self.step_execution_timeout_seconds)
    }

    /// Get default task timeout as Duration
    pub fn default_timeout(&self) -> Duration {
        Duration::from_secs(self.default_timeout_seconds)
    }
}

/// Task reenqueue configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReenqueueConfig {
    pub has_ready_steps: u64,
    pub waiting_for_dependencies: u64,
    pub processing: u64,
}

/// Event processing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventsConfig {
    pub batch_size: u32,
    pub enabled: bool,
    pub batch_timeout_ms: u64,
}

/// Caching configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub ttl_seconds: u64,
    pub max_size: u32,
}

/// PGMQ (PostgreSQL Message Queue) configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PgmqConfig {
    pub poll_interval_ms: u64,
    pub visibility_timeout_seconds: u64,
    pub batch_size: u32,
    pub max_retries: u32,
    pub default_namespaces: Vec<String>,
    pub queue_naming_pattern: String,
    // New constants unification fields
    pub max_batch_size: u32, // Replaces Ruby MAX_MESSAGE_COUNT constant
    pub shutdown_timeout_seconds: u64, // Replaces Ruby FALLBACK_SHUTDOWN_TIMEOUT constant
}

impl PgmqConfig {
    /// Get poll interval as Duration
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }

    /// Get visibility timeout as Duration
    pub fn visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.visibility_timeout_seconds)
    }

    /// Generate queue name for a namespace
    pub fn queue_name_for_namespace(&self, namespace: &str) -> String {
        self.queue_naming_pattern.replace("{namespace}", namespace)
    }
}

/// Orchestration system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationConfig {
    pub mode: String,
    pub task_requests_queue_name: String,
    pub tasks_per_cycle: u32,
    pub cycle_interval_ms: u64,
    pub task_request_polling_interval_ms: u64,
    pub task_request_visibility_timeout_seconds: u64,
    pub task_request_batch_size: u32,
    pub active_namespaces: Vec<String>,
    pub max_concurrent_orchestrators: u32,
    pub enable_performance_logging: bool,
    pub default_claim_timeout_seconds: u64,
    pub queues: QueueConfig,
    pub embedded_orchestrator: EmbeddedOrchestratorConfig,
    pub enable_heartbeat: bool,
    pub heartbeat_interval_ms: u64,
}

impl OrchestrationConfig {
    /// Get cycle interval as Duration
    pub fn cycle_interval(&self) -> Duration {
        Duration::from_millis(self.cycle_interval_ms)
    }

    /// Get task request polling interval as Duration
    pub fn task_request_polling_interval(&self) -> Duration {
        Duration::from_millis(self.task_request_polling_interval_ms)
    }

    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_millis(self.heartbeat_interval_ms)
    }

    /// Get task request visibility timeout as Duration
    pub fn task_request_visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.task_request_visibility_timeout_seconds)
    }

    /// Get default claim timeout as Duration
    pub fn default_claim_timeout(&self) -> Duration {
        Duration::from_secs(self.default_claim_timeout_seconds)
    }
}

/// Queue configuration for orchestration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueConfig {
    pub task_requests: String,
    pub task_processing: String,
    pub batch_results: String,
    pub step_results: String,
    pub worker_queues: HashMap<String, String>,
    pub settings: QueueSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueSettings {
    pub visibility_timeout_seconds: u64,
    pub message_retention_seconds: u64,
    pub dead_letter_queue_enabled: bool,
    pub max_receive_count: u32,
}

/// Embedded orchestrator configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddedOrchestratorConfig {
    pub auto_start: bool,
    pub namespaces: Vec<String>,
    pub shutdown_timeout_seconds: u64,
}

impl EmbeddedOrchestratorConfig {
    /// Get shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_seconds)
    }
}

/// Circuit breaker configuration integrated with YAML config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    /// Whether circuit breakers are enabled globally
    pub enabled: bool,
    
    /// Global circuit breaker settings
    pub global_settings: CircuitBreakerGlobalSettings,
    
    /// Default configuration for new circuit breakers
    pub default_config: CircuitBreakerComponentConfig,
    
    /// Specific configurations for named components
    pub component_configs: HashMap<String, CircuitBreakerComponentConfig>,
}

/// Global circuit breaker settings from YAML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerGlobalSettings {
    /// Maximum number of circuit breakers allowed
    pub max_circuit_breakers: usize,
    
    /// Interval for metrics collection and reporting in seconds
    pub metrics_collection_interval_seconds: u64,
    
    /// Whether to enable automatic circuit breaker creation
    pub auto_create_enabled: bool,
    
    /// Minimum interval between state transitions in seconds (prevents oscillation)
    pub min_state_transition_interval_seconds: f64,
}

/// Circuit breaker configuration for a specific component from YAML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerComponentConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: usize,
    
    /// Time to wait in open state before attempting recovery (in seconds)
    pub timeout_seconds: u64,
    
    /// Number of successful calls in half-open state to close circuit
    pub success_threshold: usize,
}

impl CircuitBreakerConfig {
    /// Get configuration for a specific component
    pub fn config_for_component(&self, component_name: &str) -> CircuitBreakerComponentConfig {
        self.component_configs
            .get(component_name)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }
    
}

impl CircuitBreakerComponentConfig {
    /// Convert to resilience module's format
    pub fn to_resilience_config(&self) -> crate::resilience::config::CircuitBreakerConfig {
        crate::resilience::config::CircuitBreakerConfig {
            failure_threshold: self.failure_threshold,
            timeout: Duration::from_secs(self.timeout_seconds),
            success_threshold: self.success_threshold,
        }
    }
}

impl CircuitBreakerGlobalSettings {
    /// Convert to resilience module's format
    pub fn to_resilience_config(&self) -> crate::resilience::config::GlobalCircuitBreakerSettings {
        crate::resilience::config::GlobalCircuitBreakerSettings {
            max_circuit_breakers: self.max_circuit_breakers,
            metrics_collection_interval: Duration::from_secs(self.metrics_collection_interval_seconds),
            auto_create_enabled: self.auto_create_enabled,
            min_state_transition_interval: Duration::from_secs_f64(self.min_state_transition_interval_seconds),
        }
    }
}

impl Default for TaskerConfig {
    /// Create a safe fallback configuration with minimal defaults
    /// Used when configuration loading fails completely
    fn default() -> Self {
        use std::collections::HashMap;

        Self {
            auth: AuthConfig {
                authentication_enabled: false,
                strategy: "none".to_string(),
                current_user_method: "current_user".to_string(),
                authenticate_user_method: "authenticate_user!".to_string(),
                authorization_enabled: false,
                authorization_coordinator_class: "Tasker::Authorization::BaseCoordinator"
                    .to_string(),
            },
            database: DatabaseConfig {
                enable_secondary_database: false,
                url: Some(
                    "postgresql://tasker:tasker@localhost:5432/tasker_development".to_string(),
                ),
                adapter: "postgresql".to_string(),
                encoding: "unicode".to_string(),
                host: "localhost".to_string(),
                username: "tasker".to_string(),
                password: "tasker".to_string(),
                pool: 10,
                variables: DatabaseVariables {
                    statement_timeout: 5000,
                },
                checkout_timeout: 10,
                reaping_frequency: 10,
                database: Some("tasker_development".to_string()),
                skip_migration_check: false,
            },
            telemetry: TelemetryConfig {
                enabled: false,
                service_name: "tasker-core-rs".to_string(),
                sample_rate: 1.0,
            },
            engine: EngineConfig {
                task_handler_directory: "tasks".to_string(),
                task_config_directory: "tasker/tasks".to_string(),
                identity_strategy: "default".to_string(),
                custom_events_directories: vec!["config/tasker/events".to_string()],
            },
            task_templates: TaskTemplatesConfig {
                search_paths: vec!["config/task_templates/*.{yml,yaml}".to_string()],
            },
            health: HealthConfig {
                enabled: true,
                check_interval_seconds: 60,
                alert_thresholds: AlertThresholds {
                    error_rate: 0.05,
                    queue_depth: 1000.0,
                },
            },
            dependency_graph: DependencyGraphConfig {
                max_depth: 50,
                cycle_detection_enabled: true,
                optimization_enabled: true,
            },
            system: SystemConfig {
                default_dependent_system: "default".to_string(),
                default_queue_name: "default".to_string(),
                version: "0.1.0".to_string(),
                max_recursion_depth: 50,
            },
            backoff: BackoffConfig {
                default_backoff_seconds: vec![1, 2, 4, 8, 16, 32],
                max_backoff_seconds: 300,
                backoff_multiplier: 2.0,
                jitter_enabled: true,
                jitter_max_percentage: 0.1,
                reenqueue_delays: ReenqueueDelays {
                    has_ready_steps: 0,
                    waiting_for_dependencies: 45,
                    processing: 10,
                },
                default_reenqueue_delay: 30,
                buffer_seconds: 5,
            },
            execution: ExecutionConfig {
                processing_mode: "pgmq".to_string(),
                max_concurrent_tasks: 100,
                max_concurrent_steps: 1000,
                default_timeout_seconds: 3600,
                step_execution_timeout_seconds: 300,
                environment: "development".to_string(),
                max_discovery_attempts: 3,
                step_batch_size: 10,
                max_retries: 3,
                max_workflow_steps: 1000,
                connection_timeout_seconds: 10,
            },
            reenqueue: ReenqueueConfig {
                has_ready_steps: 1,
                waiting_for_dependencies: 5,
                processing: 2,
            },
            events: EventsConfig {
                batch_size: 100,
                enabled: true,
                batch_timeout_ms: 1000,
            },
            cache: CacheConfig {
                enabled: true,
                ttl_seconds: 3600,
                max_size: 10000,
            },
            query_cache: QueryCacheConfig::for_development(),
            pgmq: PgmqConfig {
                poll_interval_ms: 250,
                visibility_timeout_seconds: 30,
                batch_size: 5,
                max_retries: 3,
                default_namespaces: vec!["default".to_string()],
                queue_naming_pattern: "{namespace}_queue".to_string(),
                max_batch_size: 100,
                shutdown_timeout_seconds: 30,
            },
            orchestration: OrchestrationConfig {
                mode: "embedded".to_string(),
                task_requests_queue_name: "task_requests_queue".to_string(),
                tasks_per_cycle: 5,
                cycle_interval_ms: 250,
                task_request_polling_interval_ms: 250,
                task_request_visibility_timeout_seconds: 300,
                task_request_batch_size: 10,
                active_namespaces: vec!["default".to_string()],
                max_concurrent_orchestrators: 1,
                enable_performance_logging: false,
                default_claim_timeout_seconds: 300,
                queues: QueueConfig {
                    task_requests: "task_requests_queue".to_string(),
                    task_processing: "task_processing_queue".to_string(),
                    batch_results: "batch_results_queue".to_string(),
                    step_results: "orchestration_step_results".to_string(),
                    worker_queues: {
                        let mut queues = HashMap::new();
                        queues.insert("default".to_string(), "default_queue".to_string());
                        queues
                    },
                    settings: QueueSettings {
                        visibility_timeout_seconds: 30,
                        message_retention_seconds: 604800,
                        dead_letter_queue_enabled: true,
                        max_receive_count: 3,
                    },
                },
                embedded_orchestrator: EmbeddedOrchestratorConfig {
                    auto_start: false,
                    namespaces: vec!["default".to_string()],
                    shutdown_timeout_seconds: 30,
                },
                enable_heartbeat: true,
                heartbeat_interval_ms: 5000,
            },
            circuit_breakers: CircuitBreakerConfig {
                enabled: true,
                global_settings: CircuitBreakerGlobalSettings {
                    max_circuit_breakers: 50,
                    metrics_collection_interval_seconds: 30,
                    auto_create_enabled: true,
                    min_state_transition_interval_seconds: 1.0,
                },
                default_config: CircuitBreakerComponentConfig {
                    failure_threshold: 5,
                    timeout_seconds: 30,
                    success_threshold: 2,
                },
                component_configs: {
                    let mut configs = HashMap::new();
                    configs.insert("database".to_string(), CircuitBreakerComponentConfig {
                        failure_threshold: 5,
                        timeout_seconds: 60,
                        success_threshold: 3,
                    });
                    configs.insert("pgmq".to_string(), CircuitBreakerComponentConfig {
                        failure_threshold: 3,
                        timeout_seconds: 15,
                        success_threshold: 2,
                    });
                    configs
                },
            },
        }
    }
}

impl TaskerConfig {
    /// Validate configuration for consistency and required fields
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        // Database configuration validation
        if self.database.host.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "database.host",
                "database configuration",
            ));
        }

        if self.database.username.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "database.username",
                "database configuration",
            ));
        }

        if self.database.pool == 0 {
            return Err(ConfigurationError::invalid_value(
                "database.pool",
                "0",
                "pool size must be greater than 0",
            ));
        }

        // Execution configuration validation
        if self.execution.environment.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "execution.environment",
                "execution configuration",
            ));
        }

        // PGMQ configuration validation
        if self.pgmq.batch_size == 0 {
            return Err(ConfigurationError::invalid_value(
                "pgmq.batch_size",
                "0",
                "batch size must be greater than 0",
            ));
        }

        // Orchestration configuration validation
        if self.orchestration.active_namespaces.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "orchestration.active_namespaces",
                "at least one active namespace must be configured",
            ));
        }

        Ok(())
    }

    /// Get database URL for the current environment
    pub fn database_url(&self) -> String {
        self.database.database_url(&self.execution.environment)
    }

    /// Check if running in test environment
    pub fn is_test_environment(&self) -> bool {
        self.execution.environment == "test"
    }

    /// Check if running in development environment
    pub fn is_development_environment(&self) -> bool {
        self.execution.environment == "development"
    }

    /// Check if running in production environment
    pub fn is_production_environment(&self) -> bool {
        self.execution.environment == "production"
    }

    // New accessor methods for constants unification

    /// Get maximum dependency depth
    pub fn max_dependency_depth(&self) -> usize {
        self.dependency_graph.max_depth as usize
    }

    /// Get maximum workflow steps
    pub fn max_workflow_steps(&self) -> usize {
        self.execution.max_workflow_steps as usize
    }

    /// Get system version string
    pub fn system_version(&self) -> &str {
        &self.system.version
    }

    /// Get connection timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.execution.connection_timeout_seconds)
    }

    /// Get maximum retries
    pub fn max_retries(&self) -> u32 {
        self.execution.max_retries
    }

    /// Get maximum recursion depth
    pub fn max_recursion_depth(&self) -> usize {
        self.system.max_recursion_depth as usize
    }

    /// Get PGMQ shutdown timeout as Duration
    pub fn pgmq_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.pgmq.shutdown_timeout_seconds)
    }

    /// Get PGMQ maximum batch size
    pub fn pgmq_max_batch_size(&self) -> u32 {
        self.pgmq.max_batch_size
    }

    /// Validate cross-language configuration consistency between Rust and Ruby
    ///
    /// This ensures that both Ruby and Rust sides use the same configuration values
    /// for critical system parameters that affect orchestration behavior.
    ///
    /// # Returns
    ///
    /// `Ok(())` if configuration is consistent, otherwise returns a `ConfigurationError`
    /// listing the inconsistencies found.
    pub fn validate_ruby_rust_consistency(&self) -> Result<(), ConfigurationError> {
        let mut inconsistencies = Vec::new();

        // Define expected Ruby-compatible defaults that should match
        let dependency_depth: u32 = 50;
        let max_steps: u32 = 1000;
        let max_retries: u32 = 3;
        let visibility_timeout: u64 = 30;
        let batch_size: u32 = 5;

        // Check dependency_graph.max_depth
        if self.dependency_graph.max_depth != dependency_depth {
            inconsistencies.push(format!(
                "dependency_graph.max_depth: Rust={}, Ruby Expected={}",
                self.dependency_graph.max_depth, dependency_depth
            ));
        }

        // Check execution.max_workflow_steps
        if self.execution.max_workflow_steps != max_steps {
            inconsistencies.push(format!(
                "execution.max_workflow_steps: Rust={}, Ruby Expected={}",
                self.execution.max_workflow_steps, max_steps
            ));
        }

        // Check execution.max_retries
        if self.execution.max_retries != max_retries {
            inconsistencies.push(format!(
                "execution.max_retries: Rust={}, Ruby Expected={}",
                self.execution.max_retries, max_retries
            ));
        }

        // Check pgmq.visibility_timeout_seconds
        if self.pgmq.visibility_timeout_seconds != visibility_timeout {
            inconsistencies.push(format!(
                "pgmq.visibility_timeout_seconds: Rust={}, Ruby Expected={}",
                self.pgmq.visibility_timeout_seconds, visibility_timeout
            ));
        }

        // Check pgmq.batch_size
        if self.pgmq.batch_size != batch_size {
            inconsistencies.push(format!(
                "pgmq.batch_size: Rust={}, Ruby Expected={}",
                self.pgmq.batch_size, batch_size
            ));
        }

        if !inconsistencies.is_empty() {
            return Err(ConfigurationError::invalid_value(
                "cross_language_consistency",
                inconsistencies.join(", "),
                "Ruby and Rust configuration values must match for consistent behavior",
            ));
        }

        Ok(())
    }

    /// Get configuration warnings for potential cross-language issues
    ///
    /// This provides non-fatal warnings about configuration that might cause
    /// issues in cross-language coordination between Ruby and Rust components.
    ///
    /// # Returns
    ///
    /// Vector of warning messages describing potential configuration issues.
    pub fn cross_language_configuration_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for environment-specific concerns
        match self.execution.environment.as_str() {
            "test" => {
                if self.dependency_graph.max_depth > 20 {
                    warnings.push(format!(
                        "dependency_graph.max_depth={} is high for test environment, may slow test execution",
                        self.dependency_graph.max_depth
                    ));
                }

                if self.execution.max_workflow_steps > 500 {
                    warnings.push(format!(
                        "execution.max_workflow_steps={} is high for test environment, may cause test timeouts",
                        self.execution.max_workflow_steps
                    ));
                }

                if self.pgmq.poll_interval_ms < 100 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is very low, may cause high CPU usage in tests",
                        self.pgmq.poll_interval_ms
                    ));
                }
            }
            "production" => {
                if self.pgmq.poll_interval_ms < 200 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is low for production, may cause high CPU usage",
                        self.pgmq.poll_interval_ms
                    ));
                }

                if self.execution.max_retries < 2 {
                    warnings.push(format!(
                        "execution.max_retries={} is low for production, may cause permanent failures for transient errors",
                        self.execution.max_retries
                    ));
                }

                if self.execution.max_workflow_steps > 10000 {
                    warnings.push(format!(
                        "execution.max_workflow_steps={} is very high, may cause memory issues",
                        self.execution.max_workflow_steps
                    ));
                }
            }
            "development" => {
                // Development warnings are generally less strict
                if self.pgmq.poll_interval_ms < 100 {
                    warnings.push(format!(
                        "pgmq.poll_interval_ms={}ms is low for development, may impact debugging experience",
                        self.pgmq.poll_interval_ms
                    ));
                }
            }
            _ => {
                warnings.push(format!(
                    "Unknown environment '{}', configuration validation may not be accurate",
                    self.execution.environment
                ));
            }
        }

        // Check for missing configuration that Ruby expects
        if self.system.version != "0.1.0" {
            warnings.push(format!(
                "system.version='{}' differs from expected Ruby version '0.1.0', may cause version mismatch issues",
                self.system.version
            ));
        }

        warnings
    }
}
