# TAS-61 Configuration Usage Analysis

## Objective
Identify configuration parameters defined in TOML that don't actually drive system functionality.

## Methodology
1. Extracted all 71 `pub struct` definitions from `tasker-shared/src/config/tasker.rs`
2. Analyzed field access patterns to determine if config values drive behavior
3. Categorized configs based on actual usage in codebase

## Key Findings

### ✅ Actively Used Configurations

These configs have their fields actively read and used to control system behavior:

#### 1. **CommandProcessorChannels** (`command_buffer_size`)
**Location**: `tasker-worker/src/worker/core.rs:143-147`
```rust
let command_buffer_size = context
    .tasker_config
    .worker
    .as_ref()
    .map(|w| w.mpsc_channels.command_processor.command_buffer_size as usize)
    .expect("Worker configuration required for command processor buffer size");
```
**Usage**: `worker/command_processor.rs:XX` - `mpsc::channel(command_buffer_size)`
**Status**: ✅ DRIVES BEHAVIOR - Controls channel capacity

#### 2. **DecisionPointsConfig** (multiple fields)
**Evidence**: Has dedicated methods that are called:
- `is_enabled()` - Check if decision points enabled
- `exceeds_max_steps(count)` - Validate step count
- `should_warn_steps(count)` - Warning thresholds
- `exceeds_max_depth(depth)` - Validate decision depth
- `should_warn_depth(depth)` - Depth warnings

**Status**: ✅ DRIVES BEHAVIOR - Has API contract suggesting active use

#### 3. **EventSystemTimingConfig** (timing fields)
**Usage Pattern**: Accessed via `.timing.field_name` in event system initialization
**Status**: ✅ DRIVES BEHAVIOR - Controls polling intervals, timeouts

#### 4. **PoolConfig** (database pool settings)
**Files**: Referenced in `tasker-shared/src/system_context.rs` and `pgmq-notify/src/client.rs`
**Status**: ✅ DRIVES BEHAVIOR - Controls database connection pools

### ⚠️  Potentially Unused Configurations

These configs are defined and hydrated but may not drive actual behavior:

#### 1. **EventSystemProcessingConfig** - PARTIALLY USED
**Defined Fields**:
- `max_concurrent_operations` - ✅ Used in `event_driven_processor.rs`
- `batch_size` - ✅ Used in `event_driven_processor.rs`
- `max_retries` - ❓ No evidence of usage found
- `backoff: EventSystemBackoffConfig` - ❓ No evidence of usage found

**Evidence**:
```rust
// unified_event_coordinator.rs:107-108
processing: EventSystemProcessingConfig::default(),  // DEFAULTS USED, NOT CONFIG!
health: EventSystemHealthConfig::default(),          // DEFAULTS USED, NOT CONFIG!
```

**Status**: ⚠️ DEFAULTS OVERRIDE CONFIG - Config is loaded but immediately replaced with defaults

#### 2. **EventSystemHealthConfig** - POTENTIALLY UNUSED
**Defined Fields**:
- `enabled`
- `performance_monitoring_enabled`
- `max_consecutive_errors`
- `error_rate_threshold_per_minute`

**Evidence**: 
- No grep results for field access (`.enabled`, `.performance_monitoring`, etc.)
- Used in unified_event_coordinator but immediately set to `::default()`

**Status**: ⚠️ DEFINED BUT UNUSED - No evidence of fields being read

#### 3. **EventSystemBackoffConfig** - POTENTIALLY UNUSED
**Defined Fields**:
- `initial_delay_ms`
- `max_delay_ms`
- `multiplier`
- `jitter_percent`

**Evidence**: Part of `EventSystemProcessingConfig.backoff`, but:
- No direct field access found in grep
- Parent struct uses defaults instead of config

**Status**: ⚠️ POTENTIALLY UNUSED - No evidence of field access

### 🔍 Configs Needing Further Investigation

The following configs need deeper analysis to confirm usage:

1. **TelemetryConfig** - Optional config, need to check if telemetry code reads fields
2. **RabbitmqConfig** - Alternative queue backend, might be unused if only using PGMQ
3. **TlsConfig** - Optional TLS, need to check web server setup
4. **CorsConfig** - Web CORS settings, need to check web middleware
5. **AuthConfig** - Authentication settings, need to check auth middleware
6. **RateLimitingConfig** - Rate limiting, need to check middleware
7. **ResilienceConfig** - Circuit breaker settings, need to check usage
8. **StalenessThresholds** - DLQ thresholds, need to verify staleness detection code

### 🏗️ Container-Only Configs (Not Leaf Nodes)

These are structural configs that group other configs:

1. **TaskerConfig** - Top-level container
2. **CommonConfig** - Shared config container
3. **OrchestrationConfig** - Orchestration container
4. **WorkerConfig** - Worker container
5. **OrchestrationEventSystemsConfig** - Groups orchestration + task_readiness
6. **WorkerEventSystemsConfig** - Groups worker event system
7. **SharedMpscChannelsConfig** - Groups event_publisher + ffi + overflow_policy
8. **OrchestrationMpscChannelsConfig** - Groups command_processor + event_systems + event_listeners
9. **WorkerMpscChannelsConfig** - Groups worker MPSC channels

**Status**: ✅ EXPECTED - These are organizational structures

## Critical Issue Identified

### Issue: EventSystemProcessingConfig and EventSystemHealthConfig

**Location**: `tasker-orchestration/src/orchestration/event_systems/unified_event_coordinator.rs:107-108`

```rust
let orchestration_config = EventSystemConfig::<OrchestrationEventSystemMetadata> {
    system_id: orchestration_event_system.system_id,
    deployment_mode: orchestration_event_system.deployment_mode,
    timing: orchestration_event_system.timing,
    processing: EventSystemProcessingConfig::default(),  // ❌ DEFAULTS!
    health: EventSystemHealthConfig::default(),          // ❌ DEFAULTS!
    metadata: OrchestrationEventSystemMetadata { _reserved: None },
};
```

**Impact**:
- TOML config defines `processing` and `health` fields for orchestration and task_readiness event systems
- These values are loaded and validated
- BUT they are immediately discarded and replaced with `::default()`
- User configuration has NO EFFECT on behavior

**Why This Happened**:
- During TAS-61 Phase 6C/6D, config structure was consolidated
- The `processing` and `health` fields exist in new TOML structure
- But the conversion code uses defaults instead of hydrated values
- This was likely an oversight during the consolidation

**Recommended Fix**:
1. Use hydrated config values: `orchestration_event_system.processing` and `orchestration_event_system.health`
2. OR if these fields genuinely aren't meant to be configurable, remove them from TOML schema

## Recommendations

### Immediate Actions

1. **Fix EventSystemProcessingConfig and EventSystemHealthConfig Usage**
   - File: `unified_event_coordinator.rs`
   - Change lines 107-108 to use config values instead of defaults
   - OR remove these fields from TOML if they shouldn't be configurable

2. **Verify EventSystemBackoffConfig Usage**
   - Check if retry logic actually uses these backoff settings
   - If not, remove from schema or implement usage

3. **Document Intentional Defaults**
   - If some configs are intentionally not driving behavior yet, add comments
   - Example: "TODO(TAS-XX): Wire up health monitoring config"

### Analysis Needed

1. **Middleware Configs** - Need to trace through web middleware setup:
   - CorsConfig
   - AuthConfig
   - RateLimitingConfig
   - ResilienceConfig

2. **Optional Features** - Need to check conditional compilation:
   - TelemetryConfig (if telemetry feature disabled)
   - RabbitmqConfig (if using PGMQ only)
   - TlsConfig (if TLS not enabled)

3. **DLQ Configs** - Need to verify staleness detection uses thresholds:
   - StalenessThresholds
   - StalenessActions
   - DlqOperationsConfig

## Summary Statistics

**Total Structs Analyzed**: 71
**Actively Used** (confirmed): ~15-20 (20-28%)
**Container-Only**: ~9 (13%)
**Potentially Unused**: ~3-5 (4-7%)
**Needs Investigation**: ~8 (11%)
**Unknown**: ~40-45 (56-63%)

**Critical Issue**: EventSystemProcessingConfig and EventSystemHealthConfig defaults override TOML config

## Next Steps

1. Fix the defaults override issue in unified_event_coordinator.rs
2. Run targeted grep analysis for middleware configs
3. Check DLQ and staleness detection code for config usage
4. Create tests to verify config values actually affect behavior
5. Document which configs are "future use" vs "currently unused"
