//! Tasker V2 Configuration Proposal (TAS-61)
//!
//! This is a proposal for the next-generation configuration architecture based on
//! TAS-61 design documents. The structure follows a context-based approach:
//!
//! - **Common**: Configuration shared across all contexts (database, queues, circuit breakers)
//! - **Orchestration**: Orchestration-specific configuration (optional)
//! - **Worker**: Worker-specific configuration (optional)
//!
//! ## Design Principles (from TAS-61)
//!
//! 1. **Single File Loading**: Runtime loads one pre-merged TOML file per context
//! 2. **Optional Contexts**: TaskerConfig has required `common` and optional `orchestration`/`worker`
//! 3. **Fail-Fast Configuration**: No fallback paths, explicit TASKER_CONFIG_PATH required
//! 4. **Domain Logic Validation**: Validate domain constraints, not operational tuning
//! 5. **Context Separation**: Clear boundaries between common, orchestration, and worker configs
//!
//! ## TOML Structure
//!
//! ```toml
//! # Generated deployment file structure
//! [common]
//! [common.system]
//! [common.database]
//! [common.queues]
//! [common.circuit_breakers]
//! [common.mpsc_channels]
//!
//! [orchestration]  # Optional - present for orchestration/complete contexts
//! [orchestration.event_systems]
//! [orchestration.decision_points]
//! [orchestration.mpsc_channels]
//! [orchestration.system]
//!
//! [worker]  # Optional - present for worker/complete contexts
//! [worker.event_systems]
//! [worker.step_processing]
//! [worker.health_monitoring]
//! [worker.mpsc_channels]
//! ```
//!
//! ## Validation Strategy
//!
//! Uses `validator` crate for declarative validation:
//! - `#[validate(range(min = 1, max = 1000))]` for numeric bounds
//! - `#[validate(length(min = 1))]` for collections
//! - `#[validate(nested)]` for struct composition
//! - Enum deserialization handles invalid variants (no custom validator needed)
//! - TODO(TAS-61): Custom validators for domain-specific logic (e.g., PostgreSQL URLs)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

// ============================================================================
// TOP-LEVEL CONFIGURATION
// ============================================================================

/// Unified Tasker configuration with optional contexts
///
/// This is the root configuration struct that contains:
/// - `common`: Required configuration shared across all contexts
/// - `orchestration`: Optional orchestration-specific configuration
/// - `worker`: Optional worker-specific configuration
///
/// The presence of optional fields depends on the context:
/// - **Orchestration**: `common` + `orchestration` (Some), `worker` (None)
/// - **Worker**: `common` + `worker` (Some), `orchestration` (None)
/// - **Complete**: `common` + `orchestration` (Some) + `worker` (Some)
///
/// ## Loading
///
/// ```rust,ignore
/// // Orchestration
/// let config = ConfigLoader::load_for_orchestration(&config_path)?;
/// assert!(config.orchestration.is_some());
///
/// // Worker
/// let config = ConfigLoader::load_for_worker(&config_path)?;
/// assert!(config.worker.is_some());
///
/// // Complete (tests)
/// let config = ConfigLoader::load_complete(&config_path)?;
/// assert!(config.is_complete());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskerConfig {
    /// Shared configuration (always present)
    #[validate(nested)]
    pub common: CommonConfig,

    /// Orchestration-specific configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub orchestration: Option<OrchestrationConfig>,

    /// Worker-specific configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub worker: Option<WorkerConfig>,
}

impl TaskerConfig {
    /// Check if orchestration context is present
    pub fn has_orchestration(&self) -> bool {
        self.orchestration.is_some()
    }

    /// Check if worker context is present
    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }

    /// Check if this is a complete configuration (both contexts)
    pub fn is_complete(&self) -> bool {
        self.orchestration.is_some() && self.worker.is_some()
    }
}

// ============================================================================
// COMMON CONFIGURATION (Shared across all contexts)
// ============================================================================

/// Configuration shared across all contexts
///
/// Contains only fields that are genuinely shared across orchestration, worker,
/// and all other system contexts. Analysis shows SystemContext needs these components.
///
/// ## Components
/// - **system**: Version, environment, recursion limits
/// - **database**: Connection pool configuration
/// - **queues**: Message queue (PGMQ) configuration
/// - **circuit_breakers**: Resilience configuration
/// - **mpsc_channels**: Shared channel buffer sizes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CommonConfig {
    /// System-level configuration
    #[validate(nested)]
    pub system: SystemConfig,

    /// Database connection and pool configuration
    #[validate(nested)]
    pub database: DatabaseConfig,

    /// Message queue configuration
    #[validate(nested)]
    pub queues: QueuesConfig,

    /// Circuit breaker resilience configuration
    #[validate(nested)]
    pub circuit_breakers: CircuitBreakerConfig,

    /// Shared MPSC channel configuration
    #[validate(nested)]
    pub mpsc_channels: SharedMpscChannelsConfig,

    /// Task execution configuration (shared)
    #[validate(nested)]
    pub execution: ExecutionConfig,

    /// Backoff and retry configuration (shared)
    #[validate(nested)]
    pub backoff: BackoffConfig,

    /// Task template configuration
    #[validate(nested)]
    pub task_templates: TaskTemplatesConfig,

    /// Telemetry configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub telemetry: Option<TelemetryConfig>,
}

impl CommonConfig {
    /// Get database URL with environment variable substitution
    ///
    /// Expands ${DATABASE_URL} template if present in configuration.
    pub fn database_url(&self) -> String {
        if self.database.url.contains("${DATABASE_URL}") {
            std::env::var("DATABASE_URL").unwrap_or_else(|_| self.database.url.clone())
        } else {
            self.database.url.clone()
        }
    }
}

/// System-level configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct SystemConfig {
    /// System version
    #[validate(length(min = 1))]
    pub version: String,

    /// Default dependent system identifier
    #[validate(length(min = 1))]
    pub default_dependent_system: String,

    /// Maximum recursion depth for workflow processing
    #[validate(range(min = 1, max = 1000))]
    pub max_recursion_depth: u32,
}

/// Database connection and pool configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseConfig {
    /// Database connection URL
    ///
    /// Accepts PostgreSQL URLs or ${DATABASE_URL} template substitution.
    /// TODO(TAS-61): Add custom validator for PostgreSQL URL format validation
    pub url: String,

    /// Database name
    #[validate(length(min = 1))]
    pub database: String,

    /// Skip migration check on startup
    pub skip_migration_check: bool,

    /// Connection pool configuration
    #[validate(nested)]
    pub pool: PoolConfig,

    /// Database session variables
    #[validate(nested)]
    pub variables: DatabaseVariablesConfig,
}

/// Connection pool configuration
///
/// Note: No cross-field validation (e.g., max >= min).
/// Operational tuning is deployment concern, not application validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PoolConfig {
    /// Maximum number of connections in the pool
    #[validate(range(min = 1, max = 1000))]
    pub max_connections: u32,

    /// Minimum number of connections to maintain
    #[validate(range(min = 0, max = 100))]
    pub min_connections: u32,

    /// Connection acquisition timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub acquire_timeout_seconds: u32,

    /// Idle connection timeout in seconds
    #[validate(range(min = 1, max = 3600))]
    pub idle_timeout_seconds: u32,

    /// Maximum connection lifetime in seconds
    #[validate(range(min = 60, max = 86400))]
    pub max_lifetime_seconds: u32,
}

/// Database session variables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseVariablesConfig {
    /// Statement timeout in milliseconds
    #[validate(range(min = 100, max = 600000))]
    pub statement_timeout: u32,
}

/// Message queue configuration (PGMQ)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct QueuesConfig {
    /// Queue backend type ("pgmq" or "rabbitmq")
    #[validate(length(min = 1))]
    pub backend: String,

    /// Orchestration queue namespace
    #[validate(length(min = 1))]
    pub orchestration_namespace: String,

    /// Worker queue namespace
    #[validate(length(min = 1))]
    pub worker_namespace: String,

    /// Default message visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub default_visibility_timeout_seconds: u32,

    /// Default batch size for message fetching
    #[validate(range(min = 1, max = 1000))]
    pub default_batch_size: u32,

    /// Maximum batch size
    #[validate(range(min = 1, max = 10000))]
    pub max_batch_size: u32,

    /// Queue naming pattern (e.g., "{namespace}_{name}_queue")
    #[validate(length(min = 1))]
    pub naming_pattern: String,

    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval: u32,

    /// Named orchestration queues
    #[validate(nested)]
    pub orchestration_queues: OrchestrationQueuesConfig,

    /// PGMQ-specific configuration
    #[validate(nested)]
    pub pgmq: PgmqConfig,

    /// RabbitMQ-specific configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub rabbitmq: Option<RabbitmqConfig>,
}

/// Named orchestration queue configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationQueuesConfig {
    /// Task request queue name
    #[validate(length(min = 1))]
    pub task_requests: String,

    /// Task finalization queue name
    #[validate(length(min = 1))]
    pub task_finalizations: String,

    /// Step results queue name
    #[validate(length(min = 1))]
    pub step_results: String,
}

/// PGMQ-specific configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PgmqConfig {
    /// Polling interval in milliseconds
    #[validate(range(min = 10, max = 10000))]
    pub poll_interval_ms: u32,

    /// Shutdown timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub shutdown_timeout_seconds: u32,

    /// Maximum retry attempts
    #[validate(range(max = 100))]
    pub max_retries: u32,
}

/// RabbitMQ-specific configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct RabbitmqConfig {
    /// Connection timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,
}

/// Circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerConfig {
    /// Enable circuit breakers
    pub enabled: bool,

    /// Global circuit breaker settings
    #[validate(nested)]
    pub global_settings: GlobalCircuitBreakerSettings,

    /// Default circuit breaker configuration
    #[validate(nested)]
    pub default_config: CircuitBreakerDefaultConfig,

    /// Component-specific circuit breaker configurations
    #[validate(nested)]
    pub component_configs: ComponentCircuitBreakerConfigs,
}

/// Global circuit breaker settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GlobalCircuitBreakerSettings {
    /// Maximum number of circuit breakers
    #[validate(range(min = 1, max = 1000))]
    pub max_circuit_breakers: u32,

    /// Metrics collection interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub metrics_collection_interval_seconds: u32,

    /// Minimum state transition interval (seconds)
    #[validate(range(min = 0.1, max = 60.0))]
    pub min_state_transition_interval_seconds: f64,
}

/// Default circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerDefaultConfig {
    /// Failure threshold before opening
    #[validate(range(min = 1, max = 100))]
    pub failure_threshold: u32,

    /// Timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: u32,

    /// Success threshold for closing
    #[validate(range(min = 1, max = 100))]
    pub success_threshold: u32,
}

/// Component-specific circuit breaker configurations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ComponentCircuitBreakerConfigs {
    /// Task readiness circuit breaker
    #[validate(nested)]
    pub task_readiness: CircuitBreakerComponentConfig,

    /// PGMQ circuit breaker
    #[validate(nested)]
    pub pgmq: CircuitBreakerComponentConfig,
}

/// Component circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerComponentConfig {
    /// Failure threshold
    #[validate(range(min = 1, max = 100))]
    pub failure_threshold: u32,

    /// Timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: u32,

    /// Success threshold
    #[validate(range(min = 1, max = 100))]
    pub success_threshold: u32,
}

/// Shared MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct SharedMpscChannelsConfig {
    /// Event publisher channels
    #[validate(nested)]
    pub event_publisher: EventPublisherChannels,

    /// FFI channels (Ruby/Python)
    #[validate(nested)]
    pub ffi: FfiChannels,

    /// Overflow policy
    #[validate(nested)]
    pub overflow_policy: OverflowPolicyConfig,
}

/// Event publisher channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventPublisherChannels {
    /// Event queue buffer size
    #[validate(range(min = 100, max = 1000000))]
    pub event_queue_buffer_size: u32,
}

/// FFI channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct FfiChannels {
    /// Ruby FFI buffer size
    #[validate(range(min = 100, max = 100000))]
    pub ruby_event_buffer_size: u32,
}

/// Overflow policy configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OverflowPolicyConfig {
    /// Warning threshold (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub log_warning_threshold: f64,

    /// Drop policy ("block" or "drop")
    #[validate(length(min = 1))]
    pub drop_policy: String,

    /// Metrics configuration
    #[validate(nested)]
    pub metrics: OverflowMetricsConfig,
}

/// Overflow metrics configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OverflowMetricsConfig {
    /// Enable metrics
    pub enabled: bool,

    /// Saturation check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub saturation_check_interval_seconds: u32,
}

/// Task execution configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionConfig {
    /// Maximum concurrent tasks
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_tasks: u32,

    /// Maximum concurrent steps
    #[validate(range(min = 1, max = 1000000))]
    pub max_concurrent_steps: u32,

    /// Default timeout (seconds)
    #[validate(range(min = 1, max = 86400))]
    pub default_timeout_seconds: u32,

    /// Step execution timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub step_execution_timeout_seconds: u32,

    /// Maximum discovery attempts
    #[validate(range(min = 1, max = 10))]
    pub max_discovery_attempts: u32,

    /// Step batch size
    #[validate(range(min = 1, max = 1000))]
    pub step_batch_size: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    pub max_retries: u32,

    /// Maximum workflow steps
    #[validate(range(min = 1, max = 10000))]
    pub max_workflow_steps: u32,

    /// Connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,

    /// Environment name
    #[validate(length(min = 1))]
    pub environment: String,
}

/// Backoff and retry configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct BackoffConfig {
    /// Default backoff sequence (seconds)
    #[validate(length(min = 1, max = 20))]
    pub default_backoff_seconds: Vec<u32>,

    /// Maximum backoff delay (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub max_backoff_seconds: u32,

    /// Backoff multiplier
    #[validate(range(min = 1.0, max = 10.0))]
    pub backoff_multiplier: f64,

    /// Enable jitter
    pub jitter_enabled: bool,

    /// Maximum jitter percentage (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_max_percentage: f64,

    /// State-specific reenqueue delays
    #[validate(nested)]
    pub reenqueue_delays: ReenqueueDelaysConfig,
}

/// Reenqueue delays for task states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ReenqueueDelaysConfig {
    /// Initializing state delay (seconds)
    #[validate(range(max = 300))]
    pub initializing: u32,

    /// Enqueueing steps delay (seconds)
    #[validate(range(max = 300))]
    pub enqueuing_steps: u32,

    /// Steps in process delay (seconds)
    #[validate(range(max = 300))]
    pub steps_in_process: u32,

    /// Evaluating results delay (seconds)
    #[validate(range(max = 300))]
    pub evaluating_results: u32,

    /// Waiting for dependencies delay (seconds)
    #[validate(range(max = 3600))]
    pub waiting_for_dependencies: u32,

    /// Waiting for retry delay (seconds)
    #[validate(range(max = 3600))]
    pub waiting_for_retry: u32,

    /// Blocked by failures delay (seconds)
    #[validate(range(max = 3600))]
    pub blocked_by_failures: u32,
}

/// Task template configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskTemplatesConfig {
    /// Search paths for task templates
    #[validate(length(min = 1))]
    pub search_paths: Vec<String>,
}

/// Telemetry configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryConfig {
    /// Enable telemetry
    pub enabled: bool,

    /// Service name
    #[validate(length(min = 1))]
    pub service_name: String,

    /// Sample rate (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub sample_rate: f64,
}

// ============================================================================
// ORCHESTRATION CONFIGURATION (Orchestration-specific)
// ============================================================================

/// Orchestration-specific configuration
///
/// Contains all configuration needed for the orchestration service.
/// Present when context is "orchestration" or "complete".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationConfig {
    /// Orchestration mode
    #[validate(length(min = 1))]
    pub mode: String,

    /// Enable performance logging
    pub enable_performance_logging: bool,

    /// Event systems configuration
    #[validate(nested)]
    pub event_systems: OrchestrationEventSystemsConfig,

    /// Decision points configuration (TAS-53)
    #[validate(nested)]
    pub decision_points: DecisionPointsConfig,

    /// MPSC channel configuration
    #[validate(nested)]
    pub mpsc_channels: OrchestrationMpscChannelsConfig,

    /// Web API configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub web: Option<OrchestrationWebConfig>,
}

/// Deployment mode for event systems
///
/// Enum deserialization will fail if TOML contains invalid value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum DeploymentMode {
    /// Pure event-driven using PostgreSQL LISTEN/NOTIFY
    EventDrivenOnly,
    /// Traditional polling-based coordination
    PollingOnly,
    /// Event-driven with polling fallback (recommended)
    #[default]
    Hybrid,
}

/// Orchestration event systems configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationEventSystemsConfig {
    /// Orchestration event system
    #[validate(nested)]
    pub orchestration: EventSystemConfig,

    /// Task readiness event system
    #[validate(nested)]
    pub task_readiness: EventSystemConfig,
}

/// Generic event system configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemConfig {
    /// System identifier
    #[validate(length(min = 1))]
    pub system_id: String,

    /// Deployment mode
    pub deployment_mode: DeploymentMode,

    /// Timing configuration
    #[validate(nested)]
    pub timing: EventSystemTimingConfig,

    /// Processing configuration
    #[validate(nested)]
    pub processing: EventSystemProcessingConfig,

    /// Health configuration
    #[validate(nested)]
    pub health: EventSystemHealthConfig,
}

/// Event system timing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemTimingConfig {
    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval_seconds: u32,

    /// Fallback polling interval (seconds)
    #[validate(range(min = 1, max = 300))]
    pub fallback_polling_interval_seconds: u32,

    /// Visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    /// Processing timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub processing_timeout_seconds: u32,

    /// Claim timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub claim_timeout_seconds: u32,
}

/// Event system processing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemProcessingConfig {
    /// Maximum concurrent operations
    #[validate(range(min = 1, max = 10000))]
    pub max_concurrent_operations: u32,

    /// Batch size
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    pub max_retries: u32,

    /// Backoff configuration
    #[validate(nested)]
    pub backoff: EventSystemBackoffConfig,
}

/// Event system backoff configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemBackoffConfig {
    /// Initial delay (milliseconds)
    #[validate(range(min = 1, max = 60000))]
    pub initial_delay_ms: u32,

    /// Maximum delay (milliseconds)
    #[validate(range(min = 1, max = 600000))]
    pub max_delay_ms: u32,

    /// Multiplier
    #[validate(range(min = 1.0, max = 10.0))]
    pub multiplier: f64,

    /// Jitter percent (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_percent: f64,
}

/// Event system health configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemHealthConfig {
    /// Enable health checks
    pub enabled: bool,

    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,

    /// Maximum consecutive errors
    #[validate(range(min = 1, max = 1000))]
    pub max_consecutive_errors: u32,

    /// Error rate threshold per minute
    #[validate(range(min = 1, max = 10000))]
    pub error_rate_threshold_per_minute: u32,
}

/// Decision points configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DecisionPointsConfig {
    /// Enable decision points
    pub enabled: bool,

    /// Maximum steps per decision
    #[validate(range(min = 1, max = 1000))]
    pub max_steps_per_decision: u32,

    /// Maximum decision depth
    #[validate(range(min = 1, max = 100))]
    pub max_decision_depth: u32,

    /// Warning threshold for steps
    #[validate(range(min = 1, max = 1000))]
    pub warn_threshold_steps: u32,

    /// Warning threshold for depth
    #[validate(range(min = 1, max = 100))]
    pub warn_threshold_depth: u32,

    /// Enable detailed logging
    pub enable_detailed_logging: bool,

    /// Enable metrics
    pub enable_metrics: bool,
}

/// Orchestration MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationMpscChannelsConfig {
    /// Command processor channels
    #[validate(nested)]
    pub command_processor: CommandProcessorChannels,

    /// Event system channels
    #[validate(nested)]
    pub event_systems: EventSystemChannels,

    /// Event listener channels
    #[validate(nested)]
    pub event_listeners: EventListenerChannels,
}

/// Command processor channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CommandProcessorChannels {
    /// Command buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub command_buffer_size: u32,
}

/// Event system channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemChannels {
    /// Event channel buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub event_channel_buffer_size: u32,
}

/// Event listener channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventListenerChannels {
    /// PGMQ event buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub pgmq_event_buffer_size: u32,
}

/// Orchestration web API configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationWebConfig {
    /// Enable web API
    pub enabled: bool,

    /// Bind address
    #[validate(length(min = 1))]
    pub bind_address: String,

    /// Request timeout (milliseconds)
    #[validate(range(min = 100, max = 300000))]
    pub request_timeout_ms: u32,

    /// TLS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub tls: Option<TlsConfig>,

    /// Database pool configuration
    #[validate(nested)]
    pub database_pools: WebDatabasePoolsConfig,

    /// CORS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub cors: Option<CorsConfig>,

    /// Authentication configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub auth: Option<AuthConfig>,

    /// Rate limiting configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub rate_limiting: Option<RateLimitingConfig>,

    /// Resilience configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub resilience: Option<ResilienceConfig>,
}

/// TLS configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,

    /// Certificate path
    #[validate(length(min = 1))]
    pub cert_path: String,

    /// Key path
    #[validate(length(min = 1))]
    pub key_path: String,
}

/// Web API database pool configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WebDatabasePoolsConfig {
    /// Web API pool size
    #[validate(range(min = 1, max = 1000))]
    pub web_api_pool_size: u32,

    /// Web API max connections
    #[validate(range(min = 1, max = 1000))]
    pub web_api_max_connections: u32,

    /// Web API connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    pub web_api_connection_timeout_seconds: u32,

    /// Web API idle timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub web_api_idle_timeout_seconds: u32,

    /// Max total connections hint
    #[validate(range(min = 1, max = 10000))]
    pub max_total_connections_hint: u32,
}

/// CORS configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CorsConfig {
    /// Enable CORS
    pub enabled: bool,

    /// Allowed origins
    #[validate(length(min = 1))]
    pub allowed_origins: Vec<String>,

    /// Allowed methods
    #[validate(length(min = 1))]
    pub allowed_methods: Vec<String>,

    /// Allowed headers
    #[validate(length(min = 1))]
    pub allowed_headers: Vec<String>,

    /// Max age (seconds)
    #[validate(range(min = 1, max = 86400))]
    pub max_age_seconds: u32,
}

/// Authentication configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,

    /// JWT issuer
    #[validate(length(min = 1))]
    pub jwt_issuer: String,

    /// JWT audience
    #[validate(length(min = 1))]
    pub jwt_audience: String,

    /// JWT token expiry (hours)
    #[validate(range(min = 1, max = 168))]
    pub jwt_token_expiry_hours: u32,

    /// JWT private key
    pub jwt_private_key: String,

    /// JWT public key
    pub jwt_public_key: String,

    /// API key
    pub api_key: String,

    /// API key header
    #[validate(length(min = 1))]
    pub api_key_header: String,

    /// Route-specific authentication configuration
    ///
    /// Uses TOML array of tables for ergonomic route declarations.
    /// At load time, converted to HashMap for efficient runtime lookups.
    #[serde(default)]
    pub protected_routes: Vec<ProtectedRouteConfig>,
}

impl AuthConfig {
    /// Convert protected routes list to HashMap for efficient runtime lookups
    ///
    /// Creates route keys in format "METHOD /path" for pattern matching.
    pub fn routes_map(&self) -> HashMap<String, RouteAuthConfig> {
        self.protected_routes
            .iter()
            .map(|route| {
                let key = format!("{} {}", route.method, route.path);
                let config = RouteAuthConfig {
                    auth_type: route.auth_type.clone(),
                    required: route.required,
                };
                (key, config)
            })
            .collect()
    }
}

/// Protected route configuration (TOML-friendly format)
///
/// This struct is designed for ergonomic TOML declaration using array of tables:
///
/// ```toml
/// [[orchestration.web.auth.protected_routes]]
/// method = "GET"
/// path = "/v1/tasks"
/// auth_type = "bearer"
/// required = false
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedRouteConfig {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    #[validate(length(min = 1))]
    pub method: String,

    /// Route path pattern (supports parameters like /v1/tasks/{task_uuid})
    #[validate(length(min = 1))]
    pub path: String,

    /// Type of authentication required ("bearer", "api_key")
    #[validate(length(min = 1))]
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}

/// Runtime route authentication config (for HashMap lookups)
///
/// This is the internal representation used by the legacy WebAuthConfig
/// for efficient route lookups at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteAuthConfig {
    /// Type of authentication required ("bearer", "api_key")
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}

/// Rate limiting configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct RateLimitingConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Requests per minute
    #[validate(range(min = 1, max = 1000000))]
    pub requests_per_minute: u32,

    /// Burst size
    #[validate(range(min = 1, max = 10000))]
    pub burst_size: u32,

    /// Per-client limit
    pub per_client_limit: bool,
}

/// Resilience configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ResilienceConfig {
    /// Enable circuit breaker
    pub circuit_breaker_enabled: bool,

    /// Request timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    pub request_timeout_seconds: u32,

    /// Maximum concurrent requests
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_requests: u32,
}

// ============================================================================
// WORKER CONFIGURATION (Worker-specific)
// ============================================================================

/// Worker-specific configuration
///
/// Contains all configuration needed for the worker service.
/// Present when context is "worker" or "complete".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerConfig {
    /// Worker identifier
    #[validate(length(min = 1))]
    pub worker_id: String,

    /// Worker type
    #[validate(length(min = 1))]
    pub worker_type: String,

    /// Event systems configuration
    #[validate(nested)]
    pub event_systems: WorkerEventSystemsConfig,

    /// Step processing configuration
    #[validate(nested)]
    pub step_processing: StepProcessingConfig,

    /// Health monitoring configuration
    #[validate(nested)]
    pub health_monitoring: HealthMonitoringConfig,

    /// MPSC channel configuration
    #[validate(nested)]
    pub mpsc_channels: WorkerMpscChannelsConfig,

    /// Web API configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub web: Option<WorkerWebConfig>,
}

/// Worker event systems configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemsConfig {
    /// Worker event system
    #[validate(nested)]
    pub worker: WorkerEventSystemConfig,
}

/// Worker-specific event system configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemConfig {
    /// System identifier
    #[validate(length(min = 1))]
    pub system_id: String,

    /// Deployment mode
    pub deployment_mode: DeploymentMode,

    /// Timing configuration
    #[validate(nested)]
    pub timing: EventSystemTimingConfig,

    /// Processing configuration
    #[validate(nested)]
    pub processing: EventSystemProcessingConfig,

    /// Health configuration
    #[validate(nested)]
    pub health: EventSystemHealthConfig,

    /// Worker-specific metadata
    #[validate(nested)]
    pub metadata: WorkerEventSystemMetadata,
}

/// Worker event system metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemMetadata {
    /// In-process events configuration
    #[validate(nested)]
    pub in_process_events: InProcessEventsConfig,

    /// Listener configuration
    #[validate(nested)]
    pub listener: ListenerConfig,

    /// Fallback poller configuration
    #[validate(nested)]
    pub fallback_poller: FallbackPollerConfig,

    /// Resource limits
    #[validate(nested)]
    pub resource_limits: ResourceLimitsConfig,
}

/// In-process events configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct InProcessEventsConfig {
    /// Enable FFI integration
    pub ffi_integration_enabled: bool,

    /// Deduplication cache size
    #[validate(range(min = 100, max = 100000))]
    pub deduplication_cache_size: u32,
}

/// Listener configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ListenerConfig {
    /// Retry interval (seconds)
    #[validate(range(min = 1, max = 300))]
    pub retry_interval_seconds: u32,

    /// Maximum retry attempts
    #[validate(range(max = 100))]
    pub max_retry_attempts: u32,

    /// Event timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub event_timeout_seconds: u32,

    /// Enable batch processing
    pub batch_processing: bool,

    /// Connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,
}

/// Fallback poller configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct FallbackPollerConfig {
    /// Enable poller
    pub enabled: bool,

    /// Polling interval (milliseconds)
    #[validate(range(min = 10, max = 60000))]
    pub polling_interval_ms: u32,

    /// Batch size
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    /// Age threshold (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub age_threshold_seconds: u32,

    /// Maximum age (hours)
    #[validate(range(min = 1, max = 168))]
    pub max_age_hours: u32,

    /// Visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    /// Supported namespaces
    pub supported_namespaces: Vec<String>,
}

/// Resource limits configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ResourceLimitsConfig {
    /// Maximum memory (MB)
    #[validate(range(min = 256, max = 65536))]
    pub max_memory_mb: u32,

    /// Maximum CPU percent
    #[validate(range(min = 1.0, max = 100.0))]
    pub max_cpu_percent: f64,

    /// Maximum database connections
    #[validate(range(min = 1, max = 1000))]
    pub max_database_connections: u32,

    /// Maximum queue connections
    #[validate(range(min = 1, max = 1000))]
    pub max_queue_connections: u32,
}

/// Step processing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct StepProcessingConfig {
    /// Claim timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub claim_timeout_seconds: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    pub max_retries: u32,

    /// Maximum concurrent steps
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_steps: u32,
}

/// Health monitoring configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct HealthMonitoringConfig {
    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval_seconds: u32,

    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,

    /// Error rate threshold
    #[validate(range(min = 0.0, max = 1.0))]
    pub error_rate_threshold: f64,
}

/// Worker MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerMpscChannelsConfig {
    /// Command processor channels
    #[validate(nested)]
    pub command_processor: WorkerCommandProcessorChannels,

    /// Event system channels
    #[validate(nested)]
    pub event_systems: WorkerEventSystemChannels,

    /// Event subscriber channels
    #[validate(nested)]
    pub event_subscribers: WorkerEventSubscriberChannels,

    /// In-process event channels
    #[validate(nested)]
    pub in_process_events: WorkerInProcessEventChannels,

    /// Event listener channels
    #[validate(nested)]
    pub event_listeners: WorkerEventListenerChannels,
}

/// Worker command processor channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerCommandProcessorChannels {
    /// Command buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub command_buffer_size: u32,
}

/// Worker event system channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemChannels {
    /// Event channel buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub event_channel_buffer_size: u32,
}

/// Worker event subscriber channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSubscriberChannels {
    /// Completion buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub completion_buffer_size: u32,

    /// Result buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub result_buffer_size: u32,
}

/// Worker in-process event channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerInProcessEventChannels {
    /// Broadcast buffer size
    #[validate(range(min = 100, max = 1000000))]
    pub broadcast_buffer_size: u32,
}

/// Worker event listener channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventListenerChannels {
    /// PGMQ event buffer size
    #[validate(range(min = 10, max = 1000000))]
    pub pgmq_event_buffer_size: u32,
}

/// Worker web API configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerWebConfig {
    /// Enable web API
    pub enabled: bool,

    /// Bind address
    #[validate(length(min = 1))]
    pub bind_address: String,

    /// Request timeout (milliseconds)
    #[validate(range(min = 100, max = 300000))]
    pub request_timeout_ms: u32,

    /// TLS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub tls: Option<TlsConfig>,

    /// Database pool configuration
    #[validate(nested)]
    pub database_pools: WebDatabasePoolsConfig,

    /// CORS configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub cors: Option<CorsConfig>,

    /// Authentication configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub auth: Option<AuthConfig>,

    /// Rate limiting configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub rate_limiting: Option<RateLimitingConfig>,

    /// Resilience configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub resilience: Option<ResilienceConfig>,
}

// ============================================================================
// Default Implementations for Bridge Compatibility
// ============================================================================

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "tasker-core".to_string(),
            sample_rate: 1.0,
        }
    }
}

impl Default for DecisionPointsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_steps_per_decision: 50,
            max_decision_depth: 10,
            warn_threshold_steps: 20,
            warn_threshold_depth: 5,
            enable_detailed_logging: false,
            enable_metrics: true,
        }
    }
}

impl Default for EventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "default-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: EventSystemTimingConfig {
                health_check_interval_seconds: 30,
                fallback_polling_interval_seconds: 5,
                visibility_timeout_seconds: 30,
                processing_timeout_seconds: 30,
                claim_timeout_seconds: 300,
            },
            processing: EventSystemProcessingConfig {
                max_concurrent_operations: 10,
                batch_size: 10,
                max_retries: 3,
                backoff: EventSystemBackoffConfig {
                    initial_delay_ms: 100,
                    max_delay_ms: 5000,
                    multiplier: 2.0,
                    jitter_percent: 0.1,
                },
            },
            health: EventSystemHealthConfig {
                enabled: true,
                performance_monitoring_enabled: true,
                max_consecutive_errors: 10,
                error_rate_threshold_per_minute: 5,
            },
        }
    }
}

impl Default for WorkerEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "default-worker-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: EventSystemTimingConfig {
                health_check_interval_seconds: 30,
                fallback_polling_interval_seconds: 5,
                visibility_timeout_seconds: 30,
                processing_timeout_seconds: 30,
                claim_timeout_seconds: 300,
            },
            processing: EventSystemProcessingConfig {
                max_concurrent_operations: 10,
                batch_size: 10,
                max_retries: 3,
                backoff: EventSystemBackoffConfig {
                    initial_delay_ms: 100,
                    max_delay_ms: 5000,
                    multiplier: 2.0,
                    jitter_percent: 0.1,
                },
            },
            health: EventSystemHealthConfig {
                enabled: true,
                performance_monitoring_enabled: true,
                max_consecutive_errors: 10,
                error_rate_threshold_per_minute: 5,
            },
            metadata: WorkerEventSystemMetadata {
                in_process_events: InProcessEventsConfig {
                    ffi_integration_enabled: true,
                    deduplication_cache_size: 1000,
                },
                listener: ListenerConfig {
                    retry_interval_seconds: 5,
                    max_retry_attempts: 3,
                    event_timeout_seconds: 30,
                    batch_processing: true,
                    connection_timeout_seconds: 10,
                },
                fallback_poller: FallbackPollerConfig {
                    enabled: true,
                    polling_interval_ms: 500,
                    batch_size: 10,
                    age_threshold_seconds: 2,
                    max_age_hours: 12,
                    visibility_timeout_seconds: 30,
                    supported_namespaces: vec![],
                },
                resource_limits: ResourceLimitsConfig {
                    max_memory_mb: 2048,
                    max_cpu_percent: 80.0,
                    max_database_connections: 50,
                    max_queue_connections: 20,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tasker_config_context_helpers() {
        let common = CommonConfig {
            system: SystemConfig {
                version: "0.1.0".to_string(),
                default_dependent_system: "default".to_string(),
                max_recursion_depth: 50,
            },
            database: DatabaseConfig {
                url: "postgresql://localhost/test".to_string(),
                database: "test".to_string(),
                skip_migration_check: false,
                pool: PoolConfig {
                    max_connections: 10,
                    min_connections: 2,
                    acquire_timeout_seconds: 30,
                    idle_timeout_seconds: 600,
                    max_lifetime_seconds: 1800,
                },
                variables: DatabaseVariablesConfig {
                    statement_timeout: 5000,
                },
            },
            // ... other fields would be populated
            queues: create_test_queues_config(),
            circuit_breakers: create_test_circuit_breaker_config(),
            mpsc_channels: create_test_shared_mpsc_channels(),
            execution: create_test_execution_config(),
            backoff: create_test_backoff_config(),
            task_templates: TaskTemplatesConfig {
                search_paths: vec!["config/templates/*.yml".to_string()],
            },
            telemetry: None,
        };

        // Orchestration only
        let orch_config = TaskerConfig {
            common: common.clone(),
            orchestration: Some(create_test_orchestration_config()),
            worker: None,
        };

        assert!(orch_config.has_orchestration());
        assert!(!orch_config.has_worker());
        assert!(!orch_config.is_complete());

        // Worker only
        let worker_config = TaskerConfig {
            common: common.clone(),
            orchestration: None,
            worker: Some(create_test_worker_config()),
        };

        assert!(!worker_config.has_orchestration());
        assert!(worker_config.has_worker());
        assert!(!worker_config.is_complete());

        // Complete
        let complete_config = TaskerConfig {
            common,
            orchestration: Some(create_test_orchestration_config()),
            worker: Some(create_test_worker_config()),
        };

        assert!(complete_config.has_orchestration());
        assert!(complete_config.has_worker());
        assert!(complete_config.is_complete());
    }

    // Helper functions for tests
    fn create_test_queues_config() -> QueuesConfig {
        QueuesConfig {
            backend: "pgmq".to_string(),
            orchestration_namespace: "orchestration".to_string(),
            worker_namespace: "worker".to_string(),
            default_visibility_timeout_seconds: 30,
            default_batch_size: 5,
            max_batch_size: 100,
            naming_pattern: "{namespace}_{name}_queue".to_string(),
            health_check_interval: 60,
            orchestration_queues: OrchestrationQueuesConfig {
                task_requests: "task_requests".to_string(),
                task_finalizations: "task_finalizations".to_string(),
                step_results: "step_results".to_string(),
            },
            pgmq: PgmqConfig {
                poll_interval_ms: 250,
                shutdown_timeout_seconds: 5,
                max_retries: 3,
            },
            rabbitmq: None,
        }
    }

    fn create_test_circuit_breaker_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            enabled: true,
            global_settings: GlobalCircuitBreakerSettings {
                max_circuit_breakers: 20,
                metrics_collection_interval_seconds: 10,
                min_state_transition_interval_seconds: 1.0,
            },
            default_config: CircuitBreakerDefaultConfig {
                failure_threshold: 3,
                timeout_seconds: 5,
                success_threshold: 1,
            },
            component_configs: ComponentCircuitBreakerConfigs {
                task_readiness: CircuitBreakerComponentConfig {
                    failure_threshold: 5,
                    timeout_seconds: 30,
                    success_threshold: 2,
                },
                pgmq: CircuitBreakerComponentConfig {
                    failure_threshold: 3,
                    timeout_seconds: 15,
                    success_threshold: 2,
                },
            },
        }
    }

    fn create_test_shared_mpsc_channels() -> SharedMpscChannelsConfig {
        SharedMpscChannelsConfig {
            event_publisher: EventPublisherChannels {
                event_queue_buffer_size: 5000,
            },
            ffi: FfiChannels {
                ruby_event_buffer_size: 1000,
            },
            overflow_policy: OverflowPolicyConfig {
                log_warning_threshold: 0.8,
                drop_policy: "block".to_string(),
                metrics: OverflowMetricsConfig {
                    enabled: true,
                    saturation_check_interval_seconds: 10,
                },
            },
        }
    }

    fn create_test_execution_config() -> ExecutionConfig {
        ExecutionConfig {
            max_concurrent_tasks: 100,
            max_concurrent_steps: 1000,
            default_timeout_seconds: 3600,
            step_execution_timeout_seconds: 300,
            max_discovery_attempts: 3,
            step_batch_size: 10,
            max_retries: 3,
            max_workflow_steps: 1000,
            connection_timeout_seconds: 10,
            environment: "test".to_string(),
        }
    }

    fn create_test_backoff_config() -> BackoffConfig {
        BackoffConfig {
            default_backoff_seconds: vec![1, 2, 4],
            max_backoff_seconds: 60,
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            jitter_max_percentage: 0.1,
            reenqueue_delays: ReenqueueDelaysConfig {
                initializing: 5,
                enqueuing_steps: 0,
                steps_in_process: 10,
                evaluating_results: 5,
                waiting_for_dependencies: 45,
                waiting_for_retry: 30,
                blocked_by_failures: 60,
            },
        }
    }

    fn create_test_orchestration_config() -> OrchestrationConfig {
        OrchestrationConfig {
            mode: "standalone".to_string(),
            enable_performance_logging: false,
            event_systems: OrchestrationEventSystemsConfig {
                orchestration: create_test_event_system_config("orchestration-events"),
                task_readiness: create_test_event_system_config("task-readiness-events"),
            },
            decision_points: DecisionPointsConfig {
                enabled: true,
                max_steps_per_decision: 50,
                max_decision_depth: 10,
                warn_threshold_steps: 20,
                warn_threshold_depth: 5,
                enable_detailed_logging: false,
                enable_metrics: true,
            },
            mpsc_channels: OrchestrationMpscChannelsConfig {
                command_processor: CommandProcessorChannels {
                    command_buffer_size: 5000,
                },
                event_systems: EventSystemChannels {
                    event_channel_buffer_size: 10000,
                },
                event_listeners: EventListenerChannels {
                    pgmq_event_buffer_size: 5000,
                },
            },
            web: None,
        }
    }

    fn create_test_worker_config() -> WorkerConfig {
        WorkerConfig {
            worker_id: "test-worker".to_string(),
            worker_type: "general".to_string(),
            event_systems: WorkerEventSystemsConfig {
                worker: WorkerEventSystemConfig {
                    system_id: "worker-events".to_string(),
                    deployment_mode: DeploymentMode::Hybrid,
                    timing: create_test_timing_config(),
                    processing: create_test_processing_config(),
                    health: create_test_health_config(),
                    metadata: WorkerEventSystemMetadata {
                        in_process_events: InProcessEventsConfig {
                            ffi_integration_enabled: true,
                            deduplication_cache_size: 1000,
                        },
                        listener: ListenerConfig {
                            retry_interval_seconds: 5,
                            max_retry_attempts: 3,
                            event_timeout_seconds: 30,
                            batch_processing: true,
                            connection_timeout_seconds: 10,
                        },
                        fallback_poller: FallbackPollerConfig {
                            enabled: true,
                            polling_interval_ms: 500,
                            batch_size: 10,
                            age_threshold_seconds: 2,
                            max_age_hours: 12,
                            visibility_timeout_seconds: 30,
                            supported_namespaces: vec![],
                        },
                        resource_limits: ResourceLimitsConfig {
                            max_memory_mb: 2048,
                            max_cpu_percent: 80.0,
                            max_database_connections: 50,
                            max_queue_connections: 20,
                        },
                    },
                },
            },
            step_processing: StepProcessingConfig {
                claim_timeout_seconds: 300,
                max_retries: 3,
                max_concurrent_steps: 100,
            },
            health_monitoring: HealthMonitoringConfig {
                health_check_interval_seconds: 30,
                performance_monitoring_enabled: true,
                error_rate_threshold: 0.05,
            },
            mpsc_channels: WorkerMpscChannelsConfig {
                command_processor: WorkerCommandProcessorChannels {
                    command_buffer_size: 2000,
                },
                event_systems: WorkerEventSystemChannels {
                    event_channel_buffer_size: 5000,
                },
                event_subscribers: WorkerEventSubscriberChannels {
                    completion_buffer_size: 1000,
                    result_buffer_size: 1000,
                },
                in_process_events: WorkerInProcessEventChannels {
                    broadcast_buffer_size: 1000,
                },
                event_listeners: WorkerEventListenerChannels {
                    pgmq_event_buffer_size: 1000,
                },
            },
            web: None,
        }
    }

    fn create_test_event_system_config(system_id: &str) -> EventSystemConfig {
        EventSystemConfig {
            system_id: system_id.to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: create_test_timing_config(),
            processing: create_test_processing_config(),
            health: create_test_health_config(),
        }
    }

    fn create_test_timing_config() -> EventSystemTimingConfig {
        EventSystemTimingConfig {
            health_check_interval_seconds: 30,
            fallback_polling_interval_seconds: 5,
            visibility_timeout_seconds: 30,
            processing_timeout_seconds: 30,
            claim_timeout_seconds: 300,
        }
    }

    fn create_test_processing_config() -> EventSystemProcessingConfig {
        EventSystemProcessingConfig {
            max_concurrent_operations: 10,
            batch_size: 10,
            max_retries: 3,
            backoff: EventSystemBackoffConfig {
                initial_delay_ms: 100,
                max_delay_ms: 5000,
                multiplier: 2.0,
                jitter_percent: 0.1,
            },
        }
    }

    fn create_test_health_config() -> EventSystemHealthConfig {
        EventSystemHealthConfig {
            enabled: true,
            performance_monitoring_enabled: true,
            max_consecutive_errors: 10,
            error_rate_threshold_per_minute: 5,
        }
    }

    #[test]
    fn test_deserialize_complete_test_toml() {
        // Test that the actual complete-test.toml file can be deserialized
        let toml_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("config/v2/complete-test.toml");

        if !toml_path.exists() {
            eprintln!("Warning: complete-test.toml not found at {:?}", toml_path);
            return;
        }

        let toml_content =
            std::fs::read_to_string(&toml_path).expect("Failed to read complete-test.toml");

        let config: Result<TaskerConfig, _> = toml::from_str(&toml_content);

        match &config {
            Ok(cfg) => {
                // Verify structure
                assert!(cfg.has_orchestration(), "Should have orchestration config");
                assert!(cfg.has_worker(), "Should have worker config");
                assert!(cfg.is_complete(), "Should be complete config");

                // Verify common config
                assert_eq!(cfg.common.system.version, "0.1.0");
                assert_eq!(cfg.common.database.database, "tasker_rust_test");
                assert_eq!(cfg.common.queues.backend, "pgmq");

                // Verify orchestration config
                let orch = cfg.orchestration.as_ref().unwrap();
                assert_eq!(orch.mode, "standalone");
                assert!(orch.decision_points.enabled);

                // Verify orchestration auth with protected routes
                if let Some(web) = &orch.web {
                    if let Some(auth) = &web.auth {
                        assert!(
                            !auth.protected_routes.is_empty(),
                            "Should have protected routes"
                        );
                        assert_eq!(
                            auth.protected_routes.len(),
                            5,
                            "Should have 5 orchestration routes"
                        );

                        // Verify specific route using Vec structure
                        let delete_task_route = auth
                            .protected_routes
                            .iter()
                            .find(|r| r.method == "DELETE" && r.path == "/v1/tasks/{task_uuid}");
                        assert!(
                            delete_task_route.is_some(),
                            "Should have DELETE /v1/tasks route"
                        );
                        if let Some(r) = delete_task_route {
                            assert_eq!(r.auth_type, "bearer");
                            assert!(r.required);
                        }

                        // Verify routes_map() conversion works
                        let routes_map = auth.routes_map();
                        assert_eq!(routes_map.len(), 5, "Routes map should have 5 entries");
                        let key = "DELETE /v1/tasks/{task_uuid}";
                        assert!(
                            routes_map.contains_key(key),
                            "Routes map should contain DELETE route"
                        );
                    }
                }

                // Verify worker config
                let worker = cfg.worker.as_ref().unwrap();
                assert_eq!(worker.worker_id, "test-worker-001");
                assert_eq!(worker.worker_type, "general");

                // Verify worker auth with protected routes
                if let Some(web) = &worker.web {
                    if let Some(auth) = &web.auth {
                        assert!(
                            !auth.protected_routes.is_empty(),
                            "Should have protected routes"
                        );
                        assert_eq!(
                            auth.protected_routes.len(),
                            5,
                            "Should have 5 worker routes"
                        );

                        // Verify specific route using Vec structure
                        let delete_cache_route = auth
                            .protected_routes
                            .iter()
                            .find(|r| r.method == "DELETE" && r.path == "/templates/cache");
                        assert!(
                            delete_cache_route.is_some(),
                            "Should have DELETE /templates/cache route"
                        );
                        if let Some(r) = delete_cache_route {
                            assert_eq!(r.auth_type, "bearer");
                            assert!(r.required);
                        }

                        // Verify routes_map() conversion works
                        let routes_map = auth.routes_map();
                        assert_eq!(routes_map.len(), 5, "Routes map should have 5 entries");
                        let key = "DELETE /templates/cache";
                        assert!(
                            routes_map.contains_key(key),
                            "Routes map should contain DELETE route"
                        );
                    }
                }
            }
            Err(e) => {
                panic!("Failed to deserialize complete-test.toml: {}", e);
            }
        }

        // Verify validation passes
        let config = config.unwrap();
        match config.validate() {
            Ok(_) => println!("✓ Validation passed for complete-test.toml"),
            Err(e) => panic!("Validation failed: {:?}", e),
        }
    }
}
