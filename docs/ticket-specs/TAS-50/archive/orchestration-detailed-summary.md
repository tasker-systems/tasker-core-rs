# Tasker-Orchestration Configuration Usage Analysis

## Summary Statistics
- **Total Config Accesses**: 214 grep matches
- **Direct `tasker_config.*` Access**: 23 locations
- **Primary Access Point**: SystemContext (holds Arc<TaskerConfig>)
- **Config Conversions**: 2 `From<&TaskerConfig>` implementations found

---

## Configuration Fields Actually Used

### 1. Database Configuration (`database.*`)
**Files**:
- `tasker-shared/src/system_context.rs`

**Fields Used**:
```rust
database.pool.max_connections       // Line 129 - Pool setup
database.pool.min_connections       // Line 130 - Pool setup
database.pool.max_lifetime_seconds  // Line 134 - Connection lifetime
database.checkout_timeout           // Line 131 - Acquire timeout
database.reaping_frequency          // Line 137 - Idle timeout
database_url()                      // Line 104, 116 - Connection string
```

**Usage Pattern**: Used in `SystemContext::get_pg_pool_options()` to configure sqlx connection pool

**Required**: ✅ YES - Critical for database connectivity

---

### 2. Queue Configuration (`queues.*`)
**Files**:
- `orchestration/bootstrap.rs`
- `orchestration/orchestration_queues/listener.rs`
- `orchestration/orchestration_queues/fallback_poller.rs`
- `tasker-shared/src/system_context.rs`

**Fields Used**:
```rust
queues.orchestration_namespace           // Bootstrap config, queue init
queues.orchestration_queues.step_results // Owned queue names
queues.orchestration_queues.task_requests
queues.orchestration_queues.task_finalizations
queues.* (entire struct cloned)          // Listener, fallback poller
```

**Usage Pattern**:
- Bootstrap: Extract orchestration namespace
- SystemContext: Initialize owned queues
- Listeners/Pollers: Clone entire queue config for operations

**Required**: ✅ YES - Essential for PGMQ message routing

---

### 3. Backoff Configuration (`backoff.*`)
**Files**:
- `orchestration/backoff_calculator.rs`
- `orchestration/lifecycle/result_processing/service.rs`

**Fields Used**:
```rust
backoff.default_backoff_seconds[0]  // Base delay (first value)
backoff.max_backoff_seconds         // Maximum backoff
backoff.backoff_multiplier          // Exponential multiplier
backoff.jitter_enabled              // Enable jitter
backoff.jitter_max_percentage       // Jitter percentage
```

**Usage Pattern**: `From<&TaskerConfig>` conversion to `BackoffCalculatorConfig`

**Required**: ✅ YES - Used for retry logic in step processing

---

### 4. Orchestration Configuration (`orchestration.*`)
**Files**:
- `orchestration/bootstrap.rs`

**Fields Used**:
```rust
orchestration.web_enabled()     // Whether to start web API
orchestration.web.*            // Clone entire web config
```

**Usage Pattern**:
- Bootstrap: Determine if web API should start
- Web state creation: Clone web config for API server

**Required**: ✅ YES - Controls web API initialization

---

### 5. Event Systems Configuration (`event_systems.*`)
**Files**:
- `orchestration/bootstrap.rs`
- `orchestration/event_systems/unified_event_coordinator.rs`

**Fields Used**:
```rust
event_systems.orchestration.*      // Clone for coordinator config
event_systems.task_readiness.*     // Clone for coordinator config
```

**Usage Pattern**: Clone entire event system configs for UnifiedEventCoordinator

**Required**: ✅ YES - Critical for event-driven coordination

---

### 6. MPSC Channels Configuration (`mpsc_channels.*`)
**Files**:
- `tasker-shared/src/system_context.rs`

**Fields Used**:
```rust
mpsc_channels.shared.event_publisher.event_queue_buffer_size  // Lines 192-197, 308-312
```

**Usage Pattern**: Extract buffer size for EventPublisher initialization

**Required**: ✅ YES - TAS-51 bounded channel sizing

**Note**: Other MPSC channel configs (orchestration, task_readiness) likely used in components we haven't analyzed yet

---

### 7. Circuit Breaker Configuration (`circuit_breakers.*`)
**Files**:
- `tasker-shared/src/system_context.rs`

**Fields Used**:
```rust
circuit_breakers.enabled            // Line 147 - Check if enabled
circuit_breakers.* (entire struct)  // Line 149 - Clone for manager
```

**Usage Pattern**:
- Check if enabled
- Clone entire config for CircuitBreakerManager creation
- **Note**: Comment in code says "deprecated - circuit breakers are now redundant with sqlx"

**Required**: ⚠️ MAYBE DEPRECATED - Code suggests sqlx handles this now

---

### 8. Environment Detection (`execution.environment`)
**Files**:
- Multiple via `config.environment()` method

**Fields Used**:
```rust
execution.environment  // Via TaskerConfig::environment() method
```

**Usage Pattern**: Environment string used for logging, configuration selection

**Required**: ✅ YES - Critical for environment-aware operations

---

## Configuration Fields NOT Found in Orchestration

These fields were NOT found in grep analysis:

### Not Used
- `telemetry.*` - No direct usage found
- `task_templates.*` - Likely Ruby-only
- `engine.*` - Likely legacy/unused
- `state_machine.*` - Possibly just documentation
- `task_readiness.*` (direct access) - Accessed via event_systems.task_readiness
- `worker.*` - Worker-specific, not used in orchestration
- `mpsc_channels.worker.*` - Worker-specific
- `execution.max_concurrent_tasks` - Only in tests
- `execution.max_concurrent_steps` - Worker-specific

---

## Key Insights

### 1. Configuration Access Patterns

**Primary Pattern**: Configuration held in SystemContext as `Arc<TaskerConfig>`
- Single source loaded at bootstrap
- Passed through SystemContext to all components
- Some components use `From<TaskerConfig>` conversions

**Secondary Pattern**: Direct field access when needed
- Database pool configuration
- Event system configuration
- MPSC channel sizing

### 2. Actual vs Defined Configuration

**High Usage**:
- database.* - Critical, used extensively
- queues.* - Critical, used for message routing
- event_systems.* - Critical, used for coordination
- backoff.* - Important, used for retries

**Moderate Usage**:
- orchestration.web.* - Used if web API enabled
- mpsc_channels.shared.* - Used for event publisher

**Low/No Usage**:
- telemetry.* - Not observed
- task_templates.* - Likely Ruby-only
- engine.* - Likely unused
- circuit_breakers.* - Marked as deprecated in code

### 3. Conversion Patterns

Found 2 `From<&TaskerConfig>` implementations:
1. `BackoffCalculatorConfig` - Extracts backoff configuration
2. `BootstrapConfig` - Extracts bootstrap parameters

This pattern suggests components take what they need via conversion traits.

---

## Recommendations for Context-Specific Config

### Common Configuration (Orchestration + Worker)
```rust
pub struct CommonConfig {
    pub database: DatabaseConfig,          // ✅ Used by both
    pub queues: QueuesConfig,              // ✅ Used by both
    pub backoff: BackoffConfig,            // ✅ Used by both
    pub environment: String,               // ✅ Used by both
    pub mpsc_channels: SharedChannelsConfig, // ✅ Event publisher used by both
}
```

### Orchestration-Specific Configuration
```rust
pub struct OrchestrationConfig {
    pub orchestration: OrchestrationSystemConfig,  // ✅ Web API control
    pub event_systems: OrchestrationEventSystemsConfig, // ✅ Event coordination
    pub mpsc_channels: OrchestrationChannelsConfig,    // TBD - Need to analyze components
    pub circuit_breakers: CircuitBreakerConfig,        // ⚠️ Possibly deprecated
}
```

---

## Next Steps

1. ✅ Analyze tasker-worker configuration usage (Step 1.2)
2. ✅ Analyze Ruby worker configuration usage (Step 1.3)
3. ✅ Create comprehensive usage matrix (Step 1.4)
4. Determine which MPSC channel configs are used where
5. Investigate deprecated circuit breaker usage
6. Verify telemetry configuration is unused or accessed indirectly

---

**Analysis Date**: 2025-10-14
**Analyst**: TAS-50 Configuration Analysis
