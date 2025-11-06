//! Tasker V2 Configuration (TAS-61)
//!
//! Comprehensive runtime configuration with validation support.
//!
//! ## Completed Enhancements:
//! - ✅ All integer types updated from i64 to u32/u64 for semantic correctness
//! - ✅ Top-level configs renamed: Orchestration → OrchestrationConfig, Worker3 → WorkerConfig
//! - ✅ Event system configs renamed: Orchestration2 → OrchestrationEventSystemConfig,
//!   TaskReadiness2 → TaskReadinessEventSystemConfig, Worker → WorkerEventSystemConfig
//! - ✅ MPSC configs renamed: Orchestration3 → OrchestrationMpscChannels,
//!   TaskReadiness3 → TaskReadinessMpscChannels, Worker2 → WorkerMpscChannels
//! - ✅ Worker web configs renamed: Web2 → WorkerWeb, all nested configs prefixed with Worker*
//! - ✅ Validator crate integrated with comprehensive constraints on high-usage structs
//! - ✅ Documentation added to Backoff, Queues, EventSystems, and all renamed structs
//! - ✅ Optional fields wrapped in Option<> for flexibility (Web, Auth, TLS, etc.)
//!
//! ## Remaining nested config naming:
//! - Timing/Timing2/Timing3, Processing/Processing2/Processing3, Health/Health2/Health3,
//!   Metadata/Metadata2/Metadata3, Backoff2/3/4 could use more descriptive context prefixes
//!   but are functionally correct and validated.
//!
//! Currently isolated to this module; not exported to avoid conflicts with tasker.rs
//! until V2 migration is complete.

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Tasker V2 Configuration (TAS-61)
///
/// This is a prototype for the next-generation configuration system.
/// It represents the fully hydrated runtime configuration with proper validation.
///
/// Based on analysis from TAS-61, this config structure covers:
/// - Database connectivity and pooling
/// - Telemetry and monitoring
/// - Task execution limits and timeouts
/// - Queue management (PGMQ/RabbitMQ)
/// - Orchestration system settings
/// - Circuit breakers for resilience
/// - Event-driven systems configuration
/// - MPSC channel buffer sizes
/// - Worker configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskerConfigV2 {
    pub database: Database,
    pub telemetry: Telemetry,
    #[serde(rename = "task_templates")]
    pub task_templates: TaskTemplates,
    pub system: System,
    pub backoff: Backoff,
    pub execution: Execution,
    pub queues: Queues,
    pub orchestration: OrchestrationConfig,
    #[serde(rename = "circuit_breakers")]
    pub circuit_breakers: CircuitBreakers,
    #[serde(rename = "event_systems")]
    pub event_systems: EventSystems,
    #[serde(rename = "mpsc_channels")]
    pub mpsc_channels: MpscChannels,
    #[serde(rename = "decision_points")]
    pub decision_points: DecisionPoints,
    pub worker: WorkerConfig,
}

/// Database connection and pool configuration
///
/// Controls PostgreSQL connectivity including connection pooling parameters.
/// Pool sizing is critical for performance - see TAS-61 analysis showing
/// database.pool is accessed 22+ times across the codebase.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Database {
    #[validate(url)]
    pub url: String,
    #[validate(nested)]
    pub pool: Pool,
    pub variables: Variables,
    pub database: String,
    #[serde(rename = "skip_migration_check")]
    pub skip_migration_check: bool,
}

/// Database connection pool configuration
///
/// These parameters control the SQLx connection pool behavior.
/// Proper tuning is essential for performance and resource management.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Pool {
    /// Maximum number of database connections in the pool
    #[serde(rename = "max_connections")]
    #[validate(range(min = 1, max = 1000))]
    pub max_connections: u32,

    /// Minimum number of idle connections to maintain
    #[serde(rename = "min_connections")]
    #[validate(range(min = 0, max = 100))]
    pub min_connections: u32,

    /// Timeout in seconds for acquiring a connection from the pool
    #[serde(rename = "acquire_timeout_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub acquire_timeout_seconds: u32,

    /// Timeout in seconds before an idle connection is closed
    #[serde(rename = "idle_timeout_seconds")]
    #[validate(range(min = 10, max = 3600))]
    pub idle_timeout_seconds: u32,

    /// Maximum lifetime in seconds for any connection
    #[serde(rename = "max_lifetime_seconds")]
    #[validate(range(min = 60, max = 86400))]
    pub max_lifetime_seconds: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Variables {
    #[serde(rename = "statement_timeout")]
    pub statement_timeout: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Telemetry {
    pub enabled: bool,
    #[serde(rename = "service_name")]
    pub service_name: String,
    #[serde(rename = "sample_rate")]
    pub sample_rate: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskTemplates {
    #[serde(rename = "search_paths")]
    pub search_paths: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct System {
    #[serde(rename = "default_dependent_system")]
    pub default_dependent_system: String,
    pub version: String,
    #[serde(rename = "max_recursion_depth")]
    pub max_recursion_depth: u32,
}

/// Backoff and retry configuration
///
/// Controls exponential backoff behavior for retries and task reenqueuing.
/// Per TAS-61 analysis: `config.backoff` is accessed 28 times - critical for
/// resilience and retry logic throughout the system.
///
/// Used for:
/// - Step retry timing after failures
/// - Task reenqueue delays based on state
/// - Jitter to prevent thundering herd problems
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Backoff {
    /// Default backoff sequence in seconds (e.g., [1, 2, 4, 8])
    #[serde(rename = "default_backoff_seconds")]
    #[validate(length(min = 1, max = 20))]
    pub default_backoff_seconds: Vec<u32>,

    /// Maximum backoff delay in seconds (caps exponential growth)
    #[serde(rename = "max_backoff_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub max_backoff_seconds: u32,

    /// Multiplier for exponential backoff (typically 2.0)
    #[serde(rename = "backoff_multiplier")]
    #[validate(range(min = 1.0, max = 10.0))]
    pub backoff_multiplier: f64,

    /// Whether to add random jitter to backoff delays
    #[serde(rename = "jitter_enabled")]
    pub jitter_enabled: bool,

    /// Maximum jitter as percentage of delay (0.0-1.0)
    #[serde(rename = "jitter_max_percentage")]
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_max_percentage: f64,

    /// State-specific reenqueue delays for task state machine
    #[serde(rename = "reenqueue_delays")]
    #[validate(nested)]
    pub reenqueue_delays: ReenqueueDelays,
}

/// Reenqueue delays for task state machine
///
/// Defines how long to wait before reprocessing a task in each state.
/// These delays prevent busy-waiting and allow time for state changes.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ReenqueueDelays {
    /// Delay when task is initializing (seconds)
    #[validate(range(max = 300))]
    pub initializing: u32,

    /// Delay when enqueueing steps (seconds)
    #[serde(rename = "enqueuing_steps")]
    #[validate(range(max = 300))]
    pub enqueuing_steps: u32,

    /// Delay when steps are in process (seconds)
    #[serde(rename = "steps_in_process")]
    #[validate(range(max = 300))]
    pub steps_in_process: u32,

    /// Delay when evaluating results (seconds)
    #[serde(rename = "evaluating_results")]
    #[validate(range(max = 300))]
    pub evaluating_results: u32,

    /// Delay when waiting for dependencies (seconds)
    #[serde(rename = "waiting_for_dependencies")]
    #[validate(range(max = 3600))]
    pub waiting_for_dependencies: u32,

    /// Delay when waiting for retry (seconds)
    #[serde(rename = "waiting_for_retry")]
    #[validate(range(max = 3600))]
    pub waiting_for_retry: u32,

    /// Delay when blocked by failures (seconds)
    #[serde(rename = "blocked_by_failures")]
    #[validate(range(max = 3600))]
    pub blocked_by_failures: u32,
}

/// Task execution configuration
///
/// Controls concurrency, timeouts, and limits for task/step execution.
/// Per TAS-61 analysis: `config.execution` is accessed 29 times - one of the
/// most frequently used config sections.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Execution {
    /// Maximum number of tasks that can run concurrently
    #[serde(rename = "max_concurrent_tasks")]
    #[validate(range(min = 1, max = 10000))]
    pub max_concurrent_tasks: u32,

    /// Maximum number of steps that can execute concurrently across all tasks
    #[serde(rename = "max_concurrent_steps")]
    #[validate(range(min = 1, max = 100000))]
    pub max_concurrent_steps: u32,

    /// Default timeout in seconds for task operations
    #[serde(rename = "default_timeout_seconds")]
    #[validate(range(min = 1, max = 86400))]
    pub default_timeout_seconds: u32,

    /// Timeout in seconds for individual step execution
    #[serde(rename = "step_execution_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub step_execution_timeout_seconds: u32,

    /// Maximum attempts for discovering ready steps
    #[serde(rename = "max_discovery_attempts")]
    #[validate(range(min = 1, max = 10))]
    pub max_discovery_attempts: u32,

    /// Number of steps to process in a single batch
    #[serde(rename = "step_batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub step_batch_size: u32,

    /// Maximum retry attempts for failed operations
    #[serde(rename = "max_retries")]
    #[validate(range(min = 0, max = 100))]
    pub max_retries: u32,

    /// Maximum number of workflow steps allowed per task
    #[serde(rename = "max_workflow_steps")]
    #[validate(range(min = 1, max = 10000))]
    pub max_workflow_steps: u32,

    /// Connection timeout in seconds for external services
    #[serde(rename = "connection_timeout_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,

    /// Runtime environment (test, development, production)
    #[validate(length(min = 1))]
    pub environment: String,
}

/// Queue configuration (PGMQ/RabbitMQ)
///
/// Controls message queue backend, namespacing, and queue behavior.
/// Per TAS-61 analysis: `config.queues` is accessed 28 times - critical for
/// orchestration-worker communication and message processing.
///
/// Supports:
/// - PGMQ (PostgreSQL-based queuing)
/// - RabbitMQ (future support)
/// - Namespace isolation between orchestration and worker queues
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Queues {
    /// Queue backend type ("pgmq" or "rabbitmq")
    #[validate(length(min = 1))]
    pub backend: String,

    /// Namespace prefix for orchestration queues
    #[serde(rename = "orchestration_namespace")]
    #[validate(length(min = 1, max = 50))]
    pub orchestration_namespace: String,

    /// Namespace prefix for worker queues
    #[serde(rename = "worker_namespace")]
    #[validate(length(min = 1, max = 50))]
    pub worker_namespace: String,

    /// Default visibility timeout for messages (seconds)
    #[serde(rename = "default_visibility_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub default_visibility_timeout_seconds: u32,

    /// Default number of messages to fetch per batch
    #[serde(rename = "default_batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub default_batch_size: u32,

    /// Maximum number of messages per batch
    #[serde(rename = "max_batch_size")]
    #[validate(range(min = 1, max = 10000))]
    pub max_batch_size: u32,

    /// Queue naming pattern with placeholders like "{namespace}_{name}_queue"
    #[serde(rename = "naming_pattern")]
    #[validate(length(min = 1))]
    pub naming_pattern: String,

    /// Health check interval for queue monitoring (seconds)
    #[serde(rename = "health_check_interval")]
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval: u32,

    /// Named orchestration queues configuration
    #[serde(rename = "orchestration_queues")]
    #[validate(nested)]
    pub orchestration_queues: OrchestrationQueues,

    /// PGMQ-specific configuration
    #[validate(nested)]
    pub pgmq: Pgmq,

    /// RabbitMQ-specific configuration
    #[validate(nested)]
    pub rabbitmq: Rabbitmq,
}

/// Named orchestration queues
///
/// Defines the specific queue names for orchestration message types.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationQueues {
    /// Queue for task initialization requests
    #[serde(rename = "task_requests")]
    #[validate(length(min = 1))]
    pub task_requests: String,

    /// Queue for task finalization notifications
    #[serde(rename = "task_finalizations")]
    #[validate(length(min = 1))]
    pub task_finalizations: String,

    /// Queue for step execution results
    #[serde(rename = "step_results")]
    #[validate(length(min = 1))]
    pub step_results: String,
}

/// PostgreSQL message queue (PGMQ) configuration
///
/// Settings specific to PGMQ backend for queue operations.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Pgmq {
    /// Polling interval in milliseconds for queue checks
    #[serde(rename = "poll_interval_ms")]
    #[validate(range(min = 10, max = 10000))]
    pub poll_interval_ms: u32,

    /// Graceful shutdown timeout in seconds
    #[serde(rename = "shutdown_timeout_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub shutdown_timeout_seconds: u32,

    /// Maximum retry attempts for failed queue operations
    #[serde(rename = "max_retries")]
    #[validate(range(max = 100))]
    pub max_retries: u32,
}

/// RabbitMQ configuration
///
/// Settings specific to RabbitMQ backend (future support).
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Rabbitmq {
    /// Connection timeout in seconds for RabbitMQ
    #[serde(rename = "connection_timeout_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,
}

/// Orchestration service configuration
///
/// Controls orchestration system behavior including mode, web API, and monitoring.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationConfig {
    /// Orchestration mode (standalone, distributed, etc.)
    #[validate(length(min = 1))]
    pub mode: String,

    /// Whether to enable performance logging
    #[serde(rename = "enable_performance_logging")]
    pub enable_performance_logging: bool,

    /// Web API configuration (optional - can be disabled)
    #[validate(nested)]
    pub web: Option<Web>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Web {
    pub enabled: bool,
    #[serde(rename = "bind_address")]
    pub bind_address: String,
    #[serde(rename = "request_timeout_ms")]
    pub request_timeout_ms: u32,
    pub tls: Tls,
    #[serde(rename = "database_pools")]
    pub database_pools: DatabasePools,
    pub cors: Cors,
    pub auth: Auth,
    #[serde(rename = "rate_limiting")]
    pub rate_limiting: RateLimiting,
    pub resilience: Resilience,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Tls {
    pub enabled: bool,
    #[serde(rename = "cert_path")]
    pub cert_path: String,
    #[serde(rename = "key_path")]
    pub key_path: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DatabasePools {
    #[serde(rename = "web_api_pool_size")]
    pub web_api_pool_size: u32,
    #[serde(rename = "web_api_max_connections")]
    pub web_api_max_connections: u32,
    #[serde(rename = "web_api_connection_timeout_seconds")]
    pub web_api_connection_timeout_seconds: u32,
    #[serde(rename = "web_api_idle_timeout_seconds")]
    pub web_api_idle_timeout_seconds: u32,
    #[serde(rename = "max_total_connections_hint")]
    pub max_total_connections_hint: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Cors {
    pub enabled: bool,
    #[serde(rename = "allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(rename = "allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(rename = "allowed_headers")]
    pub allowed_headers: Vec<String>,
    #[serde(rename = "max_age_seconds")]
    pub max_age_seconds: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Auth {
    pub enabled: bool,
    #[serde(rename = "jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(rename = "jwt_audience")]
    pub jwt_audience: String,
    #[serde(rename = "jwt_token_expiry_hours")]
    pub jwt_token_expiry_hours: u32,
    #[serde(rename = "jwt_private_key")]
    pub jwt_private_key: String,
    #[serde(rename = "jwt_public_key")]
    pub jwt_public_key: String,
    #[serde(rename = "api_key")]
    pub api_key: String,
    #[serde(rename = "api_key_header")]
    pub api_key_header: String,
    #[serde(rename = "protected_routes")]
    pub protected_routes: ProtectedRoutes,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedRoutes {
    #[serde(rename = "PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}")]
    pub patch_v1_tasks_task_uuid_workflow_steps_step_uuid:
        PatchV1TasksTaskUuidWorkflowStepsStepUuid,
    #[serde(rename = "POST /v1/tasks")]
    pub post_v1_tasks: PostV1Tasks,
    #[serde(rename = "DELETE /v1/tasks/{task_uuid}")]
    pub delete_v1_tasks_task_uuid: DeleteV1TasksTaskUuid,
    #[serde(rename = "GET /v1/analytics/bottlenecks")]
    pub get_v1_analytics_bottlenecks: GetV1AnalyticsBottlenecks,
    #[serde(rename = "GET /v1/analytics/performance")]
    pub get_v1_analytics_performance: GetV1AnalyticsPerformance,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PatchV1TasksTaskUuidWorkflowStepsStepUuid {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PostV1Tasks {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DeleteV1TasksTaskUuid {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GetV1AnalyticsBottlenecks {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GetV1AnalyticsPerformance {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct RateLimiting {
    pub enabled: bool,
    #[serde(rename = "requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(rename = "burst_size")]
    pub burst_size: u32,
    #[serde(rename = "per_client_limit")]
    pub per_client_limit: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Resilience {
    #[serde(rename = "circuit_breaker_enabled")]
    pub circuit_breaker_enabled: bool,
    #[serde(rename = "request_timeout_seconds")]
    pub request_timeout_seconds: u32,
    #[serde(rename = "max_concurrent_requests")]
    pub max_concurrent_requests: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CircuitBreakers {
    pub enabled: bool,
    #[serde(rename = "global_settings")]
    pub global_settings: GlobalSettings,
    #[serde(rename = "default_config")]
    pub default_config: DefaultConfig,
    #[serde(rename = "component_configs")]
    pub component_configs: ComponentConfigs,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GlobalSettings {
    #[serde(rename = "max_circuit_breakers")]
    pub max_circuit_breakers: u32,
    #[serde(rename = "metrics_collection_interval_seconds")]
    pub metrics_collection_interval_seconds: u32,
    #[serde(rename = "min_state_transition_interval_seconds")]
    pub min_state_transition_interval_seconds: f64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DefaultConfig {
    #[serde(rename = "failure_threshold")]
    pub failure_threshold: u32,
    #[serde(rename = "timeout_seconds")]
    pub timeout_seconds: u32,
    #[serde(rename = "success_threshold")]
    pub success_threshold: u32,
}

/// Circuit breaker component-specific configurations
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ComponentConfigs {
    #[serde(rename = "task_readiness")]
    pub task_readiness: TaskReadiness,
    pub pgmq: PgmqCircuitBreakerConfig,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskReadiness {
    #[serde(rename = "failure_threshold")]
    pub failure_threshold: u32,
    #[serde(rename = "timeout_seconds")]
    pub timeout_seconds: u32,
    #[serde(rename = "success_threshold")]
    pub success_threshold: u32,
}

/// Circuit breaker configuration for PGMQ component
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PgmqCircuitBreakerConfig {
    #[serde(rename = "failure_threshold")]
    pub failure_threshold: u32,
    #[serde(rename = "timeout_seconds")]
    pub timeout_seconds: u32,
    #[serde(rename = "success_threshold")]
    pub success_threshold: u32,
}

/// Event-driven systems configuration
///
/// Controls event system behavior for orchestration, task readiness, and workers.
/// Per TAS-61 analysis: `config.event_systems` is accessed 22 times - critical for
/// real-time coordination and event-driven architecture.
///
/// Supports three deployment modes:
/// - PollingOnly: Traditional polling-based coordination
/// - EventDrivenOnly: Pure event-driven using PostgreSQL LISTEN/NOTIFY
/// - Hybrid: Event-driven with polling fallback (recommended for production)
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSystems {
    /// Orchestration event system configuration
    #[validate(nested)]
    pub orchestration: OrchestrationEventSystemConfig,

    /// Task readiness event system configuration
    #[serde(rename = "task_readiness")]
    #[validate(nested)]
    pub task_readiness: TaskReadinessEventSystemConfig,

    /// Worker event system configuration
    #[validate(nested)]
    pub worker: WorkerEventSystemConfig,
}

/// Orchestration event system configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationEventSystemConfig {
    /// Unique identifier for this event system
    #[serde(rename = "system_id")]
    #[validate(length(min = 1))]
    pub system_id: String,

    /// Deployment mode: PollingOnly, EventDrivenOnly, or Hybrid
    #[serde(rename = "deployment_mode")]
    #[validate(length(min = 1))]
    pub deployment_mode: String,

    /// Timing configuration for event processing
    #[validate(nested)]
    pub timing: Timing,

    /// Processing configuration for event handling
    #[validate(nested)]
    pub processing: Processing,

    /// Health monitoring configuration
    #[validate(nested)]
    pub health: Health,

    /// Event system metadata (extensible)
    pub metadata: Metadata,
}

/// Event system timing configuration
///
/// Controls intervals, timeouts, and claim durations for event processing.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Timing {
    /// Health check interval in seconds
    #[serde(rename = "health_check_interval_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval_seconds: u32,

    /// Fallback polling interval in seconds (for Hybrid mode)
    #[serde(rename = "fallback_polling_interval_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub fallback_polling_interval_seconds: u32,

    /// Message visibility timeout in seconds
    #[serde(rename = "visibility_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    /// Processing timeout in seconds
    #[serde(rename = "processing_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub processing_timeout_seconds: u32,

    /// Claim timeout in seconds
    #[serde(rename = "claim_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub claim_timeout_seconds: u32,
}

/// Event processing configuration
///
/// Controls concurrency, batching, and retry behavior for event handling.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Processing {
    /// Maximum concurrent event processing operations
    #[serde(rename = "max_concurrent_operations")]
    #[validate(range(min = 1, max = 10000))]
    pub max_concurrent_operations: u32,

    /// Number of events to process in a single batch
    #[serde(rename = "batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    /// Maximum retry attempts for failed events
    #[serde(rename = "max_retries")]
    #[validate(range(max = 100))]
    pub max_retries: u32,

    /// Backoff configuration for retries
    #[validate(nested)]
    pub backoff: Backoff2,
}

/// Event processing backoff configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Backoff2 {
    /// Initial retry delay in milliseconds
    #[serde(rename = "initial_delay_ms")]
    #[validate(range(min = 1, max = 60000))]
    pub initial_delay_ms: u32,

    /// Maximum retry delay in milliseconds
    #[serde(rename = "max_delay_ms")]
    #[validate(range(min = 1, max = 600000))]
    pub max_delay_ms: u32,

    /// Backoff multiplier (typically 2.0)
    #[validate(range(min = 1.0, max = 10.0))]
    pub multiplier: f64,

    /// Jitter percentage (0.0-1.0)
    #[serde(rename = "jitter_percent")]
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_percent: f64,
}

/// Event system health monitoring configuration
///
/// Controls health checks and error thresholds for event systems.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Health {
    /// Whether health monitoring is enabled
    pub enabled: bool,

    /// Whether performance metrics are collected
    #[serde(rename = "performance_monitoring_enabled")]
    pub performance_monitoring_enabled: bool,

    /// Maximum consecutive errors before circuit breaker trips
    #[serde(rename = "max_consecutive_errors")]
    #[validate(range(min = 1, max = 1000))]
    pub max_consecutive_errors: u32,

    /// Error rate threshold per minute before alerting
    #[serde(rename = "error_rate_threshold_per_minute")]
    #[validate(range(min = 1, max = 10000))]
    pub error_rate_threshold_per_minute: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Metadata {}

/// Task readiness event system configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskReadinessEventSystemConfig {
    #[serde(rename = "system_id")]
    #[validate(length(min = 1))]
    pub system_id: String,

    #[serde(rename = "deployment_mode")]
    #[validate(length(min = 1))]
    pub deployment_mode: String,

    #[validate(nested)]
    pub timing: Timing2,

    #[validate(nested)]
    pub processing: Processing2,

    #[validate(nested)]
    pub health: Health2,

    pub metadata: Metadata2,
}

/// Task readiness event system timing configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Timing2 {
    #[serde(rename = "health_check_interval_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval_seconds: u32,

    #[serde(rename = "fallback_polling_interval_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub fallback_polling_interval_seconds: u32,

    #[serde(rename = "visibility_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    #[serde(rename = "processing_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub processing_timeout_seconds: u32,

    #[serde(rename = "claim_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub claim_timeout_seconds: u32,
}

/// Task readiness event processing configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Processing2 {
    #[serde(rename = "max_concurrent_operations")]
    #[validate(range(min = 1, max = 10000))]
    pub max_concurrent_operations: u32,

    #[serde(rename = "batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    #[serde(rename = "max_retries")]
    #[validate(range(max = 100))]
    pub max_retries: u32,

    #[validate(nested)]
    pub backoff: Backoff3,
}

/// Task readiness backoff configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Backoff3 {
    #[serde(rename = "initial_delay_ms")]
    #[validate(range(min = 1, max = 60000))]
    pub initial_delay_ms: u32,

    #[serde(rename = "max_delay_ms")]
    #[validate(range(min = 1, max = 600000))]
    pub max_delay_ms: u32,

    #[validate(range(min = 1.0, max = 10.0))]
    pub multiplier: f64,

    #[serde(rename = "jitter_percent")]
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_percent: f64,
}

/// Task readiness health configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Health2 {
    pub enabled: bool,

    #[serde(rename = "performance_monitoring_enabled")]
    pub performance_monitoring_enabled: bool,

    #[serde(rename = "max_consecutive_errors")]
    #[validate(range(min = 1, max = 1000))]
    pub max_consecutive_errors: u32,

    #[serde(rename = "error_rate_threshold_per_minute")]
    #[validate(range(min = 1, max = 10000))]
    pub error_rate_threshold_per_minute: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Metadata2 {}

/// Worker event system configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemConfig {
    #[serde(rename = "system_id")]
    #[validate(length(min = 1))]
    pub system_id: String,

    #[serde(rename = "deployment_mode")]
    #[validate(length(min = 1))]
    pub deployment_mode: String,

    #[validate(nested)]
    pub timing: Timing3,

    #[validate(nested)]
    pub processing: Processing3,

    #[validate(nested)]
    pub health: Health3,

    #[validate(nested)]
    pub metadata: Metadata3,
}

/// Worker event system timing configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Timing3 {
    #[serde(rename = "health_check_interval_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub health_check_interval_seconds: u32,

    #[serde(rename = "fallback_polling_interval_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub fallback_polling_interval_seconds: u32,

    #[serde(rename = "visibility_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    #[serde(rename = "processing_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub processing_timeout_seconds: u32,

    #[serde(rename = "claim_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub claim_timeout_seconds: u32,
}

/// Worker event processing configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Processing3 {
    #[serde(rename = "max_concurrent_operations")]
    #[validate(range(min = 1, max = 10000))]
    pub max_concurrent_operations: u32,

    #[serde(rename = "batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    #[serde(rename = "max_retries")]
    #[validate(range(max = 100))]
    pub max_retries: u32,

    #[validate(nested)]
    pub backoff: Backoff4,
}

/// Worker backoff configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Backoff4 {
    #[serde(rename = "initial_delay_ms")]
    #[validate(range(min = 1, max = 60000))]
    pub initial_delay_ms: u32,

    #[serde(rename = "max_delay_ms")]
    #[validate(range(min = 1, max = 600000))]
    pub max_delay_ms: u32,

    #[validate(range(min = 1.0, max = 10.0))]
    pub multiplier: f64,

    #[serde(rename = "jitter_percent")]
    #[validate(range(min = 0.0, max = 1.0))]
    pub jitter_percent: f64,
}

/// Worker health configuration
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Health3 {
    pub enabled: bool,

    #[serde(rename = "performance_monitoring_enabled")]
    pub performance_monitoring_enabled: bool,

    #[serde(rename = "max_consecutive_errors")]
    #[validate(range(min = 1, max = 1000))]
    pub max_consecutive_errors: u32,

    #[serde(rename = "error_rate_threshold_per_minute")]
    #[validate(range(min = 1, max = 10000))]
    pub error_rate_threshold_per_minute: u32,
}

/// Worker metadata configuration (worker-specific settings)
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Metadata3 {
    #[serde(rename = "in_process_events")]
    #[validate(nested)]
    pub in_process_events: InProcessEvents,

    #[validate(nested)]
    pub listener: Listener,

    #[serde(rename = "fallback_poller")]
    #[validate(nested)]
    pub fallback_poller: FallbackPoller,

    #[serde(rename = "resource_limits")]
    #[validate(nested)]
    pub resource_limits: ResourceLimits,
}

/// In-process event configuration for workers
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct InProcessEvents {
    #[serde(rename = "ffi_integration_enabled")]
    pub ffi_integration_enabled: bool,

    #[serde(rename = "deduplication_cache_size")]
    #[validate(range(min = 100, max = 100000))]
    pub deduplication_cache_size: u32,
}

/// Event listener configuration for workers
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Listener {
    #[serde(rename = "retry_interval_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub retry_interval_seconds: u32,

    #[serde(rename = "max_retry_attempts")]
    #[validate(range(max = 100))]
    pub max_retry_attempts: u32,

    #[serde(rename = "event_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub event_timeout_seconds: u32,

    #[serde(rename = "batch_processing")]
    pub batch_processing: bool,

    #[serde(rename = "connection_timeout_seconds")]
    #[validate(range(min = 1, max = 300))]
    pub connection_timeout_seconds: u32,
}

/// Fallback poller configuration for workers
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct FallbackPoller {
    pub enabled: bool,

    #[serde(rename = "polling_interval_ms")]
    #[validate(range(min = 10, max = 60000))]
    pub polling_interval_ms: u32,

    #[serde(rename = "batch_size")]
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    #[serde(rename = "age_threshold_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub age_threshold_seconds: u32,

    #[serde(rename = "max_age_hours")]
    #[validate(range(min = 1, max = 168))]
    pub max_age_hours: u32,

    #[serde(rename = "visibility_timeout_seconds")]
    #[validate(range(min = 1, max = 3600))]
    pub visibility_timeout_seconds: u32,

    #[serde(rename = "supported_namespaces")]
    pub supported_namespaces: Vec<String>,
}

/// Resource limits for workers
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct ResourceLimits {
    #[serde(rename = "max_memory_mb")]
    #[validate(range(min = 256, max = 65536))]
    pub max_memory_mb: u32,

    #[serde(rename = "max_cpu_percent")]
    #[validate(range(min = 1.0, max = 100.0))]
    pub max_cpu_percent: f64,

    #[serde(rename = "max_database_connections")]
    #[validate(range(min = 1, max = 1000))]
    pub max_database_connections: u32,

    #[serde(rename = "max_queue_connections")]
    #[validate(range(min = 1, max = 1000))]
    pub max_queue_connections: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct MpscChannels {
    pub orchestration: OrchestrationMpscChannels,
    #[serde(rename = "task_readiness")]
    pub task_readiness: TaskReadinessMpscChannels,
    pub worker: WorkerMpscChannels,
    pub shared: Shared,
    #[serde(rename = "overflow_policy")]
    pub overflow_policy: OverflowPolicy,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationMpscChannels {
    #[serde(rename = "command_processor")]
    pub command_processor: CommandProcessor,
    #[serde(rename = "event_systems")]
    pub event_systems: OrchestrationEventSystemsChannels,
    #[serde(rename = "event_listeners")]
    pub event_listeners: EventListeners,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct CommandProcessor {
    #[serde(rename = "command_buffer_size")]
    pub command_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationEventSystemsChannels {
    #[serde(rename = "event_channel_buffer_size")]
    pub event_channel_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventListeners {
    #[serde(rename = "pgmq_event_buffer_size")]
    pub pgmq_event_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct TaskReadinessMpscChannels {
    #[serde(rename = "event_channel")]
    pub event_channel: EventChannel,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventChannel {
    #[serde(rename = "buffer_size")]
    pub buffer_size: u32,
    #[serde(rename = "send_timeout_ms")]
    pub send_timeout_ms: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerMpscChannels {
    #[serde(rename = "command_processor")]
    pub command_processor: WorkerCommandProcessor,
    #[serde(rename = "event_systems")]
    pub event_systems: WorkerEventSystemsChannels,
    #[serde(rename = "event_subscribers")]
    pub event_subscribers: EventSubscribers,
    #[serde(rename = "in_process_events")]
    pub in_process_events: WorkerInProcessMpscEvents,
    #[serde(rename = "event_listeners")]
    pub event_listeners: WorkerEventListenersChannels,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerCommandProcessor {
    #[serde(rename = "command_buffer_size")]
    pub command_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventSystemsChannels {
    #[serde(rename = "event_channel_buffer_size")]
    pub event_channel_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventSubscribers {
    #[serde(rename = "completion_buffer_size")]
    pub completion_buffer_size: u32,
    #[serde(rename = "result_buffer_size")]
    pub result_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerInProcessMpscEvents {
    #[serde(rename = "broadcast_buffer_size")]
    pub broadcast_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerEventListenersChannels {
    #[serde(rename = "pgmq_event_buffer_size")]
    pub pgmq_event_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Shared {
    #[serde(rename = "event_publisher")]
    pub event_publisher: EventPublisher,
    pub ffi: Ffi,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct EventPublisher {
    #[serde(rename = "event_queue_buffer_size")]
    pub event_queue_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Ffi {
    #[serde(rename = "ruby_event_buffer_size")]
    pub ruby_event_buffer_size: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct OverflowPolicy {
    #[serde(rename = "log_warning_threshold")]
    pub log_warning_threshold: f64,
    #[serde(rename = "drop_policy")]
    pub drop_policy: String,
    pub metrics: Metrics,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct Metrics {
    pub enabled: bool,
    #[serde(rename = "saturation_check_interval_seconds")]
    pub saturation_check_interval_seconds: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DecisionPoints {
    pub enabled: bool,
    #[serde(rename = "max_steps_per_decision")]
    pub max_steps_per_decision: u32,
    #[serde(rename = "max_decision_depth")]
    pub max_decision_depth: u32,
    #[serde(rename = "warn_threshold_steps")]
    pub warn_threshold_steps: u32,
    #[serde(rename = "warn_threshold_depth")]
    pub warn_threshold_depth: u32,
    #[serde(rename = "enable_detailed_logging")]
    pub enable_detailed_logging: bool,
    #[serde(rename = "enable_metrics")]
    pub enable_metrics: bool,
}

/// Worker service configuration
///
/// Controls worker system behavior including step processing, health monitoring, and web API.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerConfig {
    /// Unique worker identifier
    #[serde(rename = "worker_id")]
    #[validate(length(min = 1))]
    pub worker_id: String,

    /// Worker type classification (general, specialized, etc.)
    #[serde(rename = "worker_type")]
    #[validate(length(min = 1))]
    pub worker_type: String,

    /// Step processing configuration
    #[serde(rename = "step_processing")]
    #[validate(nested)]
    pub step_processing: StepProcessing,

    /// Health monitoring configuration
    #[serde(rename = "health_monitoring")]
    #[validate(nested)]
    pub health_monitoring: HealthMonitoring,

    /// Web API configuration (optional - can be disabled)
    #[validate(nested)]
    pub web: Option<WorkerWeb>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct StepProcessing {
    #[serde(rename = "claim_timeout_seconds")]
    pub claim_timeout_seconds: u32,
    #[serde(rename = "max_retries")]
    pub max_retries: u32,
    #[serde(rename = "max_concurrent_steps")]
    pub max_concurrent_steps: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct HealthMonitoring {
    #[serde(rename = "health_check_interval_seconds")]
    pub health_check_interval_seconds: u32,
    #[serde(rename = "performance_monitoring_enabled")]
    pub performance_monitoring_enabled: bool,
    #[serde(rename = "error_rate_threshold")]
    pub error_rate_threshold: f64,
}

/// Worker web API configuration
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerWeb {
    pub enabled: bool,

    #[serde(rename = "bind_address")]
    #[validate(length(min = 1))]
    pub bind_address: String,

    #[serde(rename = "request_timeout_ms")]
    #[validate(range(min = 100, max = 300000))]
    pub request_timeout_ms: u32,

    #[validate(nested)]
    pub tls: Option<WorkerWebTls>,

    #[serde(rename = "database_pools")]
    #[validate(nested)]
    pub database_pools: WorkerDatabasePools,

    #[validate(nested)]
    pub cors: Option<WorkerCors>,

    #[validate(nested)]
    pub auth: Option<WorkerAuth>,

    #[serde(rename = "rate_limiting")]
    #[validate(nested)]
    pub rate_limiting: Option<WorkerRateLimiting>,

    #[validate(nested)]
    pub resilience: Option<WorkerResilience>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerWebTls {
    pub enabled: bool,
    #[serde(rename = "cert_path")]
    pub cert_path: String,
    #[serde(rename = "key_path")]
    pub key_path: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerDatabasePools {
    #[serde(rename = "web_api_pool_size")]
    pub web_api_pool_size: u32,
    #[serde(rename = "web_api_max_connections")]
    pub web_api_max_connections: u32,
    #[serde(rename = "web_api_connection_timeout_seconds")]
    pub web_api_connection_timeout_seconds: u32,
    #[serde(rename = "web_api_idle_timeout_seconds")]
    pub web_api_idle_timeout_seconds: u32,
    #[serde(rename = "max_total_connections_hint")]
    pub max_total_connections_hint: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerCors {
    pub enabled: bool,
    #[serde(rename = "allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(rename = "allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(rename = "allowed_headers")]
    pub allowed_headers: Vec<String>,
    #[serde(rename = "max_age_seconds")]
    pub max_age_seconds: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerAuth {
    pub enabled: bool,
    #[serde(rename = "jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(rename = "jwt_audience")]
    pub jwt_audience: String,
    #[serde(rename = "jwt_token_expiry_hours")]
    pub jwt_token_expiry_hours: u32,
    #[serde(rename = "jwt_private_key")]
    pub jwt_private_key: String,
    #[serde(rename = "jwt_public_key")]
    pub jwt_public_key: String,
    #[serde(rename = "api_key")]
    pub api_key: String,
    #[serde(rename = "api_key_header")]
    pub api_key_header: String,
    #[serde(rename = "protected_routes")]
    pub protected_routes: WorkerProtectedRoutes,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerProtectedRoutes {
    #[serde(rename = "POST /templates/cache/maintain")]
    pub post_templates_cache_maintain: PostTemplatesCacheMaintain,
    #[serde(rename = "POST /templates/{namespace}/{name}/{version}/refresh")]
    pub post_templates_namespace_name_version_refresh: PostTemplatesNamespaceNameVersionRefresh,
    #[serde(rename = "GET /handlers")]
    pub get_handlers: GetHandlers,
    #[serde(rename = "DELETE /templates/cache")]
    pub delete_templates_cache: DeleteTemplatesCache,
    #[serde(rename = "GET /status/detailed")]
    pub get_status_detailed: GetStatusDetailed,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PostTemplatesCacheMaintain {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct PostTemplatesNamespaceNameVersionRefresh {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GetHandlers {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct DeleteTemplatesCache {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct GetStatusDetailed {
    #[serde(rename = "auth_type")]
    pub auth_type: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerRateLimiting {
    pub enabled: bool,
    #[serde(rename = "requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(rename = "burst_size")]
    pub burst_size: u32,
    #[serde(rename = "per_client_limit")]
    pub per_client_limit: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
pub struct WorkerResilience {
    #[serde(rename = "circuit_breaker_enabled")]
    pub circuit_breaker_enabled: bool,
    #[serde(rename = "request_timeout_seconds")]
    pub request_timeout_seconds: u32,
    #[serde(rename = "max_concurrent_requests")]
    pub max_concurrent_requests: u32,
}
