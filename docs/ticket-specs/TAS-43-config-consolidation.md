# TAS-43: Configuration Consolidation and Architecture Cleanup

**Status**: Planning
**Priority**: High
**Estimated Effort**: 1-2 weeks
**Dependencies**: TAS-43 PGMQ Notify completion

## Problem Statement

During pre-merge audit of TAS-43 branch, identified critical configuration architecture issues:

### 1. Configuration Parameter Overlaps
- **Timeout parameters** scattered across 6+ TOML files with inconsistent values
- **Retry logic** defined in 5+ different places (max_retries = 3 everywhere but different contexts)
- **Circuit breaker config** duplicated in `circuit_breakers.toml` AND `task_readiness.toml`
- **Queue configuration** spread across `pgmq.toml`, `orchestration.toml`, `worker.toml`

### 2. ConfigManager Bypass Issues
- Many `*Config` structs use `::default()` instead of loading from ConfigManager
- SystemContext.config_manager exists but inconsistently used as source of truth
- Components mix ConfigManager usage with hardcoded defaults

## Solution Architecture

### Phase 1: Queue Configuration Abstraction (Prepare for RabbitMQ)

#### 1.1 Create queues.toml (replaces pgmq.toml)
```toml
# New: queues.toml
[queues]
# Backend selection (aligns with UnifiedMessageClient)
backend = "pgmq"  # Future: "rabbitmq", "redis", etc.
orchestration_namespace = "orchestration"
worker_namespace = "worker"

# Universal queue configuration (backend-agnostic)
default_visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100
naming_pattern = "{namespace}_{name}_queue"

# Default namespaces across all backends
default_namespaces = ["default"]

# Queue type definitions (moved from orchestration.toml)
[queues.orchestration_queues]
task_requests = "orchestration_task_requests_queue"
task_finalizations = "orchestration_task_finalizations_queue"
step_results = "orchestration_step_results_queue"

[queues.worker_queues]
default = "worker_default_queue"
fulfillment = "worker_fulfillment_queue"
inventory = "worker_inventory_queue"
notifications = "worker_notifications_queue"

# Backend-specific configuration
[queues.pgmq]
poll_interval_ms = 250
shutdown_timeout_seconds = 5
max_retries = 3

# Future RabbitMQ configuration
[queues.rabbitmq]
connection_timeout_seconds = 10
heartbeat_interval_seconds = 60
channel_pool_size = 10
```

#### 1.2 Remove Overlapping Queue Config
- **Delete**: `pgmq.toml` → migrate to `queues.toml`
- **Remove**: `[orchestration.queues]` section → move to `queues.toml`
- **Update**: `worker.toml` → remove queue-specific config, reference queues.toml

### Phase 2: Contextual Configuration Consolidation

#### 2.1 Database Configuration (database.toml)
Keep database timeouts contextual - these are connection pool specific:
```toml
[database.pool]
max_connections = 30
min_connections = 8
acquire_timeout_seconds = 30      # Database-specific
idle_timeout_seconds = 300        # Database-specific
max_lifetime_seconds = 3600       # Database-specific
```

#### 2.2 Worker Configuration (worker.toml)
Keep worker-specific timeouts contextual - these are processing specific:
```toml
[worker.step_processing]
claim_timeout_seconds = 300       # Step claim duration
max_retries = 3                   # Step processing retries
heartbeat_interval_seconds = 30   # Worker heartbeat
```

#### 2.3 Task Readiness Configuration (task_readiness.toml)
**REMOVE CIRCUIT BREAKER CONFIG** - conflicts with circuit_breakers.toml:
```toml
# REMOVE THIS ENTIRE SECTION:
# [task_readiness.error_handling.circuit_breaker]
# enabled = true
# failure_threshold = 5
# recovery_timeout_seconds = 30
```

Keep task readiness specific timeouts:
```toml
[task_readiness.coordinator]
operation_timeout_ms = 5000       # Task readiness operations
startup_timeout_seconds = 30      # Coordinator startup
shutdown_timeout_seconds = 10     # Coordinator shutdown
```

#### 2.4 System Configuration (system.toml)
Keep high-level system constraints, avoid timeout over-centralization:
```toml
[system.execution]
max_concurrent_tasks = 100
max_concurrent_steps = 1000
default_timeout_seconds = 3600    # Overall task timeout (high-level)
step_execution_timeout_seconds = 300  # Individual step timeout (high-level)
```

### Phase 3: ConfigManager Usage Enforcement

#### 3.1 Eliminate ::default() Pattern
**Current Problem**:
```rust
// Found in orchestration/mod.rs and others:
step_enqueuer_config: StepEnqueuerConfig::default(),
step_result_processor_config: StepResultProcessorConfig::default(),
operational_state: OperationalStateConfig::default(),
```

**Solution - Consistent Constructor Pattern**:
```rust
impl OrchestrationConfig {
    /// Load configuration from ConfigManager - NO defaults allowed
    pub fn from_config_manager(config_manager: &ConfigManager) -> TaskerResult<Self> {
        let config = config_manager.config();

        Ok(Self {
            mode: config.orchestration.mode.clone(),
            tasks_per_cycle: config.orchestration.tasks_per_cycle,
            cycle_interval_ms: config.orchestration.cycle_interval_ms,
            // Load ALL fields from configuration - no ::default() calls
            step_enqueuer_config: StepEnqueuerConfig::from_config_manager(config_manager)?,
            step_result_processor_config: StepResultProcessorConfig::from_config_manager(config_manager)?,
            operational_state: OperationalStateConfig::from_config_manager(config_manager)?,
        })
    }
}
```

#### 3.2 SystemContext as Single Source of Truth
**Ensure all component initialization uses SystemContext**:
```rust
// In bootstrap code:
let orchestration_config = OrchestrationConfig::from_config_manager(
    &system_context.config_manager
)?;

// NO MORE:
let config = SomeConfig::default();
let config = SomeConfig::new();
```

#### 3.3 Update All Config Struct Constructors
**Target Files** (from audit):
- `tasker-shared/src/config/orchestration/mod.rs` - 6+ ::default() calls
- `tasker-shared/src/config/mod.rs` - 15+ ::default() calls
- `tasker-shared/src/config/orchestration/task_claim_step_enqueuer.rs` - 3+ ::default() calls

## Implementation Plan

### Week 1: Queue Configuration Abstraction
- [ ] Create `config/tasker/base/queues.toml`
- [ ] Migrate `pgmq.toml` content to `queues.toml` with backend abstraction
- [ ] Move `[orchestration.queues]` to `queues.toml`
- [ ] Update environment overrides: `environments/*/queues.toml`
- [ ] Delete `config/tasker/base/pgmq.toml`

### Week 2: Remove Configuration Conflicts
- [ ] Remove circuit breaker config from `task_readiness.toml`
- [ ] Remove queue config from `worker.toml` (reference queues.toml instead)
- [ ] Update `orchestration.toml` - remove `[orchestration.queues]` section
- [ ] Validate no conflicting timeout parameters between files

### Week 3: Fix ConfigManager Usage
- [ ] Update `OrchestrationConfig::from_config_manager()` - remove ::default() calls
- [ ] Update all config constructors in `tasker-shared/src/config/`
- [ ] Ensure bootstrap code uses SystemContext consistently
- [ ] Update component initialization patterns

### Week 4: Testing & Validation
- [ ] Update all configuration tests for new structure
- [ ] Test environment overrides work correctly
- [ ] Validate UnifiedMessageClient works with new queues.toml
- [ ] End-to-end testing with consolidated configuration

## Success Criteria

### ✅ Configuration Architecture
- [ ] Single source of queue configuration in `queues.toml`
- [ ] Backend abstraction ready for RabbitMQ integration
- [ ] No parameter conflicts between TOML files
- [ ] Contextual timeout organization (database, worker, task readiness specific)

### ✅ ConfigManager Usage
- [ ] Zero `::default()` calls in config constructors
- [ ] All components use `SystemContext.config_manager` as source of truth
- [ ] Consistent `from_config_manager()` pattern across all Config structs

### ✅ Maintainability
- [ ] Clear separation of concerns between configuration domains
- [ ] No cross-file references or dependencies
- [ ] Easy to add new backends (RabbitMQ, Redis) to queue abstraction

## Pre-Merge Checklist

- [ ] All TAS-43 tests pass with new configuration structure
- [ ] Integration tests validate queue backend abstraction
- [ ] Environment-specific overrides work correctly
- [ ] No configuration conflicts detected
- [ ] Ready to merge TAS-43 → TAS-40 feature branch

## Risk Mitigation

### Configuration Migration Risk
- **Risk**: Breaking existing configuration during migration
- **Mitigation**: Implement migration in small increments, validate each step

### Backend Abstraction Complexity
- **Risk**: Over-engineering queue abstraction before RabbitMQ implementation
- **Mitigation**: Keep abstraction simple, focus on proven UnifiedMessageClient patterns

### Testing Overhead
- **Risk**: Configuration changes break existing tests
- **Mitigation**: Update configuration tests first, then implementation

---

**Related Tickets**: TAS-43 PGMQ Notify, TAS-40 Worker Foundations
**Reviewer**: @petetaylor
**Ready for Implementation**: After TAS-43 PGMQ Notify merge approval
