# TAS-50: Configuration Categories and TOML Mapping

**Document Version**: 1.0
**Date**: 2025-10-14
**Status**: Ready for Implementation Planning

---

## Executive Summary

Based on comprehensive code analysis of configuration usage across `tasker-orchestration`, `tasker-worker` (Rust), and `workers/ruby` (Ruby FFI), this document defines formal configuration categories and maps the existing 45 TOML files to context-specific deployments.

### Key Finding: Configuration Context Bifurcation

The analysis revealed that the current monolithic `TaskerConfig` forces unnecessary configuration loading:

- **Orchestration**: Uses 50/80 fields (62.5% efficiency)
- **Rust Worker**: Uses 20/80 fields (25% efficiency - **75% waste!**)
- **Ruby Worker**: Uses 0/80 fields (uses environment variables instead)

### Solution: Four Distinct Configuration Contexts

1. **CommonConfig** (~20 fields) - Shared infrastructure (database, queues, environment)
2. **OrchestrationConfig** (~30 fields) - Orchestration-specific (backoff, task readiness, orchestration events)
3. **RustWorkerConfig** (~15 fields) - Rust worker-specific (worker web API, worker events)
4. **RubyWorkerConfig** (9 env vars) - Ruby worker-specific (environment variables, no TOML)

---

## Part 1: Formal Category Definitions

### 1.1 CommonConfig Structure

**Purpose**: Configuration shared by all Rust-based components (orchestration + rust worker)

**Rust Type Definition**:
```rust
/// Common configuration used by both orchestration and worker systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonConfig {
    /// Database connection and pool configuration
    pub database: DatabaseConfig,

    /// Queue namespace and routing configuration
    pub queues: QueuesConfig,

    /// Current execution environment
    pub environment: String,

    /// Shared MPSC channel buffer sizes
    pub mpsc_channels_shared: SharedChannelsConfig,

    /// Orchestration API location (for workers to connect)
    pub orchestration_api: OrchestrationApiClientConfig,

    /// Circuit breaker configuration (if not deprecated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breakers: Option<CircuitBreakerConfig>,
}

/// Database configuration (inherited by SystemContext)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,  // From DATABASE_URL env var
    pub adapter: String,
    pub encoding: String,
    pub host: String,
    pub username: String,
    pub password: String,
    pub checkout_timeout: u32,
    pub reaping_frequency: u32,
    pub skip_migration_check: bool,
    pub pool: DatabasePoolConfig,
    pub variables: DatabaseVariablesConfig,
    // enable_secondary_database removed (always false, test-only)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabasePoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_seconds: u32,
    pub idle_timeout_seconds: u32,
    pub max_lifetime_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseVariablesConfig {
    pub statement_timeout: u32,
}

/// Queue configuration (namespace awareness)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuesConfig {
    pub backend: String,  // Always "pgmq" currently
    pub orchestration_namespace: String,
    pub worker_namespace: String,
    pub default_namespace: String,
    pub naming_pattern: String,
    pub default_visibility_timeout_seconds: u32,
    pub health_check_interval: u32,
    pub default_batch_size: u32,
    pub max_batch_size: u32,
    pub orchestration_queues: OrchestrationQueuesConfig,
    pub pgmq: PgmqConfig,
    // rabbitmq removed (future, not used)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationQueuesConfig {
    pub task_requests: String,
    pub task_finalizations: String,
    pub step_results: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqConfig {
    pub poll_interval_ms: u32,
    pub shutdown_timeout_seconds: u32,
    pub max_retries: u32,
}

/// Shared MPSC channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedChannelsConfig {
    pub event_publisher: EventPublisherChannelConfig,
    pub ffi: FfiChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPublisherChannelConfig {
    pub event_queue_buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiChannelConfig {
    pub ruby_event_buffer_size: usize,
}

/// Orchestration API client configuration (for workers to connect)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationApiClientConfig {
    pub bind_address: String,  // Where orchestration listens
    pub request_timeout_ms: u64,
    pub auth: Option<AuthConfig>,  // Auth credentials to connect
}
```

**TOML Files Required for CommonConfig**:
- `base/database.toml` (excluding deprecated fields)
- `base/queues.toml` (excluding rabbitmq, backend always "pgmq")
- `base/system.toml` (environment detection)
- `base/mpsc_channels.toml` (shared section only)
- `base/orchestration.toml` (web section only - for API location)
- `base/circuit_breakers.toml` (if not deprecated)

**Total Fields**: ~20-25 fields

---

### 1.2 OrchestrationConfig Structure

**Purpose**: Configuration specific to orchestration system deployment

**Rust Type Definition**:
```rust
/// Orchestration-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    /// Retry and backoff configuration
    pub backoff: BackoffConfig,

    /// Orchestration system configuration
    pub orchestration: OrchestrationSystemConfig,

    /// Orchestration event systems
    pub event_systems: OrchestrationEventSystemsConfig,

    /// Orchestration MPSC channels
    pub mpsc_channels: OrchestrationChannelsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    pub default_backoff_seconds: Vec<u32>,
    pub max_backoff_seconds: u32,
    pub backoff_multiplier: f64,
    pub jitter_enabled: bool,
    pub jitter_max_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSystemConfig {
    pub mode: String,
    pub max_concurrent_orchestrators: u32,
    pub enable_performance_logging: bool,
    pub use_unified_state_machine: bool,
    pub operational_state: OperationalStateConfig,
    pub web: OrchestrationWebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationalStateConfig {
    pub enable_shutdown_aware_monitoring: bool,
    pub suppress_alerts_during_shutdown: bool,
    pub startup_health_threshold_multiplier: f64,
    pub shutdown_health_threshold_multiplier: f64,
    pub graceful_shutdown_timeout_seconds: u32,
    pub emergency_shutdown_timeout_seconds: u32,
    pub enable_transition_logging: bool,
    pub transition_log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationWebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub max_request_size_mb: u32,
    pub tls: TlsConfig,
    pub database_pools: WebDatabasePoolsConfig,
    pub cors: CorsConfig,
    pub auth: AuthConfig,
    pub rate_limiting: RateLimitingConfig,
    pub resilience: ResilienceConfig,
    pub resource_monitoring: ResourceMonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEventSystemsConfig {
    pub orchestration: OrchestrationEventSystemConfig,
    pub task_readiness: TaskReadinessEventSystemConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEventSystemConfig {
    pub system_id: String,
    pub deployment_mode: String,
    pub timing: EventTimingConfig,
    pub processing: EventProcessingConfig,
    pub health: EventHealthConfig,
    pub metadata: OrchestrationEventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessEventSystemConfig {
    pub system_id: String,
    pub deployment_mode: String,
    pub timing: EventTimingConfig,
    pub processing: EventProcessingConfig,
    pub health: EventHealthConfig,
    pub metadata: TaskReadinessEventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationChannelsConfig {
    pub command_processor: CommandProcessorChannelConfig,
    pub event_systems: EventSystemsChannelConfig,
    pub event_listeners: EventListenersChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessChannelsConfig {
    pub event_channel: TaskReadinessEventChannelConfig,
}
```

**TOML Files Required for OrchestrationConfig**:
- `base/backoff.toml`
- `base/orchestration.toml` (excluding web section used in CommonConfig)
- `base/event_systems.toml` (orchestration + task_readiness sections)
- `base/mpsc_channels.toml` (orchestration + task_readiness sections)
- `base/task_readiness.toml` (if exists and used)

**Total Fields**: ~30-35 fields

---

### 1.3 RustWorkerConfig Structure

**Purpose**: Configuration specific to Rust-based worker deployment

**Rust Type Definition**:
```rust
/// Rust worker-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustWorkerConfig {
    /// Worker system configuration
    pub worker: WorkerSystemConfig,

    /// Worker event systems
    pub event_systems: WorkerEventSystemsConfig,

    /// Worker MPSC channels
    pub mpsc_channels: WorkerChannelsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSystemConfig {
    pub worker_id: String,
    pub worker_type: String,
    pub event_driven: EventDrivenConfig,
    pub step_processing: StepProcessingConfig,
    pub listener: WorkerListenerConfig,
    pub fallback_poller: WorkerFallbackPollerConfig,
    pub health_monitoring: WorkerHealthMonitoringConfig,
    pub resource_limits: WorkerResourceLimitsConfig,
    pub queue_config: WorkerQueueConfig,
    pub web: WorkerWebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDrivenConfig {
    pub enabled: bool,
    pub deployment_mode: String,
    pub health_monitoring_enabled: bool,
    pub health_check_interval_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepProcessingConfig {
    pub processor_ownership_timeout_seconds: u32,
    pub max_retries: u32,
    pub retry_backoff_multiplier: f64,
    pub heartbeat_interval_seconds: u32,
    pub max_concurrent_steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerListenerConfig {
    pub retry_interval_seconds: u32,
    pub max_retry_attempts: u32,
    pub event_timeout_seconds: u32,
    pub health_check_interval_seconds: u32,
    pub batch_processing: bool,
    pub connection_timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerFallbackPollerConfig {
    pub enabled: bool,
    pub polling_interval_ms: u32,
    pub batch_size: u32,
    pub age_threshold_seconds: u32,
    pub max_age_hours: u32,
    pub visibility_timeout_seconds: u32,
    pub processable_states: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthMonitoringConfig {
    pub health_check_interval_seconds: u32,
    pub metrics_collection_enabled: bool,
    pub performance_monitoring_enabled: bool,
    pub step_processing_rate_threshold: f64,
    pub error_rate_threshold: f64,
    pub memory_usage_threshold_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResourceLimitsConfig {
    pub max_memory_mb: u32,
    pub max_cpu_percent: u32,
    pub max_database_connections: u32,
    pub max_queue_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerQueueConfig {
    pub visibility_timeout_seconds: u32,
    pub batch_size: u32,
    pub polling_interval_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerWebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub max_request_size_mb: u32,
    pub tls: TlsConfig,
    pub database_pools: WebDatabasePoolsConfig,
    pub cors: CorsConfig,
    pub auth: AuthConfig,
    pub rate_limiting: RateLimitingConfig,
    pub resilience: ResilienceConfig,
    pub resource_monitoring: ResourceMonitoringConfig,
    pub endpoints: WorkerEndpointsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEventSystemsConfig {
    pub worker: WorkerEventSystemConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEventSystemConfig {
    pub system_id: String,
    pub deployment_mode: String,
    pub timing: EventTimingConfig,
    pub processing: EventProcessingConfig,
    pub health: EventHealthConfig,
    pub metadata: WorkerEventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerChannelsConfig {
    pub command_processor: CommandProcessorChannelConfig,
    pub event_systems: EventSystemsChannelConfig,
    pub event_subscribers: EventSubscribersChannelConfig,
    pub in_process_events: InProcessEventsChannelConfig,
    pub event_listeners: EventListenersChannelConfig,
}
```

**TOML Files Required for RustWorkerConfig**:
- `base/worker.toml`
- `base/event_systems.toml` (worker section only)
- `base/mpsc_channels.toml` (worker section only)

**Total Fields**: ~15-20 fields (plus worker web API config which mirrors orchestration web)

---

### 1.4 RubyWorkerConfig Structure

**Purpose**: Environment variable configuration for Ruby FFI-based worker deployment

**Format**: Shell environment variables (.env file or export statements)

**Environment Variables**:
```bash
# Required: Environment and Database
TASKER_ENV=development                          # Environment: test/development/production
RAILS_ENV=development                           # Rails environment fallback
DATABASE_URL=postgresql://user:pass@host/db     # Database connection string

# Required: Template Discovery
TASKER_TEMPLATE_PATH=/path/to/templates         # Explicit template path
WORKSPACE_PATH=/app                             # Workspace root for auto-discovery

# Optional: Test Mode
TASKER_FORCE_EXAMPLE_HANDLERS=true              # Load example handlers in test

# Optional: Logging
LOG_LEVEL=debug                                 # Ruby log level (debug/info/warn/error)
RUST_LOG=info                                   # Rust log level (passed to Rust FFI)

# Optional: Production Tuning
RUBY_GC_HEAP_GROWTH_FACTOR=1.1                 # Ruby GC tuning for production
```

**Total Variables**: 9 environment variables

**Configuration Type**: Plain text .env file or shell export statements

---

## Part 2: TOML File Mapping to Categories

### 2.1 Base Configuration Files (16 files)

#### Common Configuration (Used by All Rust Components)

| File | Category | Usage | Fields Used | Remove? |
|------|----------|-------|-------------|---------|
| `base/database.toml` | Common | ✅ Yes | ~12 fields (pool, connection) | No, but remove `enable_secondary_database` |
| `base/queues.toml` | Common | ✅ Yes | ~10 fields (namespaces, orchestration queues) | No, but remove `rabbitmq` section |
| `base/system.toml` | Common | ✅ Yes | ~2 fields (environment detection) | No |
| `base/mpsc_channels.toml` (shared) | Common | ✅ Yes | ~3 fields (event_publisher, ffi) | No |
| `base/orchestration.toml` (web only) | Common | ✅ Yes | ~8 fields (API location for clients) | No, but extract web section |
| `base/circuit_breakers.toml` | Common | ⚠️ Maybe deprecated | ~5 fields | Maybe (needs verification) |

**Common Config Total**: ~40 fields from 6 files

#### Orchestration-Specific Configuration

| File | Category | Usage | Fields Used | Remove? |
|------|----------|-------|-------------|---------|
| `base/backoff.toml` | Orchestration | ✅ Yes | ~5 fields (retry logic) | No |
| `base/orchestration.toml` (non-web) | Orchestration | ✅ Yes | ~10 fields (mode, operational state) | No, but split from web section |
| `base/event_systems.toml` (orch+task_readiness) | Orchestration | ✅ Yes | ~40 fields (orchestration + task_readiness) | No |
| `base/mpsc_channels.toml` (orch+task_readiness) | Orchestration | ✅ Yes | ~6 fields (orchestration + task_readiness channels) | No |
| `base/task_readiness.toml` | Orchestration | ❓ Unknown | Need to check if exists | Check if used |
| `base/query_cache.toml` | Orchestration | ❓ Unknown | Need analysis | Check if used |

**Orchestration Config Total**: ~61 fields from 4-6 files

#### Rust Worker-Specific Configuration

| File | Category | Usage | Fields Used | Remove? |
|------|----------|-------|-------------|---------|
| `base/worker.toml` | Rust Worker | ✅ Yes | ~40 fields (worker system + web API) | No |
| `base/event_systems.toml` (worker) | Rust Worker | ✅ Yes | ~25 fields (worker event system) | No |
| `base/mpsc_channels.toml` (worker) | Rust Worker | ✅ Yes | ~5 fields (worker channels) | No |

**Rust Worker Config Total**: ~70 fields from 3 files

#### Unused Configuration (Candidates for Removal)

| File | Category | Usage | Fields Used | Remove? |
|------|----------|-------|-------------|---------|
| `base/telemetry.toml` | ❌ Unused | 🚫 No | 0 fields | ✅ Yes |
| `base/task_templates.toml` | ❌ Unused | 🚫 No | 0 fields (database-driven) | ✅ Yes |
| `base/engine.toml` | ❌ Unused | 🚫 No | 0 fields (doesn't exist) | ✅ Yes |
| `base/state_machine.toml` | ❌ Unused | 🚫 No | 0 fields (documentation only?) | ✅ Yes |
| `base/execution.toml` | ❌ Test-only | 🚫 No | 1 field (max_concurrent_tasks, test-only) | ✅ Yes |

**Unused Files Total**: 5 files for removal (~20-30 fields)

---

### 2.2 Environment Override Files (29 files)

#### Test Environment (13 files)

| File | Category | Keep? | Reason |
|------|----------|-------|--------|
| `test/database.toml` | Common | ✅ Yes | Test database settings (smaller pools) |
| `test/queues.toml` | Common | ✅ Yes | Test queue settings (smaller batches) |
| `test/system.toml` | Common | ✅ Yes | Test environment detection |
| `test/mpsc_channels.toml` | Common/Orch/Worker | ✅ Yes | Small buffers for testing (100-500) |
| `test/backoff.toml` | Orchestration | ✅ Yes | Faster retries for testing |
| `test/orchestration.toml` | Orchestration | ✅ Yes | Test orchestration settings |
| `test/event_systems.toml` | Orchestration/Worker | ✅ Yes | Test event system settings |
| `test/task_readiness.toml` | Orchestration | ✅ Yes | Test task readiness settings |
| `test/worker.toml` | Rust Worker | ✅ Yes | Test worker settings |
| `test/execution.toml` | ❌ Unused | 🚫 Remove | Test-only execution config |
| `test/state_machine.toml` | ❌ Unused | 🚫 Remove | Documentation only |
| `test/telemetry.toml` | ❌ Unused | 🚫 Remove | Unused |

**Test Overrides to Keep**: 9 files

#### Development Environment (9 files)

| File | Category | Keep? | Reason |
|------|----------|-------|--------|
| `development/database.toml` | Common | ✅ Yes | Development database settings |
| `development/queues.toml` | Common | ✅ Yes | Development queue settings |
| `development/system.toml` | Common | ✅ Yes | Development environment |
| `development/mpsc_channels.toml` | Common/Orch/Worker | ✅ Yes | Medium buffers (500-1000) |
| `development/orchestration.toml` | Orchestration | ✅ Yes | Development orchestration settings |
| `development/event_systems.toml` | Orchestration/Worker | ✅ Yes | Development event system settings |
| `development/task_readiness.toml` | Orchestration | ✅ Yes | Development task readiness settings |
| `development/worker.toml` | Rust Worker | ✅ Yes | Development worker settings |
| `development/telemetry.toml` | ❌ Unused | 🚫 Remove | Unused |

**Development Overrides to Keep**: 8 files

#### Production Environment (8 files)

| File | Category | Keep? | Reason |
|------|----------|-------|--------|
| `production/database.toml` | Common | ✅ Yes | Production database settings (large pools) |
| `production/queues.toml` | Common | ✅ Yes | Production queue settings (large batches) |
| `production/mpsc_channels.toml` | Common/Orch/Worker | ✅ Yes | Large buffers (2000-50000) |
| `production/orchestration.toml` | Orchestration | ✅ Yes | Production orchestration settings |
| `production/task_readiness.toml` | Orchestration | ✅ Yes | Production task readiness settings |
| `production/worker.toml` | Rust Worker | ✅ Yes | Production worker settings |
| `production/execution.toml` | ❌ Unused | 🚫 Remove | Unused execution config |
| `production/telemetry.toml` | ❌ Unused | 🚫 Remove | Unused |

**Production Overrides to Keep**: 6 files

---

### 2.3 Summary: Files to Remove

**Base Files to Remove** (5 files):
- `base/telemetry.toml`
- `base/task_templates.toml`
- `base/engine.toml`
- `base/state_machine.toml`
- `base/execution.toml`

**Test Overrides to Remove** (3 files):
- `test/execution.toml`
- `test/state_machine.toml`
- `test/telemetry.toml`

**Development Overrides to Remove** (1 file):
- `development/telemetry.toml`

**Production Overrides to Remove** (2 files):
- `production/execution.toml`
- `production/telemetry.toml`

**Total Files to Remove**: 11 files out of 45 (24% reduction)

**Files Remaining**: 34 files

---

## Part 3: Configuration Loading Strategy

### 3.1 Orchestration Deployment

**Load**: CommonConfig + OrchestrationConfig

**TOML Files Loaded**:
```
Base (Common):
- database.toml (excluding enable_secondary_database)
- queues.toml (excluding rabbitmq)
- system.toml
- orchestration.toml (web section only)
- mpsc_channels.toml (shared section)
- circuit_breakers.toml (if not deprecated)

Base (Orchestration):
- backoff.toml
- orchestration.toml (non-web sections)
- event_systems.toml (orchestration + task_readiness)
- mpsc_channels.toml (orchestration + task_readiness)

Environment Override (test/development/production):
- {env}/database.toml
- {env}/queues.toml
- {env}/system.toml
- {env}/orchestration.toml
- {env}/event_systems.toml
- {env}/mpsc_channels.toml
- {env}/task_readiness.toml
- {env}/backoff.toml (test only)
```

**Total Configuration**: ~91 fields (Common: 40, Orchestration: 51)

**Efficiency**: 91 used / 91 loaded = **100% efficiency**

---

### 3.2 Rust Worker Deployment

**Load**: CommonConfig + RustWorkerConfig

**TOML Files Loaded**:
```
Base (Common):
- database.toml (excluding enable_secondary_database)
- queues.toml (excluding rabbitmq)
- system.toml
- orchestration.toml (web section only - for API client)
- mpsc_channels.toml (shared section)
- circuit_breakers.toml (if not deprecated)

Base (Rust Worker):
- worker.toml
- event_systems.toml (worker section only)
- mpsc_channels.toml (worker section only)

Environment Override (test/development/production):
- {env}/database.toml
- {env}/queues.toml
- {env}/system.toml
- {env}/orchestration.toml (web section for API client)
- {env}/mpsc_channels.toml
- {env}/event_systems.toml
- {env}/worker.toml
```

**Total Configuration**: ~110 fields (Common: 40, Rust Worker: 70)

**Efficiency**: 110 used / 110 loaded = **100% efficiency**

---

### 3.3 Ruby Worker Deployment

**Load**: Environment Variables (No TOML)

**Configuration Method**: Shell environment variables or .env file

```bash
# .env file or export statements
TASKER_ENV=development
RAILS_ENV=development
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_dev
TASKER_TEMPLATE_PATH=/path/to/templates
WORKSPACE_PATH=/app
TASKER_FORCE_EXAMPLE_HANDLERS=false
LOG_LEVEL=info
RUST_LOG=info
RUBY_GC_HEAP_GROWTH_FACTOR=1.1
```

**Total Configuration**: 9 environment variables

**Efficiency**: 9 used / 9 loaded = **100% efficiency**

---

### 3.4 Combined Deployment (Orchestration + Rust Worker)

**Load**: CommonConfig + OrchestrationConfig + RustWorkerConfig

**TOML Files Loaded**: All files from both orchestration and rust worker

**Total Configuration**: ~161 fields (Common: 40, Orchestration: 51, Rust Worker: 70)

**Use Case**: Single binary running both orchestration and worker (Docker development mode)

---

## Part 4: Configuration Categories Summary

### Current State (Monolithic TaskerConfig)

| Component | Fields Used | Fields Loaded | Efficiency | Waste |
|-----------|-------------|---------------|------------|-------|
| Orchestration | 50 | 80 | 62.5% | 30 fields (37.5%) |
| Rust Worker | 20 | 80 | 25% | 60 fields (75%) |
| Ruby Worker | 0 | 0 | N/A | Uses ENV instead |

**Total Waste**: 30-60 fields loaded but unused per deployment

---

### Proposed State (Context-Specific Configs)

| Deployment | Config Loaded | Fields Loaded | Fields Used | Efficiency | Improvement |
|------------|---------------|---------------|-------------|------------|-------------|
| Orchestration | Common + Orchestration | 91 | 91 | 100% | +37.5% |
| Rust Worker | Common + RustWorker | 110 | 110 | 100% | +75% |
| Ruby Worker | Environment Variables | 9 | 9 | 100% | N/A |
| Combined | All Three | 161 | 161 | 100% | +100% |

**Total Waste**: 0 fields loaded but unused

**File Reduction**: 45 → 34 files (24% reduction)

**Field Reduction**: ~80 → ~110 total unique fields (but context-specific loading)

---

## Part 5: Deprecation Candidates

### 5.1 Confirmed Unused (Remove Immediately)

1. **telemetry.*** - No usage found in any component
2. **task_templates.*** - Database-driven, not config-driven
3. **engine.*** - User confirmed doesn't exist anymore
4. **state_machine.*** - Possibly documentation only
5. **execution.max_concurrent_tasks** - Test-only assertion
6. **database.enable_secondary_database** - Test-only, always false
7. **queues.backend** - Always "pgmq", no abstraction used
8. **queues.rabbitmq.*** - Future feature, not implemented

**Estimated Removal**: ~20-30 fields

---

### 5.2 Needs Verification (Investigate Before Removing)

1. **circuit_breakers.*** - Code comment suggests deprecated (sqlx handles this)
   - **Action**: Verify CircuitBreakerManager usage in SystemContext
   - **Location**: `tasker-shared/src/system_context.rs:147`

2. **execution.max_concurrent_steps** - Not found in analysis
   - **Action**: Verify if worker uses this field
   - **Location**: Check worker step processing configuration

3. **query_cache.*** - Not analyzed yet
   - **Action**: Check if query caching is implemented
   - **Location**: Search for query_cache references

4. **task_readiness.*** (separate file) - May be redundant with event_systems
   - **Action**: Check if separate task_readiness.toml exists and is used
   - **Location**: Check if distinct from event_systems.task_readiness

**Estimated Potential Removal**: ~10-15 additional fields

---

## Part 6: Migration Path

### Phase 1: Add Context-Specific Structs (Non-Breaking)

1. Add new structs to `tasker-shared/src/config/`:
   - `common.rs` - CommonConfig
   - `orchestration.rs` - OrchestrationConfig
   - `worker.rs` - RustWorkerConfig
   - `ruby_worker.rs` - RubyWorkerConfig (env var struct)

2. Add conversion traits:
   ```rust
   impl From<&TaskerConfig> for CommonConfig { ... }
   impl From<&TaskerConfig> for OrchestrationConfig { ... }
   impl From<&TaskerConfig> for RustWorkerConfig { ... }
   ```

3. Keep existing TaskerConfig for backward compatibility

**Result**: New context-specific configs available, existing code unchanged

---

### Phase 2: Remove Unused Fields (Non-Breaking)

1. Remove unused TOML files (11 files)
2. Remove unused fields from Rust structs
3. Add deprecation warnings to removed fields

**Result**: Cleaner configuration, existing deployments unaffected

---

### Phase 3: Migrate to Context-Specific Loading (Breaking)

1. Update ConfigManager to support context parameter:
   ```rust
   pub enum ConfigContext {
       Orchestration,
       RustWorker,
       RubyWorker,
       Combined,
   }

   ConfigManager::load_for_context(ConfigContext::Orchestration)?;
   ```

2. Update bootstrap systems:
   - `OrchestrationCore::bootstrap()` → load CommonConfig + OrchestrationConfig
   - `WorkerBootstrap::bootstrap()` → load CommonConfig + RustWorkerConfig
   - Ruby bootstrap → generate environment variables

3. Remove TaskerConfig (breaking change)

**Result**: Context-specific configuration, 100% efficiency

---

### Phase 4: CLI Tool Development

1. **TOML Generator**: Generate context-specific tasker.toml
2. **ENV Generator**: Generate .env for Ruby workers
3. **Validator**: Validate context-specific configurations
4. **Interactive Wizard**: Guide users through configuration

**Result**: Easy configuration generation and validation

---

## Next Steps

1. ✅ Configuration usage analysis (Steps 1.1-1.4) - **COMPLETE**
2. ✅ Formal category definitions (Step 2.1) - **COMPLETE**
3. ✅ TOML file mapping (Step 2.2) - **COMPLETE**
4. ⏳ Create comprehensive implementation plan (Step 3)
   - Configuration redesign plan
   - Migration plan (phased approach)
   - CLI design plan
   - Testing and validation strategy

---

**Document Status**: Ready for implementation planning phase
