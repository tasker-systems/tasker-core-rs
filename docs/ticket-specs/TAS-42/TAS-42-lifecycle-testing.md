# TAS-42: Comprehensive Lifecycle Testing Framework - Implementation Record

## Executive Summary

This document records the actual implementation of the comprehensive lifecycle testing framework for validating task and workflow step lifecycles. The framework ensures all tests properly exercise the orchestration system through an **integrated approach** that validates both state machine transitions AND SQL function results in a single cohesive test pattern.

## Problem Statement

The original tests in `tasker-orchestration/tests/task_finalization_error_scenarios.rs` were failing because they:
1. Bypassed the orchestration framework using direct database manipulation
2. Created invalid state combinations that couldn't occur in production
3. Didn't validate the critical relationship between lifecycle actions and SQL function results
4. Failed to test WaitingForRetry states for both tasks AND steps

## Solution Overview

We developed an **integrated testing approach** that proves and documents the relationship between lifecycle actions and SQL function results. Rather than testing these separately, each test exercises the lifecycle AND immediately validates that SQL functions return expected values, creating living documentation of system behavior.

## Implementation Record

### Phase 1: Test Infrastructure Module ✅ COMPLETE

**Location**: `tests/common/lifecycle_test_helpers.rs`

#### Core Components Implemented

**1. ExpectedStepState & ExpectedTaskState**
```rust
pub struct ExpectedStepState {
    pub state: String,
    pub ready_for_execution: bool,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub backoff_request_seconds: Option<i32>,
    pub retry_limit: i32,  // Not optional - always required
}

pub struct ExpectedTaskState {
    pub state: String,
    pub total_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub error_steps: i64,
    pub completion_percentage: Option<f64>,
    pub execution_status: ExecutionStatus,
    pub recommended_action: Option<RecommendedAction>,
    pub blocked_by_errors: bool,
}
```

**2. SqlLifecycleAssertion**
Validates that SQL function results match expected lifecycle states:
```rust
impl SqlLifecycleAssertion {
    pub async fn assert_step_scenario(
        pool: &PgPool,
        task_uuid: Uuid,
        step_uuid: Uuid,
        expected: ExpectedStepState
    ) -> Result<()>

    pub async fn assert_task_scenario(
        pool: &PgPool,
        task_uuid: Uuid,
        expected: ExpectedTaskState
    ) -> Result<()>
}
```

**3. StepErrorSimulator**
Simulates realistic error scenarios:
```rust
impl StepErrorSimulator {
    pub async fn simulate_validation_error(...) // Non-retryable
    pub async fn simulate_execution_error(...) // Retryable
    pub async fn exhaust_retries(...)         // Permanent failure
}
```

**4. TestScenarioBuilder**
Loads and analyzes YAML templates:
```rust
impl TestScenarioBuilder {
    pub fn new(template_dir: &str) -> Self
    pub fn list_available_templates() -> Result<Vec<String>>
    pub fn load_template(name: &str) -> Self
    pub async fn build() -> Result<TestScenario>
}
```

**5. TestOrchestrator (Simplified)**
Focus on SQL validation and error simulation rather than complex component wrapping:
```rust
impl TestOrchestrator {
    pub async fn new(pool: PgPool) -> Result<Self>
    pub async fn create_task_from_template(...) -> Result<Task>
    pub async fn execute_step(...) -> Result<StepExecutionResult>
    pub async fn mark_task_complete(...) -> Result<Task>
}
```

### Phase 2: Fix Existing Tests ✅ COMPLETE

**Location**: `tests/task_finalization_error_scenarios.rs` (moved from package-level)

#### All 5 Tests Refactored with Integrated Approach

1. **test_task_finalization_with_all_steps_in_error_state**
   - Uses StepErrorSimulator to create realistic permanent failures
   - Validates SQL functions show all steps as non-retryable
   - Confirms task state machine transitions to Error

2. **test_task_finalization_with_mixed_step_states**
   - Creates complex scenario with success/error/pending steps
   - Validates SQL aggregations match expected counts
   - Demonstrates partial completion handling

3. **test_task_completion_with_all_steps_successful**
   - All steps complete successfully
   - SQL validation confirms ExecutionStatus::AllComplete
   - Task properly transitions to Complete state

4. **test_task_blocked_by_errors_detection**
   - Tests dependency blocking due to permanent failures
   - SQL validates dependencies_satisfied = false for blocked steps
   - Framework correctly identifies blocked state

5. **test_task_state_transition_integrity**
   - Validates complete audit trail through state transitions
   - SQL functions align with state machine transitions
   - Ensures no invalid state combinations

#### Key Implementation Pattern

Each test follows this integrated structure:
```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_example(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 Testing specific scenario");

    // STEP 1: Exercise lifecycle using framework
    let orchestrator = TestOrchestrator::new(pool.clone()).await?;
    let task = orchestrator.create_task_from_template(...).await?;
    StepErrorSimulator::simulate_error(...).await?;

    // STEP 2: Immediately validate SQL functions
    SqlLifecycleAssertion::assert_step_scenario(
        &pool,
        task.task_uuid,
        step.workflow_step_uuid,
        ExpectedStepState { ... }
    ).await?;

    // STEP 3: Document the relationship
    tracing::info!(
        lifecycle_action = "what we did",
        sql_result = "what SQL returned",
        "✅ INTEGRATION: Lifecycle → SQL alignment verified"
    );
}
```

### Key Discoveries & Adaptations

#### 1. Type System Alignment
**Challenge**: Significant mismatches between assumed and actual types
**Solution**: Comprehensive type discovery and alignment:
- ExecutionStatus: `Complete` → `AllComplete`, `Running` → `HasReadySteps`
- RecommendedAction: `ProcessReadySteps` → `ExecuteReadySteps`
- StepExecutionMetadata: Added required fields (custom, error_code, worker_hostname, etc.)
- Database columns: Removed non-existent fields, used correct column names

#### 2. Simplified but Superior Approach
**Original Plan**: Complex wrapper around all orchestration components
**Implemented Solution**: Focus on lifecycle ↔ SQL function integration
**Why It's Better**:
- Avoids bootstrap complexity of orchestration components
- Proves the critical relationships that matter most
- Creates living documentation of system behavior
- Each test demonstrates AND validates the integration

#### 3. Integrated SQL Validation
**Original Plan**: Separate SQL function tests
**Implemented Solution**: SQL validation embedded directly in each lifecycle test
**Why It's Better**:
- Single test proves both lifecycle action AND SQL result
- Immediate feedback on integration correctness
- Clear documentation of cause → effect relationships

## Current Status

### ✅ Completed
- **Infrastructure**: Complete test framework with all helper components
- **Test Migration**: All 5 failing tests moved and refactored
- **Type Alignment**: Full compatibility with actual codebase types
- **Integration Pattern**: Proven approach for lifecycle ↔ SQL validation
- **Compilation**: All code compiles successfully with proper imports

### 🚀 Ready for Next Phase
The foundation is solid and we can now efficiently implement additional test coverage using the established pattern.

## Next Steps - Phases 3 & 4

### Phase 3A: Step-Level WaitingForRetry Tests
**Location**: `tests/step_retry_lifecycle_tests.rs`

Tests to implement:
1. **test_step_waiting_for_retry_transitions**
   - Validate Error → WaitingForRetry → Ready state flow
   - Verify SQL functions track retry timing correctly

2. **test_exponential_backoff_calculation**
   - Validate formula: `LEAST(power(2, attempts) * interval '1 second', interval '30 seconds')`
   - Confirm SQL function calculates correct backoff times

3. **test_custom_vs_exponential_backoff**
   - Test backoff_request_seconds override behavior
   - Validate custom backoff takes precedence

4. **test_step_retry_eligibility_boundary_conditions**
   - Test retry_limit enforcement (default = 3)
   - Validate exhaustion detection

### Phase 3B: Task-Level WaitingForRetry Tests
**Location**: `tests/task_retry_lifecycle_tests.rs`

Tests to implement:
1. **test_task_waiting_for_retry_with_mixed_steps**
   - Multiple steps in different retry states
   - Task-level aggregation validation

2. **test_task_transitions_when_retries_ready**
   - Task state changes when retry backoff expires
   - Proper transition sequencing

3. **test_task_retry_coordination_with_dependencies**
   - Retry behavior with dependency chains
   - Blocked vs ready state detection

### Phase 3C: Complex Retry Scenarios
**Location**: `tests/complex_retry_scenarios.rs`

Tests to implement:
1. **test_cascading_retries_with_dependencies**
   - Diamond pattern with mixed success/retry paths
   - Dependency propagation validation

2. **test_partial_recovery_scenario**
   - Linear workflow with recovery after retries
   - Complete success path validation

3. **test_mixed_error_types**
   - ValidationError (not retryable)
   - ExecutionError (retryable)
   - ExternalServiceError (retryable with backoff)

4. **test_dependency_chain_failure_propagation**
   - Complex dependency failures
   - Cascading blockage detection

### Phase 4: Advanced Integration Patterns

#### TemplateTestRunner
Parameterized testing with YAML templates:
```rust
pub struct TemplateTestRunner {
    pub async fn run_template_with_errors(
        template_path: &str,
        error_pattern: ErrorPattern
    ) -> Result<TestExecutionSummary>
}

pub enum ErrorPattern {
    AllSuccess,
    FirstStepFails { retryable: bool },
    MiddleStepFails { step_name: String, attempts_before_success: i32 },
    LastStepFails { permanently: bool },
    RandomFailures { probability: f32, max_retries: i32 },
    DependencyBlockage { blocked_step: String, blocking_step: String },
}
```

#### Assertion Traits
Enhanced assertions for complex scenarios:
```rust
pub trait TaskAssertions {
    async fn assert_task_complete(&self, task_uuid: Uuid);
    async fn assert_task_blocked(&self, task_uuid: Uuid, blocked_count: usize);
    async fn assert_task_waiting_for_retry(&self, task_uuid: Uuid);
    async fn assert_task_error(&self, task_uuid: Uuid, error_count: usize);
}

pub trait StepAssertions {
    async fn assert_step_ready(&self, step_uuid: Uuid);
    async fn assert_step_blocked(&self, step_uuid: Uuid);
    async fn assert_step_waiting(&self, step_uuid: Uuid, retry_at: DateTime<Utc>);
    async fn assert_step_complete(&self, step_uuid: Uuid);
}
```

## Test Execution

### Running Tests
```bash
# Run all lifecycle tests
cargo test --all-features --test '*lifecycle*'

# Run with detailed tracing
RUST_LOG=info cargo test --test task_finalization_error_scenarios -- --nocapture

# Run specific test categories
cargo test --test step_retry_lifecycle_tests -- --nocapture
cargo test --test complex_retry_scenarios -- --nocapture
```

### Key Testing Principles
1. **Integrated Validation**: Every lifecycle action immediately validates SQL results
2. **Living Documentation**: Tests document the lifecycle → SQL relationship
3. **Realistic Scenarios**: Use actual YAML templates and production patterns
4. **Structured Tracing**: Clear logging showing cause → effect relationships
5. **Type Safety**: Full alignment with actual codebase types

## Success Metrics

### ✅ Achieved
1. All tests use proper state machine transitions (no direct DB manipulation)
2. SQL function validation integrated into each test
3. Complete type system alignment with codebase
4. Clear separation between retryable and non-retryable errors
5. Comprehensive structured tracing for debugging

### 📋 To Be Completed
1. Full WaitingForRetry state coverage (Phase 3A-3C)
2. Advanced integration patterns (Phase 4)
3. Template-based parameterized testing
4. Complete test execution validation for CI/CD

## Lessons Learned

1. **Integration Over Separation**: Testing lifecycle and SQL together provides superior validation
2. **Simplification Wins**: Avoiding complex orchestration wrappers made tests more maintainable
3. **Type Discovery Critical**: Understanding actual types early prevents significant rework
4. **Living Documentation Value**: Tests that document relationships are more valuable than isolated tests

## Dependencies

- Test fixtures: `tests/fixtures/task_templates/rust/`
- SQL functions: Database migrations with retry logic
- State machines: `tasker-shared/src/models/state_machines/`
- Error handling: Retry mechanics and backoff calculations

## Timeline

- **Phase 1-2**: ✅ COMPLETE (Test infrastructure and existing test fixes)
- **Phase 3A**: Step retry tests (Est: 2-3 hours)
- **Phase 3B**: Task retry tests (Est: 2-3 hours)
- **Phase 3C**: Complex scenarios (Est: 3-4 hours)
- **Phase 4**: Advanced patterns (Est: 3-4 hours)

## Conclusion

The integrated testing framework successfully validates the critical relationship between lifecycle actions and SQL function results. This approach creates living documentation while ensuring system correctness. The foundation is complete and ready for expanded test coverage using the proven pattern.