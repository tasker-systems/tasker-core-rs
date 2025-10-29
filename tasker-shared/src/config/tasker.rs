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
//! use tasker_shared::config::UnifiedConfigLoader;
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

use serde::{Deserialize, Serialize};
use std::time::Duration;

// Re-export types and errors
pub use super::circuit_breaker::{
    CircuitBreakerComponentConfig, CircuitBreakerConfig, CircuitBreakerGlobalSettings,
};
pub use super::error::{ConfigResult, ConfigurationError};
pub use super::event_systems::{
    EventSystemConfig, EventSystemHealthConfig, EventSystemProcessingConfig,
    EventSystemTimingConfig,
    OrchestrationEventSystemConfig as UnifiedOrchestrationEventSystemConfig,
    TaskReadinessEventSystemConfig as UnifiedTaskReadinessEventSystemConfig,
    WorkerEventSystemConfig as UnifiedWorkerEventSystemConfig,
};
pub use super::orchestration::{
    event_systems::OrchestrationEventSystemConfig, DecisionPointsConfig, ExecutorConfig,
    ExecutorType, OrchestrationConfig, OrchestrationSystemConfig,
};
pub use super::queues::{
    OrchestrationQueuesConfig, PgmqBackendConfig, QueuesConfig, RabbitMqBackendConfig,
};

pub use super::queue_classification::{ConfigDrivenMessageEvent, QueueClassifier, QueueType};
pub use super::worker::{
    EventSystemConfig as WorkerLegacyEventSystemConfig, HealthMonitoringConfig,
    StepProcessingConfig, WorkerConfig,
};

pub use super::web::*;

// TAS-43 Task Readiness System exports
pub use super::task_readiness::{
    BackoffConfig as TaskReadinessBackoffConfig, ConnectionConfig, EnhancedCoordinatorSettings,
    ErrorHandlingConfig, EventChannelConfig, EventClassificationConfig, NamespacePatterns,
    ReadinessFallbackConfig, TaskReadinessConfig, TaskReadinessCoordinatorConfig,
    TaskReadinessNotificationConfig,
};

// TAS-51 MPSC Channels Configuration export
pub use super::mpsc_channels::MpscChannelsConfig;

/// Root configuration structure for component-based config system
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskerConfig {
    /// Database connection and pooling configuration
    pub database: DatabaseConfig,

    /// Telemetry and monitoring settings
    pub telemetry: TelemetryConfig,

    /// TaskTemplate discovery configuration
    pub task_templates: TaskTemplatesConfig,

    /// System-wide settings
    pub system: SystemConfig,

    /// Backoff and retry configuration
    pub backoff: BackoffConfig,

    /// Task execution settings
    pub execution: ExecutionConfig,

    /// Queue system configuration with backend abstraction
    /// Note: PGMQ configuration is now handled within queues.pgmq
    pub queues: QueuesConfig,

    /// Orchestration system configuration
    pub orchestration: OrchestrationConfig,

    /// Circuit breaker configuration for resilience patterns
    pub circuit_breakers: CircuitBreakerConfig,

    /// Task readiness event-driven system configuration
    pub task_readiness: TaskReadinessConfig,

    // REMOVED: task_claimer for TAS-41 state machine approach
    /// Unified event systems configuration
    pub event_systems: EventSystemsConfig,

    /// MPSC channels configuration (TAS-51)
    /// Unified configuration for all Tokio MPSC channel buffer sizes
    pub mpsc_channels: MpscChannelsConfig,

    /// Decision points configuration (TAS-53)
    /// Dynamic workflow decision points for runtime conditional branching
    pub decision_points: DecisionPointsConfig,

    /// Worker configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<WorkerConfig>,
}

/// Unified event systems configuration
///
/// Contains all event system configurations in a standardized format
/// to eliminate configuration drift between orchestration, task readiness, and worker systems.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EventSystemsConfig {
    /// Orchestration event system configuration
    pub orchestration: UnifiedOrchestrationEventSystemConfig,

    /// Task readiness event system configuration
    pub task_readiness: UnifiedTaskReadinessEventSystemConfig,

    /// Worker event system configuration
    pub worker: UnifiedWorkerEventSystemConfig,
}

/// Database connection and pooling configuration
///
/// ## Architecture: Unified High-Performance Configuration
///
/// This unified configuration supports both Ruby workers and Rust orchestration with
/// structured pool configuration that provides fine-grained control over database
/// connection lifecycle and performance characteristics.
///
/// **Supports:**
/// - High-performance Rust orchestration (>10k events/sec)
/// - Ruby worker processes via structured pool mapping
/// - Sub-millisecond connection acquisition requirements
/// - Complex connection lifecycle management
/// - Production performance tuning with timeout controls
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Database URL - MUST be provided via TOML or DATABASE_URL env var
    pub url: Option<String>,
    /// Structured pool configuration for high-performance orchestration
    pub pool: DatabasePoolConfig,
    /// Database runtime variables (statement timeouts, etc.)
    pub variables: DatabaseVariables,
    /// Environment-specific database name override (optional)
    pub database: Option<String>,
    /// Skip migration check on startup (useful for development/testing)
    #[serde(default)]
    pub skip_migration_check: bool,
    // REMOVED (TAS-50 Phase 1): enable_secondary_database, adapter, encoding, host,
    // username, password, checkout_timeout, reaping_frequency
    // These fields were never used outside of config introspection
}

/// Database connection pool configuration
///
/// Provides fine-grained control over database connection lifecycle and performance
/// characteristics for high-throughput orchestration workloads.
///
/// **Performance Impact:**
/// - `max_connections`: Limits total connections (prevent resource exhaustion)
/// - `acquire_timeout_seconds`: Prevents deadlocks in high-concurrency scenarios
/// - `idle_timeout_seconds`: Reduces connection overhead during low activity periods
/// - `max_lifetime_seconds`: Prevents connection leaks and handles database restarts
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabasePoolConfig {
    /// Maximum concurrent database connections
    pub max_connections: u32,
    /// Minimum idle connections to maintain (for quick acquisition)
    pub min_connections: u32,
    /// Seconds to wait when acquiring a connection before timing out
    pub acquire_timeout_seconds: u64,
    /// Seconds a connection can be idle before being closed
    pub idle_timeout_seconds: u64,
    /// Maximum lifetime of a connection in seconds (prevents leaks)
    pub max_lifetime_seconds: u64,
}

impl Default for DatabasePoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            acquire_timeout_seconds: 30,
            idle_timeout_seconds: 300,
            max_lifetime_seconds: 3600,
        }
    }
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
    ///
    /// FAIL-FAST: This method expects DATABASE_URL to be set either:
    /// 1. In the TOML config as `database.url`
    /// 2. As the DATABASE_URL environment variable
    ///
    /// If neither is provided, the system will fail during validation.
    pub fn database_url(&self, _environment: &str) -> String {
        // Try TOML url field first
        if let Some(url) = &self.url {
            // Handle ${DATABASE_URL} or ${DATABASE_URL:-default} expansion
            if url.starts_with("${DATABASE_URL") {
                // Extract default value if present: ${DATABASE_URL:-default}
                if let Some(default_start) = url.find(":-") {
                    let default_end = url.rfind('}').unwrap_or(url.len());
                    let default_value = &url[default_start + 2..default_end];

                    return std::env::var("DATABASE_URL")
                        .unwrap_or_else(|_| default_value.to_string());
                } else {
                    // ${DATABASE_URL} without default - environment var REQUIRED
                    return std::env::var("DATABASE_URL")
                        .expect("DATABASE_URL environment variable must be set");
                }
            } else if !url.is_empty() {
                // Use the URL as-is (not a variable reference)
                return url.clone();
            }
        }

        // Fallback to environment variable
        std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be provided in config or environment")
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
    /// Optional module namespace (from orchestration)
    pub default_module_namespace: Option<String>,
    /// Optional identity strategy class (from orchestration)
    pub identity_strategy_class: Option<String>,
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

/// System-wide configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub default_dependent_system: String,
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
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            default_backoff_seconds: vec![1, 2, 4, 8, 16, 32],
            max_backoff_seconds: 60,
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            jitter_max_percentage: 0.1, // TAS-57: Aligned with TOML config (10% jitter)
            reenqueue_delays: ReenqueueDelays::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReenqueueDelays {
    pub initializing: u64,
    pub enqueuing_steps: u64,
    pub steps_in_process: u64,
    pub evaluating_results: u64,
    pub waiting_for_dependencies: u64,
    pub waiting_for_retry: u64,
    pub blocked_by_failures: u64,
}

impl Default for ReenqueueDelays {
    fn default() -> Self {
        Self {
            initializing: 5,
            enqueuing_steps: 0,
            steps_in_process: 10,
            evaluating_results: 5,
            waiting_for_dependencies: 45,
            waiting_for_retry: 30,
            blocked_by_failures: 60,
        }
    }
}

/// Task execution configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    pub max_concurrent_tasks: u32,
    pub max_concurrent_steps: u32,
    pub default_timeout_seconds: u64,
    pub step_execution_timeout_seconds: u64,
    pub max_discovery_attempts: u32,
    pub step_batch_size: u32,
    // New constants unification fields
    pub max_retries: u32,                // Replaces Ruby FALLBACK_MAX_RETRIES
    pub max_workflow_steps: u32,         // Replaces Rust constants::system::MAX_WORKFLOW_STEPS
    pub connection_timeout_seconds: u64, // Replaces Ruby hardcoded API timeouts
    #[serde(default)]
    pub environment: String,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 100,
            max_concurrent_steps: 1000,
            default_timeout_seconds: 3600,
            step_execution_timeout_seconds: 300,
            max_discovery_attempts: 3,
            step_batch_size: 10,
            max_retries: 3,
            max_workflow_steps: 100,
            connection_timeout_seconds: 10,
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
        }
    }
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

    #[inline]
    pub fn environment(&self) -> &str {
        self.environment.as_str()
    }
}

impl Default for TaskerConfig {
    /// Create a safe fallback configuration with minimal defaults
    /// Used when configuration loading fails completely
    fn default() -> Self {
        use std::collections::HashMap;

        Self {
            database: DatabaseConfig {
                url: Some(
                    "postgresql://tasker:tasker@localhost:5432/tasker_development".to_string(),
                ),
                pool: DatabasePoolConfig::default(),
                variables: DatabaseVariables {
                    statement_timeout: 5000,
                },
                database: None,
                skip_migration_check: false,
            },
            telemetry: TelemetryConfig {
                enabled: false,
                service_name: "tasker-core".to_string(),
                sample_rate: 1.0,
            },
            task_templates: TaskTemplatesConfig {
                search_paths: vec!["config/task_templates/*.{yml,yaml}".to_string()],
            },
            system: SystemConfig {
                default_dependent_system: "default".to_string(),
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
                    initializing: 5,
                    enqueuing_steps: 10,
                    steps_in_process: 30,
                    evaluating_results: 15,
                    waiting_for_dependencies: 45,
                    waiting_for_retry: 60,
                    blocked_by_failures: 120,
                },
            },
            execution: ExecutionConfig {
                max_concurrent_tasks: 100,
                max_concurrent_steps: 1000,
                default_timeout_seconds: 3600,
                step_execution_timeout_seconds: 300,
                max_discovery_attempts: 3,
                step_batch_size: 10,
                max_retries: 3,
                max_workflow_steps: 1000,
                connection_timeout_seconds: 10,
                environment: std::env::var("TASKER_ENV")
                    .unwrap_or_else(|_| "development".to_string()),
            },
            queues: QueuesConfig::default(),
            orchestration: OrchestrationConfig {
                mode: "distributed".to_string(),
                enable_performance_logging: false,
                // Event systems configuration now comes from unified TaskerConfig.event_systems
                // Queue configuration now comes from centralized QueuesConfig
                // Heartbeat configuration moved to task_claim_step_enqueuer for TAS-41
                web: WebConfig::default(),
            },
            circuit_breakers: CircuitBreakerConfig {
                enabled: true,
                global_settings: CircuitBreakerGlobalSettings {
                    max_circuit_breakers: 50,
                    metrics_collection_interval_seconds: 30,
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
            task_readiness: TaskReadinessConfig::default(), // TAS-43 Task Readiness System
            // REMOVED: task_claimer for TAS-41 state machine approach
            event_systems: EventSystemsConfig::default(), // Unified Event Systems Configuration
            mpsc_channels: MpscChannelsConfig::default(), // TAS-51 MPSC Channels Configuration
            decision_points: DecisionPointsConfig::default(), // TAS-53 Decision Points Configuration
            worker: None, // Optional, only populated when TOML contains worker configuration
        }
    }
}

impl TaskerConfig {
    /// Validate configuration for consistency and required fields
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        // Database configuration validation
        // Note: host, username, password, adapter are now optional fallback fields
        // with defaults, so we don't validate them as they're only used when
        // DATABASE_URL env var is not set (which is rare in practice)

        if self.database.pool.max_connections == 0 {
            return Err(ConfigurationError::invalid_value(
                "database.pool.max_connections",
                "0",
                "pool max_connections must be greater than 0",
            ));
        }

        // Execution configuration validation
        if self.execution.environment.is_empty() {
            return Err(ConfigurationError::missing_required_field(
                "execution.environment",
                "execution configuration",
            ));
        }

        // Queue configuration validation (via centralized queues configuration)
        if self.queues.default_batch_size == 0 {
            return Err(ConfigurationError::invalid_value(
                "queues.default_batch_size",
                "0",
                "batch size must be greater than 0",
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
        Duration::from_secs(self.queues.pgmq.shutdown_timeout_seconds)
    }

    /// Get queue maximum batch size
    pub fn pgmq_max_batch_size(&self) -> u32 {
        self.queues.max_batch_size
    }

    pub fn environment(&self) -> &str {
        &self.execution.environment
    }
}
