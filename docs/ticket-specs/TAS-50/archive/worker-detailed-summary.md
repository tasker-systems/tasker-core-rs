# Tasker-Worker Configuration Usage Analysis

## Summary Statistics
- **Total Config Accesses**: ~10 grep matches (vs 214 in orchestration)
- **Direct `tasker_config.*` Access**: 3 locations only
- **Primary Access Point**: SystemContext (holds Arc<TaskerConfig>) - shared with orchestration
- **Config Conversions**: 1 `From<&TaskerConfig>` implementation found

---

## Configuration Fields Actually Used

### 1. Database Configuration (`database.*`)
**Files**:
- `tasker-shared/src/system_context.rs` (shared with orchestration)

**Fields Used**:
```rust
database.pool.max_connections       // SystemContext pool setup
database.pool.min_connections       // SystemContext pool setup
database.pool.max_lifetime_seconds  // Connection lifetime
database.checkout_timeout           // Acquire timeout
database.reaping_frequency          // Idle timeout
database_url()                      // Connection string
```

**Usage Pattern**: Inherited from SystemContext initialization

**Required**: ✅ YES - Critical for database connectivity

**Context**: Common (used by both orchestration and worker)

---

### 2. Queue Configuration (`queues.*`)
**Files**:
- `tasker-worker/src/worker/orchestration_result_sender.rs`
- `tasker-worker/src/worker/command_processor.rs`

**Fields Used**:
```rust
queues.* (entire struct cloned)  // Lines 135, 201 - For result submission
```

**Usage Pattern**: Clone entire queue config for result submission to orchestration

**Required**: ✅ YES - Essential for sending results back to orchestration

**Context**: Common (queue namespace awareness needed by both)

---

### 3. Worker Configuration (`worker.*`)
**Files**:
- `tasker-worker/src/web/state.rs`
- `tasker-worker/src/bootstrap.rs`

**Fields Used**:
```rust
worker.web.enabled                    // Whether to start web API
worker.web.bind_address               // Web server bind address
worker.web.request_timeout_ms         // Request timeout
worker.web.auth.enabled               // Authentication enabled
worker.web.cors.enabled               // CORS enabled
```

**Usage Pattern**: `From<&TaskerConfig>` conversion to `WorkerBootstrapConfig` and `WorkerWebConfig`

**Required**: ✅ YES - Controls worker web API initialization

**Context**: Worker-specific

---

### 4. Event Systems Configuration (`event_systems.worker.*`)
**Files**:
- `tasker-worker/src/bootstrap.rs`

**Fields Used**:
```rust
event_systems.worker.deployment_mode          // Deployment mode (Hybrid/EventDriven/Polling)
event_systems.worker.deployment_mode.has_event_driven()  // Check if event-driven enabled
```

**Usage Pattern**: Extract deployment mode for worker event system configuration

**Required**: ✅ YES - Critical for event-driven worker coordination

**Context**: Worker-specific (different from orchestration event system)

---

### 5. Orchestration API Configuration (accessed via tasker-client)
**Files**:
- `tasker-client/src/api_clients/orchestration_client.rs`
- `tasker-worker/src/bootstrap.rs`

**Fields Used**:
```rust
orchestration.web_config().bind_address        // Orchestration API URL
orchestration.web_config().request_timeout_ms  // Request timeout
orchestration.web_config().auth                // Auth configuration
```

**Usage Pattern**: Worker needs to know where orchestration API is to submit results

**Required**: ✅ YES - Worker must communicate with orchestration

**Context**: Common (workers need orchestration location, orchestration defines it)

---

### 6. Environment Detection (`execution.environment`)
**Files**:
- `tasker-worker/src/bootstrap.rs`

**Fields Used**:
```rust
environment()  // Via TaskerConfig::environment() method
```

**Usage Pattern**: Environment string used for logging, configuration selection

**Required**: ✅ YES - Critical for environment-aware operations

**Context**: Common (used by both)

---

### 7. MPSC Channels Configuration (`mpsc_channels.*`)
**Files**:
- `tasker-shared/src/system_context.rs` (shared)

**Fields Used**:
```rust
mpsc_channels.shared.event_publisher.event_queue_buffer_size  // SystemContext event publisher
```

**Usage Pattern**: Inherited from SystemContext initialization

**Required**: ✅ YES - TAS-51 bounded channel sizing

**Context**: Common (shared event publisher)

**Note**: Worker-specific MPSC channel configs likely exist but not analyzed yet

---

### 8. Circuit Breaker Configuration (`circuit_breakers.*`)
**Files**:
- `tasker-shared/src/system_context.rs` (shared)

**Fields Used**:
```rust
circuit_breakers.enabled            // Check if enabled
circuit_breakers.* (entire struct)  // Clone for manager
```

**Usage Pattern**: Inherited from SystemContext initialization

**Required**: ⚠️ MAYBE DEPRECATED - Code suggests sqlx handles this now

**Context**: Common (if used at all)

---

## Configuration Fields NOT Found in Worker

These fields were NOT found in worker-specific grep analysis:

### Not Used by Worker
- `backoff.*` - Not used directly (orchestration handles retries)
- `telemetry.*` - No direct usage found
- `task_templates.*` - Accessed via database, not config
- `engine.*` - Likely legacy/unused
- `state_machine.*` - Possibly just documentation
- `execution.max_concurrent_tasks` - Orchestration-specific
- `execution.max_concurrent_steps` - Need to verify (might be worker-specific)
- `mpsc_channels.worker.*` - Need deeper analysis
- `orchestration.* (except web)` - Orchestration-specific
- `event_systems.orchestration.*` - Orchestration-specific
- `event_systems.task_readiness.*` - Orchestration-specific

---

## Key Insights

### 1. Worker vs Orchestration Configuration Footprint

**Worker Configuration is MUCH Smaller**:
- Only 3 direct `tasker_config.*` accesses vs 23 in orchestration
- Only 1 `From<&TaskerConfig>` implementation vs 2 in orchestration
- Most config inherited from SystemContext (database, circuit breakers)

**Worker-Specific Config is Focused**:
- `worker.web.*` - Web API configuration
- `event_systems.worker.*` - Event system deployment mode
- Orchestration API location (to send results)

### 2. Configuration Access Patterns

**Primary Pattern**: SystemContext provides common infrastructure
- Database pool configuration
- Event publisher
- Circuit breakers (if used)
- Message client

**Secondary Pattern**: Worker-specific bootstrap config
- Web API control
- Event system mode
- Orchestration API connection

### 3. Actual vs Defined Configuration

**High Usage (Common)**:
- `database.*` - Critical, used extensively
- `queues.*` - Critical, used for message routing
- `environment` - Critical for environment detection
- `orchestration.web.*` (for API location) - Critical for worker-orchestration communication

**High Usage (Worker-Specific)**:
- `worker.web.*` - Important if worker web API enabled
- `event_systems.worker.*` - Important for event-driven processing

**Moderate Usage**:
- `mpsc_channels.shared.*` - Used for event publisher
- `circuit_breakers.*` - Possibly deprecated

**Low/No Usage**:
- `backoff.*` - Orchestration concern
- `telemetry.*` - Not observed
- `task_templates.*` - Database-driven, not config
- `engine.*` - Likely unused
- `event_systems.orchestration.*` - Not worker's concern
- `event_systems.task_readiness.*` - Not worker's concern

### 4. Worker-Orchestration Communication

**Critical Shared Config**:
- Worker needs to know `orchestration.web.bind_address` to send results
- Both need same `queues.*` configuration for message routing
- Both need same `database.*` configuration for shared state

**Asymmetric Usage**:
- Orchestration has complex event system with task readiness
- Worker has simpler event system focused on step execution
- Orchestration manages backoff/retry logic
- Worker just executes steps and reports results

---

## Recommendations for Context-Specific Config

### Common Configuration (Orchestration + Worker)
```rust
pub struct CommonConfig {
    pub database: DatabaseConfig,              // ✅ Used by both
    pub queues: QueuesConfig,                  // ✅ Used by both
    pub environment: String,                   // ✅ Used by both
    pub mpsc_channels: SharedChannelsConfig,   // ✅ Event publisher used by both
    pub orchestration_api: OrchestrationApiLocation, // ✅ Workers need to find orchestration
    pub circuit_breakers: CircuitBreakerConfig, // ⚠️ Possibly deprecated - verify
}

pub struct OrchestrationApiLocation {
    pub bind_address: String,                  // Where orchestration API listens
    pub request_timeout_ms: u64,               // Client timeout
}
```

### Worker-Specific Configuration
```rust
pub struct WorkerConfig {
    pub worker: WorkerSystemConfig,            // ✅ Web API control
    pub event_systems: WorkerEventSystemConfig, // ✅ Event coordination
    pub mpsc_channels: WorkerChannelsConfig,   // TBD - Need to analyze components
}

pub struct WorkerSystemConfig {
    pub web: WorkerWebConfig,                  // Web API settings
}

pub struct WorkerWebConfig {
    pub enabled: bool,
    pub bind_address: String,
    pub request_timeout_ms: u64,
    pub auth: AuthConfig,
    pub cors: CorsConfig,
}

pub struct WorkerEventSystemConfig {
    pub deployment_mode: DeploymentMode,       // Hybrid/EventDriven/Polling
}
```

### Orchestration-Specific Configuration
```rust
pub struct OrchestrationConfig {
    pub orchestration: OrchestrationSystemConfig,  // ✅ Web API control
    pub event_systems: OrchestrationEventSystemsConfig, // ✅ Event coordination
    pub mpsc_channels: OrchestrationChannelsConfig,    // TBD - Need to analyze
    pub backoff: BackoffConfig,                        // ✅ Retry logic
}
```

---

## Next Steps

1. ✅ Analyze tasker-orchestration configuration usage (Step 1.1)
2. ✅ Analyze tasker-worker configuration usage (Step 1.2)
3. ⏳ Analyze Ruby worker configuration usage (Step 1.3)
4. ⏳ Create comprehensive usage matrix (Step 1.4)
5. Determine which MPSC channel configs are used where
6. Investigate deprecated circuit breaker usage
7. Verify telemetry configuration is unused or accessed indirectly
8. Verify `execution.max_concurrent_steps` usage in worker

---

## Critical Discovery: Configuration Asymmetry

**The worker configuration footprint is ~10% of orchestration's footprint.**

This validates the user's insight that:
- Current monolithic TaskerConfig forces workers to load orchestration-only config
- Most worker config is actually common infrastructure (database, queues)
- Worker-specific config is minimal (web API, event mode)
- Bifurcation into Common/Orchestration/Worker makes sense

**Design Implication**:
- Workers should load: CommonConfig + WorkerConfig (~20% of current TaskerConfig)
- Orchestration should load: CommonConfig + OrchestrationConfig (~80% of current TaskerConfig)

---

**Analysis Date**: 2025-10-14
**Analyst**: TAS-50 Configuration Analysis
