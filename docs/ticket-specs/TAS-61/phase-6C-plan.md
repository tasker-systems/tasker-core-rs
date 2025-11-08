# TAS-61 Phase 6C: Direct V2 Usage Migration

**Status**: In Progress
**Started**: 2025-11-08
**Target Completion**: 3-5 days

---

## Overview

**CRITICAL STRATEGY CLARIFICATION:**

This phase is NOT just about swapping `.tasker_config` → `.tasker_config_v2`. It's about:

1. **Replacing legacy config structs** (in `config/**/*.rs`) with V2 structs (from `tasker_v2.rs`)
2. **Migrating services** to use V2 struct types directly (import from `tasker_v2`)
3. **Deleting legacy config files** after all services migrated

### The Real Architecture

**Legacy Config Structs** (TO BE DELETED):
- `config/orchestration/decision_points.rs::DecisionPointsConfig`
- `config/orchestration/step_enqueuer.rs::StepEnqueuerConfig`
- `config/orchestration/step_result_processor.rs::StepResultProcessorConfig`
- `config/worker.rs::WorkerConfig`
- etc.

**V2 Config Structs** (THE FUTURE):
- `tasker_v2::DecisionPointsConfig` (with bon Builder, validation)
- `tasker_v2::CommonConfig`, `OrchestrationConfig`, `WorkerConfig`
- All config navigation happens via V2 tree branches

### Migration Approach

For each service/component:
1. Identify which legacy config struct it uses
2. Find/create equivalent V2 struct in `tasker_v2.rs`
3. Add any missing helper methods to V2 struct (or adapt service code)
4. Update service imports to use V2 struct
5. Change `.tasker_config` → `.tasker_config_v2` for navigation
6. Mark legacy config file for deletion

### Prerequisites (COMPLETE ✅)

- ✅ Phase 6A: SystemContext has both `tasker_config` and `tasker_config_v2` fields
- ✅ Phase 6B: All component configs have `From<&TaskerConfigV2>` implementations
- ✅ Bon Builder: All V2 structs have ergonomic builder pattern with inline defaults

### Goal

Convert **27 usages** of `.tasker_config` to `.tasker_config_v2` across **15 files**, enabling direct V2 configuration access.

---

## Complete Inventory

### Lifecycle Services (10 files, 14 usages)

**Low Risk** - Isolated service components with clear config dependencies:

1. `tasker-orchestration/src/orchestration/lifecycle/decision_point/service.rs` (1 usage)
2. `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs` (1 usage)
3. `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/service.rs` (1 usage)
4. `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/batch_processor.rs` (4 usages)
5. `tasker-orchestration/src/orchestration/lifecycle/step_result_processor.rs` (1 usage)
6. `tasker-orchestration/src/orchestration/lifecycle/result_processing/service.rs` (1 usage)
7. `tasker-orchestration/src/orchestration/lifecycle/result_processing/message_handler.rs` (3 usages)
8. `tasker-orchestration/src/orchestration/lifecycle/result_processing/metadata_processor.rs` (2 usages)

### Event Systems (2 files, 2 usages)

**Low Risk** - Event-driven coordination with straightforward config access:

9. `tasker-orchestration/src/orchestration/orchestration_queues/listener.rs` (1 usage)
10. `tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs` (1 usage)

### Bootstrap & Core (3 files, 8 usages)

**Medium Risk** - Central initialization code, but well-tested:

11. `tasker-orchestration/src/orchestration/bootstrap.rs` (4 usages)
12. `tasker-worker/src/bootstrap.rs` (3 usages)
13. `tasker-orchestration/src/web/state.rs` (1 usage)

### Support Services (2 files, 2 usages)

**Low Risk** - Isolated utility components:

14. `tasker-worker/src/worker/command_processor.rs` (1 usage)
15. `tasker-shared/src/registry/task_handler_registry.rs` (1 usage)

---

## Migration Patterns

### Pattern 0: The Real Pattern - Replace Config Struct Type

**Example: DecisionPointService**

```rust
// BEFORE (legacy config struct)
use tasker_shared::config::DecisionPointsConfig; // ← Legacy struct

pub struct DecisionPointService {
    context: Arc<SystemContext>,
    config: DecisionPointsConfig, // ← Legacy type
}

impl DecisionPointService {
    pub fn new(context: Arc<SystemContext>) -> Self {
        let config = context.tasker_config.decision_points.clone(); // ← Legacy source
        Self { context, config }
    }
}

// AFTER (V2 config struct)
use tasker_shared::config::tasker::tasker_v2::DecisionPointsConfig; // ← V2 struct

pub struct DecisionPointService {
    context: Arc<SystemContext>,
    config: DecisionPointsConfig, // ← V2 type
}

impl DecisionPointService {
    pub fn new(context: Arc<SystemContext>) -> Self {
        // Navigate V2 tree to get config
        let config = context
            .tasker_config_v2
            .orchestration
            .as_ref()
            .map(|o| o.decision_points.clone())
            .unwrap_or_default(); // ← V2 source with tree navigation

        Self { context, config }
    }
}
```

**Key Changes:**
1. **Import path**: `config::DecisionPointsConfig` → `config::tasker::tasker_v2::DecisionPointsConfig`
2. **Type used**: V2 struct (with `u32`, bon Builder, validation)
3. **Source access**: Navigate V2 tree with `orchestration.as_ref().map()`
4. **Helper methods**: Either add to V2 struct OR adapt service code for type differences (`u32` vs `usize`)

### Pattern 1: Common Fields (Always Present)

Common configuration is always available in `config.common.*`:

```rust
// BEFORE (legacy)
let queue_namespace = context.tasker_config.queues.orchestration_namespace.clone();
let db_url = context.tasker_config.database_url();
let step_batch_size = context.tasker_config.execution.step_batch_size;

// AFTER (V2 direct)
let queue_namespace = context.tasker_config_v2.common.queues.orchestration_namespace.clone();
let db_url = context.tasker_config_v2.common.database.url.clone(); // or database_url() method
let step_batch_size = context.tasker_config_v2.common.execution.step_batch_size;
```

### Pattern 2: Orchestration-Specific Fields (Optional)

Orchestration config is optional, requires unwrapping:

```rust
// BEFORE (legacy - orchestration always present in legacy config)
let perf_logging = context.tasker_config.orchestration.enable_performance_logging;
let decision_config = context.tasker_config.orchestration.decision_points.clone();

// AFTER (V2 - orchestration is Option<OrchestrationConfig>)
let perf_logging = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.enable_performance_logging)
    .unwrap_or(false);

let decision_config = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.decision_points.clone())
    .unwrap_or_default();
```

### Pattern 3: Worker-Specific Fields (Optional)

Worker config is optional, requires unwrapping:

```rust
// BEFORE (legacy - worker always present in legacy config)
let worker_id = context.tasker_config.worker.worker_id.clone();
let resource_limits = context.tasker_config.worker.resource_limits.clone();

// AFTER (V2 - worker is Option<WorkerConfig>)
let worker_id = context.tasker_config_v2.worker
    .as_ref()
    .map(|w| w.worker_id.clone())
    .unwrap_or_else(|| "default-worker".to_string());

let resource_limits = context.tasker_config_v2.worker
    .as_ref()
    .map(|w| w.resource_limits.clone())
    .unwrap_or_default();
```

### Pattern 4: Component Config Conversions

Already have `From<&TaskerConfigV2>` implementations:

```rust
// BEFORE (legacy)
let enqueuer_config = StepEnqueuerConfig::from(&context.tasker_config);
let backoff_config = BackoffCalculatorConfig::from(&context.tasker_config);

// AFTER (V2 - no change needed, just swap source)
let enqueuer_config = StepEnqueuerConfig::from(&context.tasker_config_v2);
let backoff_config = BackoffCalculatorConfig::from(&context.tasker_config_v2);
```

### Pattern 5: Event Systems Config

Event systems configuration in V2:

```rust
// BEFORE (legacy)
let event_config = context.tasker_config.event_systems.orchestration.clone();
let task_readiness = context.tasker_config.event_systems.task_readiness.clone();

// AFTER (V2)
let event_config = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.event_systems.orchestration.clone())
    .unwrap_or_default();

let task_readiness = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.event_systems.task_readiness.clone())
    .unwrap_or_default();
```

---

## Migration Order (Dependency-Based)

### Phase 1: Lifecycle Services (Day 1)
Start with isolated service components - safest migrations:

- [x] `lifecycle/decision_point/service.rs` (1 usage)
- [ ] `lifecycle/step_enqueuer.rs` (1 usage)
- [ ] `lifecycle/step_enqueuer_services/service.rs` (1 usage)
- [ ] `lifecycle/step_enqueuer_services/batch_processor.rs` (4 usages)
- [ ] `lifecycle/step_result_processor.rs` (1 usage)
- [ ] `lifecycle/result_processing/service.rs` (1 usage)
- [ ] `lifecycle/result_processing/message_handler.rs` (3 usages)
- [ ] `lifecycle/result_processing/metadata_processor.rs` (2 usages)

### Phase 2: Event Systems (Day 1-2)
Event coordination with straightforward config:

- [ ] `orchestration_queues/listener.rs` (1 usage)
- [ ] `orchestration_queues/fallback_poller.rs` (1 usage)

### Phase 3: Support Services (Day 2)
Utility components:

- [ ] `worker/command_processor.rs` (1 usage)
- [ ] `registry/task_handler_registry.rs` (1 usage)

### Phase 4: Bootstrap & Core (Day 2-3)
Central initialization - save for last after all services migrated:

- [ ] `web/state.rs` (1 usage)
- [ ] `orchestration/bootstrap.rs` (4 usages)
- [ ] `worker/bootstrap.rs` (3 usages)

---

## Testing Strategy

### Per-File Testing
After each file migration:

```bash
# 1. Verify compilation
cargo check --all-features

# 2. Run affected package tests
cargo test --all-features --package tasker-orchestration
cargo test --all-features --package tasker-worker
cargo test --all-features --package tasker-shared

# 3. Check for regressions
cargo clippy --all-targets --all-features
```

### Integration Testing
After each phase:

```bash
# Full test suite
cargo test --all-features

# Run with test config to verify orchestration-only context works
TASKER_ENV=test cargo test --all-features --package tasker-orchestration
```

### Final Validation
After all migrations complete:

```bash
# 1. Verify zero legacy usage remaining
rg "\.tasker_config\." --type rust tasker-shared/src tasker-orchestration/src tasker-worker/src
# Expected: 0 results

# 2. Full test suite
cargo test --all-features
# Expected: All tests passing

# 3. Integration tests
docker-compose -f docker/docker-compose.test.yml up -d
cargo test --test e2e_tests
docker-compose -f docker/docker-compose.test.yml down
```

---

## Common Gotchas & Solutions

### 1. Optional Context Unwrapping

**Problem**: `orchestration` and `worker` are `Option<_>` in V2

**Solution**: Always use `.as_ref().map().unwrap_or()` pattern

```rust
// ❌ WRONG - will panic if orchestration context not loaded
let value = context.tasker_config_v2.orchestration.unwrap().some_field;

// ✅ CORRECT - gracefully handles missing context
let value = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.some_field.clone())
    .unwrap_or_default();
```

### 2. Environment Method

**Problem**: `environment()` method moved

**Solution**: Use `common.execution.environment` field

```rust
// BEFORE
let env = context.tasker_config.environment();

// AFTER
let env = context.tasker_config_v2.common.execution.environment.clone();
```

### 3. Database URL Construction

**Problem**: `database_url()` method might need updating

**Solution**: Check if CommonConfig has the method, otherwise access field directly

```rust
// Check tasker_v2.rs CommonConfig for database_url() method
let db_url = context.tasker_config_v2.common.database_url(); // if method exists

// Or access field directly
let db_url = context.tasker_config_v2.common.database.url.clone();
```

### 4. Nested Field Access

**Problem**: Deeply nested structures in V2

**Solution**: Use intermediate variables for clarity

```rust
// ❌ Hard to read
let value = context.tasker_config_v2.orchestration.as_ref().map(|o| o.event_systems.orchestration.deployment_mode.clone()).unwrap_or_default();

// ✅ Clear and readable
let orch_config = context.tasker_config_v2.orchestration.as_ref();
let event_systems = orch_config.map(|o| &o.event_systems);
let deployment_mode = event_systems
    .map(|es| es.orchestration.deployment_mode.clone())
    .unwrap_or_default();
```

---

## Success Criteria

### Per-File Success
- ✅ File compiles without errors
- ✅ All tests in affected package pass
- ✅ No clippy warnings introduced
- ✅ Behavior unchanged (existing tests verify)

### Phase 6C Completion
- ✅ All 27 usages of `.tasker_config` replaced with `.tasker_config_v2`
- ✅ Zero grep results for `.tasker_config.` in business logic
- ✅ All 529+ tests passing
- ✅ Integration tests pass with orchestration-only and worker-only configs
- ✅ No performance regressions
- ✅ All clippy warnings resolved

---

## Risk Assessment

### Low Risk (22 usages across 12 files)
- Lifecycle services - isolated components with clear dependencies
- Event systems - straightforward config access
- Support services - utility components

### Medium Risk (8 usages across 3 files)
- Bootstrap systems - central initialization, but well-tested
- Web state - API configuration, isolated from business logic

### Mitigation Strategies
1. **Incremental Migration**: One file at a time with full test validation
2. **Parallel Config Safety Net**: Both configs available during Phase 6C
3. **Comprehensive Testing**: Package tests after each file, full suite after each phase
4. **Easy Rollback**: Can revert to `.tasker_config` if issues found

---

## Verification Commands

```bash
# Find remaining .tasker_config usage
rg "\.tasker_config\." --type rust tasker-shared/src tasker-orchestration/src tasker-worker/src

# Count remaining usages
rg "\.tasker_config\." --type rust tasker-shared/src tasker-orchestration/src tasker-worker/src | wc -l

# Show which files still need migration
rg -l "\.tasker_config\." --type rust tasker-shared/src tasker-orchestration/src tasker-worker/src

# Run tests
cargo test --all-features

# Check compilation
cargo check --all-features

# Run clippy
cargo clippy --all-targets --all-features
```

---

## Next Phase Preview

After Phase 6C completion, we'll be ready for **Phase 6D: Bridge Removal**:
- Delete `tasker-shared/src/config/tasker/bridge.rs` (829 lines)
- Remove `tasker_config: Arc<TaskerConfig>` from SystemContext
- Delete `From<&TaskerConfig>` implementations
- Delete `config/contexts/*.rs` intermediate layer
- Achieve single-layer conversion: `TaskerConfigV2 → ComponentConfig`
