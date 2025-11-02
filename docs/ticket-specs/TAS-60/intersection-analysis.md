# TAS-60 Intersection Analysis: Configuration Hydration and Usage Patterns

**Created**: 2025-10-31
**Status**: Complete - Ready for Task 5 (Redesign Proposal)
**Dependencies**:
- Task 1: [tasker-config-struct-analysis.md](./tasker-config-struct-analysis.md)
- Task 2: [toml-structure-analysis.md](./toml-structure-analysis.md)
- Task 3: [unified-loader-pattern-analysis.md](./unified-loader-pattern-analysis.md)

---

## Executive Summary

This document maps the intersection between TOML configuration files and Rust struct hydration, revealing critical insights into actual configuration usage versus declared configuration. The analysis uncovers:

1. **Configuration Sprawl**: 574-line TaskReadinessConfig struct is entirely unused in runtime code
2. **"Common" Configuration**: Only 4 of 14 TaskerConfig fields actually used by SystemContext
3. **Legacy Artifacts**: Multiple structs and fields remain after migrations (TAS-51, event system refactoring)
4. **Field Mismatches**: 5 required fields missing from TOML, multiple extraneous TOML fields ignored
5. **Inconsistent Naming**: TOML tables use different names than struct fields

**Key User Insight Confirmed**: "task readiness is actually now modeled in code within the event system framework, so we seem to have sprawl" - TaskReadinessConfig is legacy, replaced by EventSystemConfig.

---

## Part 1: SystemContext Dependencies - The Real "Common" Configuration

### SystemContext Structure
```rust
pub struct SystemContext {
    pub processor_uuid: Uuid,                                      // Generated at runtime
    pub tasker_config: Arc<TaskerConfig>,                          // Full config (but what's accessed?)
    pub message_client: Arc<UnifiedPgmqClient>,                    // From database + queues
    pub database_pool: PgPool,                                     // From database
    pub task_handler_registry: Arc<TaskHandlerRegistry>,           // From database
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>, // From circuit_breakers
    pub event_publisher: Arc<EventPublisher>,                      // From mpsc_channels.shared
}
```

### SystemContext Configuration Usage

#### 1. Database Configuration
**File**: `tasker-shared/src/system_context.rs:336-372`

```rust
// USED: Full database pool configuration
let database_url = config.database_url();
let database_pool = Self::get_pg_pool_options(config)
    .connect(&database_url)
    .await?;

fn get_pg_pool_options(config: &TaskerConfig) -> sqlx::postgres::PgPoolOptions {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(config.database.pool.max_connections)          // ✅ USED
        .min_connections(config.database.pool.min_connections)          // ✅ USED
        .acquire_timeout(Duration::from_secs(
            config.database.pool.acquire_timeout_seconds                // ✅ USED
        ))
        .idle_timeout(Some(Duration::from_secs(
            config.database.pool.idle_timeout_seconds                   // ✅ USED
        )))
        .max_lifetime(Some(Duration::from_secs(
            config.database.pool.max_lifetime_seconds                   // ✅ USED
        )))
}
```

**TOML Location**: `config/tasker/base/common.toml` lines 4-17
```toml
[database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"
encoding = "unicode"      # ❌ EXTRANEOUS - not in DatabaseConfig
host = "localhost"        # ❌ EXTRANEOUS - not in DatabaseConfig
checkout_timeout = 10     # ❌ EXTRANEOUS - not in DatabaseConfig
reaping_frequency = 10    # ❌ EXTRANEOUS - not in DatabaseConfig

[database.pool]
max_connections = 30      # ✅ USED by SystemContext
min_connections = 8       # ✅ USED by SystemContext
acquire_timeout_seconds = 30   # ✅ USED by SystemContext
idle_timeout_seconds = 300     # ✅ USED by SystemContext
max_lifetime_seconds = 3600    # ✅ USED by SystemContext

[database.variables]
statement_timeout = 5000  # ❓ UNCLEAR - may be used by query execution
```

**Analysis**: Database configuration heavily used, but has 4 extraneous fields that are silently ignored during deserialization.

#### 2. Circuit Breaker Configuration
**File**: `tasker-shared/src/system_context.rs:379-394`

```rust
// USED: Circuit breaker enablement check
let circuit_breaker_config = if config.circuit_breakers.enabled {   // ✅ USED
    Some(config.circuit_breakers.clone())                            // ✅ USED (entire struct)
} else {
    None
};

let circuit_breaker_manager = if let Some(cb_config) = circuit_breaker_config {
    Some(Arc::new(CircuitBreakerManager::from_config(&cb_config)))   // ✅ USED
} else {
    None
};
```

**TOML Location**: `config/tasker/base/common.toml` lines 158-192
```toml
[circuit_breakers]
enabled = true            # ✅ USED by SystemContext

[circuit_breakers.global_settings]
max_circuit_breakers = 50                      # ✅ USED
metrics_collection_interval_seconds = 30       # ✅ USED
min_state_transition_interval_seconds = 1      # ✅ USED

[circuit_breakers.default_config]
failure_threshold = 5     # ✅ USED
timeout_seconds = 30      # ✅ USED
success_threshold = 2     # ✅ USED

[circuit_breakers.component_configs.pgmq]
# ... component-specific overrides ✅ USED
```

**Analysis**: Circuit breaker configuration fully used when enabled.

#### 3. MPSC Channels (Shared) Configuration
**File**: `tasker-shared/src/system_context.rs:424-429`

```rust
// USED: Event publisher channel capacity (TAS-51)
let event_publisher_buffer_size = config
    .mpsc_channels
    .shared
    .event_publisher
    .event_queue_buffer_size;                                        // ✅ USED
let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));
```

**TOML Location**: `config/tasker/base/common.toml` lines 126-138
```toml
[mpsc_channels.shared.event_publisher]
event_queue_buffer_size = 5000                 # ✅ USED by SystemContext

[mpsc_channels.shared.ffi]
ruby_event_buffer_size = 1000                  # ✅ USED by Ruby FFI bootstrap

[mpsc_channels.overflow_policy]
log_warning_threshold = 0.8                    # ✅ USED by ChannelMonitor
drop_policy = "block"                          # ✅ USED for backpressure

[mpsc_channels.overflow_policy.metrics]
enabled = true                                 # ✅ USED
saturation_check_interval_seconds = 60         # ✅ USED
```

**Analysis**: TAS-51 migration successful - all mpsc_channels.shared fields actively used.

#### 4. Queues Configuration
**File**: `tasker-shared/src/system_context.rs:465-473`

```rust
// USED: Queue naming and orchestration queue definitions
let queue_config = self.tasker_config.queues.clone();                // ✅ USED (entire struct)

let orchestration_owned_queues = vec![
    queue_config.orchestration_queues.step_results.as_str(),         // ✅ USED
    queue_config.orchestration_queues.task_requests.as_str(),        // ✅ USED
    queue_config.orchestration_queues.task_finalizations.as_str(),   // ✅ USED
];
```

**TOML Location**: `config/tasker/base/common.toml` lines 61-87
```toml
[queues]
backend = "pgmq"                               # ✅ USED
orchestration_namespace = "orchestration"      # ✅ USED
worker_namespace = "worker"                    # ✅ USED
default_namespace = "default"                  # ❌ EXTRANEOUS - not in QueuesConfig
naming_pattern = "{namespace}_{name}_queue"    # ✅ USED
health_check_interval = 60                     # ✅ USED
default_batch_size = 10                        # ✅ USED
max_batch_size = 100                           # ✅ USED
default_visibility_timeout_seconds = 30        # ✅ USED

[queues.orchestration_queues]
task_requests = "orchestration_task_requests_queue"           # ✅ USED by SystemContext
task_finalizations = "orchestration_task_finalizations_queue" # ✅ USED by SystemContext
step_results = "orchestration_step_results_queue"             # ✅ USED by SystemContext

[queues.pgmq]
poll_interval_ms = 250                         # ✅ USED
shutdown_timeout_seconds = 5                   # ✅ USED
max_retries = 3                                # ✅ USED
```

**Analysis**: Queues configuration heavily used, with 1 extraneous field (`default_namespace`).

### Summary: SystemContext "Common" Configuration

**ACTUALLY USED BY SYSTEMCONTEXT** (4 of 14 TaskerConfig fields):
1. ✅ `database` - Full database pool configuration
2. ✅ `circuit_breakers` - Entire struct when enabled
3. ✅ `mpsc_channels.shared` - Event publisher and FFI buffer sizes
4. ✅ `queues` - Backend, namespaces, and orchestration queue names

**NOT USED BY SYSTEMCONTEXT** (10 fields):
- ❌ `telemetry` - Missing from TOML entirely
- ❌ `task_templates` - Missing from TOML entirely
- ❌ `system` - Missing from TOML entirely
- ❌ `execution` - Missing from TOML entirely (contains TASKER_ENV!)
- ❌ `decision_points` - Separate file, not merged
- ❌ `backoff` - Used by orchestration, not SystemContext
- ❌ `orchestration` - Used by orchestration bootstrap, not SystemContext
- ❌ `task_readiness` - LEGACY, replaced by event_systems.task_readiness
- ❌ `event_systems` - Used by event coordinators, not SystemContext
- ❌ `worker` - Optional, used by worker bootstrap, not SystemContext

**Extraneous TOML Fields** (in TOML but not in structs):
- `database.encoding`, `database.host`, `database.checkout_timeout`, `database.reaping_frequency`
- `queues.default_namespace`

---

## Part 2: Configuration Sprawl Analysis - TaskReadinessConfig

### The Legacy TaskReadinessConfig

**File**: `tasker-shared/src/config/task_readiness.rs`
**Size**: 574 lines, 15 structs, 4 levels deep
**Usage in Runtime Code**: **ZERO**

```rust
pub struct TaskReadinessConfig {
    pub enabled: bool,                                              // ❌ NOT USED
    pub event_system: Option<TaskReadinessEventSystemConfig>,       // ❌ NOT USED (replaced by event_systems.task_readiness)
    pub enhanced_settings: EnhancedCoordinatorSettings,             // ❌ NOT USED
    pub notification: TaskReadinessNotificationConfig,              // ❌ NOT USED
    pub fallback_polling: ReadinessFallbackConfig,                  // ❌ NOT USED
    pub event_channel: EventChannelConfig,                          // ❌ NOT USED (moved to mpsc_channels.task_readiness)
    pub coordinator: TaskReadinessCoordinatorConfig,                // ❌ NOT USED
    pub error_handling: ErrorHandlingConfig,                        // ❌ NOT USED
}
```

### Evidence of Non-Usage

#### Grep Search Results
```bash
$ rg "\.task_readiness\." --type rust

# ONLY FOUND:
tasker-shared/src/config/mpsc_channels.rs:359:
    assert_eq!(config.task_readiness.event_channel.buffer_size, 1000);  # TEST ONLY

tasker-shared/src/config/task_readiness.rs:133:
    /// Access via: config.mpsc_channels.task_readiness.event_channel    # COMMENT ONLY

tasker-shared/src/config/tasker.rs:105:
    pub task_readiness: TaskReadinessConfig,                            # STRUCT DEFINITION

tasker-shared/src/config/tasker.rs:520:
    task_readiness: TaskReadinessConfig::default(),                     # DEFAULT ONLY
```

**NO RUNTIME CODE ACCESSES `config.task_readiness.*`**

#### What Code Actually Uses

**File**: `tasker-orchestration/src/orchestration/bootstrap.rs:264`
```rust
// NOT config.task_readiness - uses event_systems.task_readiness!
let task_readiness_config = tasker_config.event_systems.task_readiness.clone();  // ✅ ACTUALLY USED
```

**File**: `tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:82`
```rust
// NOT config.task_readiness - uses event_systems.task_readiness!
let task_readiness_config = config.event_systems.task_readiness.clone();         // ✅ ACTUALLY USED
```

### EventSystemConfig vs TaskReadinessConfig

**ACTUALLY USED** - `event_systems.task_readiness`:
```rust
pub struct EventSystemConfig<TaskReadinessEventSystemMetadata> {
    pub system_id: String,                                          // ✅ USED
    pub deployment_mode: DeploymentMode,                            // ✅ USED
    pub timing: EventSystemTimingConfig,                            // ✅ USED
    pub processing: EventSystemProcessingConfig,                    // ✅ USED
    pub health: EventSystemHealthConfig,                            // ✅ USED
    pub metadata: TaskReadinessEventSystemMetadata,                 // ✅ USED (placeholder)
}
```

**TOML Location**: `config/tasker/base/orchestration.toml` lines 293-320
```toml
[task_readiness_events]          # NOTE: Different name than struct!
system_id = "task-readiness-event-system"           # ✅ USED
deployment_mode = "Hybrid"                          # ✅ USED

[task_readiness_events.timing]
health_check_interval_seconds = 30                  # ✅ USED
fallback_polling_interval_seconds = 5               # ✅ USED
visibility_timeout_seconds = 30                     # ✅ USED
processing_timeout_seconds = 30                     # ✅ USED
claim_timeout_seconds = 300                         # ✅ USED

[task_readiness_events.processing]
batch_size = 50                                     # ✅ USED
max_concurrent_operations = 100                     # ✅ USED
max_retries = 3                                     # ✅ USED

[task_readiness_events.health]
enabled = true                                      # ✅ USED
performance_monitoring_enabled = true               # ✅ USED
max_consecutive_errors = 10                         # ✅ USED
error_rate_threshold_per_minute = 5                 # ✅ USED
```

### TAS-51 Migration Artifacts

**Old Location** (still in TaskReadinessConfig):
```rust
pub struct EventChannelConfig {
    pub buffer_size: usize,           // ❌ LEGACY - moved to mpsc_channels.task_readiness
    pub send_timeout_ms: u64,         // ❌ LEGACY - moved to mpsc_channels.task_readiness
}
```

**New Location** (TAS-51):
```rust
// tasker-shared/src/config/mpsc_channels.rs
pub struct TaskReadinessMpscConfig {
    pub event_channel: TaskReadinessEventChannelConfig,  // ✅ ACTUALLY USED
}

pub struct TaskReadinessEventChannelConfig {
    pub buffer_size: usize,           // ✅ USED by event coordination
    pub send_timeout_ms: u64,         // ✅ USED for backpressure
}
```

**TOML Location**: `config/tasker/base/orchestration.toml` (TAS-51)
```toml
[mpsc_channels.task_readiness.event_channel]
buffer_size = 1000                                  # ✅ USED (new location)
send_timeout_ms = 50                                # ✅ USED (new location)
```

### Configuration Sprawl Summary

| Subsection | Lines | Structs | TOML Populated? | Runtime Used? |
|-----------|-------|---------|-----------------|---------------|
| `event_system` | ~100 | 5 | ❌ No (moved to event_systems.task_readiness) | ❌ No |
| `enhanced_settings` | ~50 | 2 | ❌ No (defaults only) | ❌ No |
| `notification` | ~80 | 3 | ❌ No (defaults only) | ❌ No |
| `fallback_polling` | ~60 | 2 | ❌ No (defaults only) | ❌ No |
| `event_channel` | ~40 | 1 | ❌ No (moved to mpsc_channels) | ❌ No |
| `coordinator` | ~100 | 4 | ❌ No (defaults only) | ❌ No |
| `error_handling` | ~50 | 2 | ❌ No (defaults only) | ❌ No |
| **TOTAL** | **574** | **15** | **0 / 8 subsections** | **NONE** |

**Conclusion**: TaskReadinessConfig is 100% configuration sprawl - 574 lines of code with zero runtime usage.

---

## Part 3: TOML to Struct Hydration Mapping

### Successful Hydration Paths

#### 1. Database Configuration
```
TOML Path                          → Struct Path                           → Used By
--------------------------------------------------------------------------------------------------------
[database]                         → TaskerConfig.database                 → SystemContext ✅
[database.pool]                    → DatabaseConfig.pool                   → SystemContext ✅
[database.variables]               → DatabaseConfig.variables              → Query execution ❓
```

#### 2. Circuit Breakers Configuration
```
TOML Path                                → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[circuit_breakers]                       → TaskerConfig.circuit_breakers                 → SystemContext ✅
[circuit_breakers.global_settings]       → CircuitBreakerConfig.global_settings          → CircuitBreakerManager ✅
[circuit_breakers.default_config]        → CircuitBreakerConfig.default_config           → CircuitBreakerManager ✅
[circuit_breakers.component_configs.*]   → CircuitBreakerConfig.component_configs        → CircuitBreakerManager ✅
```

#### 3. MPSC Channels Configuration (TAS-51)
```
TOML Path                                          → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[mpsc_channels.shared.event_publisher]             → MpscChannelsConfig.shared.event_publisher     → SystemContext ✅
[mpsc_channels.shared.ffi]                         → MpscChannelsConfig.shared.ffi                 → Ruby FFI ✅
[mpsc_channels.orchestration.command_processor]    → MpscChannelsConfig.orchestration.*            → Orchestration ✅
[mpsc_channels.orchestration.event_listeners]      → MpscChannelsConfig.orchestration.*            → Orchestration ✅
[mpsc_channels.worker.command_processor]           → MpscChannelsConfig.worker.*                   → Worker ✅
[mpsc_channels.worker.event_listeners]             → MpscChannelsConfig.worker.*                   → Worker ✅
[mpsc_channels.task_readiness.event_channel]       → MpscChannelsConfig.task_readiness.*           → Event coordination ✅
[mpsc_channels.overflow_policy]                    → MpscChannelsConfig.overflow_policy            → ChannelMonitor ✅
```

#### 4. Queues Configuration
```
TOML Path                                → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[queues]                                 → TaskerConfig.queues                           → SystemContext ✅
[queues.orchestration_queues]            → QueuesConfig.orchestration_queues             → SystemContext ✅
[queues.pgmq]                            → QueuesConfig.pgmq                             → PGMQ client ✅
[queues.rabbitmq]                        → QueuesConfig.rabbitmq                         → Future RabbitMQ ⏳
```

#### 5. Event Systems Configuration
```
TOML Path                               → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[orchestration_events]                  → EventSystemsConfig.orchestration              → Orchestration bootstrap ✅
[orchestration_events.timing]           → EventSystemConfig.timing                      → Event coordinator ✅
[orchestration_events.processing]       → EventSystemConfig.processing                  → Event coordinator ✅
[orchestration_events.health]           → EventSystemConfig.health                      → Health monitor ✅

[task_readiness_events]                 → EventSystemsConfig.task_readiness             → Event coordinator ✅
[task_readiness_events.timing]          → EventSystemConfig.timing                      → Event coordinator ✅
[task_readiness_events.processing]      → EventSystemConfig.processing                  → Event coordinator ✅

[worker_events]                         → EventSystemsConfig.worker                     → Worker bootstrap ✅
[worker_events.metadata.*]              → WorkerEventSystemMetadata.*                   → Worker subsystems ✅
```

#### 6. Backoff Configuration
```
TOML Path                               → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[backoff]                               → TaskerConfig.backoff                          → Orchestration retry logic ✅
[backoff.default_backoff_seconds]       → BackoffConfig.default_backoff_seconds         → Retry calculation ✅
[backoff.reenqueue_delays]              → BackoffConfig.reenqueue_delays                → State-specific delays ✅
```

#### 7. Orchestration System Configuration
```
TOML Path                               → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[orchestration_system]                  → TaskerConfig.orchestration                    → Orchestration bootstrap ✅
[orchestration_system.web]              → OrchestrationConfig.web                       → Web API ✅
[orchestration_system.web.auth]         → WebConfig.auth                                → Auth middleware ✅
[orchestration_system.web.cors]         → WebConfig.cors                                → CORS middleware ✅
```

#### 8. Worker System Configuration
```
TOML Path                               → Struct Path                                    → Used By
--------------------------------------------------------------------------------------------------------
[worker_system]                         → TaskerConfig.worker                           → Worker bootstrap ✅
[worker_system.web]                     → WorkerConfig.web                              → Worker web API ✅
[worker_system.step_processing]         → WorkerConfig.step_processing                  → Step executor ✅
[worker_system.fallback_poller]         → WorkerConfig.fallback_poller                  → Fallback polling ✅
```

### Failed/Missing Hydration Paths

#### Missing Required Fields (Explains "missing field 'telemetry'" Error)

```
Struct Field                     → TOML Location              → Status
--------------------------------------------------------------------------------------------------------
TaskerConfig.telemetry           → [telemetry]                → ❌ MISSING FROM ALL TOML FILES
TaskerConfig.task_templates      → [task_templates]           → ❌ MISSING FROM ALL TOML FILES
TaskerConfig.system              → [system]                   → ❌ MISSING FROM ALL TOML FILES
TaskerConfig.execution           → [execution]                → ❌ MISSING FROM ALL TOML FILES (contains TASKER_ENV!)
TaskerConfig.decision_points     → decision_points.toml       → ⚠️ EXISTS IN SEPARATE FILE, NOT MERGED
```

**Impact**: Config generation from base files fails validation. Only works when using backward compatibility bridge with `..TaskerConfig::default()` (fills missing fields incorrectly).

#### Extraneous TOML Fields (Silently Ignored)

```
TOML Path                        → Expected Struct Field        → Status
--------------------------------------------------------------------------------------------------------
[database.encoding]              → DatabaseConfig.encoding      → ❌ DOES NOT EXIST (ignored)
[database.host]                  → DatabaseConfig.host          → ❌ DOES NOT EXIST (ignored)
[database.checkout_timeout]      → DatabaseConfig.???           → ❌ UNCLEAR PURPOSE (ignored)
[database.reaping_frequency]     → DatabaseConfig.???           → ❌ UNCLEAR PURPOSE (ignored)
[queues.default_namespace]       → QueuesConfig.default_namespace → ❌ DOES NOT EXIST (ignored)
```

**Impact**: Silently ignored during deserialization. May be legacy from PostgreSQL adapter configuration or Rails conventions.

#### Naming Inconsistencies

```
TOML Table Name                  → Struct Field Name            → Mismatch
--------------------------------------------------------------------------------------------------------
[orchestration_system]           → TaskerConfig.orchestration   → Different name
[worker_system]                  → TaskerConfig.worker          → Different name
[orchestration_events]           → event_systems.orchestration  → Different name
[task_readiness_events]          → event_systems.task_readiness → Different name
[worker_events]                  → event_systems.worker         → Different name
```

**Impact**: Requires custom deserialization logic in ConfigManager to map between names.

---

## Part 4: Context-Specific Configuration Usage

### Orchestration Context Configuration

**Bootstrap File**: `tasker-orchestration/src/orchestration/bootstrap.rs`

#### Actually Used Configuration Sections
```rust
// 1. Event Systems (lines 264-278)
let task_readiness_config = tasker_config.event_systems.task_readiness.clone();  // ✅ USED
let orchestration_config = tasker_config.event_systems.orchestration.clone();    // ✅ USED

// 2. SystemContext (creates from full TaskerConfig)
let system_context = SystemContext::from_config(config_manager).await?;           // ✅ USED
// Internally uses: database, circuit_breakers, mpsc_channels.shared, queues

// 3. MPSC Channels - Orchestration specific
let command_buffer = tasker_config.mpsc_channels.orchestration.command_processor.command_buffer_size;  // ✅ USED

// 4. Backoff Configuration
let backoff_config = tasker_config.backoff.clone();                              // ✅ USED

// 5. Orchestration System Settings
let web_enabled = tasker_config.orchestration.web.enabled;                       // ✅ USED
```

#### Orchestration Context Summary
```
USED:
  ✅ database (via SystemContext)
  ✅ circuit_breakers (via SystemContext)
  ✅ mpsc_channels.shared (via SystemContext)
  ✅ mpsc_channels.orchestration (direct)
  ✅ queues (via SystemContext)
  ✅ event_systems.orchestration (direct)
  ✅ event_systems.task_readiness (direct)
  ✅ backoff (direct)
  ✅ orchestration (direct)

NOT USED:
  ❌ telemetry (missing)
  ❌ task_templates (missing)
  ❌ system (missing)
  ❌ execution (missing)
  ❌ decision_points (separate file)
  ❌ task_readiness (legacy, replaced by event_systems)
  ❌ worker (wrong context)
  ❌ mpsc_channels.worker (wrong context)
  ❌ event_systems.worker (wrong context)
```

### Worker Context Configuration

**Bootstrap File**: `tasker-worker/src/bootstrap.rs`

#### Actually Used Configuration Sections
```rust
// 1. Event Systems
let worker_event_config = tasker_config.event_systems.worker.clone();            // ✅ USED

// 2. SystemContext (creates from full TaskerConfig)
let system_context = SystemContext::from_config(config_manager).await?;           // ✅ USED
// Internally uses: database, circuit_breakers, mpsc_channels.shared, queues

// 3. MPSC Channels - Worker specific
let command_buffer = tasker_config.mpsc_channels.worker.command_processor.command_buffer_size;  // ✅ USED

// 4. Worker System Settings
let worker_id = tasker_config.worker.worker_id.clone();                          // ✅ USED
let max_concurrent_steps = tasker_config.worker.step_processing.max_concurrent_steps;  // ✅ USED
```

#### Worker Context Summary
```
USED:
  ✅ database (via SystemContext)
  ✅ circuit_breakers (via SystemContext)
  ✅ mpsc_channels.shared (via SystemContext)
  ✅ mpsc_channels.worker (direct)
  ✅ queues (via SystemContext)
  ✅ event_systems.worker (direct)
  ✅ worker (direct)

NOT USED:
  ❌ telemetry (missing)
  ❌ task_templates (missing)
  ❌ system (missing)
  ❌ execution (missing)
  ❌ decision_points (separate file)
  ❌ task_readiness (legacy)
  ❌ backoff (orchestration-only)
  ❌ orchestration (wrong context)
  ❌ mpsc_channels.orchestration (wrong context)
  ❌ event_systems.orchestration (wrong context)
  ❌ event_systems.task_readiness (orchestration-only)
```

---

## Part 5: Recommendations for Task 5 (Redesign Proposal)

### Critical Issues Requiring Resolution

#### 1. Configuration Sprawl - Immediate Removal Candidates

**TaskReadinessConfig** - 574 lines, ZERO runtime usage:
```rust
// REMOVE ENTIRELY - Replaced by event_systems.task_readiness
pub struct TaskReadinessConfig { /* ... */ }  // ❌ DELETE
```

**Extraneous Database Fields** - Silently ignored:
```toml
# REMOVE from common.toml
[database]
encoding = "unicode"      # ❌ REMOVE
host = "localhost"        # ❌ REMOVE
checkout_timeout = 10     # ❌ REMOVE
reaping_frequency = 10    # ❌ REMOVE
```

**Legacy Event Channel Config** - Moved to mpsc_channels:
```rust
// REMOVE from TaskReadinessConfig if TaskReadinessConfig is kept
pub struct EventChannelConfig { /* ... */ }  // ❌ DELETE (moved to mpsc_channels)
```

#### 2. Missing Required Fields - Add to Common Config

```toml
# ADD to config/tasker/base/common.toml

[telemetry]
enabled = true
# ... (define TelemetryConfig structure)

[system]
# ... (define SystemConfig structure)

[execution]
environment = "${TASKER_ENV:-development}"  # CRITICAL - currently missing!
# ... (other execution settings)

[task_templates]
# ... (define TaskTemplatesConfig structure)
```

#### 3. Shallow Merge Bug - Copy Deep Merge to UnifiedConfigLoader

```rust
// FIX: tasker-shared/src/config/unified_loader.rs
fn merge_toml(&self, base: &mut toml::Value, overlay: toml::Value) -> ConfigResult<()> {
    // REPLACE shallow merge with ConfigMerger's deep_merge_tables logic
    // Copy implementation from merger.rs:212-251
}
```

#### 4. Naming Inconsistencies - Align TOML with Structs

**Option A**: Change struct fields to match TOML (less breaking):
```rust
pub struct TaskerConfig {
    pub orchestration_system: OrchestrationConfig,  // Rename from "orchestration"
    pub worker_system: Option<WorkerConfig>,        // Rename from "worker"
}
```

**Option B**: Change TOML tables to match structs (cleaner):
```toml
# Rename in base files
[orchestration]         # Instead of [orchestration_system]
[worker]                # Instead of [worker_system]
```

### Ideal State Architecture Alignment

Based on user's proposed structure: `{ common, orchestration, worker }`

#### Current Problems with Monolithic TaskerConfig

1. **14 required fields** but only **4 used by SystemContext**
2. **Context confusion** - orchestration/worker configs mixed in single struct
3. **Optional worker** but required orchestration (asymmetric)
4. **Backward compatibility bridge** uses `..Default::default()` (incorrect)

#### Proposed Ideal State

```rust
pub struct TaskerConfig {
    /// Configuration used by SystemContext and shared across all components
    pub common: CommonConfig,

    /// Orchestration-specific configuration (optional - only for orchestration context)
    pub orchestration: Option<OrchestrationConfig>,

    /// Worker-specific configuration (optional - only for worker context)
    pub worker: Option<WorkerConfig>,
}

pub struct CommonConfig {
    pub environment: String,                      // From execution.environment
    pub database: DatabaseConfig,                 // ✅ Used by SystemContext
    pub circuit_breakers: CircuitBreakerConfig,   // ✅ Used by SystemContext
    pub mpsc_channels: SharedMpscConfig,          // ✅ Used by SystemContext (shared only)
    pub queues: QueuesConfig,                     // ✅ Used by SystemContext
    pub telemetry: TelemetryConfig,               // Add (currently missing)
    pub system: SystemConfig,                     // Add (currently missing)
}

pub struct OrchestrationConfig {
    pub backoff: BackoffConfig,
    pub orchestration_system: OrchestrationSystemConfig,
    pub mpsc_channels: OrchestrationMpscConfig,
    pub event_systems: OrchestrationEventSystemsConfig,  // orchestration + task_readiness
    pub staleness_detection: StalenessDetectionConfig,
    pub dlq: DlqConfig,
    pub archive: ArchivalConfig,
}

pub struct WorkerConfig {
    pub worker_system: WorkerSystemConfig,
    pub mpsc_channels: WorkerMpscConfig,
    pub event_systems: WorkerEventSystemsConfig,
}
```

#### TOML Structure Alignment

```toml
# config/tasker/base/common.toml
environment = "development"

[database]
# ...

[circuit_breakers]
# ...

[mpsc_channels.shared]
# Only shared channels - NOT orchestration/worker

[queues]
# ...

[telemetry]
# Add missing config

[system]
# Add missing config


# config/tasker/base/orchestration.toml
[orchestration.backoff]
# ...

[orchestration.orchestration_system]
# ...

[orchestration.mpsc_channels]
# Only orchestration channels

[orchestration.event_systems.orchestration]
# ...

[orchestration.event_systems.task_readiness]
# ...


# config/tasker/base/worker.toml
[worker.worker_system]
# ...

[worker.mpsc_channels]
# Only worker channels

[worker.event_systems.worker]
# ...
```

### Migration Path

#### Phase 1: Immediate Fixes (TAS-60 Completion)
1. Fix shallow merge bug (copy deep merge to UnifiedConfigLoader)
2. Add missing required fields (telemetry, system, execution, task_templates)
3. Remove extraneous TOML fields (database.encoding, etc.)
4. Merge decision_points.toml into config generation

#### Phase 2: Cleanup (TAS-61)
1. Remove TaskReadinessConfig entirely (574 lines)
2. Remove legacy EventChannelConfig
3. Consolidate merge logic (single implementation)
4. Fix naming inconsistencies (TOML vs struct names)

#### Phase 3: Architectural Redesign (TAS-62)
1. Adopt `{ common, orchestration, worker }` structure
2. Split TaskerConfig into context-specific configs
3. Make orchestration/worker truly optional
4. Remove backward compatibility bridge
5. Update all bootstrap code to use new structure

---

## Part 6: Detailed Field Usage Matrix

### Complete TaskerConfig Field Analysis

| Field | Size (LOC) | TOML Populated? | SystemContext Used? | Runtime Used? | Status |
|-------|-----------|-----------------|---------------------|---------------|--------|
| `database` | ~50 | ✅ Yes (common.toml) | ✅ Yes (pool config) | ✅ Yes | Keep |
| `telemetry` | ??? | ❌ No | ❌ No | ❓ Unknown | Add to TOML |
| `task_templates` | ??? | ❌ No | ❌ No | ❓ Unknown | Add to TOML |
| `system` | ??? | ❌ No | ❌ No | ❓ Unknown | Add to TOML |
| `backoff` | ~80 | ✅ Yes (orchestration.toml) | ❌ No | ✅ Yes (orchestration) | Keep (orchestration) |
| `execution` | ??? | ❌ No (has TASKER_ENV!) | ❌ No | ❓ Unknown | Add to TOML |
| `queues` | ~100 | ✅ Yes (common.toml) | ✅ Yes (orchestration_queues) | ✅ Yes | Keep |
| `orchestration` | ~200 | ✅ Yes (orchestration.toml) | ❌ No | ✅ Yes (bootstrap) | Keep (orchestration) |
| `circuit_breakers` | ~80 | ✅ Yes (common.toml) | ✅ Yes (manager creation) | ✅ Yes | Keep |
| `task_readiness` | 574 | ❌ No | ❌ No | ❌ NO | **DELETE** |
| `event_systems` | ~300 | ✅ Yes (all contexts) | ❌ No | ✅ Yes (coordinators) | Keep |
| `mpsc_channels` | ~400 | ✅ Yes (all contexts) | ✅ Yes (shared.event_publisher) | ✅ Yes | Keep |
| `decision_points` | ~50 | ⚠️ Separate file | ❌ No | ❓ Unknown | Merge into generation |
| `worker` | ~200 | ✅ Yes (worker.toml) | ❌ No | ✅ Yes (bootstrap) | Keep (worker) |

### MPSC Channels Detailed Usage

| Channel Config | Buffer Size | SystemContext? | Runtime Used? | Context |
|---------------|-------------|----------------|---------------|---------|
| `shared.event_publisher.event_queue_buffer_size` | 5000 | ✅ Yes | ✅ Yes | All |
| `shared.ffi.ruby_event_buffer_size` | 1000 | ❌ No | ✅ Yes | Ruby FFI |
| `orchestration.command_processor.command_buffer_size` | 1000 | ❌ No | ✅ Yes | Orchestration |
| `orchestration.event_listeners.pgmq_event_buffer_size` | 10000 | ❌ No | ✅ Yes | Orchestration |
| `orchestration.event_systems.event_channel_buffer_size` | 1000 | ❌ No | ✅ Yes | Orchestration |
| `worker.command_processor.command_buffer_size` | 1000 | ❌ No | ✅ Yes | Worker |
| `worker.event_listeners.pgmq_event_buffer_size` | 1000 | ❌ No | ✅ Yes | Worker |
| `worker.event_subscribers.completion_buffer_size` | 1000 | ❌ No | ✅ Yes | Worker |
| `worker.event_subscribers.result_buffer_size` | 1000 | ❌ No | ✅ Yes | Worker |
| `worker.in_process_events.broadcast_buffer_size` | 1000 | ❌ No | ✅ Yes | Worker |
| `task_readiness.event_channel.buffer_size` | 1000 | ❌ No | ✅ Yes | Orchestration |
| `overflow_policy.log_warning_threshold` | 0.8 | ❌ No | ✅ Yes | All |

**Conclusion**: TAS-51 migration successful - all mpsc_channels fields actively used by appropriate contexts.

### Event Systems Detailed Usage

| Event System | Struct Used | TOML Table | Runtime Used? | Context |
|-------------|-------------|------------|---------------|---------|
| Orchestration | `EventSystemConfig<OrchestrationEventSystemMetadata>` | `[orchestration_events]` | ✅ Yes | Orchestration |
| Task Readiness | `EventSystemConfig<TaskReadinessEventSystemMetadata>` | `[task_readiness_events]` | ✅ Yes | Orchestration |
| Worker | `EventSystemConfig<WorkerEventSystemMetadata>` | `[worker_events]` | ✅ Yes | Worker |

**Conclusion**: Event system framework successful replacement for TaskReadinessConfig.

---

## Conclusions

### Key Findings Summary

1. **Configuration Sprawl Confirmed**: 574-line TaskReadinessConfig has zero runtime usage
2. **"Common" Configuration Identified**: Only 4 of 14 TaskerConfig fields used by SystemContext
3. **TAS-51 Success**: All mpsc_channels fields actively used, migration complete
4. **Event System Success**: EventSystemConfig replaced TaskReadinessConfig throughout codebase
5. **Missing Required Fields**: 5 TaskerConfig fields have no TOML representation (explains validation errors)
6. **Extraneous TOML Fields**: Multiple fields silently ignored (database.encoding, etc.)
7. **Shallow Merge Bug**: Root cause of "missing field 'orchestration' in 'mpsc_channels'" error

### Alignment with User's Vision

User's proposed structure: `{ common: ..., orchestration: ..., worker: ... }`

**✅ VALIDATED BY ANALYSIS**:
- SystemContext uses only 4 fields → defines "common"
- Orchestration and worker have distinct, non-overlapping configs
- Current monolithic TaskerConfig mixes concerns
- Optional contexts make semantic sense

### Ready for Task 5

This intersection analysis provides the foundation for Task 5's redesign proposal:
1. Clear mapping of which fields are actually used
2. Identification of legacy configuration to remove
3. Evidence supporting context-specific configuration split
4. Migration path from current to ideal state

---

**Next Step**: Task 5 - Develop proposal for semantically-near TOML representation aligned with `{ common, orchestration, worker }` ideal state.
