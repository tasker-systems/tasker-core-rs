# TOML Base File Structure Analysis

**Author**: Claude
**Date**: 2025-10-31
**Purpose**: TAS-60 Configuration Structure Analysis - Understanding the current TOML file organization to support semantically-near struct representation design

## Executive Summary

The TOML configuration is split into 4 base files (`common.toml`, `orchestration.toml`, `worker.toml`, `decision_points.toml`) organized by **deployment context** rather than struct hierarchy. When merged, all tables collapse to a **flat root structure** - but critically, **several required TaskerConfig fields are missing** from the TOML files entirely (`telemetry`, `task_templates`, `system`, `execution`). This mismatch between TOML representation and struct requirements is the root cause of TAS-60 validation failures.

## Base TOML Files Overview

### File Inventory

| File | Size | Purpose | Primary Tables |
|------|------|---------|----------------|
| `common.toml` | 7KB | Shared configuration across all contexts | `database`, `queues`, `mpsc_channels.shared`, `circuit_breakers` |
| `orchestration.toml` | 13KB | Orchestration-specific configuration | `backoff`, `orchestration_system`, `orchestration_events`, `staleness_detection`, `dlq`, `archive` |
| `worker.toml` | 11KB | Worker-specific configuration | `worker_system`, `worker_events`, `mpsc_channels.worker` |
| `decision_points.toml` | 6KB | Decision point configuration (TAS-53) | `decision_points` |

**Total**: 37KB across 4 files

## Detailed File Analysis

### 1. common.toml Structure

**Lines**: 193 (including documentation sections)

**Root-Level Keys**:
```toml
environment = "development"  # Top-level scalar (not in a table)
```

**TOML Tables** (15 total):

#### Database Configuration (Lines 4-88)
```toml
[database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"
encoding = "unicode"
host = "localhost"
checkout_timeout = 10
reaping_frequency = 10

[database.pool]
max_connections = 30
min_connections = 8
acquire_timeout_seconds = 30
idle_timeout_seconds = 300
max_lifetime_seconds = 3600

[database.pool._docs.max_connections]  # Documentation metadata (stripped during merge)
description = "Maximum number of concurrent database connections in the pool"
type = "u32"
# ... more documentation

[database.variables]
statement_timeout = 5000
```

**Depth**: 3 levels (`database.pool._docs.*`)
**Observations**:
- Uses environment variable expansion: `${DATABASE_URL:-fallback}`
- Documentation in `_docs` sections (stripped by ConfigMerger)
- Fields like `encoding`, `host`, `checkout_timeout`, `reaping_frequency` **DO NOT EXIST** in DatabaseConfig struct!

#### Queue Configuration (Lines 57-120)
```toml
[queues]
backend = "pgmq"
orchestration_namespace = "orchestration"
worker_namespace = "worker"
default_namespace = "default"  # NOT in QueuesConfig struct!
naming_pattern = "{namespace}_{name}_queue"
health_check_interval = 60
default_batch_size = 10
max_batch_size = 100
default_visibility_timeout_seconds = 30

[queues.orchestration_queues]
task_requests = "orchestration_task_requests_queue"
task_finalizations = "orchestration_task_finalizations_queue"
step_results = "orchestration_step_results_queue"

[queues.pgmq]
poll_interval_ms = 250
shutdown_timeout_seconds = 5
max_retries = 3

[queues.rabbitmq]
connection_timeout_seconds = 10
```

**Matches**: `QueuesConfig` struct (mostly)
**Mismatches**: `default_namespace` field exists in TOML but not in struct

#### Shared MPSC Channels (Lines 122-152)
```toml
[mpsc_channels.shared.event_publisher]
event_queue_buffer_size = 5000

[mpsc_channels.shared.ffi]
ruby_event_buffer_size = 1000

[mpsc_channels.overflow_policy]
log_warning_threshold = 0.8
drop_policy = "block"

[mpsc_channels.overflow_policy.metrics]
enabled = true
saturation_check_interval_seconds = 60
```

**Matches**: `MpscChannelsConfig.shared` and `MpscChannelsConfig.overflow_policy`
**TAS-51**: Successfully migrated structure

#### Circuit Breakers (Lines 154-193)
```toml
[circuit_breakers]
enabled = true

[circuit_breakers.global_settings]
max_circuit_breakers = 50
metrics_collection_interval_seconds = 30
min_state_transition_interval_seconds = 1

[circuit_breakers.default_config]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2

[circuit_breakers.component_configs.pgmq]
failure_threshold = 3
timeout_seconds = 15
success_threshold = 2

[circuit_breakers.component_configs.task_readiness]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2
```

**Matches**: `CircuitBreakerConfig` struct perfectly
**Pattern**: HashMap of named component overrides

### 2. orchestration.toml Structure

**Lines**: 352 (including documentation)

**Root-Level Keys**:
```toml
environment = "development"  # Duplicated from common.toml
```

**TOML Tables** (27 total):

#### Backoff Configuration (Lines 4-21)
```toml
[backoff]
default_backoff_seconds = [1, 2, 4, 8, 16, 32]
max_backoff_seconds = 60
backoff_multiplier = 2.0
jitter_enabled = true
jitter_max_percentage = 0.1

[backoff.reenqueue_delays]
initializing = 5
enqueuing_steps = 0
steps_in_process = 10
evaluating_results = 5
waiting_for_dependencies = 45
waiting_for_retry = 30
blocked_by_failures = 60
```

**Matches**: `BackoffConfig` struct perfectly
**Context**: This is for **task/step retry logic**, not event processing

#### Orchestration System Configuration (Lines 23-128)
```toml
[orchestration_system]
mode = "standalone"
enable_performance_logging = false

[orchestration_system.web]
enabled = true
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8080}"
request_timeout_ms = 30000

[orchestration_system.web.tls]
enabled = false
cert_path = ""
key_path = ""

[orchestration_system.web.database_pools]
web_api_pool_size = 10
web_api_max_connections = 15
web_api_connection_timeout_seconds = 30
web_api_idle_timeout_seconds = 600
max_total_connections_hint = 30

[orchestration_system.web.cors]
enabled = true
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"]
allowed_headers = ["*"]

[orchestration_system.web.auth]
enabled = false
jwt_issuer = "tasker-core"
jwt_audience = "tasker-api"
jwt_token_expiry_hours = 24
jwt_private_key = ""
jwt_public_key = ""
api_key = ""
api_key_header = "X-API-Key"

[orchestration_system.web.auth.protected_routes]
"DELETE /v1/tasks/{task_uuid}" = { auth_type = "bearer", required = true }
"POST /v1/tasks" = { auth_type = "bearer", required = false }
# ... more routes

[orchestration_system.web.rate_limiting]
enabled = false
requests_per_minute = 1000
burst_size = 100
per_client_limit = true

[orchestration_system.web.resilience]
circuit_breaker_enabled = true
request_timeout_seconds = 30
max_concurrent_requests = 100
```

**Depth**: 4 levels (`orchestration_system.web.auth.protected_routes.*`)
**Matches**: `TaskerConfig.orchestration: OrchestrationConfig` (legacy struct)
**Pattern**: Uses WebConfig struct (shared with worker)

#### Orchestration Event System (Lines 130-220)
```toml
[orchestration_events]
system_id = "orchestration-event-system"
deployment_mode = "Hybrid"

[orchestration_events.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 5
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[orchestration_events.processing]
max_concurrent_operations = 10
batch_size = 10
max_retries = 3

[orchestration_events.processing.backoff]  # Second BackoffConfig!
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[orchestration_events.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5

[orchestration_events.metadata]
# Empty - populated at runtime
```

**Matches**: `EventSystemsConfig.orchestration: OrchestrationEventSystemConfig`
**Naming Collision**: `backoff` at two different paths with different field structures!

#### Task Readiness Event System (Lines 222-263)
```toml
[task_readiness_events]
system_id = "task-readiness-event-system"
deployment_mode = "Hybrid"

[task_readiness_events.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 5
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[task_readiness_events.processing]
max_concurrent_operations = 100
batch_size = 50
max_retries = 3

[task_readiness_events.processing.backoff]  # Third BackoffConfig!
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[task_readiness_events.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5
```

**Matches**: `EventSystemsConfig.task_readiness: TaskReadinessEventSystemConfig`
**Note**: Simplified from full TaskReadinessConfig (no notification, fallback_polling, event_channel, coordinator, error_handling subsections)

#### Orchestration MPSC Channels (Lines 265-292)
```toml
[mpsc_channels.orchestration.command_processor]
command_buffer_size = 1000

[mpsc_channels.orchestration.event_systems]
event_channel_buffer_size = 1000

[mpsc_channels.orchestration.event_listeners]
pgmq_event_buffer_size = 10000

[mpsc_channels.task_readiness.event_channel]
buffer_size = 1000
send_timeout_ms = 100
```

**Matches**: `MpscChannelsConfig.orchestration` and `MpscChannelsConfig.task_readiness`
**TAS-51**: Proper subsystem hierarchy

#### TAS-49 DLQ Configuration (Lines 294-351)
```toml
[staleness_detection]
enabled = true
detection_interval_seconds = 300
batch_size = 100
dry_run = false

[staleness_detection.thresholds]
waiting_for_dependencies_minutes = 60
waiting_for_retry_minutes = 30
steps_in_process_minutes = 30
task_max_lifetime_hours = 24

[staleness_detection.actions]
auto_transition_to_error = true
auto_move_to_dlq = true
emit_events = true
event_channel = "task_staleness_detected"

[dlq]
enabled = true
auto_dlq_on_staleness = true
include_full_task_snapshot = true
max_pending_age_hours = 168

[dlq.reasons]
staleness_timeout = true
max_retries_exceeded = true
worker_unavailable = true
dependency_cycle_detected = true
manual_dlq = true

[archive]
enabled = true
retention_days = 30
archive_batch_size = 1000
archive_interval_hours = 24

[archive.policies]
archive_completed = true
archive_failed = true
archive_cancelled = false
archive_dlq_resolved = true
```

**Matches**: `StalenessDetectionConfig`, `DlqConfig`, `ArchiveConfig`
**Context**: Used by `contexts::OrchestrationConfig` (NOT in root TaskerConfig)

### 3. worker.toml Structure

**Lines**: 336 (including documentation)

**Root-Level Keys**:
```toml
environment = "development"  # Duplicated again
```

**TOML Tables** (28 total):

#### Worker System Configuration (Lines 4-153)
```toml
[worker_system]
worker_id = "worker-001"
worker_type = "general"

[worker_system.event_driven]
enabled = true
deployment_mode = "Hybrid"
health_monitoring_enabled = true
health_check_interval_seconds = 30

[worker_system.step_processing]
claim_timeout_seconds = 300
max_retries = 3
max_concurrent_steps = 100

[worker_system.listener]
retry_interval_seconds = 5
max_retry_attempts = 3
event_timeout_seconds = 30
health_check_interval_seconds = 60
batch_processing = true
connection_timeout_seconds = 10

[worker_system.fallback_poller]
enabled = true
polling_interval_ms = 45000
batch_size = 5
age_threshold_seconds = 10
max_age_hours = 12
visibility_timeout_seconds = 30
processable_states = ["pending", "waiting_for_dependencies", "waiting_for_retry"]

[worker_system.health_monitoring]
health_check_interval_seconds = 10
performance_monitoring_enabled = true
error_rate_threshold = 0.05

[worker_system.resource_limits]
max_memory_mb = 2048
max_cpu_percent = 80
max_database_connections = 50
max_queue_connections = 20

[worker_system.queue_config]
visibility_timeout_seconds = 30
batch_size = 10
polling_interval_ms = 100

[worker_system.web]
# Same structure as orchestration_system.web
enabled = true
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"  # Different port!
request_timeout_ms = 30000

[worker_system.web.tls]
# ... (same as orchestration)

[worker_system.web.database_pools]
# ... (same structure, different values)

[worker_system.web.cors]
# ... (same as orchestration)

[worker_system.web.auth]
# ... (same structure, different values)

[worker_system.web.rate_limiting]
# ... (same as orchestration)

[worker_system.web.resilience]
# ... (same as orchestration)
```

**Matches**: `WorkerConfig` struct
**Observations**:
- Extensive nested configuration (7 subsections)
- Shares WebConfig structure with orchestration
- Different port binding (8081 vs 8080)

#### Worker Event System (Lines 227-300)
```toml
[worker_events]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[worker_events.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 1  # Faster than orchestration!
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[worker_events.processing]
max_concurrent_operations = 100  # More than orchestration!
batch_size = 10
max_retries = 3

[worker_events.processing.backoff]  # Fourth BackoffConfig!
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[worker_events.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5

[worker_events.metadata.in_process_events]
ffi_integration_enabled = true
deduplication_cache_size = 1000

[worker_events.metadata.listener]
retry_interval_seconds = 5
max_retry_attempts = 3
event_timeout_seconds = 30
batch_processing = true
connection_timeout_seconds = 10

[worker_events.metadata.fallback_poller]
enabled = true
polling_interval_ms = 500
batch_size = 10
age_threshold_seconds = 2
max_age_hours = 12
visibility_timeout_seconds = 30

[worker_events.metadata.resource_limits]
max_memory_mb = 2048
max_cpu_percent = 80.0
max_database_connections = 50
max_queue_connections = 20
```

**Matches**: `EventSystemsConfig.worker: WorkerEventSystemConfig`
**Note**: Worker metadata is complex (unlike empty orchestration metadata)

#### Worker MPSC Channels (Lines 302-336)
```toml
[mpsc_channels.worker.command_processor]
command_buffer_size = 1000

[mpsc_channels.worker.event_systems]
event_channel_buffer_size = 1000

[mpsc_channels.worker.event_subscribers]
completion_buffer_size = 1000
result_buffer_size = 1000

[mpsc_channels.worker.in_process_events]
broadcast_buffer_size = 1000

[mpsc_channels.worker.event_listeners]
pgmq_event_buffer_size = 1000
```

**Matches**: `MpscChannelsConfig.worker`
**TAS-51**: Proper subsystem hierarchy

### 4. decision_points.toml Structure

**Lines**: 136 (including extensive documentation)

**TOML Tables** (11 total):

```toml
[decision_points]
enabled = true
max_steps_per_decision = 50
max_decision_depth = 10
warn_threshold_steps = 20
warn_threshold_depth = 5
enable_detailed_logging = false
enable_metrics = true

[decision_points._docs.max_steps_per_decision]
description = "Maximum number of workflow steps..."
type = "usize"
valid_range = "1-1000"
default = "50"
system_impact = "Prevents runaway step creation..."
related = ["decision_points.warn_threshold_steps"]
example = """...example configuration..."""

[decision_points._docs.max_steps_per_decision.recommendations]
test = { value = "10", rationale = "..." }
development = { value = "50", rationale = "..." }
production = { value = "100", rationale = "..." }

# ... more _docs sections
```

**Matches**: `DecisionPointsConfig` struct
**Observations**:
- Extensive documentation (75% of file is `_docs` sections)
- TAS-53 feature addition
- Relatively simple flat structure (1-2 levels)

## Merged Configuration Analysis (complete-test.toml)

**Lines**: 494
**Root-Level Tables**: 27 (all flattened to root)

### Structure Comparison

**Base Files** (split by context):
```
common.toml          → database, queues, mpsc_channels.shared, circuit_breakers
orchestration.toml   → backoff, orchestration_*, task_readiness_events, mpsc_channels.orchestration, mpsc_channels.task_readiness, staleness_detection, dlq, archive
worker.toml          → worker_system, worker_events, mpsc_channels.worker
decision_points.toml → decision_points
```

**Merged File** (flat):
```
[archive]
[backoff]
[circuit_breakers]
[database]
[dlq]
[mpsc_channels.orchestration.*]
[mpsc_channels.overflow_policy]
[mpsc_channels.shared.*]
[mpsc_channels.task_readiness.*]
[mpsc_channels.worker.*]
[orchestration_events]
[orchestration_system]
[queues]
[staleness_detection]
[task_readiness_events]
[worker_events]
[worker_system]
```

**Key Observations**:
1. **All context separation removed** - no `[common]`, `[orchestration]`, `[worker]` tables
2. **Alphabetically sorted** - tables appear in alpha order, not logical grouping
3. **Documentation stripped** - all `_docs` sections removed (as expected)
4. **Environment duplicates merged** - single `environment = "test"` at top

## Critical Issue: Missing Required Fields

### TaskerConfig Required Fields (from struct analysis)

**Required in Rust** (14 fields):
1. ✅ `database: DatabaseConfig` - present
2. ❌ `telemetry: TelemetryConfig` - **MISSING**
3. ❌ `task_templates: TaskTemplatesConfig` - **MISSING**
4. ❌ `system: SystemConfig` - **MISSING**
5. ✅ `backoff: BackoffConfig` - present
6. ❌ `execution: ExecutionConfig` - **MISSING**
7. ✅ `queues: QueuesConfig` - present
8. ✅ `orchestration: OrchestrationConfig` - present (as `orchestration_system`)
9. ✅ `circuit_breakers: CircuitBreakerConfig` - present
10. ✅ `task_readiness: TaskReadinessConfig` - partial (only event system portion)
11. ✅ `event_systems: EventSystemsConfig` - present (as separate tables)
12. ✅ `mpsc_channels: MpscChannelsConfig` - present
13. ❌ `decision_points: DecisionPointsConfig` - present in separate file but not merged
14. ⚠️ `worker: Option<WorkerConfig>` - present (as `worker_system`)

**Missing Fields**: 5 required fields (telemetry, task_templates, system, execution, decision_points)

### Impact on Deserialization

When UnifiedConfigLoader tries to deserialize the merged TOML into TaskerConfig:
```rust
// This will FAIL because telemetry, task_templates, system, execution are required
let config: TaskerConfig = toml::from_str(&merged_toml)?;
```

**Error**: `missing field 'telemetry' in 'TaskerConfig'`

This is **exactly** the error the user was experiencing!

## TOML Table Naming Patterns

### Context-Specific Tables (Used in Base Files)

**Orchestration Context**:
- `orchestration_system` → `TaskerConfig.orchestration: OrchestrationConfig`
- `orchestration_events` → `TaskerConfig.event_systems.orchestration`
- `staleness_detection` → `contexts::OrchestrationConfig.staleness_detection`
- `dlq` → `contexts::OrchestrationConfig.dlq`
- `archive` → `contexts::OrchestrationConfig.archive`

**Worker Context**:
- `worker_system` → `Option<TaskerConfig.worker: WorkerConfig>`
- `worker_events` → `TaskerConfig.event_systems.worker`

**Task Readiness Context**:
- `task_readiness_events` → `TaskerConfig.event_systems.task_readiness`

### Naming Mismatches

**TOML Table Name** → **Rust Struct Path**:
- `orchestration_system` → `TaskerConfig.orchestration` (should be `[orchestration]`)
- `orchestration_events` → `TaskerConfig.event_systems.orchestration` (should be `[event_systems.orchestration]`)
- `task_readiness_events` → `TaskerConfig.event_systems.task_readiness` (should be `[event_systems.task_readiness]`)
- `worker_system` → `TaskerConfig.worker` (should be `[worker]`)
- `worker_events` → `TaskerConfig.event_systems.worker` (should be `[event_systems.worker]`)

**Pattern**: TOML uses descriptive names (`_system`, `_events`) while Rust uses terse names.

## Documentation Metadata Pattern

All base files use consistent `_docs` pattern:

```toml
[some_section.parameter]
value = 123

[some_section._docs.parameter]
description = "Human-readable description"
type = "u64"
valid_range = "1-1000"
default = "123"
system_impact = "What happens if this is wrong"
related = ["other.parameter", "another.setting"]
example = """
# Multi-line example
[some_section]
parameter = 456
"""

[some_section._docs.parameter.recommendations]
test = { value = "10", rationale = "Why test uses this" }
development = { value = "50", rationale = "Why dev uses this" }
production = { value = "200", rationale = "Why prod uses this" }
```

**Purpose**: Rich documentation for operators
**Fate**: Stripped by `ConfigMerger.strip_docs_from_value()` (line 253-272 of merger.rs)
**Benefit**: Provides environment-specific recommendations inline

## Environment Variable Expansion

### Pattern Used Throughout

```toml
[database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"

[orchestration_system.web]
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8080}"
```

**Syntax**: `${VAR_NAME:-default_value}`

**Processing**: Handled by `UnifiedConfigLoader.expand_env_vars()` (unified_loader.rs)

**Benefit**: Single config file works across environments with different runtime variables

## Depth and Complexity Metrics

### By File

| File | Max Depth | Tables | Nested Structures |
|------|-----------|--------|-------------------|
| common.toml | 3 | 8 | database.pool._docs.* |
| orchestration.toml | 4 | 19 | orchestration_system.web.auth.protected_routes |
| worker.toml | 4 | 20 | worker_system.web.auth.protected_routes, worker_events.metadata.* |
| decision_points.toml | 2 | 6 | decision_points._docs.* |

### By Subsystem

| Subsystem | Tables | Files | Complexity |
|-----------|--------|-------|------------|
| Database | 3 | common | Low (simple nested pool config) |
| Queues | 3 | common | Low (3 backends: pgmq, rabbitmq, orchestration_queues) |
| Circuit Breakers | 4 | common | Low (HashMap of component overrides) |
| MPSC Channels | 12 | all | Medium (subsystem organization) |
| Orchestration System | 7 | orchestration | High (deep web config nesting) |
| Orchestration Events | 5 | orchestration | Medium (event system pattern) |
| Task Readiness Events | 5 | orchestration | Medium (event system pattern) |
| Worker System | 9 | worker | High (complex nested config) |
| Worker Events | 9 | worker | Very High (complex metadata structure) |
| DLQ/Archive | 6 | orchestration | Medium (TAS-49 additions) |
| Decision Points | 2 | decision_points | Low (flat structure) |

## Observed Issues

### 1. Missing Required Fields

**Critical**: 5 required TaskerConfig fields have NO TOML representation:
- `telemetry: TelemetryConfig`
- `task_templates: TaskTemplatesConfig`
- `system: SystemConfig`
- `execution: ExecutionConfig`
- `decision_points: DecisionPointsConfig` (separate file not merged into complete)

**Impact**: Complete config generation fails validation.

### 2. Table Name Semantic Gap

**Issue**: TOML table names don't match struct field names:
- `orchestration_system` ≠ `orchestration`
- `worker_system` ≠ `worker`
- `orchestration_events` ≠ `event_systems.orchestration`

**Impact**: Requires custom deserialization mapping or struct field renaming.

### 3. BackoffConfig Naming Collision

**Issue**: Four different `backoff` sections with different field structures:
1. `[backoff]` - task/step retry backoff (root level)
2. `[orchestration_events.processing.backoff]` - event processing backoff
3. `[task_readiness_events.processing.backoff]` - event processing backoff
4. `[worker_events.processing.backoff]` - event processing backoff

**Impact**: Confusing for operators; hard to know which `backoff` applies where.

### 4. TaskReadinessConfig Incompleteness

**Issue**: TaskReadinessConfig in Rust has 8 subsections:
- `enabled`
- `event_system`
- `enhanced_settings`
- `notification`
- `fallback_polling`
- `event_channel`
- `coordinator`
- `error_handling`

**TOML Only Has**: `task_readiness_events` (event_system portion only)

**Impact**: Full TaskReadinessConfig cannot be hydrated from TOML.

### 5. Redundant Environment Field

**Issue**: `environment = "development"` appears at top of all 4 base files.

**Impact**: During merge, last one wins (arbitrary). Should be in common.toml only.

### 6. Extraneous Fields

**Issue**: TOML has fields not in structs:
- `database.encoding` (not in DatabaseConfig)
- `database.host` (not in DatabaseConfig)
- `database.checkout_timeout` (not in DatabaseConfig)
- `database.reaping_frequency` (not in DatabaseConfig)
- `queues.default_namespace` (not in QueuesConfig)

**Impact**: Silently ignored during deserialization (Serde default behavior).

### 7. Context Separation Lost in Merge

**Issue**: Base files organized by deployment context (common, orchestration, worker), but merge produces flat structure.

**Impact**:
- Semantic organization lost
- Hard to see what config applies to what component
- Cannot generate context-specific configs (e.g., "orchestration-only config")

## File Organization Rationale (Inferred)

### Why Split by Context?

**Benefits**:
1. **Deployment Flexibility**: Can deploy orchestration-only or worker-only
2. **Configuration Reuse**: Common config shared across contexts
3. **Reduced Duplication**: Shared settings (database, queues) defined once
4. **Context Clarity**: Obvious which settings affect which components

**Drawbacks**:
1. **Struct Mismatch**: TOML organization doesn't match Rust struct hierarchy
2. **Merge Complexity**: Deep merge required to preserve nested tables
3. **Missing Fields**: Some fields have no natural "context" (telemetry, system, execution)
4. **Namespace Collision Risk**: Same table names across contexts

### Why Common vs Orchestration vs Worker?

**Common** = Shared infrastructure:
- Database connection (both need to connect)
- Message queues (both use PGMQ)
- Circuit breakers (both use resilience patterns)
- Shared MPSC channels (cross-cutting concerns)

**Orchestration** = Task lifecycle management:
- Task/step retry logic (backoff)
- Orchestration event system
- Task readiness event system
- DLQ and archival
- Orchestration-specific MPSC channels

**Worker** = Step execution:
- Worker identification
- Step processing limits
- Worker event system
- FFI integration (Ruby handlers)
- Worker-specific MPSC channels

**Decision Points** = Feature-specific (TAS-53):
- Separate file to isolate optional feature
- Can be disabled entirely by not including file

### What Should Be Where?

**Truly Shared** (should be in common):
- Database
- Telemetry (observability is shared)
- System (version, limits)
- Execution environment (TASKER_ENV)

**Context-Specific** (current placement is correct):
- Backoff (orchestration needs it for task retries)
- Event systems (different for orchestration vs worker)
- MPSC channels (subsystem-specific buffer sizing)
- Web API configs (different ports, different auth)

**Ambiguous** (needs design decision):
- Queues: Used by both, but orchestration "owns" queues and worker consumes
- Circuit breakers: Used by both, could be in common or duplicated per context
- Task templates: Needed by orchestration for initialization, but worker needs for execution

## Recommendations for TAS-60

### 1. Add Missing Required Fields

Create sections for missing fields in appropriate files:

**common.toml additions**:
```toml
[telemetry]
enabled = false
service_name = "tasker-core"
sample_rate = 1.0

[task_templates]
search_paths = ["config/task_templates/*.{yml,yaml}"]

[system]
default_dependent_system = "default"
version = "0.1.0"
max_recursion_depth = 50

[execution]
max_concurrent_tasks = 100
max_concurrent_steps = 1000
default_timeout_seconds = 3600
step_execution_timeout_seconds = 300
max_discovery_attempts = 3
step_batch_size = 10
max_retries = 3
max_workflow_steps = 1000
connection_timeout_seconds = 10
environment = "development"  # Remove from other files
```

### 2. Align Table Names with Struct Fields

**Option A**: Rename TOML tables to match structs:
```toml
[orchestration]      # instead of [orchestration_system]
[worker]             # instead of [worker_system]
[event_systems.orchestration]  # instead of [orchestration_events]
[event_systems.worker]         # instead of [worker_events]
[event_systems.task_readiness] # instead of [task_readiness_events]
```

**Option B**: Use `#[serde(rename = "...")]` in Rust structs to match TOML names.

**Recommendation**: Option A - TOML should match struct hierarchy.

### 3. Disambiguate BackoffConfig Naming

Use fully qualified paths in TOML:
```toml
[backoff]                                  # Root task/step retry backoff
[event_systems.orchestration.processing.backoff]  # Orchestration event backoff
[event_systems.task_readiness.processing.backoff] # Task readiness event backoff
[event_systems.worker.processing.backoff]         # Worker event backoff
```

### 4. Complete TaskReadinessConfig

Either:
- **Option A**: Add missing sections to orchestration.toml
- **Option B**: Simplify TaskReadinessConfig struct to match current TOML

**Recommendation**: Option A - complete the configuration.

### 5. Consolidate Environment Field

Remove `environment` from orchestration.toml and worker.toml, keep only in common.toml.

Or better: Move to `[execution.environment]` (it's already a field in ExecutionConfig).

### 6. Document Field Mapping

Create explicit documentation of TOML path → Rust struct path mapping for operators.

### 7. Consider Complete vs Context Generation

**Current**: Merge produces flat complete config (all contexts together)

**Proposal**: Support multiple merge modes:
- `complete`: Current behavior (orchestration + worker together)
- `orchestration`: Only orchestration-specific config
- `worker`: Only worker-specific config
- `common`: Only shared infrastructure config

This enables:
- Orchestration-only deployments
- Worker-only deployments
- Clear separation of concerns

## Next Steps

1. **Task 3**: Analyze unified_loader to understand how TOML is loaded and merged
2. **Task 4**: Map TOML paths to struct hydration points
3. **Task 5**: Propose semantically-near TOML representation

## Appendix: Complete Table Inventory

### common.toml Tables (8)
1. `[database]`
2. `[database.pool]`
3. `[database.variables]`
4. `[queues]`
5. `[queues.orchestration_queues]`
6. `[queues.pgmq]`
7. `[queues.rabbitmq]`
8. `[mpsc_channels.shared.event_publisher]`
9. `[mpsc_channels.shared.ffi]`
10. `[mpsc_channels.overflow_policy]`
11. `[mpsc_channels.overflow_policy.metrics]`
12. `[circuit_breakers]`
13. `[circuit_breakers.global_settings]`
14. `[circuit_breakers.default_config]`
15. `[circuit_breakers.component_configs.pgmq]`
16. `[circuit_breakers.component_configs.task_readiness]`

### orchestration.toml Tables (19)
1. `[backoff]`
2. `[backoff.reenqueue_delays]`
3. `[orchestration_system]`
4. `[orchestration_system.web]`
5. `[orchestration_system.web.tls]`
6. `[orchestration_system.web.database_pools]`
7. `[orchestration_system.web.cors]`
8. `[orchestration_system.web.auth]`
9. `[orchestration_system.web.auth.protected_routes]`
10. `[orchestration_system.web.rate_limiting]`
11. `[orchestration_system.web.resilience]`
12. `[orchestration_events]`
13. `[orchestration_events.timing]`
14. `[orchestration_events.processing]`
15. `[orchestration_events.processing.backoff]`
16. `[orchestration_events.health]`
17. `[orchestration_events.metadata]`
18. `[task_readiness_events]`
19. `[task_readiness_events.timing]`
20. `[task_readiness_events.processing]`
21. `[task_readiness_events.processing.backoff]`
22. `[task_readiness_events.health]`
23. `[mpsc_channels.orchestration.command_processor]`
24. `[mpsc_channels.orchestration.event_systems]`
25. `[mpsc_channels.orchestration.event_listeners]`
26. `[mpsc_channels.task_readiness.event_channel]`
27. `[staleness_detection]`
28. `[staleness_detection.thresholds]`
29. `[staleness_detection.actions]`
30. `[dlq]`
31. `[dlq.reasons]`
32. `[archive]`
33. `[archive.policies]`

### worker.toml Tables (20)
1. `[worker_system]`
2. `[worker_system.event_driven]`
3. `[worker_system.step_processing]`
4. `[worker_system.listener]`
5. `[worker_system.fallback_poller]`
6. `[worker_system.health_monitoring]`
7. `[worker_system.resource_limits]`
8. `[worker_system.queue_config]`
9. `[worker_system.web]`
10. `[worker_system.web.tls]`
11. `[worker_system.web.database_pools]`
12. `[worker_system.web.cors]`
13. `[worker_system.web.auth]`
14. `[worker_system.web.auth.protected_routes]`
15. `[worker_system.web.rate_limiting]`
16. `[worker_system.web.resilience]`
17. `[worker_events]`
18. `[worker_events.timing]`
19. `[worker_events.processing]`
20. `[worker_events.processing.backoff]`
21. `[worker_events.health]`
22. `[worker_events.metadata.in_process_events]`
23. `[worker_events.metadata.listener]`
24. `[worker_events.metadata.fallback_poller]`
25. `[worker_events.metadata.resource_limits]`
26. `[mpsc_channels.worker.command_processor]`
27. `[mpsc_channels.worker.event_systems]`
28. `[mpsc_channels.worker.event_subscribers]`
29. `[mpsc_channels.worker.in_process_events]`
30. `[mpsc_channels.worker.event_listeners]`

### decision_points.toml Tables (2)
1. `[decision_points]`
2. `[decision_points._docs.*]` (documentation - stripped)

**Total**: 73 configuration tables across 4 files
