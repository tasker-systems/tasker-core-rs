//! # TaskerCore Configuration System (TAS-34 Unified TOML)
//!
//! This module provides unified TOML-based configuration management with strict validation
//! and fail-fast behavior. All configuration loading is handled by UnifiedConfigLoader.
//!
//! ## Architecture
//!
//! - **Single Source of Truth**: UnifiedConfigLoader handles all configuration loading
//! - **TOML Only**: Component-based TOML configuration with environment overrides
//! - **Fail-Fast Validation**: No silent fallbacks or defaults
//! - **Strict Type Safety**: ValidatedConfig provides type-safe access to all components
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::config::UnifiedConfigLoader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration with environment detection
//! let mut loader = UnifiedConfigLoader::new_from_env()?;
//! let config = loader.load_tasker_config()?;
//!
//! // Access configuration values
//! let database_url = config.database_url();
//! let pool_size = config.database.pool;
//! let timeout = config.execution.step_execution_timeout_seconds;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod loader;
pub mod query_cache_config;
pub mod unified_loader;

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// Primary exports - TAS-34 Unified Configuration System
pub use unified_loader::{UnifiedConfigLoader, ValidatedConfig};

// Re-export types and errors
pub use error::{ConfigResult, ConfigurationError};
pub use query_cache_config::{CacheTypeConfig, QueryCacheConfig};

// Compatibility wrapper (thin wrapper around UnifiedConfigLoader)
pub use loader::ConfigManager;

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

/// Root configuration structure for component-based config system
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

    /// Orchestration executor pools configuration (TAS-34)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_pools: Option<ExecutorPoolsConfig>,

    /// Web API configuration (TAS-28)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebConfig>,
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
            if url == "${DATABASE_URL}" || url.starts_with("${DATABASE_URL}") {
                // Try to expand ${DATABASE_URL} environment variable
                if let Ok(env_url) = std::env::var("DATABASE_URL") {
                    return env_url;
                }
                // If DATABASE_URL is not set, fall through to build from components
            } else if !url.is_empty() {
                // Use the URL as-is (not a variable reference)
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

/// TAS-37 Supplemental: Operational state configuration for shutdown-aware monitoring
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperationalStateConfig {
    /// Enable shutdown-aware health monitoring
    pub enable_shutdown_aware_monitoring: bool,
    /// Suppress health alerts during graceful shutdown
    pub suppress_alerts_during_shutdown: bool,
    /// Health threshold multiplier during startup (0.0-1.0)
    pub startup_health_threshold_multiplier: f64,
    /// Health threshold multiplier during graceful shutdown (0.0-1.0)
    pub shutdown_health_threshold_multiplier: f64,
    /// Timeout for graceful shutdown operations in seconds
    pub graceful_shutdown_timeout_seconds: u64,
    /// Timeout for emergency shutdown operations in seconds
    pub emergency_shutdown_timeout_seconds: u64,
    /// Enable operational state transition logging
    pub enable_transition_logging: bool,
    /// Log level for operational state transitions ("DEBUG", "INFO", "WARN", "ERROR")
    pub transition_log_level: String,
}

impl OperationalStateConfig {
    /// Get graceful shutdown timeout as Duration
    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout_seconds)
    }

    /// Get emergency shutdown timeout as Duration
    pub fn emergency_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.emergency_shutdown_timeout_seconds)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.startup_health_threshold_multiplier < 0.0
            || self.startup_health_threshold_multiplier > 1.0
        {
            return Err(
                "startup_health_threshold_multiplier must be between 0.0 and 1.0".to_string(),
            );
        }

        if self.shutdown_health_threshold_multiplier < 0.0
            || self.shutdown_health_threshold_multiplier > 1.0
        {
            return Err(
                "shutdown_health_threshold_multiplier must be between 0.0 and 1.0".to_string(),
            );
        }

        if self.graceful_shutdown_timeout_seconds == 0 {
            return Err("graceful_shutdown_timeout_seconds must be greater than 0".to_string());
        }

        if self.emergency_shutdown_timeout_seconds == 0 {
            return Err("emergency_shutdown_timeout_seconds must be greater than 0".to_string());
        }

        match self.transition_log_level.as_str() {
            "DEBUG" | "INFO" | "WARN" | "ERROR" => Ok(()),
            _ => Err("transition_log_level must be one of: DEBUG, INFO, WARN, ERROR".to_string()),
        }
    }
}

impl Default for OperationalStateConfig {
    fn default() -> Self {
        Self {
            enable_shutdown_aware_monitoring: true,
            suppress_alerts_during_shutdown: true,
            startup_health_threshold_multiplier: 0.5, // Relaxed thresholds during startup
            shutdown_health_threshold_multiplier: 0.0, // No health requirements during shutdown
            graceful_shutdown_timeout_seconds: 30,
            emergency_shutdown_timeout_seconds: 5,
            enable_transition_logging: true,
            transition_log_level: "INFO".to_string(),
        }
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
    /// TAS-37 Supplemental: Shutdown-aware monitoring configuration
    pub operational_state: OperationalStateConfig,
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

    /// Convert to OrchestrationSystemConfig for bootstrapping the orchestration system
    pub fn to_orchestration_system_config(
        &self,
    ) -> crate::orchestration::OrchestrationSystemConfig {
        use crate::orchestration::{
            orchestration_loop::OrchestrationLoopConfig, step_enqueuer::StepEnqueuerConfig,
            step_result_processor::StepResultProcessorConfig, task_claimer::TaskClaimerConfig,
            OrchestrationSystemConfig,
        };
        use std::time::SystemTime;

        // Generate orchestrator ID if not provided
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let orchestrator_id = format!("orchestrator-{timestamp}");

        // Create orchestration loop configuration
        let orchestration_loop_config = OrchestrationLoopConfig {
            tasks_per_cycle: self.tasks_per_cycle as i32,
            namespace_filter: None,
            cycle_interval: Duration::from_millis(self.cycle_interval_ms),
            max_cycles: None,
            enable_performance_logging: self.enable_performance_logging,
            enable_heartbeat: self.enable_heartbeat,
            task_claimer_config: TaskClaimerConfig {
                max_batch_size: self.tasks_per_cycle as i32,
                default_claim_timeout: self.default_claim_timeout_seconds as i32,
                heartbeat_interval: Duration::from_millis(self.heartbeat_interval_ms),
                enable_heartbeat: self.enable_heartbeat,
            },
            step_enqueuer_config: StepEnqueuerConfig::default(),
            step_result_processor_config: StepResultProcessorConfig::default(),
        };

        OrchestrationSystemConfig {
            task_requests_queue_name: self.task_requests_queue_name.clone(),
            orchestrator_id,
            orchestration_loop_config,
            task_request_polling_interval_ms: self.task_request_polling_interval_ms,
            task_request_visibility_timeout_seconds: self.task_request_visibility_timeout_seconds
                as i32,
            task_request_batch_size: self.task_request_batch_size as i32,
            active_namespaces: self.active_namespaces.clone(),
            max_concurrent_orchestrators: self.max_concurrent_orchestrators as usize,
            enable_performance_logging: self.enable_performance_logging,
        }
    }
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        use std::collections::HashMap;

        Self {
            mode: "embedded".to_string(),
            task_requests_queue_name: "task_requests_queue".to_string(),
            tasks_per_cycle: 5,
            cycle_interval_ms: 250,
            task_request_polling_interval_ms: 250,
            task_request_visibility_timeout_seconds: 300,
            task_request_batch_size: 10,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            max_concurrent_orchestrators: 3,
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
                    queues.insert("fulfillment".to_string(), "fulfillment_queue".to_string());
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
                namespaces: vec!["default".to_string(), "fulfillment".to_string()],
                shutdown_timeout_seconds: 30,
            },
            enable_heartbeat: true,
            heartbeat_interval_ms: 5000,
            operational_state: OperationalStateConfig::default(), // TAS-37 Supplemental: Add missing field
        }
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
            metrics_collection_interval: Duration::from_secs(
                self.metrics_collection_interval_seconds,
            ),
            auto_create_enabled: self.auto_create_enabled,
            min_state_transition_interval: Duration::from_secs_f64(
                self.min_state_transition_interval_seconds,
            ),
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
                operational_state: OperationalStateConfig::default(), // TAS-37 Supplemental: Add missing field
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
                    configs.insert(
                        "database".to_string(),
                        CircuitBreakerComponentConfig {
                            failure_threshold: 5,
                            timeout_seconds: 60,
                            success_threshold: 3,
                        },
                    );
                    configs.insert(
                        "pgmq".to_string(),
                        CircuitBreakerComponentConfig {
                            failure_threshold: 3,
                            timeout_seconds: 15,
                            success_threshold: 2,
                        },
                    );
                    configs
                },
            },
            executor_pools: None, // Optional, only populated when YAML contains executor_pools
            web: None,            // Optional, only populated when TOML contains web configuration
        }
    }
}

/// Orchestration executor pools configuration (TAS-34)
/// Configures the advanced executor pool system that replaces naive tokio polling loops
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutorPoolsConfig {
    /// Coordinator configuration for auto-scaling and health monitoring
    pub coordinator: ExecutorCoordinatorConfig,

    /// Task request processor configuration
    pub task_request_processor: ExecutorInstanceConfig,

    /// Task claimer configuration
    pub task_claimer: ExecutorInstanceConfig,

    /// Step enqueuer configuration
    pub step_enqueuer: ExecutorInstanceConfig,

    /// Step result processor configuration
    pub step_result_processor: ExecutorInstanceConfig,

    /// Task finalizer configuration
    pub task_finalizer: ExecutorInstanceConfig,
}

/// Coordinator configuration for managing executor pools
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutorCoordinatorConfig {
    /// Whether auto-scaling is enabled
    pub auto_scaling_enabled: bool,

    /// Target utilization for scaling decisions (0.0-1.0)
    pub target_utilization: f64,

    /// How often to check scaling conditions (seconds)
    pub scaling_interval_seconds: u64,

    /// How often to check executor health (seconds)
    pub health_check_interval_seconds: u64,

    /// Cooldown period between scaling operations (seconds)
    pub scaling_cooldown_seconds: u64,

    /// Maximum database pool usage before applying backpressure (0.0-1.0)
    pub max_db_pool_usage: f64,
}

/// Configuration for a specific executor instance type
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutorInstanceConfig {
    /// Minimum number of executors to maintain
    pub min_executors: usize,

    /// Maximum number of executors allowed
    pub max_executors: usize,

    /// Polling interval in milliseconds
    pub polling_interval_ms: u64,

    /// Maximum batch size for processing
    pub batch_size: usize,

    /// Processing timeout in milliseconds
    pub processing_timeout_ms: u64,

    /// Maximum number of retries for failed operations
    pub max_retries: u32,

    /// Whether circuit breaker is enabled
    pub circuit_breaker_enabled: bool,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,
}

impl ExecutorInstanceConfig {
    /// Convert to ExecutorConfig from the executor traits module
    pub fn to_executor_config(&self) -> crate::orchestration::executor::traits::ExecutorConfig {
        crate::orchestration::executor::traits::ExecutorConfig {
            polling_interval_ms: self.polling_interval_ms,
            batch_size: self.batch_size,
            processing_timeout_ms: self.processing_timeout_ms,
            max_retries: self.max_retries,
            backpressure_factor: 1.0, // Default, will be adjusted by coordinator
            circuit_breaker_enabled: self.circuit_breaker_enabled,
            circuit_breaker_threshold: self.circuit_breaker_threshold,
        }
    }
}

impl Default for ExecutorPoolsConfig {
    fn default() -> Self {
        Self {
            coordinator: ExecutorCoordinatorConfig::default(),
            task_request_processor: ExecutorInstanceConfig::default_for_type(
                "task_request_processor",
            ),
            task_claimer: ExecutorInstanceConfig::default_for_type("task_claimer"),
            step_enqueuer: ExecutorInstanceConfig::default_for_type("step_enqueuer"),
            step_result_processor: ExecutorInstanceConfig::default_for_type(
                "step_result_processor",
            ),
            task_finalizer: ExecutorInstanceConfig::default_for_type("task_finalizer"),
        }
    }
}

impl Default for ExecutorCoordinatorConfig {
    fn default() -> Self {
        Self {
            auto_scaling_enabled: true,
            target_utilization: 0.75,
            scaling_interval_seconds: 30,
            health_check_interval_seconds: 10,
            scaling_cooldown_seconds: 60,
            max_db_pool_usage: 0.85,
        }
    }
}

impl ExecutorInstanceConfig {
    /// Create default configuration for a specific executor type
    pub fn default_for_type(executor_type: &str) -> Self {
        match executor_type {
            "task_request_processor" => Self {
                min_executors: 1,
                max_executors: 5,
                polling_interval_ms: 100,
                batch_size: 10,
                processing_timeout_ms: 30000,
                max_retries: 3,
                circuit_breaker_enabled: true,
                circuit_breaker_threshold: 5,
            },
            "task_claimer" => Self {
                min_executors: 2,
                max_executors: 10,
                polling_interval_ms: 50,
                batch_size: 20,
                processing_timeout_ms: 30000,
                max_retries: 3,
                circuit_breaker_enabled: true,
                circuit_breaker_threshold: 3,
            },
            "step_enqueuer" => Self {
                min_executors: 2,
                max_executors: 8,
                polling_interval_ms: 50,
                batch_size: 50,
                processing_timeout_ms: 30000,
                max_retries: 3,
                circuit_breaker_enabled: true,
                circuit_breaker_threshold: 5,
            },
            "step_result_processor" => Self {
                min_executors: 2,
                max_executors: 10,
                polling_interval_ms: 100,
                batch_size: 20,
                processing_timeout_ms: 30000,
                max_retries: 3,
                circuit_breaker_enabled: true,
                circuit_breaker_threshold: 3,
            },
            "task_finalizer" => Self {
                min_executors: 1,
                max_executors: 4,
                polling_interval_ms: 200,
                batch_size: 10,
                processing_timeout_ms: 30000,
                max_retries: 3,
                circuit_breaker_enabled: true,
                circuit_breaker_threshold: 5,
            },
            _ => Self::default(), // Fallback to default
        }
    }
}

impl Default for ExecutorInstanceConfig {
    fn default() -> Self {
        Self {
            min_executors: 1,
            max_executors: 5,
            polling_interval_ms: 100,
            batch_size: 10,
            processing_timeout_ms: 30000,
            max_retries: 3,
            circuit_breaker_enabled: true,
            circuit_breaker_threshold: 5,
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

    /// Get executor pools configuration with fallback to defaults if not configured
    pub fn executor_pools(&self) -> ExecutorPoolsConfig {
        match &self.executor_pools {
            Some(pools) => pools.clone(),
            None => ExecutorPoolsConfig::default(),
        }
    }

    /// Check if executor pools are explicitly configured in YAML
    pub fn has_executor_pools_config(&self) -> bool {
        self.executor_pools.is_some()
    }

    /// Get executor configuration for a specific executor type
    pub fn get_executor_config(
        &self,
        executor_type: crate::orchestration::executor::traits::ExecutorType,
    ) -> crate::orchestration::executor::traits::ExecutorConfig {
        let executor_pools = self.executor_pools();

        let instance_config = match executor_type {
            crate::orchestration::executor::traits::ExecutorType::TaskRequestProcessor => {
                &executor_pools.task_request_processor
            }
            crate::orchestration::executor::traits::ExecutorType::OrchestrationLoop => {
                &executor_pools.step_enqueuer
            } // OrchestrationLoop handles step enqueueing
            crate::orchestration::executor::traits::ExecutorType::StepResultProcessor => {
                &executor_pools.step_result_processor
            }
        };

        instance_config.to_executor_config()
    }

    /// Get executor instance configuration for a specific executor type
    pub fn get_executor_instance_config(
        &self,
        executor_type: crate::orchestration::executor::traits::ExecutorType,
    ) -> ExecutorInstanceConfig {
        let executor_pools = self.executor_pools();

        match executor_type {
            crate::orchestration::executor::traits::ExecutorType::TaskRequestProcessor => {
                executor_pools.task_request_processor.clone()
            }
            crate::orchestration::executor::traits::ExecutorType::OrchestrationLoop => {
                executor_pools.step_enqueuer.clone()
            } // OrchestrationLoop handles step enqueueing
            crate::orchestration::executor::traits::ExecutorType::StepResultProcessor => {
                executor_pools.step_result_processor.clone()
            }
        }
    }
}

/// Web API configuration for TAS-28 Axum Web API
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebConfig {
    /// Whether the web API is enabled
    pub enabled: bool,

    /// Address to bind the web server to
    pub bind_address: String,

    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,

    /// Maximum request size in megabytes
    pub max_request_size_mb: u64,

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

    /// Resource monitoring configuration
    pub resource_monitoring: WebResourceMonitoringConfig,
}

/// Web API TLS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
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

    /// Whether to coordinate with orchestration pool
    pub coordinate_with_orchestration_pool: bool,

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

/// Web API resource monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebResourceMonitoringConfig {
    /// Whether to report pool usage to health monitor
    pub report_pool_usage_to_health_monitor: bool,

    /// Pool usage warning threshold (0.0-1.0)
    pub pool_usage_warning_threshold: f64,

    /// Pool usage critical threshold (0.0-1.0)
    pub pool_usage_critical_threshold: f64,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "0.0.0.0:8080".to_string(),
            request_timeout_ms: 30000,
            max_request_size_mb: 16,
            tls: WebTlsConfig::default(),
            database_pools: WebDatabasePoolsConfig::default(),
            cors: WebCorsConfig::default(),
            auth: WebAuthConfig::default(),
            rate_limiting: WebRateLimitConfig::default(),
            resilience: WebResilienceConfig::default(),
            resource_monitoring: WebResourceMonitoringConfig::default(),
        }
    }
}

impl Default for WebTlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: String::new(),
            key_path: String::new(),
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
            coordinate_with_orchestration_pool: true,
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

impl Default for WebResourceMonitoringConfig {
    fn default() -> Self {
        Self {
            report_pool_usage_to_health_monitor: true,
            pool_usage_warning_threshold: 0.75,
            pool_usage_critical_threshold: 0.90,
        }
    }
}

impl TaskerConfig {
    /// Get web configuration with fallback to defaults
    pub fn web_config(&self) -> WebConfig {
        self.web.clone().unwrap_or_default()
    }

    /// Check if web API is enabled
    pub fn web_enabled(&self) -> bool {
        self.web.as_ref().map_or(false, |w| w.enabled)
    }

    /// Get total database connections across all pools for resource coordination
    pub fn total_database_connections(&self) -> u32 {
        let orchestration_pool = self.database.pool;
        let web_pool = self.web_config().database_pools.web_api_max_connections;
        orchestration_pool + web_pool
    }

    /// Validate database connection limits for resource coordination
    pub fn validate_database_limits(&self) -> Result<(), String> {
        let total = self.total_database_connections();
        let web_config = self.web_config();

        if web_config.database_pools.coordinate_with_orchestration_pool {
            let hint = web_config.database_pools.max_total_connections_hint;
            if total > hint {
                return Err(format!(
                    "Total database connections ({}) exceeds resource hint ({}). \
                     Consider adjusting pool sizes or increasing database server limits.",
                    total, hint
                ));
            }
        }

        Ok(())
    }

    /// Get detailed resource allocation report for database pools (TAS-37 Web Integration)
    pub fn get_database_resource_allocation(&self) -> DatabaseResourceAllocation {
        let orchestration_pool = self.database.pool;
        let web_config = self.web_config();
        let web_pool = web_config.database_pools.web_api_max_connections;
        let total = orchestration_pool + web_pool;

        let orchestration_percentage = if total > 0 {
            (orchestration_pool as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let web_percentage = if total > 0 {
            (web_pool as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let coordination_enabled = web_config.database_pools.coordinate_with_orchestration_pool;
        let resource_hint = web_config.database_pools.max_total_connections_hint;
        let is_within_limits = !coordination_enabled || total <= resource_hint;

        DatabaseResourceAllocation {
            orchestration_pool_size: orchestration_pool,
            web_pool_size: web_pool,
            total_connections: total,
            orchestration_percentage,
            web_percentage,
            coordination_enabled,
            resource_hint,
            is_within_limits,
            utilization_ratio: if resource_hint > 0 {
                total as f64 / resource_hint as f64
            } else {
                0.0
            },
        }
    }

    /// Check if current resource allocation is optimal for workload balance
    pub fn is_resource_allocation_optimal(&self) -> ResourceAllocationAssessment {
        let allocation = self.get_database_resource_allocation();

        // Define optimal allocation ratios based on typical workload patterns
        // Orchestration typically needs more resources for heavy processing
        let optimal_orchestration_ratio = 0.70; // 70% for orchestration
        let optimal_web_ratio = 0.30; // 30% for web API
        let tolerance = 0.15; // 15% tolerance

        let orchestration_ratio = allocation.orchestration_percentage / 100.0;
        let web_ratio = allocation.web_percentage / 100.0;

        let orchestration_deviation = (orchestration_ratio - optimal_orchestration_ratio).abs();
        let web_deviation = (web_ratio - optimal_web_ratio).abs();

        let is_orchestration_optimal = orchestration_deviation <= tolerance;
        let is_web_optimal = web_deviation <= tolerance;
        let is_overall_optimal = is_orchestration_optimal && is_web_optimal;

        let mut recommendations = Vec::new();

        if !is_orchestration_optimal {
            if orchestration_ratio < optimal_orchestration_ratio - tolerance {
                recommendations.push(format!(
                    "Consider increasing orchestration pool size from {} to {} (target: {:.0}% of total)",
                    allocation.orchestration_pool_size,
                    (allocation.total_connections as f64 * optimal_orchestration_ratio) as u32,
                    optimal_orchestration_ratio * 100.0
                ));
            } else {
                recommendations.push(format!(
                    "Consider decreasing orchestration pool size from {} to {} (target: {:.0}% of total)",
                    allocation.orchestration_pool_size,
                    (allocation.total_connections as f64 * optimal_orchestration_ratio) as u32,
                    optimal_orchestration_ratio * 100.0
                ));
            }
        }

        if !is_web_optimal {
            if web_ratio < optimal_web_ratio - tolerance {
                recommendations.push(format!(
                    "Consider increasing web API pool size from {} to {} (target: {:.0}% of total)",
                    allocation.web_pool_size,
                    (allocation.total_connections as f64 * optimal_web_ratio) as u32,
                    optimal_web_ratio * 100.0
                ));
            } else {
                recommendations.push(format!(
                    "Consider decreasing web API pool size from {} to {} (target: {:.0}% of total)",
                    allocation.web_pool_size,
                    (allocation.total_connections as f64 * optimal_web_ratio) as u32,
                    optimal_web_ratio * 100.0
                ));
            }
        }

        if !allocation.is_within_limits {
            recommendations.push(format!(
                "Total connections ({}) exceed resource hint ({}). Consider increasing database server limits or reducing pool sizes.",
                allocation.total_connections, allocation.resource_hint
            ));
        }

        ResourceAllocationAssessment {
            is_optimal: is_overall_optimal,
            allocation,
            recommendations,
            orchestration_deviation,
            web_deviation,
        }
    }
}

/// Database resource allocation details (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct DatabaseResourceAllocation {
    pub orchestration_pool_size: u32,
    pub web_pool_size: u32,
    pub total_connections: u32,
    pub orchestration_percentage: f64,
    pub web_percentage: f64,
    pub coordination_enabled: bool,
    pub resource_hint: u32,
    pub is_within_limits: bool,
    pub utilization_ratio: f64,
}

/// Resource allocation assessment with recommendations (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct ResourceAllocationAssessment {
    pub is_optimal: bool,
    pub allocation: DatabaseResourceAllocation,
    pub recommendations: Vec<String>,
    pub orchestration_deviation: f64,
    pub web_deviation: f64,
}
