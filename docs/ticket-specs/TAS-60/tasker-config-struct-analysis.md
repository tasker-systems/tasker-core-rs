# TaskerConfig Struct Tree Analysis

**Author**: Claude
**Date**: 2025-10-31
**Purpose**: TAS-60 Configuration Structure Analysis - Understanding the complete TaskerConfig struct hierarchy to guide TOML representation design

## Executive Summary

The `TaskerConfig` struct in `tasker-shared/src/config/tasker.rs` is the root configuration structure containing 15 top-level fields (14 required, 1 optional). The structure exhibits deep nesting (3-4 levels) with clear separation of concerns across subsystems. This analysis documents the complete struct tree to support TAS-60's goal of creating a semantically-near TOML representation.

## Root Structure: TaskerConfig

**Location**: `tasker-shared/src/config/tasker.rs` (lines 74-122)

```rust
pub struct TaskerConfig {
    pub database: DatabaseConfig,                      // Database connection and pooling
    pub telemetry: TelemetryConfig,                   // Observability and monitoring
    pub task_templates: TaskTemplatesConfig,          // Template discovery paths
    pub system: SystemConfig,                         // System-wide settings
    pub backoff: BackoffConfig,                       // Retry and backoff logic
    pub execution: ExecutionConfig,                   // Task/step execution limits
    pub queues: QueuesConfig,                         // Message queue configuration
    pub orchestration: OrchestrationConfig,           // Orchestration-specific settings
    pub circuit_breakers: CircuitBreakerConfig,       // Resilience configuration
    pub task_readiness: TaskReadinessConfig,          // TAS-43 task readiness system
    pub event_systems: EventSystemsConfig,            // Unified event system config
    pub mpsc_channels: MpscChannelsConfig,            // TAS-51 channel buffer sizing
    pub decision_points: DecisionPointsConfig,        // Decision point configuration
    pub worker: Option<WorkerConfig>,                 // Optional worker config
}
```

### Critical Observations

1. **Optional vs Required**: Only `worker` is optional (`Option<WorkerConfig>`), all others are required
2. **Environment Detection**: Environment is detected via `TASKER_ENV` variable, stored in `execution.environment`
3. **Subsystem Organization**: Clear separation between orchestration, worker, and shared concerns
4. **TAS Integration**: Fields show evolution via TAS tickets (TAS-43, TAS-51, TAS-49, TAS-50)

## Level 1 Fields - Detailed Breakdown

### 1. DatabaseConfig (database)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 135-206)

```rust
pub struct DatabaseConfig {
    pub url: Option<String>,                          // DATABASE_URL or ${DATABASE_URL:-default}
    pub pool: DatabasePoolConfig,                     // Connection pool settings
    pub variables: DatabaseVariables,                 // PostgreSQL session variables
    pub database: Option<String>,                     // Explicit database name
    pub skip_migration_check: bool,                   // Skip migration validation
}

pub struct DatabasePoolConfig {
    pub max_connections: u32,                         // Pool size
    pub min_connections: u32,                         // Minimum connections
    pub acquire_timeout_seconds: u64,                 // Connection acquisition timeout
    pub idle_timeout_seconds: u64,                    // Idle connection cleanup
    pub max_lifetime_seconds: u64,                    // Max connection lifetime
}

pub struct DatabaseVariables {
    pub statement_timeout: u64,                       // PostgreSQL statement_timeout (ms)
}
```

**Key Patterns**:
- Environment variable expansion: `${DATABASE_URL:-fallback}`
- Fail-fast: Panics if DATABASE_URL not provided when required
- Environment-aware database naming: `tasker_rust_{environment}`

### 2. TelemetryConfig (telemetry)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 268-274)

```rust
pub struct TelemetryConfig {
    pub enabled: bool,                                // Enable telemetry
    pub service_name: String,                         // OpenTelemetry service name
    pub sample_rate: f64,                             // Trace sampling rate (0.0-1.0)
}
```

**Observations**: Simple flat structure, no nesting

### 3. TaskTemplatesConfig (task_templates)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 289-293)

```rust
pub struct TaskTemplatesConfig {
    pub search_paths: Vec<String>,                    // Glob patterns for YAML discovery
}
```

**Example**: `["config/task_templates/*.{yml,yaml}"]`

### 4. SystemConfig (system)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 309-316)

```rust
pub struct SystemConfig {
    pub default_dependent_system: String,             // Default system identifier
    pub version: String,                              // tasker-core version
    pub max_recursion_depth: u32,                     // Recursion limit (was hardcoded)
}
```

**TAS-50 Integration**: Replaces hardcoded constants with configuration

### 5. BackoffConfig (backoff)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 318-365)

```rust
pub struct BackoffConfig {
    pub default_backoff_seconds: Vec<u64>,            // [1, 2, 4, 8, 16, 32]
    pub max_backoff_seconds: u64,                     // Maximum backoff cap
    pub backoff_multiplier: f64,                      // Exponential multiplier
    pub jitter_enabled: bool,                         // Add randomization
    pub jitter_max_percentage: f64,                   // Max jitter (0.0-1.0)
    pub reenqueue_delays: ReenqueueDelays,            // State-specific delays
}

pub struct ReenqueueDelays {
    pub initializing: u64,
    pub enqueuing_steps: u64,
    pub steps_in_process: u64,
    pub evaluating_results: u64,
    pub waiting_for_dependencies: u64,
    pub waiting_for_retry: u64,
    pub blocked_by_failures: u64,
}
```

**Depth**: 2 levels
**Complexity**: State machine-aware delays for each task state

### 6. ExecutionConfig (execution)

**Location**: `tasker-shared/src/config/tasker.rs` (lines 367-416)

```rust
pub struct ExecutionConfig {
    pub max_concurrent_tasks: u32,                    // Task concurrency limit
    pub max_concurrent_steps: u32,                    // Step concurrency limit
    pub default_timeout_seconds: u64,                 // Default task timeout
    pub step_execution_timeout_seconds: u64,          // Step execution timeout
    pub max_discovery_attempts: u32,                  // Step discovery retries
    pub step_batch_size: u32,                         // Batch size for step operations
    pub max_retries: u32,                             // Global retry limit
    pub max_workflow_steps: u32,                      // Max steps per workflow
    pub connection_timeout_seconds: u64,              // API connection timeout
    pub environment: String,                          // TASKER_ENV (cached)
}
```

**Critical Field**: `environment` is the canonical source of TASKER_ENV

### 7. QueuesConfig (queues)

**Location**: `tasker-shared/src/config/queues.rs` (lines 1-178)

```rust
pub struct QueuesConfig {
    pub backend: String,                              // "pgmq", "rabbitmq", etc.
    pub orchestration_namespace: String,              // "orchestration"
    pub worker_namespace: String,                     // "worker"
    pub default_visibility_timeout_seconds: u32,
    pub default_batch_size: u32,
    pub max_batch_size: u32,
    pub naming_pattern: String,                       // "{namespace}_{name}_queue"
    pub health_check_interval: u64,
    pub orchestration_queues: OrchestrationQueuesConfig,
    pub pgmq: PgmqBackendConfig,
    pub rabbitmq: Option<RabbitMqBackendConfig>,
}

pub struct OrchestrationQueuesConfig {
    pub task_requests: String,                        // "orchestration_task_requests_queue"
    pub task_finalizations: String,
    pub step_results: String,
}

pub struct PgmqBackendConfig {
    pub poll_interval_ms: u64,
    pub shutdown_timeout_seconds: u64,
    pub max_retries: u32,
}

pub struct RabbitMqBackendConfig {
    pub connection_timeout_seconds: u64,
}
```

**Depth**: 3 levels
**Backend Abstraction**: Prepared for RabbitMQ integration (TAS-40+)

### 8. OrchestrationConfig (orchestration)

**Location**: `tasker-shared/src/config/orchestration/mod.rs` (lines 16-51)

```rust
pub struct OrchestrationConfig {
    pub mode: String,                                 // "standalone", "distributed"
    pub enable_performance_logging: bool,
    pub web: WebConfig,                               // Web API configuration
}
```

**Note**: This is the "legacy" OrchestrationConfig in TaskerConfig. There's also a context-specific OrchestrationConfig in `contexts/orchestration.rs` (see Section 4).

### 9. CircuitBreakerConfig (circuit_breakers)

**Location**: `tasker-shared/src/config/circuit_breaker.rs` (lines 1-82)

```rust
pub struct CircuitBreakerConfig {
    pub enabled: bool,                                // Global enable/disable
    pub global_settings: CircuitBreakerGlobalSettings,
    pub default_config: CircuitBreakerComponentConfig,
    pub component_configs: HashMap<String, CircuitBreakerComponentConfig>,
}

pub struct CircuitBreakerGlobalSettings {
    pub max_circuit_breakers: usize,
    pub metrics_collection_interval_seconds: u64,
    pub min_state_transition_interval_seconds: f64,
}

pub struct CircuitBreakerComponentConfig {
    pub failure_threshold: u32,                       // Failures before open
    pub timeout_seconds: u64,                         // Open state duration
    pub success_threshold: u32,                       // Successes to close
}
```

**Depth**: 3 levels
**Pattern**: Named component overrides via HashMap

### 10. TaskReadinessConfig (task_readiness)

**Location**: `tasker-shared/src/config/task_readiness.rs` (lines 1-574)

**Complexity**: HIGHEST - 8 nested structs, 4 levels deep

```rust
pub struct TaskReadinessConfig {
    pub enabled: bool,
    pub event_system: Option<TaskReadinessEventSystemConfig>,
    pub enhanced_settings: EnhancedCoordinatorSettings,
    pub notification: TaskReadinessNotificationConfig,
    pub fallback_polling: ReadinessFallbackConfig,
    pub event_channel: EventChannelConfig,            // NOTE: TAS-51 migrated buffer_size
    pub coordinator: TaskReadinessCoordinatorConfig,
    pub error_handling: ErrorHandlingConfig,
}

// Level 2 structs (8 of them):
pub struct EnhancedCoordinatorSettings { ... }
pub struct TaskReadinessNotificationConfig {
    pub global_channels: Vec<String>,
    pub namespace_patterns: NamespacePatterns,        // Level 3
    pub event_classification: EventClassificationConfig, // Level 3
    pub connection: ConnectionConfig,                 // Level 3
}
pub struct ReadinessFallbackConfig { ... }
pub struct EventChannelConfig {
    pub max_retries: u32,
    pub backoff: BackoffConfig,                       // Level 3 (different from root BackoffConfig)
}
pub struct TaskReadinessCoordinatorConfig { ... }
pub struct ErrorHandlingConfig { ... }

// Level 3 structs:
pub struct NamespacePatterns { ... }
pub struct EventClassificationConfig { ... }
pub struct ConnectionConfig { ... }
pub struct BackoffConfig { ... }                      // Naming collision!
```

**Key Issues**:
- **Naming Collision**: `EventChannelConfig.backoff: BackoffConfig` conflicts with root `TaskerConfig.backoff: BackoffConfig`
- **TAS-51 Migration**: `event_channel.buffer_size` moved to `mpsc_channels.task_readiness.event_channel.buffer_size`
- **Deep Nesting**: 4 levels deep (`config.task_readiness.notification.connection.auto_reconnect`)

### 11. EventSystemsConfig (event_systems)

**Location**: `tasker-shared/src/config/event_systems.rs` (lines 1-439)

```rust
// NOTE: This is NOT defined as EventSystemsConfig in the file!
// Instead, it's built from individual event system configs.
// Based on context, EventSystemsConfig likely contains:

pub struct EventSystemsConfig {
    pub orchestration: OrchestrationEventSystemConfig,
    pub task_readiness: TaskReadinessEventSystemConfig,
    pub worker: WorkerEventSystemConfig,
}

// Generic pattern for all event systems:
pub struct EventSystemConfig<T = ()> {
    pub system_id: String,
    pub deployment_mode: DeploymentMode,              // EventDrivenOnly, PollingOnly, Hybrid
    pub timing: EventSystemTimingConfig,
    pub processing: EventSystemProcessingConfig,
    pub health: EventSystemHealthConfig,
    pub metadata: T,                                  // System-specific metadata
}

// Level 2 structs (shared across all event systems):
pub struct EventSystemTimingConfig {
    pub health_check_interval_seconds: u64,
    pub fallback_polling_interval_seconds: u64,
    pub visibility_timeout_seconds: u64,
    pub processing_timeout_seconds: u64,
    pub claim_timeout_seconds: u64,
}

pub struct EventSystemProcessingConfig {
    pub max_concurrent_operations: usize,
    pub batch_size: u32,
    pub max_retries: u32,
    pub backoff: BackoffConfig,                       // Level 3 - naming collision!
}

pub struct EventSystemHealthConfig {
    pub enabled: bool,
    pub performance_monitoring_enabled: bool,
    pub max_consecutive_errors: u32,
    pub error_rate_threshold_per_minute: u32,
}

pub struct BackoffConfig {                            // Yet another BackoffConfig!
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
    pub jitter_percent: f64,
}

// System-specific metadata:
pub struct OrchestrationEventSystemMetadata { ... }   // Empty placeholder
pub struct TaskReadinessEventSystemMetadata { ... }   // Empty placeholder
pub struct WorkerEventSystemMetadata {
    pub in_process_events: InProcessEventConfig,
    pub listener: WorkerListenerConfig,
    pub fallback_poller: WorkerFallbackPollerConfig,
    pub resource_limits: WorkerResourceLimits,
}
```

**Depth**: 4 levels (with worker metadata)
**Pattern**: Generic `EventSystemConfig<T>` with subsystem-specific metadata
**Naming Collision**: Yet another `BackoffConfig` at level 3

### 12. MpscChannelsConfig (mpsc_channels)

**Location**: `tasker-shared/src/config/mpsc_channels.rs` (lines 1-431)

**TAS-51**: Unified bounded MPSC channel configuration

```rust
pub struct MpscChannelsConfig {
    pub orchestration: OrchestrationChannelsConfig,
    pub task_readiness: TaskReadinessChannelsConfig,
    pub worker: WorkerChannelsConfig,
    pub shared: SharedChannelsConfig,
    pub overflow_policy: OverflowPolicyConfig,
}

// Orchestration subsystem (3 components):
pub struct OrchestrationChannelsConfig {
    pub command_processor: OrchestrationCommandProcessorConfig,
    pub event_systems: OrchestrationEventSystemsConfig,
    pub event_listeners: OrchestrationEventListenersConfig,
}

pub struct OrchestrationCommandProcessorConfig {
    pub command_buffer_size: usize,
}

pub struct OrchestrationEventSystemsConfig {
    pub event_channel_buffer_size: usize,
}

pub struct OrchestrationEventListenersConfig {
    pub pgmq_event_buffer_size: usize,
}

// Task readiness subsystem:
pub struct TaskReadinessChannelsConfig {
    pub event_channel: TaskReadinessEventChannelConfig,
}

pub struct TaskReadinessEventChannelConfig {
    pub buffer_size: usize,
    pub send_timeout_ms: u64,
}

// Worker subsystem (5 components):
pub struct WorkerChannelsConfig {
    pub command_processor: WorkerCommandProcessorConfig,
    pub event_systems: WorkerEventSystemsConfig,
    pub event_subscribers: WorkerEventSubscribersConfig,
    pub in_process_events: WorkerInProcessEventsConfig,
    pub event_listeners: WorkerEventListenersConfig,
}

pub struct WorkerCommandProcessorConfig {
    pub command_buffer_size: usize,
}

pub struct WorkerEventSystemsConfig {
    pub event_channel_buffer_size: usize,
}

pub struct WorkerEventSubscribersConfig {
    pub completion_buffer_size: usize,
    pub result_buffer_size: usize,
}

pub struct WorkerInProcessEventsConfig {
    pub broadcast_buffer_size: usize,
}

pub struct WorkerEventListenersConfig {
    pub pgmq_event_buffer_size: usize,
}

// Shared/cross-cutting subsystem:
pub struct SharedChannelsConfig {
    pub event_publisher: SharedEventPublisherConfig,
    pub ffi: SharedFfiConfig,
}

pub struct SharedEventPublisherConfig {
    pub event_queue_buffer_size: usize,
}

pub struct SharedFfiConfig {
    pub ruby_event_buffer_size: usize,
}

// Overflow policy:
pub struct OverflowPolicyConfig {
    pub log_warning_threshold: f64,
    pub drop_policy: DropPolicy,
    pub metrics: OverflowMetricsConfig,
}

pub struct OverflowMetricsConfig {
    pub enabled: bool,
    pub saturation_check_interval_seconds: u64,
}

pub enum DropPolicy {
    Block,
    DropOldest,
    DropNewest,
}
```

**Depth**: 3 levels
**Organization**: Subsystem-based (orchestration, task_readiness, worker, shared)
**TAS-51 Migrations**:
- `task_readiness.event_channel` (from event_systems.toml)
- `worker.in_process_events.broadcast_buffer_size` (from event_systems.toml)

### 13. DecisionPointsConfig (decision_points)

**Location**: `tasker-shared/src/config/orchestration/decision_points.rs`

**Status**: Not yet read, structure TBD

### 14. WorkerConfig (worker - OPTIONAL)

**Location**: `tasker-shared/src/config/worker.rs` (lines 1-215)

```rust
pub struct WorkerConfig {
    pub worker_id: String,
    pub worker_type: String,
    pub step_processing: StepProcessingConfig,
    pub health_monitoring: HealthMonitoringConfig,
    #[serde(skip)]
    pub queues: QueuesConfig,                         // Populated from main queues, not TOML
    pub web: WebConfig,
}

pub struct StepProcessingConfig {
    pub claim_timeout_seconds: u64,
    pub max_retries: u32,
    pub max_concurrent_steps: usize,
}

pub struct HealthMonitoringConfig {
    pub health_check_interval_seconds: u64,
    pub performance_monitoring_enabled: bool,
    pub error_rate_threshold: f64,
}

// Note: EventSystemConfig for worker moved to unified TaskerConfig.event_systems.worker
```

**Depth**: 2 levels
**Special**: Only optional field in TaskerConfig (`Option<WorkerConfig>`)

### 15. WebConfig (used by both orchestration and worker)

**Location**: `tasker-shared/src/config/web.rs` (lines 1-334)

```rust
pub struct WebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub tls: WebTlsConfig,
    pub database_pools: WebDatabasePoolsConfig,
    pub cors: WebCorsConfig,
    pub auth: WebAuthConfig,
    pub rate_limiting: WebRateLimitConfig,
    pub resilience: WebResilienceConfig,
}

pub struct WebTlsConfig {
    pub enabled: bool,
    pub cert_path: String,
    pub key_path: String,
}

pub struct WebDatabasePoolsConfig {
    pub web_api_pool_size: u32,
    pub web_api_max_connections: u32,
    pub web_api_connection_timeout_seconds: u64,
    pub web_api_idle_timeout_seconds: u64,
    pub max_total_connections_hint: u32,
}

pub struct WebCorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub max_age_seconds: u64,
}

pub struct WebAuthConfig {
    pub enabled: bool,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    pub jwt_token_expiry_hours: u64,
    pub jwt_private_key: String,
    pub jwt_public_key: String,
    pub api_key: String,
    pub api_key_header: String,
    pub protected_routes: HashMap<String, RouteAuthConfig>,
}

pub struct RouteAuthConfig {
    pub auth_type: String,
    pub required: bool,
}

pub struct WebRateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub per_client_limit: bool,
}

pub struct WebResilienceConfig {
    pub circuit_breaker_enabled: bool,
    pub request_timeout_seconds: u64,
    pub max_concurrent_requests: u32,
}
```

**Depth**: 3 levels
**Shared**: Used by both `OrchestrationConfig.web` and `WorkerConfig.web`

## Struct Tree Hierarchy Visualization

```
TaskerConfig (root)
├── database: DatabaseConfig (depth: 2)
│   ├── url: Option<String>
│   ├── pool: DatabasePoolConfig
│   │   ├── max_connections: u32
│   │   ├── min_connections: u32
│   │   ├── acquire_timeout_seconds: u64
│   │   ├── idle_timeout_seconds: u64
│   │   └── max_lifetime_seconds: u64
│   ├── variables: DatabaseVariables
│   │   └── statement_timeout: u64
│   ├── database: Option<String>
│   └── skip_migration_check: bool
│
├── telemetry: TelemetryConfig (depth: 1)
│   ├── enabled: bool
│   ├── service_name: String
│   └── sample_rate: f64
│
├── task_templates: TaskTemplatesConfig (depth: 1)
│   └── search_paths: Vec<String>
│
├── system: SystemConfig (depth: 1)
│   ├── default_dependent_system: String
│   ├── version: String
│   └── max_recursion_depth: u32
│
├── backoff: BackoffConfig (depth: 2)
│   ├── default_backoff_seconds: Vec<u64>
│   ├── max_backoff_seconds: u64
│   ├── backoff_multiplier: f64
│   ├── jitter_enabled: bool
│   ├── jitter_max_percentage: f64
│   └── reenqueue_delays: ReenqueueDelays
│       ├── initializing: u64
│       ├── enqueuing_steps: u64
│       ├── steps_in_process: u64
│       ├── evaluating_results: u64
│       ├── waiting_for_dependencies: u64
│       ├── waiting_for_retry: u64
│       └── blocked_by_failures: u64
│
├── execution: ExecutionConfig (depth: 1)
│   ├── max_concurrent_tasks: u32
│   ├── max_concurrent_steps: u32
│   ├── default_timeout_seconds: u64
│   ├── step_execution_timeout_seconds: u64
│   ├── max_discovery_attempts: u32
│   ├── step_batch_size: u32
│   ├── max_retries: u32
│   ├── max_workflow_steps: u32
│   ├── connection_timeout_seconds: u64
│   └── environment: String  ← CANONICAL SOURCE OF TASKER_ENV
│
├── queues: QueuesConfig (depth: 3)
│   ├── backend: String
│   ├── orchestration_namespace: String
│   ├── worker_namespace: String
│   ├── default_visibility_timeout_seconds: u32
│   ├── default_batch_size: u32
│   ├── max_batch_size: u32
│   ├── naming_pattern: String
│   ├── health_check_interval: u64
│   ├── orchestration_queues: OrchestrationQueuesConfig
│   │   ├── task_requests: String
│   │   ├── task_finalizations: String
│   │   └── step_results: String
│   ├── pgmq: PgmqBackendConfig
│   │   ├── poll_interval_ms: u64
│   │   ├── shutdown_timeout_seconds: u64
│   │   └── max_retries: u32
│   └── rabbitmq: Option<RabbitMqBackendConfig>
│       └── connection_timeout_seconds: u64
│
├── orchestration: OrchestrationConfig (depth: 3)
│   ├── mode: String
│   ├── enable_performance_logging: bool
│   └── web: WebConfig
│       ├── enabled: bool
│       ├── bind_address: String
│       ├── request_timeout_ms: u64
│       ├── tls: WebTlsConfig (depth: 1)
│       ├── database_pools: WebDatabasePoolsConfig (depth: 1)
│       ├── cors: WebCorsConfig (depth: 1)
│       ├── auth: WebAuthConfig (depth: 2)
│       ├── rate_limiting: WebRateLimitConfig (depth: 1)
│       └── resilience: WebResilienceConfig (depth: 1)
│
├── circuit_breakers: CircuitBreakerConfig (depth: 3)
│   ├── enabled: bool
│   ├── global_settings: CircuitBreakerGlobalSettings
│   │   ├── max_circuit_breakers: usize
│   │   ├── metrics_collection_interval_seconds: u64
│   │   └── min_state_transition_interval_seconds: f64
│   ├── default_config: CircuitBreakerComponentConfig
│   │   ├── failure_threshold: u32
│   │   ├── timeout_seconds: u64
│   │   └── success_threshold: u32
│   └── component_configs: HashMap<String, CircuitBreakerComponentConfig>
│
├── task_readiness: TaskReadinessConfig (depth: 4) ← DEEPEST
│   ├── enabled: bool
│   ├── event_system: Option<TaskReadinessEventSystemConfig>
│   ├── enhanced_settings: EnhancedCoordinatorSettings
│   │   ├── startup_timeout_seconds: u64
│   │   ├── shutdown_timeout_seconds: u64
│   │   ├── metrics_enabled: bool
│   │   └── rollback_threshold_percent: f64
│   ├── notification: TaskReadinessNotificationConfig
│   │   ├── global_channels: Vec<String>
│   │   ├── namespace_patterns: NamespacePatterns
│   │   │   ├── task_ready: String
│   │   │   └── task_state_change: String
│   │   ├── event_classification: EventClassificationConfig
│   │   │   ├── max_payload_size_bytes: usize
│   │   │   └── parse_timeout_ms: u64
│   │   └── connection: ConnectionConfig
│   │       ├── max_connection_retries: u32
│   │       ├── connection_retry_delay_seconds: u64
│   │       ├── health_check_interval_seconds: u64
│   │       └── auto_reconnect: bool
│   ├── fallback_polling: ReadinessFallbackConfig
│   │   ├── enabled: bool
│   │   ├── polling_interval_ms: u64
│   │   ├── age_threshold_seconds: u64
│   │   ├── max_age_seconds: u64
│   │   ├── batch_size: u32
│   │   └── max_concurrent_pollers: u32
│   ├── event_channel: EventChannelConfig
│   │   ├── max_retries: u32
│   │   └── backoff: BackoffConfig (naming collision!)
│   │       ├── initial_delay_ms: u64
│   │       ├── max_delay_ms: u64
│   │       ├── multiplier: f64
│   │       └── jitter_percent: f64
│   ├── coordinator: TaskReadinessCoordinatorConfig
│   │   ├── instance_id_prefix: String
│   │   ├── max_concurrent_operations: u32
│   │   ├── operation_timeout_ms: u64
│   │   └── stats_interval_seconds: u64
│   └── error_handling: ErrorHandlingConfig
│       ├── max_consecutive_errors: u32
│       └── error_rate_threshold_per_minute: u32
│
├── event_systems: EventSystemsConfig (depth: 4)
│   ├── orchestration: OrchestrationEventSystemConfig
│   │   ├── system_id: String
│   │   ├── deployment_mode: DeploymentMode
│   │   ├── timing: EventSystemTimingConfig
│   │   │   ├── health_check_interval_seconds: u64
│   │   │   ├── fallback_polling_interval_seconds: u64
│   │   │   ├── visibility_timeout_seconds: u64
│   │   │   ├── processing_timeout_seconds: u64
│   │   │   └── claim_timeout_seconds: u64
│   │   ├── processing: EventSystemProcessingConfig
│   │   │   ├── max_concurrent_operations: usize
│   │   │   ├── batch_size: u32
│   │   │   ├── max_retries: u32
│   │   │   └── backoff: BackoffConfig (naming collision!)
│   │   ├── health: EventSystemHealthConfig
│   │   │   ├── enabled: bool
│   │   │   ├── performance_monitoring_enabled: bool
│   │   │   ├── max_consecutive_errors: u32
│   │   │   └── error_rate_threshold_per_minute: u32
│   │   └── metadata: OrchestrationEventSystemMetadata (empty)
│   ├── task_readiness: TaskReadinessEventSystemConfig (same structure as above)
│   └── worker: WorkerEventSystemConfig
│       ├── system_id: String
│       ├── deployment_mode: DeploymentMode
│       ├── timing: EventSystemTimingConfig
│       ├── processing: EventSystemProcessingConfig
│       ├── health: EventSystemHealthConfig
│       └── metadata: WorkerEventSystemMetadata
│           ├── in_process_events: InProcessEventConfig
│           │   ├── ffi_integration_enabled: bool
│           │   └── deduplication_cache_size: usize
│           ├── listener: WorkerListenerConfig
│           │   ├── retry_interval_seconds: u64
│           │   ├── max_retry_attempts: u32
│           │   ├── event_timeout_seconds: u64
│           │   ├── batch_processing: bool
│           │   └── connection_timeout_seconds: u64
│           ├── fallback_poller: WorkerFallbackPollerConfig
│           │   ├── enabled: bool
│           │   ├── polling_interval_ms: u64
│           │   ├── batch_size: u32
│           │   ├── age_threshold_seconds: u64
│           │   ├── max_age_hours: u64
│           │   ├── visibility_timeout_seconds: u64
│           │   └── supported_namespaces: Vec<String>
│           └── resource_limits: WorkerResourceLimits
│               ├── max_memory_mb: u64
│               ├── max_cpu_percent: f64
│               ├── max_database_connections: u32
│               └── max_queue_connections: u32
│
├── mpsc_channels: MpscChannelsConfig (depth: 3) ← TAS-51
│   ├── orchestration: OrchestrationChannelsConfig
│   │   ├── command_processor: OrchestrationCommandProcessorConfig
│   │   │   └── command_buffer_size: usize
│   │   ├── event_systems: OrchestrationEventSystemsConfig
│   │   │   └── event_channel_buffer_size: usize
│   │   └── event_listeners: OrchestrationEventListenersConfig
│   │       └── pgmq_event_buffer_size: usize
│   ├── task_readiness: TaskReadinessChannelsConfig
│   │   └── event_channel: TaskReadinessEventChannelConfig
│   │       ├── buffer_size: usize
│   │       └── send_timeout_ms: u64
│   ├── worker: WorkerChannelsConfig
│   │   ├── command_processor: WorkerCommandProcessorConfig
│   │   │   └── command_buffer_size: usize
│   │   ├── event_systems: WorkerEventSystemsConfig
│   │   │   └── event_channel_buffer_size: usize
│   │   ├── event_subscribers: WorkerEventSubscribersConfig
│   │   │   ├── completion_buffer_size: usize
│   │   │   └── result_buffer_size: usize
│   │   ├── in_process_events: WorkerInProcessEventsConfig
│   │   │   └── broadcast_buffer_size: usize
│   │   └── event_listeners: WorkerEventListenersConfig
│   │       └── pgmq_event_buffer_size: usize
│   ├── shared: SharedChannelsConfig
│   │   ├── event_publisher: SharedEventPublisherConfig
│   │   │   └── event_queue_buffer_size: usize
│   │   └── ffi: SharedFfiConfig
│   │       └── ruby_event_buffer_size: usize
│   └── overflow_policy: OverflowPolicyConfig
│       ├── log_warning_threshold: f64
│       ├── drop_policy: DropPolicy (enum: Block, DropOldest, DropNewest)
│       └── metrics: OverflowMetricsConfig
│           ├── enabled: bool
│           └── saturation_check_interval_seconds: u64
│
├── decision_points: DecisionPointsConfig (TBD)
│
└── worker: Option<WorkerConfig> (depth: 3) ← OPTIONAL
    ├── worker_id: String
    ├── worker_type: String
    ├── step_processing: StepProcessingConfig
    │   ├── claim_timeout_seconds: u64
    │   ├── max_retries: u32
    │   └── max_concurrent_steps: usize
    ├── health_monitoring: HealthMonitoringConfig
    │   ├── health_check_interval_seconds: u64
    │   ├── performance_monitoring_enabled: bool
    │   └── error_rate_threshold: f64
    ├── queues: QueuesConfig [#[serde(skip)] - populated from main queues]
    └── web: WebConfig (same structure as orchestration.web)
```

## Complexity Metrics

### By Maximum Depth

| Rank | Field                | Max Depth | Struct Count |
|------|----------------------|-----------|--------------|
| 1    | `task_readiness`     | 4         | 15           |
| 2    | `event_systems`      | 4         | 25           |
| 3    | `mpsc_channels`      | 3         | 20           |
| 4    | `circuit_breakers`   | 3         | 4            |
| 5    | `queues`             | 3         | 5            |
| 6    | `orchestration.web`  | 3         | 10           |
| 7    | `worker.web`         | 3         | 10           |
| 8    | `database`           | 2         | 3            |
| 9    | `backoff`            | 2         | 2            |
| 10   | `telemetry`          | 1         | 1            |

### Naming Collisions

**Critical Issue**: Multiple `BackoffConfig` structs with different fields:

1. **Root BackoffConfig** (`tasker.rs`):
   - `default_backoff_seconds: Vec<u64>`
   - `max_backoff_seconds: u64`
   - `backoff_multiplier: f64`
   - `jitter_enabled: bool`
   - `jitter_max_percentage: f64`
   - `reenqueue_delays: ReenqueueDelays`

2. **Event Systems BackoffConfig** (`event_systems.rs`):
   - `initial_delay_ms: u64`
   - `max_delay_ms: u64`
   - `multiplier: f64`
   - `jitter_percent: f64`

3. **Task Readiness BackoffConfig** (`task_readiness.rs`):
   - Same as event systems, but nested at `task_readiness.event_channel.backoff`

**Impact**: TOML representation must disambiguate these identically-named structs.

## Context-Specific Configurations (TAS-50)

### Discovery: Dual Configuration Structures

During analysis, I discovered a parallel configuration system in `tasker-shared/src/config/contexts/`:

**contexts/orchestration.rs** - OrchestrationConfig (Context-specific)
```rust
pub struct OrchestrationConfig {
    pub backoff: BackoffConfig,                       // From TaskerConfig
    pub orchestration_system: LegacyOrchestrationConfig, // From TaskerConfig.orchestration
    pub orchestration_events: OrchestrationEventSystemConfig, // From TaskerConfig.event_systems.orchestration
    pub task_readiness_events: TaskReadinessEventSystemConfig, // From TaskerConfig.event_systems.task_readiness
    pub mpsc_channels: OrchestrationChannelsConfig,   // From TaskerConfig.mpsc_channels.orchestration
    pub staleness_detection: StalenessDetectionConfig, // TAS-49
    pub dlq: DlqConfig,                               // TAS-49
    pub archive: ArchiveConfig,                       // TAS-49
    pub environment: String,                          // Cached from execution.environment
}
```

**contexts/worker.rs** - WorkerConfig (Context-specific)
```rust
// File not yet examined, structure TBD
```

### Key Insight

There are **TWO different configuration structures** with the same names:

1. **Root TaskerConfig fields** (e.g., `TaskerConfig.orchestration: OrchestrationConfig`)
2. **Context-specific configs** (e.g., `contexts::OrchestrationConfig`)

The context-specific configs **extract and reorganize** fields from TaskerConfig:
- Removes unnecessary fields (e.g., worker config from orchestration context)
- Adds context-specific fields (e.g., TAS-49 DLQ fields)
- Flattens nested structures for easier access

**This explains the TOML table name discrepancies we've been seeing!**

## Components Directory (TAS-50)

**Location**: `tasker-shared/src/config/components/`

**Purpose**: Reusable configuration components that are composed into context-specific configs.

**Module Structure**:
```rust
pub mod backoff;           // BackoffConfig
pub mod circuit_breakers;  // CircuitBreakersConfig
pub mod database;          // DatabaseConfig, DatabasePoolConfig
pub mod dlq;               // DlqConfig, StalenessDetectionConfig, ArchiveConfig
pub mod event_systems;     // EventSystemsConfig (orchestration, task_readiness, worker)
pub mod mpsc_channels;     // MpscChannelsConfig (TAS-51)
pub mod orchestration_system; // OrchestrationSystemConfig
pub mod queues;            // QueuesConfig, PgmqConfig
pub mod web;               // WebConfig
pub mod worker_system;     // WorkerSystemConfig
```

**Pattern**: Component files often re-export types from other locations:
```rust
// components/database.rs
pub use crate::config::tasker::{DatabaseConfig, DatabasePoolConfig, DatabaseVariables};

// components/backoff.rs
pub use crate::config::tasker::{BackoffConfig, ReenqueueDelays};
```

## TAS-49 DLQ Configuration

**Location**: `tasker-shared/src/config/components/dlq.rs`

**New in TAS-49**: Comprehensive Dead Letter Queue lifecycle management

```rust
pub struct StalenessDetectionConfig {
    pub enabled: bool,
    pub detection_interval_seconds: u64,
    pub batch_size: i32,
    pub dry_run: bool,
    pub thresholds: StalenessThresholds,
    pub actions: StalenessActions,
}

pub struct StalenessThresholds {
    pub waiting_for_dependencies_minutes: i32,        // TAS-48: was hardcoded as 60
    pub waiting_for_retry_minutes: i32,               // TAS-48: was hardcoded as 30
    pub steps_in_process_minutes: i32,
    pub task_max_lifetime_hours: i32,
}

pub struct StalenessActions {
    pub auto_transition_to_error: bool,
    pub auto_move_to_dlq: bool,
    pub emit_events: bool,
    pub event_channel: String,
}

pub struct DlqConfig {
    pub enabled: bool,
    pub auto_dlq_on_staleness: bool,
    pub include_full_task_snapshot: bool,
    pub max_pending_age_hours: i32,
    pub reasons: DlqReasons,
}

pub struct DlqReasons {
    pub staleness_timeout: bool,
    pub max_retries_exceeded: bool,
    pub worker_unavailable: bool,
    pub dependency_cycle_detected: bool,
    pub manual_dlq: bool,
}

pub struct ArchiveConfig {
    pub enabled: bool,
    pub retention_days: i32,
    pub archive_batch_size: i32,
    pub archive_interval_hours: u64,
    pub policies: ArchivePolicies,
}

pub struct ArchivePolicies {
    pub archive_completed: bool,
    pub archive_failed: bool,
    pub archive_cancelled: bool,
    pub archive_dlq_resolved: bool,
}
```

**Depth**: 3 levels
**Integration**: Used by `contexts::OrchestrationConfig` (not in root TaskerConfig)

## Key Findings for TAS-60

### 1. Depth Distribution

- **Shallow (1 level)**: telemetry, task_templates, system, execution
- **Medium (2 levels)**: database, backoff
- **Deep (3 levels)**: queues, circuit_breakers, mpsc_channels, web, worker
- **Very Deep (4 levels)**: task_readiness, event_systems

### 2. Naming Collisions

**BackoffConfig** appears 3 times with different structures:
- Root config (task/step retry logic)
- Event systems config (event processing retry)
- Task readiness config (same as event systems)

**Impact**: TOML must use fully qualified paths to disambiguate.

### 3. Organization Patterns

**Subsystem Organization** (TAS-51 pattern):
- `mpsc_channels.orchestration.*`
- `mpsc_channels.task_readiness.*`
- `mpsc_channels.worker.*`
- `mpsc_channels.shared.*`

**Generic with Metadata** (event_systems pattern):
- `EventSystemConfig<T>` with system-specific metadata
- Orchestration: empty placeholder
- Task Readiness: empty placeholder
- Worker: complex nested metadata

**HashMap Overrides** (circuit_breakers pattern):
- `component_configs: HashMap<String, Config>`
- Named component-specific overrides

### 4. Optional vs Required

**Only ONE optional field** in entire TaskerConfig: `worker: Option<WorkerConfig>`

All other fields are required, which means:
- TOML must provide all fields (or rely on Default implementations)
- Validation must check all required fields
- Missing fields cause deserialization errors

### 5. Serde Attributes

**`#[serde(skip)]`**: Used for fields populated programmatically:
- `WorkerConfig.queues` (populated from TaskerConfig.queues)

**`#[serde(skip_serializing_if = "Option::is_none")]`**: Used for optional fields:
- `DatabaseConfig.url`
- `DatabaseConfig.database`
- `QueuesConfig.rabbitmq`
- `TaskReadinessConfig.event_system`
- Various metadata fields

**`#[serde(default)]`**: Used for fields with fallback defaults:
- `ExecutionConfig.environment`
- `WebAuthConfig.protected_routes`
- `EventSystemConfig.metadata`
- `OverflowPolicyConfig.drop_policy`

### 6. TAS Integration History

**TAS-34**: Component-based configuration architecture
**TAS-43**: Task readiness event-driven coordination
**TAS-49**: DLQ lifecycle management (staleness, archive)
**TAS-50**: Context-specific configuration refactoring
**TAS-51**: Bounded MPSC channels migration

Each TAS added layers of configuration complexity.

## Recommendations for TAS-60

### 1. Address Naming Collisions

Create fully qualified TOML paths:
```toml
[backoff]                                             # Root backoff
default_backoff_seconds = [1, 2, 4, 8, 16, 32]

[event_systems.orchestration.processing.backoff]     # Event systems backoff
initial_delay_ms = 100

[task_readiness.event_channel.backoff]               # Task readiness backoff
initial_delay_ms = 100
```

### 2. Flatten Context-Specific Configs

Instead of mirroring TaskerConfig's deep nesting, contexts could use semantic paths:
```toml
# Current (TaskerConfig structure):
[mpsc_channels.orchestration.command_processor]
command_buffer_size = 1000

# Proposed (flattened for context):
[orchestration.channels.command_processor]
command_buffer_size = 1000
```

### 3. Denormalize Common Config

Separate truly shared config (database, telemetry, execution.environment) from context-specific config that happens to have the same values:
- **Common**: Database, telemetry, system
- **Orchestration**: Everything orchestration needs (including its own backoff, event_systems, etc.)
- **Worker**: Everything worker needs (including its own step_processing, health_monitoring, etc.)

### 4. Document Depth Limits

Propose a maximum depth limit (e.g., 3 levels) for semantic clarity:
```toml
# ❌ TOO DEEP (4 levels)
[task_readiness.notification.connection.auto_reconnect]

# ✅ BETTER (3 levels max)
[task_readiness.connection]
auto_reconnect = true
```

### 5. Consolidate Similar Patterns

Multiple structs follow the same pattern (Config + Metadata):
- `EventSystemConfig<T>`
- `CircuitBreakerConfig` with `component_configs` HashMap

Propose a unified pattern for extensible configuration.

## Next Steps

1. **Task 2**: Analyze TOML base file structure to understand current representation
2. **Task 3**: Analyze unified_loader to understand transformation logic
3. **Task 4**: Map TOML → Struct hydration to identify gaps
4. **Task 5**: Propose semantically-near TOML representation

## Appendix: Total Struct Count

**Estimate**: 100+ configuration structs across all modules

**By Category**:
- Root level: 15 structs
- Database: 3 structs
- Queues: 5 structs
- Web: 10 structs
- Circuit Breakers: 4 structs
- Task Readiness: 15 structs
- Event Systems: 25 structs
- MPSC Channels: 20 structs
- Worker: 8 structs
- DLQ (TAS-49): 6 structs

**Complexity Driver**: Event systems and task readiness together account for ~40% of configuration structs.
