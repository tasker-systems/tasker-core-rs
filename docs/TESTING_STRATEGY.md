# Tasker Core Rust - Testing Strategy

## Overview

Our delegation-based architecture requires a multi-layered testing approach that validates:
1. **Database models and CRUD operations**
2. **DAG operations and dependency resolution**
3. **FFI serialization and delegation patterns**
4. **Orchestration logic and state management**
5. **Integration with external systems**

## Testing Library Selection

### **Core Testing Stack**

#### **1. Transactional Testing: `sqlx-test`**
```toml
sqlx-test = "0.8"  # For database transaction rollback
```
- **Purpose**: Automatic transaction rollback for isolated tests
- **Benefit**: No test data pollution, parallel test execution
- **Usage**: Wrap each test in a transaction that rolls back

#### **2. Property-Based Testing: `proptest`** ✅ (Already included)
```toml
proptest = "1.0"  # For property-based testing
```
- **Purpose**: Generate random test cases for DAG operations
- **Use Cases**: 
  - Cycle detection properties
  - Dependency resolution invariants
  - State machine transition validity

#### **3. Snapshot Testing: `insta`**
```toml
insta = { version = "1.34", features = ["serde", "json"] }
```
- **Purpose**: Verify FFI serialization formats don't break
- **Use Cases**:
  - JSON serialization for Ruby FFI
  - Database query result formats
  - Event payload structures

#### **4. Async Testing: `tokio-test`** ✅ (Already included)
```toml
tokio-test = "0.4"  # For async test utilities
```
- **Purpose**: Advanced async testing utilities
- **Features**: Time manipulation, async assertion helpers

#### **5. Mock Framework: `mockall`**
```toml
mockall = "0.12"  # For mocking external dependencies
```
- **Purpose**: Mock step execution delegates for orchestration tests
- **Use Cases**: Test delegation patterns without external dependencies

#### **6. Fuzzing: `cargo-fuzz` (Optional)**
```toml
# Cargo.toml [dev-dependencies]
arbitrary = { version = "1.0", features = ["derive"] }
```
- **Purpose**: Find edge cases in DAG operations
- **Use Cases**: Complex dependency graph scenarios

### **Specialized Testing (Future Consideration)**

#### **BDD Testing: `cucumber-rs`**
- **When to use**: If we need stakeholder-readable tests
- **Decision**: Defer until Phase 4 (FFI integration)

#### **Performance Testing: `criterion`** ✅ (Already included)
- **Purpose**: Validate 10-100x performance improvement targets
- **Use Cases**: Dependency resolution benchmarks

## Testing Architecture

### **1. Unit Tests (Model Layer)**
**Location**: `src/models/*/tests.rs` (inline)
**Framework**: Standard Rust + `sqlx-test`

```rust
// Example: Transactional test wrapper
#[sqlx::test]
async fn test_task_namespace_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test runs in transaction, auto-rollback
    let namespace = TaskNamespace::create(&pool, new_namespace).await?;
    assert_eq!(namespace.name, "test");
    // No cleanup needed - transaction rolls back
    Ok(())
}
```

### **2. Integration Tests (Orchestration Layer)**
**Location**: `tests/integration/`
**Framework**: `tokio-test` + `mockall` + `sqlx-test`

```rust
// Example: Delegation pattern test
#[sqlx::test]
async fn test_task_orchestration_delegation(pool: PgPool) -> Result<()> {
    let mut mock_delegate = MockStepExecutionDelegate::new();
    mock_delegate
        .expect_execute_steps()
        .returning(|_, _| Ok(vec![successful_step_result()]));
    
    let coordinator = OrchestrationCoordinator::new();
    let result = coordinator.orchestrate_task(task_id, &mock_delegate).await?;
    
    assert_matches!(result, TaskResult::Complete(_));
    Ok(())
}
```

### **3. Property-Based Tests (DAG Operations)**
**Location**: `tests/property_based/`
**Framework**: `proptest` + `sqlx-test`

```rust
// Example: DAG cycle detection properties
proptest! {
    #[test]
    fn dag_cycle_detection_is_correct(
        edges in prop::collection::vec(edge_strategy(), 1..20)
    ) {
        tokio_test::block_on(async {
            let pool = test_pool().await;
            
            // Build DAG from generated edges
            for edge in &edges {
                WorkflowStepEdge::create(&pool, edge.clone()).await.ok();
            }
            
            // Property: If cycle detection says no cycle, then no cycle exists
            let has_cycle = detect_cycle_bruteforce(&pool).await;
            let detection_says_cycle = WorkflowStepEdge::would_create_cycle(&pool, ...).await;
            
            prop_assert_eq!(has_cycle, detection_says_cycle);
        });
    }
}
```

### **4. Snapshot Tests (FFI Serialization)**
**Location**: `tests/snapshots/`
**Framework**: `insta` + serialization

```rust
// Example: FFI serialization format stability
#[test]
fn test_task_for_orchestration_serialization() {
    let task_data = TaskForOrchestration { /* ... */ };
    
    // JSON serialization for Ruby FFI
    let json = serde_json::to_string_pretty(&task_data).unwrap();
    insta::assert_snapshot!(json);
    
    // MessagePack for performance-critical FFI
    let msgpack = rmp_serde::to_vec(&task_data).unwrap();
    insta::assert_debug_snapshot!(msgpack);
}
```

### **5. End-to-End Tests (Full Delegation)**
**Location**: `tests/e2e/`
**Framework**: Full stack with real database

```rust
// Example: Complete workflow orchestration
#[tokio::test]
async fn test_complete_workflow_execution() {
    let test_db = TestDatabase::new().await;
    
    // Set up a real workflow with dependencies
    let (task, steps) = create_test_workflow(&test_db.pool).await;
    
    // Use real delegation (but mock step handlers)
    let delegate = TestStepDelegate::new();
    let coordinator = OrchestrationCoordinator::new();
    
    let result = coordinator.orchestrate_task(task.task_id, &delegate).await;
    
    // Verify complete workflow execution
    assert_eq!(result, TaskResult::Complete);
    
    // Verify state transitions were recorded
    let transitions = TaskTransition::get_history(&test_db.pool, task.task_id).await?;
    assert!(transitions.len() > 1);
}
```

## Test Infrastructure Components

### **1. Transactional Test Helper**
```rust
// tests/common/test_db.rs
pub struct TestDatabase {
    pool: PgPool,
}

impl TestDatabase {
    pub async fn new() -> Self {
        let pool = PgPool::connect(&test_database_url()).await.unwrap();
        Self { pool }
    }
    
    pub async fn with_transaction<F, R>(&self, test: F) -> R
    where
        F: FnOnce(&PgPool) -> BoxFuture<'_, R>,
    {
        let mut tx = self.pool.begin().await.unwrap();
        let result = test(&mut *tx).await;
        tx.rollback().await.unwrap(); // Always rollback
        result
    }
}
```

### **2. Test Data Builders**
```rust
// tests/common/builders.rs
pub struct TaskNamespaceBuilder {
    name: Option<String>,
    description: Option<String>,
}

impl TaskNamespaceBuilder {
    pub fn new() -> Self { /* ... */ }
    pub fn with_name(mut self, name: &str) -> Self { /* ... */ }
    pub fn with_description(mut self, desc: &str) -> Self { /* ... */ }
    
    pub async fn build(self, pool: &PgPool) -> TaskNamespace {
        let new_namespace = NewTaskNamespace {
            name: self.name.unwrap_or_else(|| unique_name("namespace")),
            description: self.description,
        };
        TaskNamespace::create(pool, new_namespace).await.unwrap()
    }
}

// Usage:
// let namespace = TaskNamespaceBuilder::new()
//     .with_name("test_namespace")
//     .build(&pool).await;
```

### **3. Mock Delegation Framework**
```rust
// tests/common/mock_delegate.rs
use mockall::mock;

mock! {
    pub StepExecutionDelegate {}
    
    #[async_trait]
    impl crate::orchestration::StepExecutionDelegate for StepExecutionDelegate {
        async fn execute_steps(&self, task_id: i64, steps: &[ViableStep]) 
            -> Result<Vec<StepResult>, StepExecutionError>;
        async fn get_step_handler_config(&self, step_name: &str) 
            -> Option<StepHandlerConfig>;
    }
}

// Helper for common scenarios
impl MockStepExecutionDelegate {
    pub fn success_scenario() -> Self {
        let mut mock = MockStepExecutionDelegate::new();
        mock.expect_execute_steps()
            .returning(|_, steps| {
                Ok(steps.iter().map(|_| successful_step_result()).collect())
            });
        mock
    }
    
    pub fn failure_scenario() -> Self { /* ... */ }
    pub fn retry_scenario() -> Self { /* ... */ }
}
```

### **4. Property-Based Test Strategies**
```rust
// tests/common/strategies.rs
use proptest::prelude::*;

pub fn task_namespace_strategy() -> impl Strategy<Value = NewTaskNamespace> {
    (
        "[a-zA-Z_][a-zA-Z0-9_]{0,63}",  // Valid namespace names
        prop::option::of("[a-zA-Z0-9 ]{0,255}"), // Optional description
    ).prop_map(|(name, description)| NewTaskNamespace { name, description })
}

pub fn dag_edge_strategy() -> impl Strategy<Value = (i64, i64)> {
    (1i64..=100, 1i64..=100)
        .prop_filter("No self-loops", |(from, to)| from != to)
}

pub fn workflow_scenario_strategy() -> impl Strategy<Value = WorkflowScenario> {
    // Generate realistic workflow patterns:
    // - Linear chains
    // - Diamond dependencies  
    // - Fan-out/fan-in patterns
    // - Complex but acyclic graphs
}
```

## Test Categories & Coverage

### **1. Model Layer Tests (95% coverage target)**
- **CRUD Operations**: Create, read, update, delete for all models
- **Validation**: Unique constraints, foreign keys, data integrity
- **Edge Cases**: Boundary conditions, null handling, constraint violations
- **Performance**: Query optimization, index usage

### **2. DAG Operation Tests (100% coverage target)**
- **Cycle Detection**: Property-based testing with generated graphs
- **Dependency Resolution**: Correctness of viable step calculation
- **Performance**: 10-100x improvement validation vs PostgreSQL
- **Edge Cases**: Empty graphs, single nodes, complex dependencies

### **3. FFI Serialization Tests (100% coverage target)**
- **Format Stability**: Snapshot tests for JSON/MessagePack output
- **Round-trip**: Serialize → deserialize → verify equality
- **Cross-language**: Validate Ruby/Python can consume formats
- **Performance**: Serialization overhead benchmarks

### **4. Delegation Pattern Tests (90% coverage target)**
- **Handoff Scenarios**: Task → orchestration → delegation → framework
- **Error Handling**: Delegation failures, timeouts, invalid responses
- **State Management**: Proper state transitions during delegation
- **Performance**: FFI overhead measurement

### **5. Integration Tests (80% coverage target)**
- **End-to-End Workflows**: Complete task orchestration cycles
- **Database Transactions**: ACID properties under load
- **Concurrent Operations**: Multiple tasks, race conditions
- **Error Recovery**: Partial failures, retry scenarios

## Implementation Plan

### **Phase 1: Test Infrastructure** (Current)
1. ✅ Add testing dependencies to Cargo.toml
2. ✅ Create transactional test helpers
3. ✅ Set up test data builders
4. ✅ Create mock delegation framework

### **Phase 2: Model Testing Enhancement**
1. Replace current basic tests with transactional tests
2. Add comprehensive edge case coverage
3. Implement property-based tests for validation logic
4. Add performance benchmarks for critical queries

### **Phase 3: DAG Testing**
1. Property-based tests for cycle detection
2. Performance tests vs PostgreSQL dependency resolution
3. Comprehensive dependency resolution validation
4. Stress tests with large graphs

### **Phase 4: FFI Testing**
1. Snapshot tests for serialization formats
2. Cross-language compatibility tests
3. Performance benchmarks for FFI overhead
4. Round-trip validation tests

### **Phase 5: Integration Testing**
1. Full orchestration workflow tests
2. Delegation pattern validation
3. Concurrent operation testing
4. Error scenario testing

## Success Metrics

### **Coverage Targets**
- **Unit Tests**: >95% line coverage
- **Integration Tests**: >80% scenario coverage  
- **Property-Based Tests**: 1000+ generated cases per property
- **Performance Tests**: All benchmarks pass 10x improvement targets

### **Quality Gates**
- All tests pass in CI/CD pipeline
- No flaky tests (>99% reliability)
- Test execution time <30 seconds for full suite
- Memory usage <100MB during test execution

### **Documentation**
- Every test documents the scenario it validates
- Property-based tests document the invariants they verify
- Integration tests document the workflows they validate
- Performance tests document the benchmarks they measure

This comprehensive testing strategy ensures our delegation-based architecture is thoroughly validated while maintaining fast feedback cycles and high reliability.