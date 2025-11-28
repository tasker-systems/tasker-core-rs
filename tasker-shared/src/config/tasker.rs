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

pub use crate::event_system::DeploymentMode;
use bon::Builder;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

// ============================================================================
// CUSTOM MACROS FOR V2 CONFIGURATION
// ============================================================================

/// Macro to implement Default for bon Builder structs by calling builder().build()
///
/// This provides a DRY approach where defaults are only declared once in
/// #[builder(default = ...)] attributes, and Default::default() reuses them.
///
/// Usage:
/// ```ignore
/// #[derive(Builder)]
/// struct MyConfig {
///     #[builder(default = 100)]
///     pub value: u32,
/// }
///
/// impl_builder_default!(MyConfig);
///
/// // Now you can use:
/// let config = MyConfig::default(); // Equivalent to MyConfig::builder().build()
/// ```
macro_rules! impl_builder_default {
    ($struct_name:ident) => {
        impl Default for $struct_name {
            fn default() -> Self {
                Self::builder().build()
            }
        }
    };
}

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

    /// Get staleness detection configuration
    ///
    /// Returns the staleness detection configuration from orchestration.dlq.staleness_detection,
    /// or a default configuration if orchestration is not configured.
    pub fn staleness_detection_config(&self) -> StalenessDetectionConfig {
        self.orchestration
            .as_ref()
            .map(|o| o.dlq.staleness_detection.clone())
            .unwrap_or_default()
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct SystemConfig {
    /// System version
    #[validate(length(min = 1))]
    #[builder(default = "0.1.0".to_string())]
    pub version: String,

    /// Default dependent system identifier
    #[validate(length(min = 1))]
    #[builder(default = "default".to_string())]
    pub default_dependent_system: String,

    /// Maximum recursion depth for workflow processing
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 50)]
    pub max_recursion_depth: u32,
}

impl_builder_default!(SystemConfig);

/// Database connection and pool configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseConfig {
    /// Database connection URL
    ///
    /// Accepts PostgreSQL URLs or ${DATABASE_URL} template substitution.
    /// TODO(TAS-61): Add custom validator for PostgreSQL URL format validation
    #[builder(default = "${DATABASE_URL}".to_string())]
    pub url: String,

    /// Database name
    #[validate(length(min = 1))]
    #[builder(default = "tasker_development".to_string())]
    pub database: String,

    /// Skip migration check on startup
    #[builder(default = false)]
    pub skip_migration_check: bool,

    /// Connection pool configuration
    #[validate(nested)]
    #[builder(default)]
    pub pool: PoolConfig,

    /// Database session variables
    #[validate(nested)]
    #[builder(default)]
    pub variables: DatabaseVariablesConfig,
}

impl_builder_default!(DatabaseConfig);

/// Connection pool configuration
///
/// Note: No cross-field validation (e.g., max >= min).
/// Operational tuning is deployment concern, not application validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct PoolConfig {
    /// Maximum number of connections in the pool
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 10)]
    pub max_connections: u32,

    /// Minimum number of connections to maintain
    #[validate(range(min = 0, max = 100))]
    #[builder(default = 2)]
    pub min_connections: u32,

    /// Connection acquisition timeout in seconds
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 30)]
    pub acquire_timeout_seconds: u32,

    /// Idle connection timeout in seconds
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 600)]
    pub idle_timeout_seconds: u32,

    /// Maximum connection lifetime in seconds
    #[validate(range(min = 60, max = 86400))]
    #[builder(default = 1800)]
    pub max_lifetime_seconds: u32,
}

impl_builder_default!(PoolConfig);

/// Database session variables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseVariablesConfig {
    /// Statement timeout in milliseconds
    #[validate(range(min = 100, max = 600000))]
    #[builder(default = 5000)]
    pub statement_timeout: u32,
}

impl_builder_default!(DatabaseVariablesConfig);

/// Message queue configuration (PGMQ)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct QueuesConfig {
    /// Queue backend type ("pgmq" or "rabbitmq")
    #[validate(length(min = 1))]
    #[builder(default = "pgmq".to_string())]
    pub backend: String,

    /// Orchestration queue namespace
    #[validate(length(min = 1))]
    #[builder(default = "orchestration".to_string())]
    pub orchestration_namespace: String,

    /// Worker queue namespace
    #[validate(length(min = 1))]
    #[builder(default = "worker".to_string())]
    pub worker_namespace: String,

    /// Default message visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 30)]
    pub default_visibility_timeout_seconds: u32,

    /// Default batch size for message fetching
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 5)]
    pub default_batch_size: u32,

    /// Maximum batch size
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub max_batch_size: u32,

    /// Queue naming pattern (e.g., "{namespace}_{name}_queue")
    #[validate(length(min = 1))]
    #[builder(default = "{namespace}_{name}_queue".to_string())]
    pub naming_pattern: String,

    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 60)]
    pub health_check_interval: u32,

    /// Named orchestration queues
    #[validate(nested)]
    #[builder(default)]
    pub orchestration_queues: OrchestrationQueuesConfig,

    /// PGMQ-specific configuration
    #[validate(nested)]
    #[builder(default)]
    pub pgmq: PgmqConfig,

    /// RabbitMQ-specific configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub rabbitmq: Option<RabbitmqConfig>,
}

impl_builder_default!(QueuesConfig);

/// Named orchestration queue configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationQueuesConfig {
    /// Task request queue name
    #[validate(length(min = 1))]
    #[builder(default = "orchestration_task_requests".to_string())]
    pub task_requests: String,

    /// Task finalization queue name
    #[validate(length(min = 1))]
    #[builder(default = "orchestration_task_finalizations".to_string())]
    pub task_finalizations: String,

    /// Step results queue name
    #[validate(length(min = 1))]
    #[builder(default = "orchestration_step_results".to_string())]
    pub step_results: String,
}

impl_builder_default!(OrchestrationQueuesConfig);

/// PGMQ-specific configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct PgmqConfig {
    /// Polling interval in milliseconds
    #[validate(range(min = 10, max = 10000))]
    #[builder(default = 250)]
    pub poll_interval_ms: u32,

    /// Shutdown timeout in seconds
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 5)]
    pub shutdown_timeout_seconds: u32,

    /// Maximum retry attempts
    #[validate(range(max = 100))]
    #[builder(default = 3)]
    pub max_retries: u32,
}

impl_builder_default!(PgmqConfig);

/// RabbitMQ-specific configuration
///
/// **TODO (TAS-35)**: RabbitMQ multi-backend support is planned but not yet implemented.
/// Currently all production configs use `backend = "pgmq"` and RabbitMQ client returns
/// "RabbitMQ client not yet implemented" error.
///
/// When implemented, this will enable queue backend selection between PGMQ and RabbitMQ
/// for multi-backend queue support. Keep this configuration for future roadmap.
///
/// See: `tasker-shared/src/messaging/clients/unified_client.rs` for placeholder stub
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct RabbitmqConfig {
    /// Connection timeout in seconds
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 10)]
    pub connection_timeout_seconds: u32,
}

impl_builder_default!(RabbitmqConfig);

/// Circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerConfig {
    /// Enable circuit breakers
    #[builder(default = true)]
    pub enabled: bool,

    /// Global circuit breaker settings
    #[validate(nested)]
    #[builder(default)]
    pub global_settings: GlobalCircuitBreakerSettings,

    /// Default circuit breaker configuration
    #[validate(nested)]
    #[builder(default)]
    pub default_config: CircuitBreakerDefaultConfig,

    /// Component-specific circuit breaker configurations
    #[validate(nested)]
    #[builder(default)]
    pub component_configs: ComponentCircuitBreakerConfigs,
}

impl CircuitBreakerConfig {
    /// Get configuration for a specific component by name
    ///
    /// Provides HashMap-style lookup over structured config for runtime flexibility.
    /// Returns the default config if component not found.
    pub fn config_for_component(&self, component_name: &str) -> CircuitBreakerComponentConfig {
        match component_name {
            "task_readiness" => self.component_configs.task_readiness.clone(),
            "pgmq" => self.component_configs.pgmq.clone(),
            _ => CircuitBreakerComponentConfig {
                failure_threshold: self.default_config.failure_threshold,
                timeout_seconds: self.default_config.timeout_seconds,
                success_threshold: self.default_config.success_threshold,
            },
        }
    }
}

impl_builder_default!(CircuitBreakerConfig);

/// Global circuit breaker settings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct GlobalCircuitBreakerSettings {
    /// Maximum number of circuit breakers
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 20)]
    pub max_circuit_breakers: u32,

    /// Metrics collection interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 10)]
    pub metrics_collection_interval_seconds: u32,

    /// Minimum state transition interval (seconds)
    #[validate(range(min = 0.1, max = 60.0))]
    #[builder(default = 1.0)]
    pub min_state_transition_interval_seconds: f64,
}

impl_builder_default!(GlobalCircuitBreakerSettings);

/// Default circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerDefaultConfig {
    /// Failure threshold before opening
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 3)]
    pub failure_threshold: u32,

    /// Timeout in seconds
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 5)]
    pub timeout_seconds: u32,

    /// Success threshold for closing
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 1)]
    pub success_threshold: u32,
}

impl_builder_default!(CircuitBreakerDefaultConfig);

/// Component-specific circuit breaker configurations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ComponentCircuitBreakerConfigs {
    /// Task readiness circuit breaker
    #[validate(nested)]
    #[builder(default)]
    pub task_readiness: CircuitBreakerComponentConfig,

    /// PGMQ circuit breaker
    #[validate(nested)]
    #[builder(default)]
    pub pgmq: CircuitBreakerComponentConfig,
}

impl_builder_default!(ComponentCircuitBreakerConfigs);

/// Component circuit breaker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakerComponentConfig {
    /// Failure threshold
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 5)]
    pub failure_threshold: u32,

    /// Timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 30)]
    pub timeout_seconds: u32,

    /// Success threshold
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 2)]
    pub success_threshold: u32,
}

impl_builder_default!(CircuitBreakerComponentConfig);

/// Shared MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct SharedMpscChannelsConfig {
    /// Event publisher channels
    #[validate(nested)]
    #[builder(default)]
    pub event_publisher: EventPublisherChannels,

    /// FFI channels (Ruby/Python)
    #[validate(nested)]
    #[builder(default)]
    pub ffi: FfiChannels,

    /// Overflow policy
    #[validate(nested)]
    #[builder(default)]
    pub overflow_policy: OverflowPolicyConfig,
}

impl_builder_default!(SharedMpscChannelsConfig);

/// Event publisher channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct EventPublisherChannels {
    /// Event queue buffer size
    #[validate(range(min = 100, max = 1000000))]
    #[builder(default = 5000)]
    pub event_queue_buffer_size: u32,
}

impl_builder_default!(EventPublisherChannels);

/// FFI channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct FfiChannels {
    /// Ruby FFI buffer size
    #[validate(range(min = 100, max = 100000))]
    #[builder(default = 1000)]
    pub ruby_event_buffer_size: u32,
}

impl_builder_default!(FfiChannels);

/// Overflow policy configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OverflowPolicyConfig {
    /// Warning threshold (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    #[builder(default = 0.8)]
    pub log_warning_threshold: f64,

    /// Drop policy ("block" or "drop")
    #[validate(length(min = 1))]
    #[builder(default = "block".to_string())]
    pub drop_policy: String,

    /// Metrics configuration
    #[validate(nested)]
    #[builder(default)]
    pub metrics: OverflowMetricsConfig,
}

impl_builder_default!(OverflowPolicyConfig);

/// Overflow metrics configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OverflowMetricsConfig {
    /// Enable metrics
    #[builder(default = true)]
    pub enabled: bool,

    /// Saturation check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 10)]
    pub saturation_check_interval_seconds: u32,
}

impl_builder_default!(OverflowMetricsConfig);

/// Task execution configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionConfig {
    /// Maximum concurrent tasks
    #[validate(range(min = 1, max = 100000))]
    #[builder(default = 100)]
    pub max_concurrent_tasks: u32,

    /// Maximum concurrent steps
    #[validate(range(min = 1, max = 1000000))]
    #[builder(default = 1000)]
    pub max_concurrent_steps: u32,

    /// Default timeout (seconds)
    #[validate(range(min = 1, max = 86400))]
    #[builder(default = 3600)]
    pub default_timeout_seconds: u32,

    /// Step execution timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 300)]
    pub step_execution_timeout_seconds: u32,

    /// Maximum discovery attempts
    #[validate(range(min = 1, max = 10))]
    #[builder(default = 3)]
    pub max_discovery_attempts: u32,

    /// Step batch size
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 10)]
    pub step_batch_size: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    #[builder(default = 3)]
    pub max_retries: u32,

    /// Maximum workflow steps
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 1000)]
    pub max_workflow_steps: u32,

    /// Connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 10)]
    pub connection_timeout_seconds: u32,

    /// Environment name
    #[validate(length(min = 1))]
    #[builder(default = "development".to_string())]
    pub environment: String,
}

impl_builder_default!(ExecutionConfig);

/// Backoff and retry configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct BackoffConfig {
    /// Default backoff sequence (seconds)
    #[validate(length(min = 1, max = 20))]
    #[builder(default = vec![1, 2, 4])]
    pub default_backoff_seconds: Vec<u32>,

    /// Maximum backoff delay (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 60)]
    pub max_backoff_seconds: u32,

    /// Backoff multiplier
    #[validate(range(min = 1.0, max = 10.0))]
    #[builder(default = 2.0)]
    pub backoff_multiplier: f64,

    /// Enable jitter
    #[builder(default = true)]
    pub jitter_enabled: bool,

    /// Maximum jitter percentage (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    #[builder(default = 0.1)]
    pub jitter_max_percentage: f64,

    /// State-specific reenqueue delays
    #[validate(nested)]
    #[builder(default)]
    pub reenqueue_delays: ReenqueueDelaysConfig,
}

impl_builder_default!(BackoffConfig);

/// Reenqueue delays for task states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ReenqueueDelaysConfig {
    /// Initializing state delay (seconds)
    #[validate(range(max = 300))]
    #[builder(default = 5)]
    pub initializing: u32,

    /// Enqueueing steps delay (seconds)
    #[validate(range(max = 300))]
    #[builder(default = 0)]
    pub enqueuing_steps: u32,

    /// Steps in process delay (seconds)
    #[validate(range(max = 300))]
    #[builder(default = 10)]
    pub steps_in_process: u32,

    /// Evaluating results delay (seconds)
    #[validate(range(max = 300))]
    #[builder(default = 5)]
    pub evaluating_results: u32,

    /// Waiting for dependencies delay (seconds)
    #[validate(range(max = 3600))]
    #[builder(default = 45)]
    pub waiting_for_dependencies: u32,

    /// Waiting for retry delay (seconds)
    #[validate(range(max = 3600))]
    #[builder(default = 30)]
    pub waiting_for_retry: u32,

    /// Blocked by failures delay (seconds)
    #[validate(range(max = 3600))]
    #[builder(default = 60)]
    pub blocked_by_failures: u32,
}

impl_builder_default!(ReenqueueDelaysConfig);

/// Task template configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct TaskTemplatesConfig {
    /// Search paths for task templates
    #[validate(length(min = 1))]
    #[builder(default = vec!["config/templates/*.yml".to_string()])]
    pub search_paths: Vec<String>,
}

impl_builder_default!(TaskTemplatesConfig);

/// Telemetry configuration
///
/// **Important**: This TOML configuration is loaded for config endpoint observability,
/// but runtime telemetry behavior is controlled by environment variables:
///
/// - `TELEMETRY_ENABLED` - Enable/disable telemetry (overrides `enabled` field)
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` - OpenTelemetry collector endpoint
/// - `OTEL_SERVICE_NAME` - Service name for traces (overrides `service_name` field)
///
/// The TOML values serve as defaults but environment variables take precedence at runtime.
/// See `tasker-shared/src/logging.rs` for actual telemetry initialization logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct TelemetryConfig {
    /// Enable telemetry
    #[builder(default = false)]
    pub enabled: bool,

    /// Service name
    #[validate(length(min = 1))]
    #[builder(default = "tasker-core".to_string())]
    pub service_name: String,

    /// Sample rate (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    #[builder(default = 1.0)]
    pub sample_rate: f64,
}

// ============================================================================
// ORCHESTRATION CONFIGURATION (Orchestration-specific)
// ============================================================================

/// Orchestration-specific configuration
///
/// Contains all configuration needed for the orchestration service.
/// Present when context is "orchestration" or "complete".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationConfig {
    /// Orchestration mode
    #[validate(length(min = 1))]
    #[builder(default = "standalone".to_string())]
    pub mode: String,

    /// Enable performance logging
    #[builder(default = false)]
    pub enable_performance_logging: bool,

    /// Event systems configuration
    #[validate(nested)]
    #[builder(default)]
    pub event_systems: OrchestrationEventSystemsConfig,

    /// Decision points configuration (TAS-53)
    #[validate(nested)]
    #[builder(default)]
    pub decision_points: DecisionPointsConfig,

    /// MPSC channel configuration
    #[validate(nested)]
    #[builder(default)]
    pub mpsc_channels: OrchestrationMpscChannelsConfig,

    /// DLQ (Dead Letter Queue) configuration (TAS-49)
    #[validate(nested)]
    #[builder(default)]
    pub dlq: DlqOperationsConfig,

    /// Batch processing configuration (TAS-59)
    #[validate(nested)]
    #[builder(default)]
    pub batch_processing: BatchProcessingConfig,

    /// Web API configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub web: Option<OrchestrationWebConfig>,
}

impl_builder_default!(OrchestrationConfig);

/// Orchestration event systems configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationEventSystemsConfig {
    /// Orchestration event system
    #[validate(nested)]
    #[builder(default)]
    pub orchestration: EventSystemConfig,

    /// Task readiness event system
    #[validate(nested)]
    #[builder(default)]
    pub task_readiness: EventSystemConfig,
}

impl_builder_default!(OrchestrationEventSystemsConfig);

/// Generic event system configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display("EventSystemConfig(id: {}, mode: {})", system_id, deployment_mode)]
pub struct EventSystemConfig<T = ()> {
    /// System identifier
    #[validate(length(min = 1))]
    #[builder(default = "default-event-system".to_string())]
    pub system_id: String,

    /// Deployment mode
    #[builder(default = DeploymentMode::Hybrid)]
    pub deployment_mode: DeploymentMode,

    /// Timing configuration
    #[validate(nested)]
    #[builder(default)]
    pub timing: EventSystemTimingConfig,

    /// Processing configuration
    #[validate(nested)]
    #[builder(default)]
    pub processing: EventSystemProcessingConfig,

    /// Health configuration
    #[validate(nested)]
    #[builder(default)]
    pub health: EventSystemHealthConfig,

    /// System-specific metadata and configuration
    #[serde(default)]
    pub metadata: T,
}

/// Event system timing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display("EventSystemTimingConfig(health_check: {}s, polling: {}s, visibility: {}s, processing: {}s, claim: {}s)",
    health_check_interval_seconds, fallback_polling_interval_seconds, visibility_timeout_seconds,
    processing_timeout_seconds, claim_timeout_seconds)]
pub struct EventSystemTimingConfig {
    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 60)]
    pub health_check_interval_seconds: u32,

    /// Fallback polling interval (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 10)]
    pub fallback_polling_interval_seconds: u32,

    /// Visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 300)]
    pub visibility_timeout_seconds: u32,

    /// Processing timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 60)]
    pub processing_timeout_seconds: u32,

    /// Claim timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 30)]
    pub claim_timeout_seconds: u32,
}

impl_builder_default!(EventSystemTimingConfig);

/// Event system processing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display(
    "EventSystemProcessingConfig(concurrent: {}, batch: {}, retries: {})",
    max_concurrent_operations,
    batch_size,
    max_retries
)]
pub struct EventSystemProcessingConfig {
    /// Maximum concurrent operations
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub max_concurrent_operations: u32,

    /// Batch size
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 50)]
    pub batch_size: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    #[builder(default = 3)]
    pub max_retries: u32,
    // TAS-61: Removed backoff field - never accessed at runtime
    // Actual backoff logic uses config.common.backoff instead
    // See: tasker-orchestration/src/orchestration/backoff_calculator.rs:74-77
}

impl_builder_default!(EventSystemProcessingConfig);

/// Event system backoff configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display(
    "EventSystemBackoffConfig(initial: {}ms, max: {}ms, multiplier: {}, jitter: {}%)",
    initial_delay_ms,
    max_delay_ms,
    multiplier,
    jitter_percent
)]
pub struct EventSystemBackoffConfig {
    /// Initial delay (milliseconds)
    #[validate(range(min = 1, max = 60000))]
    #[builder(default = 100)]
    pub initial_delay_ms: u32,

    /// Maximum delay (milliseconds)
    #[validate(range(min = 1, max = 600000))]
    #[builder(default = 30000)]
    pub max_delay_ms: u32,

    /// Multiplier
    #[validate(range(min = 1.0, max = 10.0))]
    #[builder(default = 2.0)]
    pub multiplier: f64,

    /// Jitter percent (0.0-1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    #[builder(default = 0.1)]
    pub jitter_percent: f64,
}

// bon's Builder doesn't auto-implement Default, so use our macro to call builder().build()
impl_builder_default!(EventSystemBackoffConfig);

/// Event system health configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display(
    "EventSystemHealthConfig(enabled: {}, perf_mon: {}, max_errors: {}, error_rate: {}/min)",
    enabled,
    performance_monitoring_enabled,
    max_consecutive_errors,
    error_rate_threshold_per_minute
)]
pub struct EventSystemHealthConfig {
    /// Enable health checks
    #[builder(default = true)]
    pub enabled: bool,

    /// Enable performance monitoring
    #[builder(default = true)]
    pub performance_monitoring_enabled: bool,

    /// Maximum consecutive errors
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 5)]
    pub max_consecutive_errors: u32,

    /// Error rate threshold per minute
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub error_rate_threshold_per_minute: u32,
}

impl_builder_default!(EventSystemHealthConfig);

/// Decision points configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display(
    "DecisionPointsConfig(enabled: {}, max_steps: {}, max_depth: {})",
    enabled,
    max_steps_per_decision,
    max_decision_depth
)]
pub struct DecisionPointsConfig {
    /// Enable decision points
    #[builder(default = true)]
    pub enabled: bool,

    /// Maximum steps per decision
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 50)]
    pub max_steps_per_decision: u32,

    /// Maximum decision depth
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 10)]
    pub max_decision_depth: u32,

    /// Warning threshold for steps
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 20)]
    pub warn_threshold_steps: u32,

    /// Warning threshold for depth
    #[validate(range(min = 1, max = 100))]
    #[builder(default = 5)]
    pub warn_threshold_depth: u32,

    /// Enable detailed logging
    #[builder(default = false)]
    pub enable_detailed_logging: bool,

    /// Enable metrics
    #[builder(default = true)]
    pub enable_metrics: bool,
}

impl DecisionPointsConfig {
    /// Check if decision points are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if a step count exceeds the maximum
    pub fn exceeds_max_steps(&self, count: usize) -> bool {
        count > self.max_steps_per_decision as usize
    }

    /// Check if a step count should trigger a warning
    pub fn should_warn_steps(&self, count: usize) -> bool {
        count > self.warn_threshold_steps as usize
    }

    /// Check if a decision depth exceeds the maximum
    pub fn exceeds_max_depth(&self, depth: usize) -> bool {
        depth > self.max_decision_depth as usize
    }

    /// Check if a decision depth should trigger a warning
    pub fn should_warn_depth(&self, depth: usize) -> bool {
        depth > self.warn_threshold_depth as usize
    }

    /// Get the maximum steps per decision
    pub fn max_steps(&self) -> usize {
        self.max_steps_per_decision as usize
    }

    /// Get the maximum decision depth
    pub fn max_depth(&self) -> usize {
        self.max_decision_depth as usize
    }
}

impl_builder_default!(DecisionPointsConfig);

/// Orchestration MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationMpscChannelsConfig {
    /// Command processor channels
    #[validate(nested)]
    #[builder(default)]
    pub command_processor: CommandProcessorChannels,

    /// Event system channels
    #[validate(nested)]
    #[builder(default)]
    pub event_systems: EventSystemChannels,

    /// Event listener channels
    #[validate(nested)]
    #[builder(default)]
    pub event_listeners: EventListenerChannels,
}

impl_builder_default!(OrchestrationMpscChannelsConfig);

/// Command processor channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct CommandProcessorChannels {
    /// Command buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 5000)]
    pub command_buffer_size: u32,
}

impl_builder_default!(CommandProcessorChannels);

/// Event system channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct EventSystemChannels {
    /// Event channel buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 10000)]
    pub event_channel_buffer_size: u32,
}

impl_builder_default!(EventSystemChannels);

/// Event listener channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct EventListenerChannels {
    /// PGMQ event buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 5000)]
    pub pgmq_event_buffer_size: u32,
}

impl_builder_default!(EventListenerChannels);

/// Orchestration web API configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationWebConfig {
    /// Enable web API
    #[builder(default = true)]
    pub enabled: bool,

    /// Bind address
    #[validate(length(min = 1))]
    #[builder(default = "0.0.0.0:8080".to_string())]
    pub bind_address: String,

    /// Request timeout (milliseconds)
    #[validate(range(min = 100, max = 300000))]
    #[builder(default = 30000)]
    pub request_timeout_ms: u32,

    // TAS-61: Removed tls field - web servers run plain HTTP only (ports 8080, 8081)
    // No rustls or TLS acceptor implementation exists
    /// Database pool configuration
    #[validate(nested)]
    #[builder(default)]
    pub database_pools: WebDatabasePoolsConfig,

    // TAS-61: Removed cors field - middleware uses hardcoded tower_http::cors::Any
    // See: tasker-orchestration/src/web/middleware/mod.rs:create_cors_layer()
    /// Authentication configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub auth: Option<AuthConfig>,

    // TAS-61: Removed rate_limiting field - no rate limiting middleware implemented
    // If rate limiting needed in future, consider tower-governor or similar
    /// Resilience configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub resilience: Option<ResilienceConfig>,
}

impl_builder_default!(OrchestrationWebConfig);

// TAS-61: Removed TlsConfig - web servers run plain HTTP only (ports 8080, 8081)
// No rustls or TLS acceptor implementation exists
// If TLS needed in future, restore from git history and implement with rustls

/// Web API database pool configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WebDatabasePoolsConfig {
    /// Web API pool size
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 10)]
    pub web_api_pool_size: u32,

    /// Web API max connections
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 20)]
    pub web_api_max_connections: u32,

    /// Web API connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 30)]
    pub web_api_connection_timeout_seconds: u32,

    /// Web API idle timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 600)]
    pub web_api_idle_timeout_seconds: u32,

    /// Max total connections hint
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub max_total_connections_hint: u32,
}

impl_builder_default!(WebDatabasePoolsConfig);

// TAS-61: Removed CorsConfig struct - middleware uses hardcoded tower_http::cors::Any
// See: tasker-orchestration/src/web/middleware/mod.rs:create_cors_layer()
// If CORS configuration is needed in future, restore from git history

/// Authentication configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct AuthConfig {
    /// Enable authentication
    #[builder(default = false)]
    pub enabled: bool,

    /// JWT issuer
    #[validate(length(min = 1))]
    #[builder(default = "tasker".to_string())]
    pub jwt_issuer: String,

    /// JWT audience
    #[validate(length(min = 1))]
    #[builder(default = "tasker-api".to_string())]
    pub jwt_audience: String,

    /// JWT token expiry (hours)
    #[validate(range(min = 1, max = 168))]
    #[builder(default = 24)]
    pub jwt_token_expiry_hours: u32,

    /// JWT private key
    #[builder(default = String::new())]
    pub jwt_private_key: String,

    /// JWT public key
    #[builder(default = String::new())]
    pub jwt_public_key: String,

    /// API key
    #[builder(default = String::new())]
    pub api_key: String,

    /// API key header
    #[validate(length(min = 1))]
    #[builder(default = "X-API-Key".to_string())]
    pub api_key_header: String,

    /// Route-specific authentication configuration
    ///
    /// Uses TOML array of tables for ergonomic route declarations.
    /// At load time, converted to HashMap for efficient runtime lookups.
    #[serde(default)]
    #[builder(default = vec![])]
    pub protected_routes: Vec<ProtectedRouteConfig>,
}

impl_builder_default!(AuthConfig);

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedRouteConfig {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    #[validate(length(min = 1))]
    #[builder(default = "GET".to_string())]
    pub method: String,

    /// Route path pattern (supports parameters like /v1/tasks/{task_uuid})
    #[validate(length(min = 1))]
    #[builder(default = "/".to_string())]
    pub path: String,

    /// Type of authentication required ("bearer", "api_key")
    #[validate(length(min = 1))]
    #[builder(default = "bearer".to_string())]
    pub auth_type: String,

    /// Whether authentication is required for this route
    #[builder(default = true)]
    pub required: bool,
}

impl_builder_default!(ProtectedRouteConfig);

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

// TAS-61: Removed RateLimitingConfig - no rate limiting middleware implemented
// If rate limiting needed in future, consider tower-governor or similar
// Note: ErrorCategory::RateLimit and BackoffHintType::RateLimit are different and still used

/// Resilience configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ResilienceConfig {
    /// Enable circuit breaker
    #[builder(default = true)]
    pub circuit_breaker_enabled: bool,
    // TAS-61: Removed request_timeout_seconds - timeout hardcoded in middleware (30s)
    // TAS-61: Removed max_concurrent_requests - no concurrency limiting implemented
}

impl_builder_default!(ResilienceConfig);

// ============================================================================
// DLQ (DEAD LETTER QUEUE) CONFIGURATION (TAS-49)
// ============================================================================

/// Staleness Detection Configuration
///
/// Controls automatic detection and transition of stale tasks to DLQ.
/// Consolidates TAS-48 hardcoded thresholds into configurable system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct StalenessDetectionConfig {
    /// Enable staleness detection background service
    #[builder(default = true)]
    pub enabled: bool,

    /// Interval between staleness detection runs (seconds)
    #[validate(range(min = 30, max = 3600))]
    #[builder(default = 300)]
    pub detection_interval_seconds: u32,

    /// Maximum number of stale tasks to process per detection run
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub batch_size: u32,

    /// Dry-run mode: detect but don't transition (for validation)
    #[builder(default = false)]
    pub dry_run: bool,

    /// Staleness thresholds by task state
    #[validate(nested)]
    #[builder(default)]
    pub thresholds: StalenessThresholds,

    /// Actions to take when staleness detected
    #[validate(nested)]
    #[builder(default)]
    pub actions: StalenessActions,
}

impl_builder_default!(StalenessDetectionConfig);

/// Staleness Thresholds
///
/// Time limits before tasks are considered stale. Per-template lifecycle
/// configuration in TaskTemplate YAML takes precedence over these defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct StalenessThresholds {
    /// Max time in waiting_for_dependencies state (minutes)
    /// TAS-48 consolidation: was hardcoded as 60 in SQL
    #[validate(range(min = 1, max = 1440))]
    #[builder(default = 60)]
    pub waiting_for_dependencies_minutes: u32,

    /// Max time in waiting_for_retry state (minutes)
    /// TAS-48 consolidation: was hardcoded as 30 in SQL
    #[validate(range(min = 1, max = 1440))]
    #[builder(default = 30)]
    pub waiting_for_retry_minutes: u32,

    /// Max time in steps_in_process state (minutes)
    #[validate(range(min = 1, max = 1440))]
    #[builder(default = 30)]
    pub steps_in_process_minutes: u32,

    /// Max total task lifetime (hours)
    #[validate(range(min = 1, max = 168))]
    #[builder(default = 24)]
    pub task_max_lifetime_hours: u32,
}

impl_builder_default!(StalenessThresholds);

/// Staleness Actions
///
/// Controls what happens when staleness is detected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct StalenessActions {
    /// Automatically transition stale tasks to Error state
    #[builder(default = true)]
    pub auto_transition_to_error: bool,

    /// Automatically create DLQ investigation entry
    #[builder(default = true)]
    pub auto_move_to_dlq: bool,

    /// Emit staleness events for monitoring
    #[builder(default = true)]
    pub emit_events: bool,

    /// Event channel name for staleness notifications
    #[validate(length(min = 1, max = 255))]
    #[builder(default = "task_staleness_detected".to_string())]
    pub event_channel: String,
}

impl_builder_default!(StalenessActions);

/// DLQ Operations Configuration
///
/// Controls Dead Letter Queue investigation tracking behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct DlqOperationsConfig {
    /// Enable DLQ investigation tracking
    #[builder(default = true)]
    pub enabled: bool,

    /// Automatically create DLQ entries when staleness detected
    #[builder(default = true)]
    pub auto_dlq_on_staleness: bool,

    /// Include full task+steps state in DLQ snapshot JSONB
    #[builder(default = true)]
    pub include_full_task_snapshot: bool,

    /// Alert if investigation pending longer than this (hours)
    #[validate(range(min = 1, max = 720))]
    #[builder(default = 168)]
    pub max_pending_age_hours: u32,

    /// DLQ reasons configuration
    #[validate(nested)]
    #[builder(default)]
    pub reasons: DlqReasons,

    /// Staleness detection configuration
    #[validate(nested)]
    #[builder(default)]
    pub staleness_detection: StalenessDetectionConfig,
}

impl_builder_default!(DlqOperationsConfig);

/// DLQ Reasons Configuration
///
/// Controls which conditions trigger DLQ entry creation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct DlqReasons {
    /// Tasks exceeding state timeout thresholds
    #[builder(default = true)]
    pub staleness_timeout: bool,

    /// TAS-42 retry limit hit
    #[builder(default = true)]
    pub max_retries_exceeded: bool,

    /// No worker available for extended period
    #[builder(default = true)]
    pub worker_unavailable: bool,

    /// Circular dependency discovered
    #[builder(default = true)]
    pub dependency_cycle_detected: bool,

    /// Operator manually sent to DLQ
    #[builder(default = true)]
    pub manual_dlq: bool,
}

impl_builder_default!(DlqReasons);

/// Batch Processing Configuration (TAS-59)
///
/// Controls cursor-based batch processing behavior for large dataset workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Display, Builder)]
#[serde(rename_all = "snake_case")]
#[display(
    "BatchProcessingConfig(enabled: {}, max_parallel: {}, default_batch_size: {}, checkpoint_interval: {}, stall_minutes: {})",
    enabled,
    max_parallel_batches,
    default_batch_size,
    checkpoint_interval_default,
    checkpoint_stall_minutes
)]
pub struct BatchProcessingConfig {
    /// Enable batch processing functionality
    #[builder(default = true)]
    pub enabled: bool,

    /// Maximum number of parallel batch workers per task
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 50)]
    pub max_parallel_batches: usize,

    /// Default batch size when not specified in template
    #[validate(range(min = 1, max = 100000))]
    #[builder(default = 1000)]
    pub default_batch_size: u32,

    /// Default checkpoint interval when not specified in template
    #[validate(range(min = 1, max = 10000))]
    #[builder(default = 100)]
    pub checkpoint_interval_default: u32,

    /// Minutes without checkpoint progress before considering batch stalled
    #[validate(range(min = 1, max = 1440))]
    #[builder(default = 15)]
    pub checkpoint_stall_minutes: u32,
}

impl_builder_default!(BatchProcessingConfig);

// ============================================================================
// WORKER CONFIGURATION (Worker-specific)
// ============================================================================

/// Worker-specific configuration
///
/// Contains all configuration needed for the worker service.
/// Present when context is "worker" or "complete".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerConfig {
    /// Worker identifier
    #[validate(length(min = 1))]
    #[builder(default = "worker-001".to_string())]
    pub worker_id: String,

    /// Worker type
    #[validate(length(min = 1))]
    #[builder(default = "general".to_string())]
    pub worker_type: String,

    /// Event systems configuration
    #[validate(nested)]
    #[builder(default)]
    pub event_systems: WorkerEventSystemsConfig,

    /// Step processing configuration
    #[validate(nested)]
    #[builder(default)]
    pub step_processing: StepProcessingConfig,

    /// Health monitoring configuration
    #[validate(nested)]
    #[builder(default)]
    pub health_monitoring: HealthMonitoringConfig,

    /// MPSC channel configuration
    #[validate(nested)]
    #[builder(default)]
    pub mpsc_channels: WorkerMpscChannelsConfig,

    /// Orchestration client configuration (optional)
    ///
    /// Configures how the worker connects to the orchestration API.
    /// Separate from orchestration.web (which configures how orchestration hosts its API).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub orchestration_client: Option<OrchestrationClientConfig>,

    /// Web API configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub web: Option<WorkerWebConfig>,
}

impl_builder_default!(WorkerConfig);

/// Worker event systems configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemsConfig {
    /// Worker event system
    #[validate(nested)]
    #[builder(default)]
    pub worker: WorkerEventSystemConfig,
}

impl_builder_default!(WorkerEventSystemsConfig);

/// Worker-specific event system configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemConfig {
    /// System identifier
    #[validate(length(min = 1))]
    #[builder(default = "worker-event-system".to_string())]
    pub system_id: String,

    /// Deployment mode
    #[builder(default = DeploymentMode::Hybrid)]
    pub deployment_mode: DeploymentMode,

    /// Timing configuration
    #[validate(nested)]
    #[builder(default)]
    pub timing: EventSystemTimingConfig,

    /// Processing configuration
    #[validate(nested)]
    #[builder(default)]
    pub processing: EventSystemProcessingConfig,

    /// Health configuration
    #[validate(nested)]
    #[builder(default)]
    pub health: EventSystemHealthConfig,

    /// Worker-specific metadata
    #[validate(nested)]
    #[builder(default)]
    pub metadata: WorkerEventSystemMetadata,
}

/// Worker event system metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemMetadata {
    /// In-process events configuration
    #[validate(nested)]
    #[builder(default)]
    pub in_process_events: InProcessEventsConfig,

    /// Listener configuration
    #[validate(nested)]
    #[builder(default)]
    pub listener: ListenerConfig,

    /// Fallback poller configuration
    #[validate(nested)]
    #[builder(default)]
    pub fallback_poller: FallbackPollerConfig,

    /// Resource limits
    #[validate(nested)]
    #[builder(default)]
    pub resource_limits: ResourceLimitsConfig,
}

impl_builder_default!(WorkerEventSystemMetadata);

/// In-process events configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct InProcessEventsConfig {
    /// Enable FFI integration
    #[builder(default = true)]
    pub ffi_integration_enabled: bool,

    /// Deduplication cache size
    #[validate(range(min = 100, max = 100000))]
    #[builder(default = 1000)]
    pub deduplication_cache_size: u32,
}

impl_builder_default!(InProcessEventsConfig);

/// Listener configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ListenerConfig {
    /// Retry interval (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 5)]
    pub retry_interval_seconds: u32,

    /// Maximum retry attempts
    #[validate(range(max = 100))]
    #[builder(default = 3)]
    pub max_retry_attempts: u32,

    /// Event timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 30)]
    pub event_timeout_seconds: u32,

    /// Enable batch processing
    #[builder(default = true)]
    pub batch_processing: bool,

    /// Connection timeout (seconds)
    #[validate(range(min = 1, max = 300))]
    #[builder(default = 10)]
    pub connection_timeout_seconds: u32,
}

impl_builder_default!(ListenerConfig);

/// Fallback poller configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct FallbackPollerConfig {
    /// Enable poller
    #[builder(default = true)]
    pub enabled: bool,

    /// Polling interval (milliseconds)
    #[validate(range(min = 10, max = 60000))]
    #[builder(default = 500)]
    pub polling_interval_ms: u32,

    /// Batch size
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 10)]
    pub batch_size: u32,

    /// Age threshold (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 2)]
    pub age_threshold_seconds: u32,

    /// Maximum age (hours)
    #[validate(range(min = 1, max = 168))]
    #[builder(default = 12)]
    pub max_age_hours: u32,

    /// Visibility timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 30)]
    pub visibility_timeout_seconds: u32,

    /// Supported namespaces
    #[builder(default = vec![])]
    pub supported_namespaces: Vec<String>,
}

impl_builder_default!(FallbackPollerConfig);

/// Resource limits configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct ResourceLimitsConfig {
    /// Maximum memory (MB)
    #[validate(range(min = 256, max = 65536))]
    #[builder(default = 2048)]
    pub max_memory_mb: u32,

    /// Maximum CPU percent
    #[validate(range(min = 1.0, max = 100.0))]
    #[builder(default = 80.0)]
    pub max_cpu_percent: f64,

    /// Maximum database connections
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 50)]
    pub max_database_connections: u32,

    /// Maximum queue connections
    #[validate(range(min = 1, max = 1000))]
    #[builder(default = 20)]
    pub max_queue_connections: u32,
}

impl_builder_default!(ResourceLimitsConfig);

/// Step processing configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct StepProcessingConfig {
    /// Claim timeout (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 300)]
    pub claim_timeout_seconds: u32,

    /// Maximum retries
    #[validate(range(max = 100))]
    #[builder(default = 3)]
    pub max_retries: u32,

    /// Maximum concurrent steps
    #[validate(range(min = 1, max = 100000))]
    #[builder(default = 100)]
    pub max_concurrent_steps: u32,
}

impl_builder_default!(StepProcessingConfig);

/// Health monitoring configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct HealthMonitoringConfig {
    /// Health check interval (seconds)
    #[validate(range(min = 1, max = 3600))]
    #[builder(default = 30)]
    pub health_check_interval_seconds: u32,

    /// Enable performance monitoring
    #[builder(default = true)]
    pub performance_monitoring_enabled: bool,

    /// Error rate threshold
    #[validate(range(min = 0.0, max = 1.0))]
    #[builder(default = 0.05)]
    pub error_rate_threshold: f64,
}

impl_builder_default!(HealthMonitoringConfig);

/// Worker MPSC channel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerMpscChannelsConfig {
    /// Command processor channels
    #[validate(nested)]
    #[builder(default)]
    pub command_processor: WorkerCommandProcessorChannels,

    /// Event system channels
    #[validate(nested)]
    #[builder(default)]
    pub event_systems: WorkerEventSystemChannels,

    /// Event subscriber channels
    #[validate(nested)]
    #[builder(default)]
    pub event_subscribers: WorkerEventSubscriberChannels,

    /// In-process event channels
    #[validate(nested)]
    #[builder(default)]
    pub in_process_events: WorkerInProcessEventChannels,

    /// Event listener channels
    #[validate(nested)]
    #[builder(default)]
    pub event_listeners: WorkerEventListenerChannels,

    /// TAS-65/TAS-69: Domain event system channels
    #[validate(nested)]
    #[builder(default)]
    pub domain_events: WorkerDomainEventChannels,
}

impl_builder_default!(WorkerMpscChannelsConfig);

/// Worker command processor channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerCommandProcessorChannels {
    /// Command buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 2000)]
    pub command_buffer_size: u32,
}

impl_builder_default!(WorkerCommandProcessorChannels);

/// Worker event system channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemChannels {
    /// Event channel buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 5000)]
    pub event_channel_buffer_size: u32,
}

impl_builder_default!(WorkerEventSystemChannels);

/// Worker event subscriber channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSubscriberChannels {
    /// Completion buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 1000)]
    pub completion_buffer_size: u32,

    /// Result buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 1000)]
    pub result_buffer_size: u32,
}

impl_builder_default!(WorkerEventSubscriberChannels);

/// Worker in-process event channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerInProcessEventChannels {
    /// Broadcast buffer size
    #[validate(range(min = 100, max = 1000000))]
    #[builder(default = 1000)]
    pub broadcast_buffer_size: u32,
}

impl_builder_default!(WorkerInProcessEventChannels);

/// Worker event listener channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventListenerChannels {
    /// PGMQ event buffer size
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 1000)]
    pub pgmq_event_buffer_size: u32,
}

impl_builder_default!(WorkerEventListenerChannels);

/// TAS-65/TAS-69: Worker domain event system channels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerDomainEventChannels {
    /// Command buffer size for domain event dispatch
    #[validate(range(min = 10, max = 1000000))]
    #[builder(default = 1000)]
    pub command_buffer_size: u32,

    /// Shutdown drain timeout in milliseconds
    #[validate(range(min = 100, max = 60000))]
    #[builder(default = 5000)]
    pub shutdown_drain_timeout_ms: u32,

    /// Whether to log dropped events
    #[builder(default = true)]
    pub log_dropped_events: bool,
}

impl_builder_default!(WorkerDomainEventChannels);

/// Worker web API configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct WorkerWebConfig {
    /// Enable web API
    #[builder(default = true)]
    pub enabled: bool,

    /// Bind address
    #[validate(length(min = 1))]
    #[builder(default = "0.0.0.0:8081".to_string())]
    pub bind_address: String,

    /// Request timeout (milliseconds)
    #[validate(range(min = 100, max = 300000))]
    #[builder(default = 30000)]
    pub request_timeout_ms: u32,

    // TAS-61: Removed tls field - web servers run plain HTTP only (ports 8080, 8081)
    // No rustls or TLS acceptor implementation exists
    /// Database pool configuration
    #[validate(nested)]
    #[builder(default)]
    pub database_pools: WebDatabasePoolsConfig,

    // TAS-61: Removed cors field - middleware uses hardcoded tower_http::cors::Any
    // See: tasker-orchestration/src/web/middleware/mod.rs:create_cors_layer()
    /// Authentication configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub auth: Option<AuthConfig>,

    // TAS-61: Removed rate_limiting field - no rate limiting middleware implemented
    // If rate limiting needed in future, consider tower-governor or similar
    /// Resilience configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub resilience: Option<ResilienceConfig>,
}

impl_builder_default!(WorkerWebConfig);

/// Worker orchestration client configuration
///
/// Configures how the worker connects to the orchestration API as a client.
/// This is separate from `orchestration.web` which defines how orchestration
/// hosts its API. These configs should match in production but are validated
/// separately to support different deployment topologies.
///
/// ## Example TOML
///
/// ```toml
/// [worker.orchestration_client]
/// base_url = "http://orchestration:8080"
/// timeout_ms = 30000
/// max_retries = 3
///
/// [worker.orchestration_client.auth]
/// type = "bearer"
/// token = "${ORCHESTRATION_API_TOKEN}"
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate, Builder)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationClientConfig {
    /// Base URL for the orchestration API
    ///
    /// Examples: "http://localhost:8080", "http://orchestration:8080"
    #[validate(length(min = 1))]
    #[builder(default = "http://localhost:8080".to_string())]
    pub base_url: String,

    /// Request timeout (milliseconds)
    #[validate(range(min = 100, max = 300000))]
    #[builder(default = 30000)]
    pub timeout_ms: u32,

    /// Maximum number of retry attempts
    #[validate(range(min = 0, max = 10))]
    #[builder(default = 3)]
    pub max_retries: u32,

    /// Authentication configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub auth: Option<AuthConfig>,
}

impl_builder_default!(OrchestrationClientConfig);

// ============================================================================
// Default Implementations for Bridge Compatibility
// ============================================================================

impl_builder_default!(TelemetryConfig);

// DecisionPointsConfig Default implementation via impl_builder_default! macro (see struct definition above)

impl_builder_default!(WorkerEventSystemConfig);

/// Orchestration-specific event system metadata
///
/// TAS-50: All metadata configuration consolidated to respective component configs.
/// This type kept for backward compatibility but no longer holds configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrchestrationEventSystemMetadata {
    /// Placeholder to maintain type compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _reserved: Option<()>,
}

/// Task readiness-specific event system metadata
///
/// TAS-50: Metadata configuration consolidated to task_readiness.rs
/// This type kept for backward compatibility but no longer holds configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskReadinessEventSystemMetadata {
    /// Placeholder to maintain type compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _reserved: Option<()>,
}

// TAS-50: Task Readiness specific configurations removed
// These types have been consolidated into tasker-shared/src/config/task_readiness.rs
// which is the authoritative source for task readiness configuration

// Type aliases for convenience
pub type OrchestrationEventSystemConfig = EventSystemConfig<OrchestrationEventSystemMetadata>;
pub type TaskReadinessEventSystemConfig = EventSystemConfig<TaskReadinessEventSystemMetadata>;

// Specific Default implementations for type aliases with correct system IDs
impl Default for EventSystemConfig<()> {
    fn default() -> Self {
        EventSystemConfig::builder().metadata(()).build()
    }
}

impl Default for OrchestrationEventSystemConfig {
    fn default() -> Self {
        EventSystemConfig::builder()
            .system_id("orchestration-event-system".to_string())
            .metadata(OrchestrationEventSystemMetadata::default())
            .build()
    }
}

impl Default for TaskReadinessEventSystemConfig {
    fn default() -> Self {
        EventSystemConfig::builder()
            .system_id("task-readiness-event-system".to_string())
            .metadata(TaskReadinessEventSystemMetadata::default())
            .build()
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
            dlq: DlqOperationsConfig::default(),
            batch_processing: BatchProcessingConfig::default(),
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
                domain_events: WorkerDomainEventChannels {
                    command_buffer_size: 1000,
                    shutdown_drain_timeout_ms: 5000,
                    log_dropped_events: true,
                },
            },
            web: None,
            orchestration_client: Some(OrchestrationClientConfig::default()),
        }
    }

    fn create_test_event_system_config(system_id: &str) -> EventSystemConfig<()> {
        EventSystemConfig {
            system_id: system_id.to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: create_test_timing_config(),
            processing: create_test_processing_config(),
            health: create_test_health_config(),
            metadata: (),
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
            // TAS-61: backoff field removed - uses config.common.backoff instead
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

    /// TAS-61 Phase 6C: Test Display trait and bon Builder pattern on V2 config types
    ///
    /// This test demonstrates:
    /// 1. Display trait formatting for all config types
    /// 2. bon Builder pattern with inline defaults
    /// 3. Default::default() implementations
    #[test]
    fn test_v2_display_traits() {
        // Test DeploymentMode (already has Display)
        let mode = DeploymentMode::Hybrid;
        println!("DeploymentMode: {}", mode);
        assert_eq!(format!("{}", mode), "Hybrid");

        // Test bon Builder pattern with inline defaults
        println!("\n=== Testing bon Builder Pattern ===");

        // Build with all defaults
        let backoff_default = EventSystemBackoffConfig::builder().build();
        println!(
            "EventSystemBackoffConfig (all defaults): {}",
            backoff_default
        );
        assert_eq!(backoff_default.initial_delay_ms, 100);
        assert_eq!(backoff_default.max_delay_ms, 30000);

        // Build with some overrides
        let backoff_custom = EventSystemBackoffConfig::builder()
            .initial_delay_ms(200)
            .multiplier(3.0)
            .build();
        println!("EventSystemBackoffConfig (custom): {}", backoff_custom);
        assert_eq!(backoff_custom.initial_delay_ms, 200);
        assert_eq!(backoff_custom.multiplier, 3.0);
        assert_eq!(backoff_custom.max_delay_ms, 30000); // Still uses default

        // Test Default::default() matches builder defaults
        let backoff_from_default = EventSystemBackoffConfig::default();
        assert_eq!(backoff_default, backoff_from_default);

        // Test nested builder with defaults
        let processing = EventSystemProcessingConfig::builder()
            .max_concurrent_operations(200)
            // backoff uses default
            .build();
        println!(
            "EventSystemProcessingConfig (nested defaults): {}",
            processing
        );
        assert_eq!(processing.max_concurrent_operations, 200);
        assert_eq!(processing.batch_size, 50); // Default
                                               // TAS-61: backoff field removed - uses config.common.backoff instead

        // Test all config types for Display trait
        println!("\n=== Testing Display Trait ===");
        fn requires_display<T: std::fmt::Display>(val: &T) -> String {
            format!("{}", val)
        }

        let timing = EventSystemTimingConfig::default();
        let health = EventSystemHealthConfig::default();
        let event_system_config = EventSystemConfig {
            system_id: "test".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: timing.clone(),
            processing: processing.clone(),
            health: health.clone(),
            metadata: (),
        };

        let _backoff_str = requires_display(&backoff_default);
        let _timing_str = requires_display(&timing);
        let _processing_str = requires_display(&processing);
        let _health_str = requires_display(&health);
        let _event_system_str = requires_display(&event_system_config);

        println!("✓ Display trait test completed - all types have Display");
        println!("✓ bon Builder pattern test completed - inline defaults work");
        println!("✓ Default implementations match builder defaults");
    }
}
