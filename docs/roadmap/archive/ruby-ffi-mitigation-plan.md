# Ruby FFI Mitigation Plan

**Status**: URGENT - Critical recovery required
**Created**: 2025-01-14
**Priority**: P0 - Blocks all Ruby integration work
**Estimated Timeline**: 1-2 sessions

## Crisis Summary

### What Happened
A previous session had a working Ruby FFI implementation that properly followed the delegation architecture:
- Ruby bindings provided simple FFI bridges to core Rust logic
- Singleton patterns were used for shared resources (database connections, event publishers)
- Performance targets were met through proper resource reuse
- Integration tests were passing

However, this working implementation was lost when:
1. Session ended before committing working code to git
2. Subsequent session accidentally overwrote working FFI bridges with incorrect re-implementations
3. Current codebase violates fundamental delegation architecture principles

### Current Impact
- **Ruby-Rust Integration**: Completely broken - bindings don't use core logic
- **Performance**: Each FFI call recreates expensive resources (database connections, tokio runtimes)
- **Architecture Violation**: Ruby bindings reimplement core logic instead of delegating
- **Development Blocked**: Cannot proceed with Phase 3 enhanced event integration

## Critical Issues Analysis

### 1. Handler Re-implementation Violations

#### Files with Incorrect Implementations
- `bindings/ruby/ext/tasker_core/src/handlers/base_step_handler.rs` (203 lines)
- `bindings/ruby/ext/tasker_core/src/handlers/base_task_handler.rs` (146 lines)

#### What's Wrong
```rust
// WRONG: Creates new orchestration components per FFI call
fn execute_step_wrapper(step_data_value: Value) -> Result<Value, Error> {
    // Creates new database connection every time
    let pool = PgPool::connect(&database_url).await?;

    // Creates new orchestration components every time
    let sql_executor = SqlFunctionExecutor::new(pool.clone());
    let event_publisher = EventPublisher::new();
    let state_manager = StateManager::new(sql_executor, event_publisher.clone(), pool.clone());
    let step_executor = StepExecutor::new(state_manager, registry, event_publisher);

    // Reimplements step execution logic instead of delegating
    let state_result = step_executor.evaluate_step_state(step_id).await?;
}
```

#### What Should Be
```rust
// CORRECT: Delegate to existing core logic with singleton resources
fn execute_step_wrapper(step_data_value: Value) -> Result<Value, Error> {
    let step_context = parse_step_context(step_data_value)?;

    // Use singleton orchestration system
    let orchestration_system = get_global_orchestration_system();

    // Delegate to core step handler
    let result = orchestration_system.execute_step(step_context).await?;

    ruby_value_from_result(result)
}
```

### 2. Event System Duplication

#### Files with Incorrect Implementations
- `bindings/ruby/ext/tasker_core/src/events/bridge.rs` (271 lines)
- `bindings/ruby/ext/tasker_core/src/events/publisher.rs` (248 lines)
- `bindings/ruby/ext/tasker_core/src/events/subscriber.rs` (266 lines)

#### What's Wrong
- Complete reimplementation of event publishing logic
- Separate `EventPublisher` instances instead of using core singleton
- Manual event type mapping instead of using core event types
- No integration with core orchestration events

#### What Should Be
- Simple FFI bridges that delegate to `src/events/publisher.rs`
- Use core event types from `src/events/types.rs`
- Singleton pattern for shared event publisher
- Direct integration with core orchestration system

### 3. Architecture Violations

#### Resource Recreation Anti-Pattern
Every FFI call currently:
1. Creates new database connection (`PgPool::connect()`)
2. Creates new tokio runtime (`tokio::runtime::Builder::new_current_thread()`)
3. Creates new orchestration components (`StateManager`, `EventPublisher`, etc.)
4. Performs expensive initialization work

#### Performance Impact
- **Database Connections**: Connection pool exhaustion under load
- **Runtime Overhead**: Tokio runtime creation is expensive (~10-50ms per call)
- **Memory Usage**: No resource reuse leads to memory bloat
- **Violates Performance Targets**: <1ms FFI overhead goal impossible with current approach

## Recovery Strategy

### Phase 1: Remove Incorrect Implementations (Session 1)

#### 1.1 Delete Incorrect Handler Files
```bash
# Remove files that reimplement core logic
rm bindings/ruby/ext/tasker_core/src/handlers/base_step_handler.rs
rm bindings/ruby/ext/tasker_core/src/handlers/base_task_handler.rs
```

#### 1.2 Delete Incorrect Event Files
```bash
# Remove files that reimplement event logic
rm bindings/ruby/ext/tasker_core/src/events/bridge.rs
rm bindings/ruby/ext/tasker_core/src/events/publisher.rs
rm bindings/ruby/ext/tasker_core/src/events/subscriber.rs
```

#### 1.3 Update Module Structure
- Remove handler and event module exports from `src/lib.rs`
- Update `src/handlers/mod.rs` to remove deleted files
- Update `src/events/mod.rs` to remove deleted files

### Phase 2: Create Proper FFI Bridges (Session 1-2)

#### 2.1 Global Resource Management
Create singleton pattern for shared resources:

```rust
// bindings/ruby/ext/tasker_core/src/globals.rs
use std::sync::OnceLock;
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
use tasker_core::events::EventPublisher;
use sqlx::PgPool;

static GLOBAL_ORCHESTRATION_SYSTEM: OnceLock<OrchestrationSystem> = OnceLock::new();

pub struct OrchestrationSystem {
    pub workflow_coordinator: WorkflowCoordinator,
    pub event_publisher: EventPublisher,
    pub database_pool: PgPool,
}

pub fn get_global_orchestration_system() -> &'static OrchestrationSystem {
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // Initialize once, reuse forever
        OrchestrationSystem::new()
    })
}
```

#### 2.2 Step Handler FFI Bridge
Create minimal bridge that delegates to core:

```rust
// bindings/ruby/ext/tasker_core/src/step_handler_bridge.rs
use crate::globals::get_global_orchestration_system;
use tasker_core::orchestration::step_handler::StepExecutionContext;

fn execute_step_wrapper(step_data_value: Value) -> Result<Value, Error> {
    let step_context: StepExecutionContext = parse_step_context(step_data_value)?;

    let orchestration = get_global_orchestration_system();
    let result = orchestration.workflow_coordinator
        .execute_step(step_context)
        .await?;

    ruby_value_from_step_result(result)
}
```

#### 2.3 Task Handler FFI Bridge
Create minimal bridge that delegates to core:

```rust
// bindings/ruby/ext/tasker_core/src/task_handler_bridge.rs
use crate::globals::get_global_orchestration_system;
use tasker_core::orchestration::task_handler::TaskExecutionContext;

fn handle_task_wrapper(task_data_value: Value) -> Result<Value, Error> {
    let task_context: TaskExecutionContext = parse_task_context(task_data_value)?;

    let orchestration = get_global_orchestration_system();
    let result = orchestration.workflow_coordinator
        .handle_task(task_context)
        .await?;

    ruby_value_from_task_result(result)
}
```

#### 2.4 Event System FFI Bridge
Create minimal bridge that delegates to core:

```rust
// bindings/ruby/ext/tasker_core/src/event_bridge.rs
use crate::globals::get_global_orchestration_system;
use tasker_core::events::Event;

fn publish_event_wrapper(event_data_value: Value) -> Result<Value, Error> {
    let event: Event = parse_event_data(event_data_value)?;

    let orchestration = get_global_orchestration_system();
    orchestration.event_publisher.publish_event(event).await?;

    ruby_value_from_success()
}
```

### Phase 3: Integration Testing (Session 2)

#### 3.1 Basic FFI Tests
- Test that FFI bridges delegate to core logic
- Verify singleton pattern works correctly
- Validate resource reuse across multiple calls

#### 3.2 Ruby Integration Tests
- Test Ruby step handlers work with FFI bridges
- Verify Ruby event publishing flows through core
- Validate performance meets <1ms FFI overhead target

#### 3.3 End-to-End Workflow Tests
- Test complete Ruby-to-Rust workflow execution
- Verify event flow from Rust orchestration to Ruby subscribers
- Validate error handling and retry logic

## Success Criteria

### Functional Requirements
- [ ] Ruby step handlers execute through core orchestration logic
- [ ] Ruby task handlers delegate to core task execution
- [ ] Ruby event publishing uses core event system
- [ ] Singleton pattern maintains shared resources across FFI calls
- [ ] No duplicate implementations of core logic in Ruby bindings

### Performance Requirements
- [ ] FFI overhead <1ms per call (measured with proper resource reuse)
- [ ] Database connection reuse across multiple FFI calls
- [ ] Tokio runtime reuse across multiple FFI calls
- [ ] Memory usage stable under repeated FFI calls

### Architecture Requirements
- [ ] Ruby bindings contain only FFI bridges, no core logic reimplementation
- [ ] Delegation architecture properly implemented
- [ ] Core Rust logic is single source of truth for orchestration
- [ ] Ruby bindings provide Ruby-friendly interface to core functionality

## Risk Mitigation

### Risk: Resource Leaks
- **Mitigation**: Implement proper cleanup in singleton destructors
- **Testing**: Memory usage monitoring during extended FFI testing

### Risk: FFI Boundary Errors
- **Mitigation**: Comprehensive error handling and type conversion
- **Testing**: Fuzz testing with invalid Ruby inputs

### Risk: Performance Regression
- **Mitigation**: Benchmark against performance targets before/after
- **Testing**: Load testing with concurrent FFI calls

## Implementation Checklist

### Session 1 Goals
- [ ] Remove all incorrect implementation files
- [ ] Create global resource management system
- [ ] Implement basic step handler FFI bridge
- [ ] Implement basic task handler FFI bridge
- [ ] Update module structure and exports

### Session 2 Goals
- [ ] Implement event system FFI bridge
- [ ] Create comprehensive integration tests
- [ ] Validate performance targets are met
- [ ] Document proper FFI usage patterns
- [ ] Commit working implementation to git

### Post-Recovery
- [ ] Resume Phase 3 enhanced event integration
- [ ] Add Ruby integration to CI/CD pipeline
- [ ] Create developer documentation for FFI bridges
- [ ] Implement monitoring for FFI performance

---

**CRITICAL**: This plan must be executed before any other Ruby integration work can proceed. The current codebase violates fundamental architecture principles and must be corrected to restore the delegation pattern that makes this project viable.

**Next Steps**: Begin with Session 1 goals - remove incorrect implementations and create proper FFI bridges that delegate to core logic.
