# TAS-61 Phase 6: TaskerConfigV2 Direct Usage Migration

**Status**: In Progress - Phase 6C
**Started**: 2025-11-07
**Phase 6A Completed**: 2025-11-07
**Phase 6B Completed**: 2025-11-07
**Target Completion**: 5 weeks (through Phase 6D)

---

## Problem Statement

Current configuration system has a two-layer conversion that creates maintenance burden and complexity:

```
TaskerConfigV2 → [bridge.rs] → TaskerConfig → [From impls] → ComponentConfig
```

**Goal**: Achieve single-layer conversion directly from V2 configuration:

```
TaskerConfigV2 → [From impls] → ComponentConfig
```

### Current State Analysis

- **SystemContext**: Central holder of `Arc<TaskerConfig>` used by ALL components
- **Bridge**: 829-line conversion in `tasker-shared/src/config/tasker/bridge.rs`
- **Component Configs**: 15+ structs with `From<&TaskerConfig>` implementations
- **Files Affected**: 73 files with direct or indirect TaskerConfig usage
- **Actual Field Usage**: ~20 fields actively READ in business logic, ~10 fields only passed through conversions

### Key Challenges

1. **Optional Context Handling**: `orchestration: Option<OrchestrationConfig>` and `worker: Option<WorkerConfig>` require careful unwrapping
2. **MPSC Channel Merging**: Channels merge across common/orchestration/worker contexts
3. **Numeric Type Casts**: Some fields cast from u32 → u64 for legacy compatibility
4. **Dependency Graph**: SystemContext is foundation, all components depend on it

---

## Migration Strategy: 4 Phases (5 Weeks)

### Phase 6A: Parallel Config Support (Week 1) ✅ ZERO-RISK

**Goal**: Add TaskerConfigV2 support WITHOUT removing legacy TaskerConfig

**Approach**: Parallel fields in SystemContext

```rust
pub struct SystemContext {
    pub processor_uuid: Uuid,

    // TAS-61 Phase 6A: Both configs available during migration
    pub tasker_config: Arc<TaskerConfig>,      // Legacy (via bridge) - remove in Phase 6D
    pub tasker_config_v2: Arc<TaskerConfigV2>, // Primary source of truth

    pub message_client: Arc<UnifiedPgmqClient>,
    pub database_pool: PgPool,
    pub task_handler_registry: Arc<TaskHandlerRegistry>,
    pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,
    pub event_publisher: Arc<EventPublisher>,
}
```

**Changes Required**:

1. **SystemContext struct** (`tasker-shared/src/system_context.rs`):
   - Add `tasker_config_v2: Arc<TaskerConfigV2>` field
   - Update documentation to explain parallel config approach
   - Update Debug implementation

2. **SystemContext::from_pool_and_config()** (`tasker-shared/src/system_context.rs:241`):
   - Populate `tasker_config_v2` from ConfigManager
   - Populate `tasker_config` via bridge: `tasker_config_v2.to_legacy()`
   - Both configs available to all components

3. **SystemContext::with_pool()** test helper (`tasker-shared/src/system_context.rs:353`):
   - Update to populate both fields for test compatibility

**Why This Works**:
- Zero breaking changes - all existing code continues to work
- Incremental migration per component possible
- Easy rollback - just use `.tasker_config` instead of `.tasker_config_v2`
- Continuous validation via existing test suite

**Success Criteria**:
- ✅ SystemContext has both `tasker_config` and `tasker_config_v2` fields
- ✅ All 529+ tests passing
- ✅ No behavior changes or regressions
- ✅ Both configs populated correctly from ConfigManager

**Deliverables**:
- Updated SystemContext with parallel config support
- All tests passing
- Documentation explaining transition period
- Commit: `feat(TAS-61): Phase 6A - Add parallel TaskerConfigV2 support`

---

### Phase 6B: Component Migration (Weeks 2-3)

**Goal**: Replace `From<&TaskerConfig>` with `From<&TaskerConfigV2>` for all component configs

**Migration Order** (dependency-based, leaf-first):

**Day 1-2: Component Config Structs** (LOW RISK)
- `StepEnqueuerConfig` (`tasker-shared/src/config/orchestration/step_enqueuer.rs`)
- `StepResultProcessorConfig` (`tasker-shared/src/config/orchestration/result_processor.rs`)
- `TaskClaimStepEnqueuerConfig` (`tasker-shared/src/config/orchestration/task_claim_step_enqueuer.rs`)
- `BackoffCalculatorConfig` (`tasker-orchestration/src/orchestration/backoff_calculator.rs`)
- `WorkerBootstrapConfig` (`tasker-worker/src/bootstrap.rs`)

**Day 3: Backoff Calculator** (MEDIUM RISK)
- File: `tasker-orchestration/src/orchestration/backoff_calculator.rs`
- Single-purpose component with clear config usage

**Day 4: Decision Point Service** (MEDIUM RISK)
- File: `tasker-orchestration/src/orchestration/lifecycle/decision_point/service.rs`
- Self-contained TAS-53 feature

**Days 5-7: Lifecycle Services** (MEDIUM RISK)
- TaskInitializer components (6 files)
- Result processing components (6 files)
- Step enqueuer services (4 files)
- Task finalization components (6 files)

**Days 8-9: Bootstrap Systems** (HIGH RISK)
- `OrchestrationBootstrap` (`tasker-orchestration/src/orchestration/bootstrap.rs`)
- `WorkerBootstrap` (`tasker-worker/src/bootstrap.rs`)
- `OrchestrationCore` (`tasker-orchestration/src/orchestration/core.rs`)

**Day 10: Actor Registry** (HIGH RISK)
- File: `tasker-orchestration/src/actors/registry.rs`
- All 5 actors (TaskRequestActor, ResultProcessorActor, StepEnqueuerActor, TaskFinalizerActor, DecisionPointActor)

**Migration Pattern**:

```rust
// Before:
impl From<&TaskerConfig> for StepEnqueuerConfig {
    fn from(config: &TaskerConfig) -> Self {
        Self {
            max_steps_per_task: config.execution.step_batch_size as usize,
            enable_detailed_logging: config.orchestration.enable_performance_logging,
            enqueue_timeout_seconds: config.execution.step_execution_timeout_seconds,
        }
    }
}

// After (V2):
impl From<&TaskerConfigV2> for StepEnqueuerConfig {
    fn from(config: &TaskerConfigV2) -> Self {
        Self {
            max_steps_per_task: config.common.execution.step_batch_size as usize,
            enable_detailed_logging: config.orchestration
                .as_ref()
                .map(|o| o.enable_performance_logging)
                .unwrap_or(false),
            enqueue_timeout_seconds: config.common.execution.step_execution_timeout_seconds,
        }
    }
}

// During transition: Keep BOTH implementations
// Delete legacy From<&TaskerConfig> after component migration verified
```

**Optional Context Pattern**:

```rust
// Pattern for handling Option<OrchestrationConfig>:
let orch_config = config.orchestration
    .as_ref()
    .map(|o| o.some_field.clone())
    .unwrap_or_default();

// Pattern for handling Option<WorkerConfig>:
let worker_config = config.worker
    .as_ref()
    .map(|w| w.some_field.clone())
    .unwrap_or_default();

// Common fields (always present):
let common_field = config.common.some_field.clone();
```

**Verification Strategy**:
- Keep both `From<&TaskerConfig>` and `From<&TaskerConfigV2>` during transition
- Write tests verifying both produce equivalent results
- Delete legacy `From<&TaskerConfig>` only after component fully migrated
- Run full test suite after each component

**Success Criteria**:
- ✅ All component configs have `From<&TaskerConfigV2>` implementations
- ✅ Tests verify equivalence of old and new conversions
- ✅ No compilation warnings
- ✅ All 529+ tests passing after each component migration

---

### Phase 6C: Direct V2 Usage (Week 4)

**Goal**: Replace all direct `.tasker_config` access with `.tasker_config_v2`

**Files to Update** (~30 files with direct config access):

**Core Files**:
- `tasker-shared/src/system_context.rs` (orchestration_owned_queues initialization)
- `tasker-orchestration/src/orchestration/core.rs` (OrchestrationCore initialization)
- `tasker-orchestration/src/orchestration/bootstrap.rs` (Bootstrap configuration)
- `tasker-worker/src/bootstrap.rs` (Worker bootstrap)

**Lifecycle Services** (10+ files):
- Task initialization services
- Result processing services
- Step enqueuer services
- Task finalization services

**Event Systems**:
- `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`
- `tasker-orchestration/src/orchestration/event_systems/task_readiness_event_system.rs`
- `tasker-worker/src/worker/event_systems/worker_event_system.rs`

**Pattern Replacement**:

```rust
// Before: Direct legacy access
let queue_config = context.tasker_config.queues.clone();
let db_url = context.tasker_config.database_url();
let env = context.tasker_config.environment();

// After: Direct V2 access
let queue_config = context.tasker_config_v2.common.queues.clone();
let db_url = context.tasker_config_v2.common.database_url();
let env = context.tasker_config_v2.common.execution.environment.clone();

// Orchestration-specific access
let decision_config = context.tasker_config_v2.orchestration
    .as_ref()
    .map(|o| o.decision_points.clone())
    .unwrap_or_default();

// Worker-specific access
let worker_id = context.tasker_config_v2.worker
    .as_ref()
    .map(|w| w.worker_id.clone())
    .unwrap_or_else(|| "default-worker".to_string());
```

**Verification**:
- Grep for all `.tasker_config.` access patterns (should be zero after Phase 6C)
- Ensure all access goes through `.tasker_config_v2.common.*`, `.orchestration.*`, or `.worker.*`
- Run full integration test suite
- Performance benchmarks to ensure no regression

**Success Criteria**:
- ✅ Zero direct usages of `context.tasker_config` remaining
- ✅ All access goes through `context.tasker_config_v2`
- ✅ Integration tests pass
- ✅ No performance regressions

---

### Phase 6D: Bridge Removal (Week 5)

**Goal**: Delete bridge.rs and remove legacy `tasker_config` field from SystemContext

**Final Verification Checklist**:

```bash
# 1. No From<&TaskerConfig> implementations
grep -r "impl From<&TaskerConfig>" tasker-shared/src tasker-orchestration/src tasker-worker/src
# Expected: Zero results

# 2. No direct .tasker_config access
grep -r "\.tasker_config\." tasker-shared/src tasker-orchestration/src tasker-worker/src
# Expected: Zero results (only .tasker_config_v2)

# 3. No TaskerConfig imports (legacy)
grep -r "use.*TaskerConfig[^V2]" tasker-shared/src tasker-orchestration/src tasker-worker/src
# Expected: Only re-exports, no actual usage

# 4. All tests passing
cargo nextest run --lib --all-features
# Expected: 529+ tests passing
```

**Removal Steps**:

1. **Remove bridge.rs**:
   - Delete `tasker-shared/src/config/tasker/bridge.rs` (829 lines)
   - Remove from `tasker-shared/src/config/tasker/mod.rs`

2. **Update SystemContext**:
   ```rust
   pub struct SystemContext {
       pub processor_uuid: Uuid,

       // TAS-61 Phase 6D: Single config (V2 only)
       pub tasker_config: Arc<TaskerConfigV2>, // Renamed from tasker_config_v2

       pub message_client: Arc<UnifiedPgmqClient>,
       pub database_pool: PgPool,
       pub task_handler_registry: Arc<TaskHandlerRegistry>,
       pub circuit_breaker_manager: Option<Arc<CircuitBreakerManager>>,
       pub event_publisher: Arc<EventPublisher>,
   }
   ```

3. **Rename Field** (optional):
   - Consider renaming `tasker_config_v2` → `tasker_config` for cleaner API
   - Or keep `tasker_config_v2` to make migration explicit in code history

4. **Update Module Exports**:
   - Remove legacy TaskerConfig re-exports from `tasker-shared/src/config/tasker/mod.rs`
   - Make TaskerConfigV2 the primary export (consider aliasing as just `TaskerConfig`)

5. **Update Documentation**:
   - Remove all "Phase 6" transition documentation
   - Update SystemContext documentation
   - Update configuration loading documentation

**Success Criteria**:
- ✅ `bridge.rs` deleted
- ✅ `tasker_config: Arc<TaskerConfig>` removed from SystemContext
- ✅ Single config field: either `tasker_config: Arc<TaskerConfigV2>` or renamed
- ✅ Zero From<&TaskerConfig> implementations remaining
- ✅ All 529+ tests passing
- ✅ Single-layer conversion achieved: `TaskerConfigV2 → ComponentConfig`

---

## Risk Assessment & Mitigation

### High-Risk Areas

**1. SystemContext Replacement** ⚠️  CRITICAL
- **Risk**: Breaking ALL components simultaneously if done wrong
- **Mitigation**: Parallel config approach (Phase 6A) provides safety net
- **Test Strategy**: Integration tests MUST pass at every phase

**2. Bridge Removal** ⚠️  HIGH
- **Risk**: Removing safety net before all migrations complete
- **Mitigation**: Keep bridge through Phase 6C, only remove in Phase 6D
- **Test Strategy**: Grep verification before removal

**3. Optional Context Handling** ⚠️  MEDIUM
- **Risk**: `orchestration: Option<_>` requires unwrapping, potential panics
- **Mitigation**: Use `.as_ref().map().unwrap_or_default()` pattern consistently
- **Test Strategy**: Test with orchestration-only, worker-only configs

### Medium-Risk Areas

**1. Type Casting Changes**
- **Risk**: `as u64` casts might change behavior at numeric boundaries
- **Mitigation**: Document all casts, verify ranges in tests
- **Test Strategy**: Property-based tests for numeric conversions

**2. MPSC Channel Merging**
- **Risk**: Complex merge logic might have bugs with missing contexts
- **Mitigation**: Test with various context combinations
- **Test Strategy**: Unit tests for channel config merging

### Low-Risk Areas

**1. Component Config Conversions**
- **Risk**: Isolated, stateless conversions
- **Mitigation**: Straightforward pattern replacement
- **Test Strategy**: Existing tests verify correctness, equivalence tests during transition

---

## Detailed Component Analysis

### SystemContext Usage (Central)

**File**: `tasker-shared/src/system_context.rs`

**Current Usage**:
```rust
pub tasker_config: Arc<TaskerConfig>
```

**Access Patterns**:
- Pool configuration: `tasker_config.database.pool.*` (5 fields)
- Circuit breakers: `tasker_config.circuit_breakers.*`
- MPSC channels: `tasker_config.mpsc_channels.*`
- Queue config: `tasker_config.queues.orchestration_queues.*`
- Event publisher buffer: `tasker_config.mpsc_channels.shared.event_publisher.event_queue_buffer_size`

**Phase 6A Change**:
```rust
pub tasker_config: Arc<TaskerConfig>,      // Keep (via bridge)
pub tasker_config_v2: Arc<TaskerConfigV2>, // Add
```

**Phase 6C Change**: Update all access to use `.tasker_config_v2.common.*`

**Phase 6D Change**: Remove `.tasker_config`, keep only `.tasker_config_v2` (or rename to `.tasker_config`)

### Component Config Structs (15+)

**Examples**:

1. **StepEnqueuerConfig** (`tasker-shared/src/config/orchestration/step_enqueuer.rs`)
   - Fields: `max_steps_per_task`, `enable_detailed_logging`, `enqueue_timeout_seconds`
   - Legacy source: `config.execution.*`, `config.orchestration.*`
   - V2 source: `config.common.execution.*`, `config.orchestration.as_ref().*`

2. **BackoffCalculatorConfig** (`tasker-orchestration/src/orchestration/backoff_calculator.rs`)
   - Fields: Full backoff configuration
   - Legacy source: `config.backoff.*`
   - V2 source: `config.common.backoff.*`

3. **DecisionPointConfig** (used in DecisionPointService)
   - Fields: `enabled`, `max_steps_per_decision`, etc.
   - Legacy source: `config.decision_points.*`
   - V2 source: `config.orchestration.as_ref().map(|o| o.decision_points).unwrap_or_default()`

### Lifecycle Services (10+ files)

**Task Initialization** (6 files):
- `service.rs`, `namespace_validator.rs`, `workflow_initializer.rs`, `task_creator.rs`, `event_publisher.rs`, `step_dependency_builder.rs`
- Access patterns: Queue config, execution limits, backoff config

**Result Processing** (6 files):
- `service.rs`, `completion_handler.rs`, `error_handler.rs`, `event_publisher.rs`, `execution_context_provider.rs`, `state_handlers.rs`
- Access patterns: Queue config, execution config, decision points

**Step Enqueuer** (4 files):
- `service.rs`, `batch_processor.rs`, `queue_manager.rs`, `notification_handler.rs`
- Access patterns: Queue config, MPSC channels, backoff config

**Task Finalization** (6 files):
- `service.rs`, `completion_handler.rs`, `event_publisher.rs`, `execution_context_provider.rs`, `state_handlers.rs`, `atomic_claimer.rs`
- Access patterns: Queue config, execution config

### Bootstrap Systems

**OrchestrationBootstrap** (`tasker-orchestration/src/orchestration/bootstrap.rs`):
- Creates OrchestrationCore
- Initializes queues
- Configures event systems

**WorkerBootstrap** (`tasker-worker/src/bootstrap.rs`):
- Creates WorkerCore
- Initializes handler registry
- Configures event systems

**Access Patterns**: Heavy usage of `queues.*`, `event_systems.*`, `mpsc_channels.*`

### Actor Registry

**ActorRegistry** (`tasker-orchestration/src/actors/registry.rs`):
- Builds all 5 actors
- Each actor receives SystemContext
- Actors access config via `context.tasker_config.*`

**Phase 6C Impact**: Each actor's config access needs update to `.tasker_config_v2`

---

## Success Metrics

### Phase 6A (Parallel Support)
- ✅ All tests pass with both configs present (529+ tests)
- ✅ No regressions in existing behavior
- ✅ ConfigManager populates both fields correctly via bridge
- ✅ Performance unchanged (bridge conversion is fast)

### Phase 6B (Component Migration)
- ✅ Each component has `From<&TaskerConfigV2>` implementation
- ✅ Tests verify both `From<&TaskerConfig>` and `From<&TaskerConfigV2>` produce same results
- ✅ No compilation warnings
- ✅ All 529+ tests passing after each component migration

### Phase 6C (Direct V2 Usage)
- ✅ Zero usages of `context.tasker_config` remaining (verified by grep)
- ✅ All usages converted to `context.tasker_config_v2.common.*` or `.orchestration.*` or `.worker.*`
- ✅ Integration tests pass
- ✅ No performance regressions
- ✅ All 529+ tests passing

### Phase 6D (Bridge Removal)
- ✅ Zero `From<&TaskerConfig>` implementations remaining (verified by grep)
- ✅ `bridge.rs` deleted (829 lines removed)
- ✅ SystemContext has only V2 config field
- ✅ All 529+ tests passing
- ✅ Single-layer conversion achieved: `TaskerConfigV2 → ComponentConfig`
- ✅ Documentation updated to remove "Phase 6" references

---

## Quick Reference

### Key Files

**Configuration**:
- `tasker-shared/src/config/tasker/tasker_v2.rs` - V2 config structs (1,598 lines)
- `tasker-shared/src/config/tasker/bridge.rs` - Legacy conversion (829 lines, DELETE in Phase 6D)
- `tasker-shared/src/config/tasker/mod.rs` - Module exports

**SystemContext**:
- `tasker-shared/src/system_context.rs` - Central config holder

**Component Configs**:
- `tasker-shared/src/config/orchestration/*.rs` - Orchestration component configs
- `tasker-shared/src/config/worker.rs` - Worker component config
- `tasker-shared/src/config/event_systems.rs` - Event system configs

**Bootstrap**:
- `tasker-orchestration/src/orchestration/bootstrap.rs` - Orchestration bootstrap
- `tasker-orchestration/src/orchestration/core.rs` - Orchestration core
- `tasker-worker/src/bootstrap.rs` - Worker bootstrap

**Actors**:
- `tasker-orchestration/src/actors/registry.rs` - Actor registry

### Verification Commands

```bash
# Find all TaskerConfig usage
rg "TaskerConfig[^V2]" --type rust

# Find all .tasker_config access
rg "\.tasker_config\." --type rust

# Find all From<&TaskerConfig> implementations
rg "impl From<&TaskerConfig>" --type rust

# Run tests
cargo nextest run --lib --all-features

# Check compilation
cargo check --all-features

# Run clippy
cargo clippy --all-targets --all-features
```

---

## Timeline

**Week 1 (Phase 6A)**: Parallel config support - ZERO RISK
- Day 1-2: SystemContext updates, testing
- Day 3-5: Documentation, review, commit

**Weeks 2-3 (Phase 6B)**: Component migration - LOW to HIGH RISK
- Days 1-2: Component config structs (quick wins)
- Days 3-4: Isolated services (backoff, decision points)
- Days 5-7: Lifecycle services (10+ files)
- Days 8-9: Bootstrap systems
- Day 10: Actor registry

**Week 4 (Phase 6C)**: Direct V2 usage - MEDIUM RISK
- Days 1-3: Update ~30 files with direct config access
- Days 4-5: Testing, verification, performance validation

**Week 5 (Phase 6D)**: Bridge removal - HIGH RISK
- Days 1-2: Final verification (grep checks)
- Day 3: Delete bridge.rs, update SystemContext
- Days 4-5: Testing, documentation, final review

---

## Conclusion

The parallel config approach (Phase 6A) provides a zero-risk foundation for incremental migration. Each phase builds on the previous with clear success criteria and verification strategies.

**Key Insight**: The bridge is well-designed with straightforward field mappings. The main complexity is optional context handling, which has clear patterns (`as_ref().map().unwrap_or_default()`).

**Recommended Approach**: Execute phases sequentially with full test validation between each phase. The parallel config safety net allows easy rollback at any point before Phase 6D.

**Final State**: Single-layer conversion from TaskerConfigV2 directly to component configs, eliminating the bridge overhead and simplifying the configuration architecture.
