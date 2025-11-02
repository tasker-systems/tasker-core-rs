# TAS-60 Configuration Redesign Proposal

**Created**: 2025-10-31
**Status**: Proposal - Ready for Review
**Dependencies**:
- Task 1: [tasker-config-struct-analysis.md](./tasker-config-struct-analysis.md)
- Task 2: [toml-structure-analysis.md](./toml-structure-analysis.md)
- Task 3: [unified-loader-pattern-analysis.md](./unified-loader-pattern-analysis.md)
- Task 4: [intersection-analysis.md](./intersection-analysis.md)

---

## Executive Summary

This proposal presents a complete redesign of Tasker's configuration architecture to achieve:
1. **Semantic clarity** - Configuration structure mirrors deployment contexts
2. **Minimal coupling** - Common config drives SystemContext, contexts are optional
3. **Zero sprawl** - Remove 574 lines of unused configuration
4. **Type safety** - Eliminate backward compatibility hacks and default filling
5. **Maintainability** - TOML structure semantically near to Rust structs

**Core Principle**: `{ common, orchestration, worker }` where common is required and contexts are optional based on deployment.

### Problem Statement

Current configuration system exhibits critical issues discovered through analysis:
- **Configuration sprawl**: 574-line TaskReadinessConfig with zero runtime usage
- **Missing required fields**: 5 TaskerConfig fields absent from TOML causing validation failures
- **Shallow merge bug**: Nested tables replaced instead of merged
- **Backward compatibility hacks**: `..TaskerConfig::default()` fills missing fields incorrectly
- **Context confusion**: Monolithic TaskerConfig mixes common and context-specific concerns
- **Asymmetric design**: Optional worker but required orchestration in single struct

### Proposed Solution

**Three-Phase Migration**:
1. **Phase 1 (TAS-60)**: Immediate fixes - shallow merge, missing fields, extraneous fields
2. **Phase 2 (TAS-61)**: Cleanup - remove TaskReadinessConfig, consolidate merge logic
3. **Phase 3 (TAS-62)**: Architectural redesign - adopt `{ common, orchestration, worker }` structure

---

## Part 1: Immediate Fixes (Phase 1 - TAS-60 Completion)

### Goal
Make current configuration system functional without breaking changes.

### 1.1 Fix Shallow Merge Bug (Root Cause)

**Problem**: `UnifiedConfigLoader::merge_toml()` performs shallow merge, causing nested tables to be replaced.

**Example Failure**:
```rust
// Base (common.toml)
[mpsc_channels.shared]
event_queue_buffer_size = 5000

// Overlay (orchestration.toml)
[mpsc_channels.orchestration]
command_buffer_size = 1000

// CURRENT RESULT (WRONG): [mpsc_channels.shared] DISAPPEARS!
[mpsc_channels]
orchestration = { command_buffer_size = 1000 }

// EXPECTED RESULT: Both subsections preserved
[mpsc_channels]
shared = { event_queue_buffer_size = 5000 }
orchestration = { command_buffer_size = 1000 }
```

**Solution**: Copy `ConfigMerger::deep_merge_tables()` to `UnifiedConfigLoader`.

**File**: `tasker-shared/src/config/unified_loader.rs`

```rust
// BEFORE (lines 764-799) - BROKEN
fn merge_toml(&self, base: &mut toml::Value, overlay: toml::Value) -> ConfigResult<()> {
    if let (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) = (base, overlay) {
        for (key, value) in overlay_table {
            base_table.insert(key, value);  // BUG: Just replaces!
        }
    }
    Ok(())
}

// AFTER - FIXED (copy from merger.rs:212-251)
fn merge_toml(&self, base: &mut toml::Value, overlay: toml::Value) -> ConfigResult<()> {
    if let (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) = (base, overlay) {
        Self::deep_merge_tables(base_table, overlay_table);
    }
    Ok(())
}

fn deep_merge_tables(base: &mut toml::value::Table, overlay: toml::value::Table) {
    for (key, value) in overlay {
        match base.get_mut(&key) {
            Some(base_value) => {
                let both_are_tables = matches!(
                    (&*base_value, &value),
                    (toml::Value::Table(_), toml::Value::Table(_))
                );
                if both_are_tables {
                    // RECURSIVE MERGE!
                    if let toml::Value::Table(base_table) = base_value {
                        if let toml::Value::Table(overlay_table) = value {
                            Self::deep_merge_tables(base_table, overlay_table);
                        }
                    }
                } else {
                    *base_value = value;
                }
            }
            None => {
                base.insert(key, value);
            }
        }
    }
}
```

**Impact**: Fixes "missing field 'orchestration' in 'mpsc_channels'" error and all related nested table merge issues.

### 1.2 Add Missing Required Fields

**Problem**: 5 required TaskerConfig fields missing from TOML causing validation failures.

**Solution**: Add missing sections to `config/tasker/base/common.toml`.

```toml
# ADD TO config/tasker/base/common.toml (after [database.variables])

# ============================================================================
# TELEMETRY CONFIGURATION
# ============================================================================

[telemetry]
# Telemetry enablement and configuration
enabled = true

[telemetry.metrics]
# Metrics collection configuration
enabled = true
collection_interval_seconds = 60
export_endpoint = "${TELEMETRY_ENDPOINT:-}"

[telemetry.tracing]
# Distributed tracing configuration
enabled = true
sampling_rate = 1.0
export_endpoint = "${TRACING_ENDPOINT:-}"

# ============================================================================
# SYSTEM CONFIGURATION
# ============================================================================

[system]
# System-level configuration
max_memory_mb = 2048
max_cpu_percent = 80.0
process_name = "tasker"

# ============================================================================
# EXECUTION CONFIGURATION
# ============================================================================

[execution]
# CRITICAL: Environment detection (was missing!)
environment = "${TASKER_ENV:-development}"

[execution.timeouts]
# Global operation timeouts
default_operation_timeout_seconds = 30
shutdown_grace_period_seconds = 30

# ============================================================================
# TASK TEMPLATES CONFIGURATION
# ============================================================================

[task_templates]
# Task template discovery and caching
cache_enabled = true
template_path = "${TASKER_TEMPLATE_PATH:-}"
auto_reload = true
reload_interval_seconds = 300
```

**Environment-Specific Overrides**:

```toml
# config/tasker/environments/test/common.toml
[execution]
environment = "test"

[telemetry]
enabled = false  # Disable telemetry in tests

[system]
max_memory_mb = 512  # Lower limits for test environment


# config/tasker/environments/production/common.toml
[execution]
environment = "production"

[telemetry.metrics]
export_endpoint = "http://prometheus:9090"

[telemetry.tracing]
export_endpoint = "http://jaeger:14268"

[system]
max_memory_mb = 4096  # Higher limits for production
```

**Impact**: Eliminates "missing field 'telemetry'" error and makes execution.environment available.

### 1.3 Remove Extraneous TOML Fields

**Problem**: Multiple TOML fields silently ignored during deserialization.

**Solution**: Remove from `config/tasker/base/common.toml`.

```toml
# BEFORE (lines 4-10)
[database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"
encoding = "unicode"      # ❌ REMOVE - not in DatabaseConfig
host = "localhost"        # ❌ REMOVE - not in DatabaseConfig
checkout_timeout = 10     # ❌ REMOVE - unclear purpose
reaping_frequency = 10    # ❌ REMOVE - unclear purpose

# AFTER
[database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"
database = "tasker_development"  # Only if needed in struct
```

```toml
# BEFORE (line 66)
[queues]
backend = "pgmq"
orchestration_namespace = "orchestration"
worker_namespace = "worker"
default_namespace = "default"  # ❌ REMOVE - not in QueuesConfig

# AFTER
[queues]
backend = "pgmq"
orchestration_namespace = "orchestration"
worker_namespace = "worker"
```

**Impact**: TOML accurately reflects struct definition, no silent field ignoring.

### 1.4 Merge decision_points.toml into Config Generation

**Problem**: `decision_points.toml` exists but not merged into complete-*.toml generation.

**Solution**: Update `ConfigMerger` to include decision_points.

**File**: `tasker-shared/src/config/merger.rs`

```rust
// Add to merge_base_and_environment() method
pub fn merge_base_and_environment(
    &self,
    context: ConfigContext,
) -> ConfigResult<toml::Value> {
    // ... existing base and environment merge ...

    // ADD: Include decision_points.toml if exists
    let decision_points_path = self.base_path.join("decision_points.toml");
    if decision_points_path.exists() {
        let decision_points = std::fs::read_to_string(&decision_points_path)
            .map_err(|e| ConfigError::IoError(e))?;
        let decision_points_value: toml::Value = toml::from_str(&decision_points)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Self::deep_merge_tables(&mut result_table, decision_points_value);
        tracing::debug!("Merged decision_points.toml into configuration");
    }

    Ok(toml::Value::Table(result_table))
}
```

**Impact**: `decision_points` field populated in TaskerConfig during generation.

### Phase 1 Testing

```bash
# Test configuration generation with all contexts
cargo run --package tasker-client --bin tasker-cli -- \
    config generate --context orchestration --environment test --output /tmp/orch-test.toml

cargo run --package tasker-client --bin tasker-cli -- \
    config generate --context worker --environment test --output /tmp/worker-test.toml

cargo run --package tasker-client --bin tasker-cli -- \
    config generate --context complete --environment test --output /tmp/complete-test.toml

# Validate no missing field errors
cargo run --bin config-validator
echo $?  # Should be 0
```

### Phase 1 Success Criteria

- ✅ Config generation produces valid TOML with no missing field errors
- ✅ Nested tables correctly merged (mpsc_channels.shared + mpsc_channels.orchestration)
- ✅ All required fields present (telemetry, system, execution, task_templates, decision_points)
- ✅ No extraneous fields in base TOML files
- ✅ All existing tests pass without modification

---

## Part 2: Configuration Cleanup (Phase 2 - TAS-61)

### Goal
Remove legacy configuration and consolidate implementation without breaking public APIs.

### 2.1 Remove TaskReadinessConfig (574 Lines)

**Evidence**: Zero runtime usage, replaced by `event_systems.task_readiness`.

**Files to Modify**:

#### 2.1.1 Remove Struct Definitions
**File**: `tasker-shared/src/config/task_readiness.rs`

```rust
// DELETE ENTIRE FILE (574 lines)
// All 15 structs:
// - TaskReadinessConfig
// - EnhancedCoordinatorSettings
// - TaskReadinessNotificationConfig
// - ReadinessFallbackConfig
// - EventChannelConfig (moved to mpsc_channels)
// - TaskReadinessCoordinatorConfig
// - ErrorHandlingConfig
// - (8 more structs...)
```

#### 2.1.2 Remove from TaskerConfig
**File**: `tasker-shared/src/config/tasker.rs`

```rust
// BEFORE
pub struct TaskerConfig {
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub task_templates: TaskTemplatesConfig,
    pub system: SystemConfig,
    pub backoff: BackoffConfig,
    pub execution: ExecutionConfig,
    pub queues: QueuesConfig,
    pub orchestration: OrchestrationConfig,
    pub circuit_breakers: CircuitBreakerConfig,
    pub task_readiness: TaskReadinessConfig,  // ❌ REMOVE THIS
    pub event_systems: EventSystemsConfig,
    pub mpsc_channels: MpscChannelsConfig,
    pub decision_points: DecisionPointsConfig,
    pub worker: Option<WorkerConfig>,
}

// AFTER
pub struct TaskerConfig {
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub task_templates: TaskTemplatesConfig,
    pub system: SystemConfig,
    pub backoff: BackoffConfig,
    pub execution: ExecutionConfig,
    pub queues: QueuesConfig,
    pub orchestration: OrchestrationConfig,
    pub circuit_breakers: CircuitBreakerConfig,
    // task_readiness removed - replaced by event_systems.task_readiness
    pub event_systems: EventSystemsConfig,
    pub mpsc_channels: MpscChannelsConfig,
    pub decision_points: DecisionPointsConfig,
    pub worker: Option<WorkerConfig>,
}

impl Default for TaskerConfig {
    fn default() -> Self {
        Self {
            // ... other fields ...
            // task_readiness: TaskReadinessConfig::default(),  // ❌ REMOVE
            // ... remaining fields ...
        }
    }
}
```

#### 2.1.3 Remove from mod.rs Exports
**File**: `tasker-shared/src/config/mod.rs`

```rust
// BEFORE
pub use task_readiness::{
    EnhancedCoordinatorSettings, ErrorHandlingConfig, EventChannelConfig,
    ReadinessFallbackConfig, TaskReadinessConfig, TaskReadinessCoordinatorConfig,
    TaskReadinessNotificationConfig,
};  // ❌ REMOVE ALL

// AFTER
// All task_readiness exports removed
```

#### 2.1.4 Update Tests
**File**: `tasker-shared/src/config/mpsc_channels.rs`

```rust
// BEFORE (lines 359-360)
assert_eq!(config.task_readiness.event_channel.buffer_size, 1000);
assert_eq!(config.task_readiness.event_channel.send_timeout_ms, 1000);

// AFTER
assert_eq!(config.mpsc_channels.task_readiness.event_channel.buffer_size, 1000);
assert_eq!(config.mpsc_channels.task_readiness.event_channel.send_timeout_ms, 1000);
```

**Impact**: Removes 574 lines of unused configuration code.

### 2.2 Consolidate Merge Logic

**Problem**: Three different merge implementations across unified_loader, merger, and manager.

**Solution**: Single canonical implementation.

#### 2.2.1 Create Shared Merge Module
**File**: `tasker-shared/src/config/merge.rs` (NEW)

```rust
//! Canonical TOML merge implementation
//!
//! Single source of truth for deep merging TOML tables across all configuration loading paths.

use toml::value::Table;

/// Deep merge overlay table into base table recursively
///
/// For each key in overlay:
/// - If key exists in base AND both values are tables: recurse
/// - Otherwise: replace base value with overlay value
pub fn deep_merge_tables(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match base.get_mut(&key) {
            Some(base_value) => {
                let both_are_tables = matches!(
                    (&*base_value, &value),
                    (toml::Value::Table(_), toml::Value::Table(_))
                );
                if both_are_tables {
                    // RECURSIVE MERGE
                    if let toml::Value::Table(base_table) = base_value {
                        if let toml::Value::Table(overlay_table) = value {
                            deep_merge_tables(base_table, overlay_table);
                        }
                    }
                } else {
                    *base_value = value;
                }
            }
            None => {
                base.insert(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::Value;

    #[test]
    fn test_deep_merge_nested_tables() {
        let mut base = toml::from_str(
            r#"
            [mpsc_channels.shared]
            event_queue_buffer_size = 5000
            "#,
        )
        .unwrap();

        let overlay = toml::from_str(
            r#"
            [mpsc_channels.orchestration]
            command_buffer_size = 1000
            "#,
        )
        .unwrap();

        if let (Value::Table(base_table), Value::Table(overlay_table)) = (base, overlay) {
            let mut merged = base_table;
            deep_merge_tables(&mut merged, overlay_table);

            // Both subsections should exist
            let mpsc = merged.get("mpsc_channels").unwrap().as_table().unwrap();
            assert!(mpsc.contains_key("shared"));
            assert!(mpsc.contains_key("orchestration"));
        }
    }

    #[test]
    fn test_deep_merge_scalar_replacement() {
        let mut base: Value = toml::from_str(
            r#"
            [database.pool]
            max_connections = 30
            "#,
        )
        .unwrap();

        let overlay: Value = toml::from_str(
            r#"
            [database.pool]
            max_connections = 10
            "#,
        )
        .unwrap();

        if let (Value::Table(base_table), Value::Table(overlay_table)) = (base, overlay) {
            let mut merged = base_table;
            deep_merge_tables(&mut merged, overlay_table);

            let pool = merged
                .get("database")
                .unwrap()
                .as_table()
                .unwrap()
                .get("pool")
                .unwrap()
                .as_table()
                .unwrap();
            assert_eq!(pool.get("max_connections").unwrap().as_integer(), Some(10));
        }
    }
}
```

#### 2.2.2 Update All Merge Call Sites

**UnifiedConfigLoader** - use shared implementation:
```rust
// tasker-shared/src/config/unified_loader.rs
use crate::config::merge::deep_merge_tables;

fn merge_toml(&self, base: &mut toml::Value, overlay: toml::Value) -> ConfigResult<()> {
    if let (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) = (base, overlay) {
        deep_merge_tables(base_table, overlay_table);  // Use canonical implementation
    }
    Ok(())
}
```

**ConfigMerger** - use shared implementation:
```rust
// tasker-shared/src/config/merger.rs
use crate::config::merge::deep_merge_tables;

// Remove local deep_merge_tables implementation (lines 212-251)
// Use crate::config::merge::deep_merge_tables directly
```

**ConfigManager** - eliminate duplicate merge logic:
```rust
// tasker-shared/src/config/manager.rs
// Remove any local merge implementations
// Delegate to UnifiedConfigLoader or ConfigMerger
```

**Impact**: Single merge implementation, consistent behavior across all loading paths.

### 2.3 Eliminate Environment Variable Substitution Duplication

**Problem**: Env var substitution implemented in both `UnifiedConfigLoader` and `ConfigManager`.

**Solution**: Consolidate to `UnifiedConfigLoader`.

```rust
// ConfigManager should NOT duplicate env var logic
// Remove duplicate implementation from manager.rs
// Always use UnifiedConfigLoader for loading + substitution
```

### 2.4 Fix Naming Inconsistencies

**Problem**: TOML table names differ from struct field names.

**Solution**: Align TOML with struct names (less breaking than changing structs).

```toml
# BEFORE (orchestration.toml)
[orchestration_system]        # Struct: TaskerConfig.orchestration
mode = "standalone"

# AFTER
[orchestration]               # Matches struct field name
mode = "standalone"


# BEFORE (worker.toml)
[worker_system]               # Struct: TaskerConfig.worker
worker_id = "test-worker-001"

# AFTER
[worker]                      # Matches struct field name
worker_id = "test-worker-001"


# BEFORE (orchestration.toml)
[orchestration_events]        # Struct: event_systems.orchestration
system_id = "orch-event-system"

[task_readiness_events]       # Struct: event_systems.task_readiness
system_id = "task-readiness-event-system"

# AFTER
[event_systems.orchestration]
system_id = "orch-event-system"

[event_systems.task_readiness]
system_id = "task-readiness-event-system"


# BEFORE (worker.toml)
[worker_events]               # Struct: event_systems.worker
system_id = "worker-event-system"

# AFTER
[event_systems.worker]
system_id = "worker-event-system"
```

**Impact**: TOML structure directly mirrors Rust struct hierarchy, no custom deserialization mapping needed.

### Phase 2 Testing

```bash
# Verify TaskReadinessConfig removal
cargo build --all-features
cargo test --all-features

# Verify no references remain
rg "TaskReadinessConfig" --type rust
# Should only find migration notes/documentation

# Verify merge consolidation
cargo test --package tasker-shared --test config_merge_tests

# Verify naming alignment
cargo run --bin config-validator
```

### Phase 2 Success Criteria

- ✅ TaskReadinessConfig completely removed (574 lines deleted)
- ✅ Single canonical merge implementation used everywhere
- ✅ TOML table names match struct field names
- ✅ All tests pass without modification
- ✅ No breaking changes to public APIs

---

## Part 3: Architectural Redesign (Phase 3 - TAS-62)

### Goal
Adopt `{ common, orchestration, worker }` architecture for long-term maintainability.

### 3.1 Proposed Struct Architecture

#### 3.1.1 New Root Configuration
**File**: `tasker-shared/src/config/tasker.rs`

```rust
/// Root Tasker configuration with context-based organization
///
/// # Architecture
///
/// - `common`: Required configuration used by SystemContext across all deployments
/// - `orchestration`: Optional orchestration-specific configuration
/// - `worker`: Optional worker-specific configuration
///
/// # Deployment Contexts
///
/// - **Orchestration-only**: `{ common, orchestration: Some(_), worker: None }`
/// - **Worker-only**: `{ common, orchestration: None, worker: Some(_) }`
/// - **Combined**: `{ common, orchestration: Some(_), worker: Some(_) }`
/// - **Minimal**: `{ common, orchestration: None, worker: None }` (testing only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskerConfig {
    /// Common configuration shared across all contexts
    /// Used to initialize SystemContext
    pub common: CommonConfig,

    /// Orchestration-specific configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationConfig>,

    /// Worker-specific configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<WorkerConfig>,
}

impl TaskerConfig {
    /// Create orchestration-only configuration
    pub fn for_orchestration(common: CommonConfig, orchestration: OrchestrationConfig) -> Self {
        Self {
            common,
            orchestration: Some(orchestration),
            worker: None,
        }
    }

    /// Create worker-only configuration
    pub fn for_worker(common: CommonConfig, worker: WorkerConfig) -> Self {
        Self {
            common,
            orchestration: None,
            worker: Some(worker),
        }
    }

    /// Create combined configuration (both orchestration and worker)
    pub fn combined(
        common: CommonConfig,
        orchestration: OrchestrationConfig,
        worker: WorkerConfig,
    ) -> Self {
        Self {
            common,
            orchestration: Some(orchestration),
            worker: Some(worker),
        }
    }
}
```

#### 3.1.2 CommonConfig Definition
**File**: `tasker-shared/src/config/common.rs` (NEW)

```rust
/// Common configuration shared across all deployment contexts
///
/// This configuration is used to initialize SystemContext and is required
/// for all deployments regardless of whether they run orchestration, worker,
/// or both.
///
/// # SystemContext Dependencies
///
/// SystemContext::from_config() requires:
/// - `database` for database pool initialization
/// - `circuit_breakers` for CircuitBreakerManager creation (optional)
/// - `mpsc_channels.shared` for event publisher and FFI channels
/// - `queues` for message queue client initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonConfig {
    /// Environment identifier (test, development, production)
    /// Previously: execution.environment
    pub environment: String,

    /// Database configuration and connection pooling
    /// Used by: SystemContext (pool initialization)
    pub database: DatabaseConfig,

    /// Circuit breaker configuration for resilience
    /// Used by: SystemContext (optional CircuitBreakerManager)
    pub circuit_breakers: CircuitBreakerConfig,

    /// Shared MPSC channel configuration
    /// Used by: SystemContext (event_publisher), Ruby FFI (ffi channels)
    pub mpsc_channels: SharedMpscConfig,

    /// Queue configuration (backend, namespaces, orchestration queues)
    /// Used by: SystemContext (message client initialization)
    pub queues: QueuesConfig,

    /// Telemetry configuration (metrics, tracing)
    /// Used by: Telemetry initialization (if enabled)
    pub telemetry: TelemetryConfig,

    /// System-level configuration (resource limits, process name)
    /// Used by: System monitoring and resource management
    pub system: SystemConfig,

    /// Task template configuration
    /// Used by: Task handler registry, template discovery
    pub task_templates: TaskTemplatesConfig,
}

impl CommonConfig {
    /// Validate common configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate database URL is set
        if self.database.url.is_empty() {
            return Err(ConfigError::ValidationError(
                "database.url must be set".to_string(),
            ));
        }

        // Validate environment is recognized
        let valid_envs = ["test", "development", "production"];
        if !valid_envs.contains(&self.environment.as_str()) {
            return Err(ConfigError::ValidationError(format!(
                "Invalid environment '{}', must be one of: {}",
                self.environment,
                valid_envs.join(", ")
            )));
        }

        Ok(())
    }
}

/// Shared MPSC channel configuration (common across contexts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMpscConfig {
    /// Event publisher channel configuration
    pub event_publisher: EventPublisherChannelConfig,

    /// FFI (Ruby, Python, etc.) channel configuration
    pub ffi: FfiChannelConfig,

    /// Overflow policy for channel backpressure
    pub overflow_policy: ChannelOverflowPolicy,
}
```

#### 3.1.3 OrchestrationConfig Definition
**File**: `tasker-shared/src/config/orchestration/mod.rs` (REFACTOR)

```rust
/// Orchestration-specific configuration
///
/// This configuration is only present when running in orchestration context.
/// Contains all settings needed for task orchestration, step enqueueing,
/// and result processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    /// Backoff configuration for retries and re-enqueueing
    pub backoff: BackoffConfig,

    /// Orchestration system settings (mode, web API)
    pub system: OrchestrationSystemConfig,

    /// Orchestration-specific MPSC channels
    pub mpsc_channels: OrchestrationMpscConfig,

    /// Event systems configuration (orchestration + task readiness)
    pub event_systems: OrchestrationEventSystemsConfig,

    /// Staleness detection configuration
    pub staleness_detection: StalenessDetectionConfig,

    /// Dead letter queue configuration
    pub dlq: DlqConfig,

    /// Task archival configuration
    pub archive: ArchivalConfig,
}

/// Orchestration event systems configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEventSystemsConfig {
    /// Orchestration event system (task requests, finalizations, etc.)
    pub orchestration: EventSystemConfig<OrchestrationEventSystemMetadata>,

    /// Task readiness event system (step discovery and enqueueing)
    pub task_readiness: EventSystemConfig<TaskReadinessEventSystemMetadata>,
}

/// Orchestration-specific MPSC channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationMpscConfig {
    /// Command processor channel configuration
    pub command_processor: CommandProcessorChannelConfig,

    /// Event listeners channel configuration (PGMQ events)
    pub event_listeners: EventListenersChannelConfig,

    /// Event systems channel configuration
    pub event_systems: EventSystemsChannelConfig,

    /// Task readiness channel configuration
    pub task_readiness: TaskReadinessMpscConfig,
}
```

#### 3.1.4 WorkerConfig Definition
**File**: `tasker-shared/src/config/worker.rs` (REFACTOR)

```rust
/// Worker-specific configuration
///
/// This configuration is only present when running in worker context.
/// Contains all settings needed for step execution, handler management,
/// and result submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Worker system settings (worker_id, type, step processing)
    pub system: WorkerSystemConfig,

    /// Worker-specific MPSC channels
    pub mpsc_channels: WorkerMpscConfig,

    /// Worker event system configuration
    pub event_system: EventSystemConfig<WorkerEventSystemMetadata>,
}

/// Worker-specific MPSC channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMpscConfig {
    /// Command processor channel configuration
    pub command_processor: CommandProcessorChannelConfig,

    /// Event listeners channel configuration (PGMQ events)
    pub event_listeners: EventListenersChannelConfig,

    /// Event subscribers channel configuration (completions, results)
    pub event_subscribers: EventSubscribersChannelConfig,

    /// Event systems channel configuration
    pub event_systems: EventSystemsChannelConfig,

    /// In-process events channel configuration (broadcast)
    pub in_process_events: InProcessEventsChannelConfig,
}
```

### 3.2 Proposed TOML Structure

#### 3.2.1 Common Configuration
**File**: `config/tasker/base/common.toml`

```toml
# Tasker Common Configuration
# Required for all deployment contexts

environment = "development"

# ============================================================================
# DATABASE CONFIGURATION
# ============================================================================

[common.database]
url = "${DATABASE_URL:-postgresql://tasker:tasker@localhost:5432/tasker_development}"

[common.database.pool]
max_connections = 30
min_connections = 8
acquire_timeout_seconds = 30
idle_timeout_seconds = 300
max_lifetime_seconds = 3600

[common.database.variables]
statement_timeout = 5000

# ============================================================================
# CIRCUIT BREAKER CONFIGURATION
# ============================================================================

[common.circuit_breakers]
enabled = true

[common.circuit_breakers.global_settings]
max_circuit_breakers = 50
metrics_collection_interval_seconds = 30
min_state_transition_interval_seconds = 1

[common.circuit_breakers.default_config]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2

[common.circuit_breakers.component_configs.pgmq]
failure_threshold = 3
timeout_seconds = 15
success_threshold = 2

# ============================================================================
# SHARED MPSC CHANNELS
# ============================================================================

[common.mpsc_channels.event_publisher]
event_queue_buffer_size = 5000

[common.mpsc_channels.ffi]
ruby_event_buffer_size = 1000

[common.mpsc_channels.overflow_policy]
log_warning_threshold = 0.8
drop_policy = "block"

[common.mpsc_channels.overflow_policy.metrics]
enabled = true
saturation_check_interval_seconds = 60

# ============================================================================
# QUEUE CONFIGURATION
# ============================================================================

[common.queues]
backend = "pgmq"
orchestration_namespace = "orchestration"
worker_namespace = "worker"
naming_pattern = "{namespace}_{name}_queue"
health_check_interval = 60
default_batch_size = 10
max_batch_size = 100
default_visibility_timeout_seconds = 30

[common.queues.orchestration_queues]
task_requests = "orchestration_task_requests_queue"
task_finalizations = "orchestration_task_finalizations_queue"
step_results = "orchestration_step_results_queue"

[common.queues.pgmq]
poll_interval_ms = 250
shutdown_timeout_seconds = 5
max_retries = 3

[common.queues.rabbitmq]
connection_timeout_seconds = 10

# ============================================================================
# TELEMETRY CONFIGURATION
# ============================================================================

[common.telemetry]
enabled = true

[common.telemetry.metrics]
enabled = true
collection_interval_seconds = 60
export_endpoint = "${TELEMETRY_ENDPOINT:-}"

[common.telemetry.tracing]
enabled = true
sampling_rate = 1.0
export_endpoint = "${TRACING_ENDPOINT:-}"

# ============================================================================
# SYSTEM CONFIGURATION
# ============================================================================

[common.system]
max_memory_mb = 2048
max_cpu_percent = 80.0
process_name = "tasker"

# ============================================================================
# TASK TEMPLATES CONFIGURATION
# ============================================================================

[common.task_templates]
cache_enabled = true
template_path = "${TASKER_TEMPLATE_PATH:-}"
auto_reload = true
reload_interval_seconds = 300
```

#### 3.2.2 Orchestration Configuration
**File**: `config/tasker/base/orchestration.toml`

```toml
# Tasker Orchestration Configuration
# Only loaded for orchestration context

# ============================================================================
# BACKOFF CONFIGURATION
# ============================================================================

[orchestration.backoff]
backoff_multiplier = 2.0
jitter_enabled = true
jitter_max_percentage = 0.1
max_backoff_seconds = 3600

[orchestration.backoff.default_backoff_seconds]
0 = 1
1 = 2
2 = 4
3 = 8
4 = 16

[orchestration.backoff.reenqueue_delays]
waiting_for_dependencies = 60
waiting_for_retry = 30
blocked_by_failures = 60
enqueuing_steps = 0
steps_in_process = 10
evaluating_results = 5
initializing = 5

# ============================================================================
# ORCHESTRATION SYSTEM
# ============================================================================

[orchestration.system]
mode = "standalone"
enable_performance_logging = true

[orchestration.system.web]
enabled = true
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8080}"
request_timeout_ms = 30000

[orchestration.system.web.auth]
enabled = false
api_key_header = "X-API-Key"
# ... (rest of auth config)

[orchestration.system.web.cors]
enabled = true
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"]
allowed_headers = ["*"]

# ============================================================================
# ORCHESTRATION MPSC CHANNELS
# ============================================================================

[orchestration.mpsc_channels.command_processor]
command_buffer_size = 1000

[orchestration.mpsc_channels.event_listeners]
pgmq_event_buffer_size = 10000

[orchestration.mpsc_channels.event_systems]
event_channel_buffer_size = 1000

[orchestration.mpsc_channels.task_readiness.event_channel]
buffer_size = 1000
send_timeout_ms = 50

# ============================================================================
# EVENT SYSTEMS
# ============================================================================

[orchestration.event_systems.orchestration]
system_id = "orchestration-event-system"
deployment_mode = "Hybrid"

[orchestration.event_systems.orchestration.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 5
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[orchestration.event_systems.orchestration.processing]
max_concurrent_operations = 10
batch_size = 10
max_retries = 3

[orchestration.event_systems.orchestration.processing.backoff]
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[orchestration.event_systems.orchestration.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5

[orchestration.event_systems.task_readiness]
system_id = "task-readiness-event-system"
deployment_mode = "Hybrid"

[orchestration.event_systems.task_readiness.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 5
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[orchestration.event_systems.task_readiness.processing]
max_concurrent_operations = 100
batch_size = 50
max_retries = 3

[orchestration.event_systems.task_readiness.processing.backoff]
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[orchestration.event_systems.task_readiness.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5

# ============================================================================
# STALENESS DETECTION
# ============================================================================

[orchestration.staleness_detection]
enabled = true
detection_interval_seconds = 300
batch_size = 100
dry_run = false

[orchestration.staleness_detection.thresholds]
steps_in_process_minutes = 30
waiting_for_dependencies_minutes = 60
waiting_for_retry_minutes = 30
task_max_lifetime_hours = 24

[orchestration.staleness_detection.actions]
auto_transition_to_error = true
auto_move_to_dlq = true
emit_events = true
event_channel = "task_staleness_detected"

# ============================================================================
# DEAD LETTER QUEUE
# ============================================================================

[orchestration.dlq]
enabled = true
auto_dlq_on_staleness = true
include_full_task_snapshot = true
max_pending_age_hours = 168

[orchestration.dlq.reasons]
max_retries_exceeded = true
staleness_timeout = true
dependency_cycle_detected = true
worker_unavailable = true
manual_dlq = true

# ============================================================================
# ARCHIVAL
# ============================================================================

[orchestration.archive]
enabled = true
retention_days = 90
archive_interval_hours = 24
archive_batch_size = 1000

[orchestration.archive.policies]
archive_completed = true
archive_failed = true
archive_cancelled = false
archive_dlq_resolved = true
```

#### 3.2.3 Worker Configuration
**File**: `config/tasker/base/worker.toml`

```toml
# Tasker Worker Configuration
# Only loaded for worker context

# ============================================================================
# WORKER SYSTEM
# ============================================================================

[worker.system]
worker_id = "default-worker-001"
worker_type = "general"

[worker.system.event_driven]
enabled = true
deployment_mode = "Hybrid"
health_monitoring_enabled = true
health_check_interval_seconds = 30

[worker.system.fallback_poller]
enabled = true
polling_interval_ms = 5000
batch_size = 10
age_threshold_seconds = 10
max_age_hours = 12
visibility_timeout_seconds = 30
processable_states = ["pending", "waiting_for_dependencies", "waiting_for_retry"]

[worker.system.listener]
batch_processing = true
connection_timeout_seconds = 10
event_timeout_seconds = 30
health_check_interval_seconds = 60
max_retry_attempts = 3
retry_interval_seconds = 5

[worker.system.queue_config]
batch_size = 10
polling_interval_ms = 100
visibility_timeout_seconds = 30

[worker.system.resource_limits]
max_cpu_percent = 80
max_memory_mb = 512
max_database_connections = 10
max_queue_connections = 20

[worker.system.step_processing]
max_concurrent_steps = 10
max_retries = 3
claim_timeout_seconds = 300

[worker.system.health_monitoring]
health_check_interval_seconds = 10
error_rate_threshold = 0.05
performance_monitoring_enabled = true

[worker.system.web]
enabled = true
bind_address = "${TASKER_WEB_BIND_ADDRESS:-0.0.0.0:8081}"
request_timeout_ms = 30000

# ============================================================================
# WORKER MPSC CHANNELS
# ============================================================================

[worker.mpsc_channels.command_processor]
command_buffer_size = 1000

[worker.mpsc_channels.event_listeners]
pgmq_event_buffer_size = 1000

[worker.mpsc_channels.event_subscribers]
completion_buffer_size = 1000
result_buffer_size = 1000

[worker.mpsc_channels.event_systems]
event_channel_buffer_size = 1000

[worker.mpsc_channels.in_process_events]
broadcast_buffer_size = 1000

# ============================================================================
# WORKER EVENT SYSTEM
# ============================================================================

[worker.event_system]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[worker.event_system.timing]
health_check_interval_seconds = 30
fallback_polling_interval_seconds = 1
visibility_timeout_seconds = 30
processing_timeout_seconds = 30
claim_timeout_seconds = 300

[worker.event_system.processing]
max_concurrent_operations = 100
batch_size = 10
max_retries = 3

[worker.event_system.processing.backoff]
initial_delay_ms = 100
max_delay_ms = 5000
multiplier = 2.0
jitter_percent = 0.1

[worker.event_system.health]
enabled = true
performance_monitoring_enabled = true
max_consecutive_errors = 10
error_rate_threshold_per_minute = 5

[worker.event_system.metadata.in_process_events]
deduplication_cache_size = 1000
ffi_integration_enabled = true

[worker.event_system.metadata.listener]
batch_processing = true
connection_timeout_seconds = 10
event_timeout_seconds = 30
max_retry_attempts = 3
retry_interval_seconds = 5

[worker.event_system.metadata.fallback_poller]
enabled = true
polling_interval_ms = 500
batch_size = 10
age_threshold_seconds = 2
max_age_hours = 12
visibility_timeout_seconds = 30

[worker.event_system.metadata.resource_limits]
max_cpu_percent = 80.0
max_memory_mb = 2048
max_database_connections = 50
max_queue_connections = 20
```

### 3.3 Configuration Loading Updates

#### 3.3.1 Context-Based Loading
**File**: `tasker-shared/src/config/unified_loader.rs`

```rust
/// Load configuration for specific context
pub fn load_context(
    &self,
    context: ConfigContext,
) -> ConfigResult<TaskerConfig> {
    match context {
        ConfigContext::Orchestration => {
            // Load common + orchestration
            let common = self.load_common_config()?;
            let orchestration = self.load_orchestration_config()?;

            Ok(TaskerConfig::for_orchestration(common, orchestration))
        }
        ConfigContext::Worker => {
            // Load common + worker
            let common = self.load_common_config()?;
            let worker = self.load_worker_config()?;

            Ok(TaskerConfig::for_worker(common, worker))
        }
        ConfigContext::Combined => {
            // Load common + orchestration + worker
            let common = self.load_common_config()?;
            let orchestration = self.load_orchestration_config()?;
            let worker = self.load_worker_config()?;

            Ok(TaskerConfig::combined(common, orchestration, worker))
        }
    }
}

fn load_common_config(&self) -> ConfigResult<CommonConfig> {
    // Load base/common.toml + environments/{env}/common.toml
    let base = self.load_base_file("common.toml")?;
    let env_overlay = self.load_environment_file("common.toml")?;

    let mut merged = base;
    if let Some(overlay) = env_overlay {
        self.merge_toml(&mut merged, overlay)?;
    }

    // Deserialize into CommonConfig
    let common: CommonConfig = toml::from_str(&merged.to_string())
        .map_err(|e| ConfigError::DeserializationError(e.to_string()))?;

    common.validate()?;
    Ok(common)
}

fn load_orchestration_config(&self) -> ConfigResult<OrchestrationConfig> {
    // Load base/orchestration.toml + environments/{env}/orchestration.toml
    let base = self.load_base_file("orchestration.toml")?;
    let env_overlay = self.load_environment_file("orchestration.toml")?;

    let mut merged = base;
    if let Some(overlay) = env_overlay {
        self.merge_toml(&mut merged, overlay)?;
    }

    // Deserialize into OrchestrationConfig
    let orchestration: OrchestrationConfig = toml::from_str(&merged.to_string())
        .map_err(|e| ConfigError::DeserializationError(e.to_string()))?;

    Ok(orchestration)
}

fn load_worker_config(&self) -> ConfigResult<WorkerConfig> {
    // Load base/worker.toml + environments/{env}/worker.toml
    let base = self.load_base_file("worker.toml")?;
    let env_overlay = self.load_environment_file("worker.toml")?;

    let mut merged = base;
    if let Some(overlay) = env_overlay {
        self.merge_toml(&mut merged, overlay)?;
    }

    // Deserialize into WorkerConfig
    let worker: WorkerConfig = toml::from_str(&merged.to_string())
        .map_err(|e| ConfigError::DeserializationError(e.to_string()))?;

    Ok(worker)
}
```

#### 3.3.2 SystemContext Updates
**File**: `tasker-shared/src/system_context.rs`

```rust
/// Create SystemContext from configuration
pub async fn from_config(config: Arc<TaskerConfig>) -> TaskerResult<Self> {
    info!("Initializing SystemContext from common configuration");

    // Use common config for SystemContext initialization
    let common = &config.common;

    // Database pool from common config
    let database_pool = Self::get_pg_pool_options(&common.database)
        .connect(&common.database.url)
        .await
        .map_err(|e| {
            TaskerError::DatabaseError(format!(
                "Failed to connect to database: {e}"
            ))
        })?;

    // Circuit breaker from common config
    let circuit_breaker_manager = if common.circuit_breakers.enabled {
        Some(Arc::new(CircuitBreakerManager::from_config(
            &common.circuit_breakers,
        )))
    } else {
        None
    };

    // Message client from common config (queues + database)
    let message_client = Arc::new(UnifiedPgmqClient::new_standard(
        PgmqClient::new_with_pool(database_pool.clone()).await,
    ));

    // Event publisher from common config (mpsc_channels.shared)
    let event_publisher_buffer_size =
        common.mpsc_channels.event_publisher.event_queue_buffer_size;
    let event_publisher = Arc::new(EventPublisher::with_capacity(event_publisher_buffer_size));

    // Task handler registry
    let task_handler_registry = Arc::new(TaskHandlerRegistry::new(database_pool.clone()));

    Ok(Self {
        processor_uuid: Uuid::now_v7(),
        tasker_config: config,  // Store full config for context-specific access
        message_client,
        database_pool,
        task_handler_registry,
        circuit_breaker_manager,
        event_publisher,
    })
}

// NO MORE ..Default::default() HACKS!
```

#### 3.3.3 Bootstrap Updates

**Orchestration Bootstrap**:
```rust
// tasker-orchestration/src/orchestration/bootstrap.rs

pub async fn bootstrap() -> TaskerResult<OrchestrationCore> {
    // Load orchestration context configuration
    let config = TaskerConfig::load_for_orchestration().await?;

    // Create SystemContext from common config
    let system_context = SystemContext::from_config(Arc::new(config.clone())).await?;

    // Access orchestration-specific config
    let orch_config = config.orchestration.as_ref()
        .ok_or(TaskerError::ConfigurationError(
            "Orchestration config required for orchestration context".to_string()
        ))?;

    // Use orchestration event systems
    let orchestration_events = &orch_config.event_systems.orchestration;
    let task_readiness_events = &orch_config.event_systems.task_readiness;

    // Use orchestration MPSC channels
    let command_buffer = orch_config.mpsc_channels.command_processor.command_buffer_size;

    // ... rest of bootstrap
}
```

**Worker Bootstrap**:
```rust
// tasker-worker/src/bootstrap.rs

pub async fn bootstrap() -> TaskerResult<WorkerCore> {
    // Load worker context configuration
    let config = TaskerConfig::load_for_worker().await?;

    // Create SystemContext from common config
    let system_context = SystemContext::from_config(Arc::new(config.clone())).await?;

    // Access worker-specific config
    let worker_config = config.worker.as_ref()
        .ok_or(TaskerError::ConfigurationError(
            "Worker config required for worker context".to_string()
        ))?;

    // Use worker event system
    let worker_events = &worker_config.event_system;

    // Use worker MPSC channels
    let command_buffer = worker_config.mpsc_channels.command_processor.command_buffer_size;

    // ... rest of bootstrap
}
```

### 3.4 Migration Path

#### 3.4.1 Backward Compatibility During Migration

**Dual Loading Support**:
```rust
impl TaskerConfig {
    /// Load using new architecture (Phase 3)
    pub fn load_v2_for_orchestration() -> ConfigResult<Self> {
        // Load { common, orchestration, worker: None }
    }

    /// Load using legacy architecture (Phase 1-2) - DEPRECATED
    #[deprecated(since = "0.3.0", note = "Use load_v2_for_orchestration instead")]
    pub fn load_legacy_orchestration() -> ConfigResult<Self> {
        // Old monolithic loading for backward compatibility
    }
}
```

#### 3.4.2 Migration Steps

**Step 1**: Add new structs alongside old (no breaking changes)
- CommonConfig, OrchestrationConfig (new), WorkerConfig (refactored)
- Keep old TaskerConfig fields for compatibility

**Step 2**: Add new TOML files alongside old
- `config/tasker/base/common.toml` (new v2 format)
- `config/tasker/base/orchestration.toml` (new v2 format)
- `config/tasker/base/worker.toml` (new v2 format)
- Keep old files for backward compatibility

**Step 3**: Update bootstrap code to use new architecture
- SystemContext uses CommonConfig only
- Orchestration/Worker bootstrap access context-specific config

**Step 4**: Deprecate old loading paths
- Mark legacy loading methods as deprecated
- Update documentation to recommend new approach

**Step 5**: Remove old structs and files (breaking change, major version)
- Remove old TaskerConfig structure
- Remove old TOML files
- Remove backward compatibility code

### Phase 3 Testing

```bash
# Test new architecture loading
cargo test --package tasker-shared test_load_orchestration_v2
cargo test --package tasker-shared test_load_worker_v2
cargo test --package tasker-shared test_load_combined_v2

# Test SystemContext initialization from CommonConfig only
cargo test --package tasker-shared test_system_context_from_common

# Test orchestration bootstrap with new config
cargo test --package tasker-orchestration test_bootstrap_with_v2_config

# Test worker bootstrap with new config
cargo test --package tasker-worker test_bootstrap_with_v2_config

# Integration tests with new architecture
cargo test --test rust_worker_e2e_integration_tests

# Verify no ..Default::default() usage remains
rg "\.\.TaskerConfig::default\(\)" --type rust
# Should find zero matches after Phase 3
```

### Phase 3 Success Criteria

- ✅ New `{ common, orchestration, worker }` architecture fully implemented
- ✅ TOML structure directly mirrors Rust struct hierarchy
- ✅ SystemContext initialized from CommonConfig only (no full TaskerConfig needed)
- ✅ No backward compatibility hacks (`..Default::default()` eliminated)
- ✅ Context-specific config truly optional (orchestration and worker can be None)
- ✅ All tests pass with new architecture
- ✅ Documentation updated with new configuration approach
- ✅ Migration guide provided for existing deployments

---

## Part 4: Benefits Analysis

### Immediate Benefits (Phase 1)

1. **Configuration Generation Works**: No more "missing field 'telemetry'" errors
2. **Nested Tables Merge Correctly**: Fixes "missing field 'orchestration' in 'mpsc_channels'"
3. **TOML Accurately Reflects Structs**: No extraneous fields silently ignored
4. **All Required Fields Present**: execution.environment available, decision_points merged

### Cleanup Benefits (Phase 2)

1. **574 Lines Deleted**: TaskReadinessConfig completely removed
2. **Single Merge Implementation**: Consistent behavior across all loading paths
3. **No Code Duplication**: Env var substitution consolidated
4. **Semantic TOML Names**: Table names match struct field names exactly

### Architectural Benefits (Phase 3)

1. **Clear Context Separation**: `{ common, orchestration, worker }` mirrors deployment reality
2. **Minimal SystemContext Coupling**: Only depends on common config (4 fields)
3. **True Optional Contexts**: Orchestration and worker truly optional based on deployment
4. **Type Safety**: No `..Default::default()` filling missing fields incorrectly
5. **Maintainability**: TOML structure semantically near Rust structs
6. **Zero Configuration Sprawl**: All config fields actually used

### Performance Benefits

1. **Smaller Config Footprint**: Only load configuration needed for context
2. **Faster Deserialization**: Smaller structs, targeted loading
3. **Reduced Memory Usage**: No unused configuration in memory

### Developer Experience Benefits

1. **Easy to Understand**: Structure mirrors deployment contexts
2. **Easy to Extend**: Add new common/orchestration/worker config independently
3. **Easy to Test**: Can test contexts in isolation
4. **Easy to Debug**: Clear separation makes issues obvious

---

## Part 5: Risk Analysis and Mitigation

### Phase 1 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Deep merge breaks existing behavior | Low | Medium | Comprehensive test coverage, phased rollout |
| New required fields break deployments | Medium | High | Environment variable defaults, documentation |
| Extraneous field removal breaks Rails | Low | Low | Verify Rails doesn't rely on ignored fields |

### Phase 2 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| TaskReadinessConfig removal breaks code | Low | High | Grep search confirms zero usage |
| TOML renaming breaks existing configs | Medium | Medium | Deprecation period, migration script |
| Merge consolidation introduces bugs | Low | Medium | Extensive test coverage |

### Phase 3 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Major architecture change breaks APIs | High | High | Dual loading support, gradual migration |
| SystemContext changes break bootstrap | Medium | High | Backward compatibility layer during migration |
| TOML v2 format breaks existing tools | Medium | Medium | Keep v1 files during transition period |

### General Mitigations

1. **Comprehensive Test Coverage**: Add tests for all new loading paths
2. **Gradual Migration**: Keep backward compatibility during transition
3. **Documentation**: Clear migration guides for each phase
4. **Rollback Plan**: Ability to revert to previous configuration system
5. **Monitoring**: Track configuration loading success rates in production

---

## Part 6: Implementation Checklist

### Phase 1 (TAS-60 Completion)

- [ ] Copy `deep_merge_tables()` to `UnifiedConfigLoader`
- [ ] Add `telemetry` section to `config/tasker/base/common.toml`
- [ ] Add `system` section to `config/tasker/base/common.toml`
- [ ] Add `execution` section to `config/tasker/base/common.toml`
- [ ] Add `task_templates` section to `config/tasker/base/common.toml`
- [ ] Update `ConfigMerger` to include `decision_points.toml`
- [ ] Remove extraneous database fields from `common.toml`
- [ ] Remove extraneous queue fields from `common.toml`
- [ ] Add environment overrides for new sections
- [ ] Test config generation for all contexts (orchestration, worker, complete)
- [ ] Verify `config-validator` passes
- [ ] Update complete-test.toml generation
- [ ] Run full test suite

### Phase 2 (TAS-61 Cleanup)

- [ ] Delete `tasker-shared/src/config/task_readiness.rs`
- [ ] Remove `task_readiness` field from `TaskerConfig`
- [ ] Remove `task_readiness` exports from `mod.rs`
- [ ] Update test assertions to use `mpsc_channels.task_readiness`
- [ ] Create `tasker-shared/src/config/merge.rs` with canonical implementation
- [ ] Update `UnifiedConfigLoader` to use shared merge
- [ ] Update `ConfigMerger` to use shared merge
- [ ] Remove duplicate merge implementations
- [ ] Consolidate env var substitution to `UnifiedConfigLoader`
- [ ] Rename TOML tables to match struct names
  - [ ] `[orchestration_system]` → `[orchestration]`
  - [ ] `[worker_system]` → `[worker]`
  - [ ] `[orchestration_events]` → `[event_systems.orchestration]`
  - [ ] `[task_readiness_events]` → `[event_systems.task_readiness]`
  - [ ] `[worker_events]` → `[event_systems.worker]`
- [ ] Update all tests to reflect new names
- [ ] Run full test suite

### Phase 3 (TAS-62 Redesign)

- [ ] Create `tasker-shared/src/config/common.rs` with `CommonConfig`
- [ ] Refactor `OrchestrationConfig` to new structure
- [ ] Refactor `WorkerConfig` to new structure
- [ ] Update `TaskerConfig` to `{ common, orchestration, worker }`
- [ ] Create new TOML structure:
  - [ ] `config/tasker/v2/base/common.toml`
  - [ ] `config/tasker/v2/base/orchestration.toml`
  - [ ] `config/tasker/v2/base/worker.toml`
- [ ] Update `UnifiedConfigLoader` with context-specific loading
- [ ] Update `SystemContext::from_config()` to use `CommonConfig` only
- [ ] Update orchestration bootstrap to use new config
- [ ] Update worker bootstrap to use new config
- [ ] Add backward compatibility layer (`load_legacy_*` methods)
- [ ] Add deprecation warnings to old methods
- [ ] Write comprehensive migration tests
- [ ] Update documentation
- [ ] Create migration guide
- [ ] Run full test suite
- [ ] Run integration tests

---

## Part 7: Documentation Updates

### Files to Update

1. **README.md** - Update configuration section
2. **docs/configuration.md** - Comprehensive configuration guide (create if missing)
3. **CHANGELOG.md** - Document all changes across phases
4. **MIGRATION.md** - Step-by-step migration guide for users
5. **tasker-shared/src/config/mod.rs** - Update module documentation
6. **SystemContext rustdoc** - Update usage examples

### Documentation Outline

```markdown
# Configuration Guide

## Architecture

Tasker uses a context-based configuration architecture:
- **Common**: Required configuration shared across all contexts
- **Orchestration**: Optional configuration for orchestration deployments
- **Worker**: Optional configuration for worker deployments

## File Structure

config/tasker/base/
├── common.toml           # Common configuration
├── orchestration.toml    # Orchestration-specific configuration
└── worker.toml          # Worker-specific configuration

config/tasker/environments/{env}/
├── common.toml           # Environment-specific common overrides
├── orchestration.toml    # Environment-specific orchestration overrides
└── worker.toml          # Environment-specific worker overrides

## Loading Configuration

### Orchestration Context
TASKER_ENV=production TASKER_CONFIG_ROOT=./config cargo run --bin orchestration-server

### Worker Context
TASKER_ENV=production TASKER_CONFIG_ROOT=./config cargo run --bin worker-server

### Programmatic Loading
use tasker_shared::config::TaskerConfig;

let config = TaskerConfig::load_for_orchestration().await?;
let system_context = SystemContext::from_config(Arc::new(config)).await?;

## Configuration Reference

[Detailed field-by-field documentation...]
```

---

## Conclusion

This proposal provides a comprehensive path from the current broken state to a maintainable, semantically clear configuration architecture aligned with the user's vision: `{ common, orchestration, worker }`.

**Key Achievements**:
1. **Immediate Fix** (Phase 1) - Makes configuration functional
2. **Cleanup** (Phase 2) - Removes 574 lines of unused code
3. **Long-term Architecture** (Phase 3) - Sustainable, maintainable design

**Next Steps**:
1. Review this proposal with stakeholders
2. Approve phased implementation plan
3. Begin Phase 1 implementation (TAS-60 completion)
4. Schedule Phase 2 (TAS-61) for next sprint
5. Plan Phase 3 (TAS-62) as major version release

The proposed architecture eliminates configuration sprawl, provides clear context separation, and ensures long-term maintainability while maintaining backward compatibility during migration.
