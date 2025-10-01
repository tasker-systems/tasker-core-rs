# Comprehensive Lifecycle Testing Framework Guide

This guide demonstrates the complete lifecycle testing framework implemented for TAS-42, showing patterns, examples, and best practices for validating task and workflow step lifecycles with integrated SQL function validation.

## Table of Contents

1. [Framework Overview](#framework-overview)
2. [Core Testing Patterns](#core-testing-patterns)
3. [Advanced Assertion Traits](#advanced-assertion-traits)
4. [Template-Based Testing](#template-based-testing)
5. [SQL Function Integration](#sql-function-integration)
6. [Example Test Executions](#example-test-executions)
7. [Tracing Output Examples](#tracing-output-examples)
8. [Best Practices](#best-practices)
9. [Troubleshooting](#troubleshooting)

## Framework Overview

### Architecture

The comprehensive lifecycle testing framework consists of several key components:

```rust
// Core Infrastructure
TestOrchestrator          // Wrapper around orchestration components
StepErrorSimulator        // Realistic error scenario simulation
SqlLifecycleAssertion     // SQL function validation
TestScenarioBuilder       // YAML template loading

// Advanced Patterns
TemplateTestRunner        // Parameterized error pattern testing
ErrorPattern              // Comprehensive error configuration
TaskAssertions           // Task-level validation trait
StepAssertions           // Step-level validation trait
```

### Integration Strategy

Each test follows the **integrated validation pattern**:

1. **Exercise Lifecycle**: Use orchestration framework to create scenario
2. **Capture SQL State**: Call SQL functions to get current state
3. **Assert Integration**: Validate SQL functions return expected values
4. **Document Relationship**: Structured tracing showing cause → effect

## Core Testing Patterns

### Pattern 1: Basic Lifecycle Validation

```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_basic_lifecycle_validation(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 Testing basic lifecycle progression");

    // STEP 1: Exercise lifecycle using framework
    let orchestrator = TestOrchestrator::new(pool.clone());
    let task = orchestrator.create_simple_task("test", "basic_validation").await?;
    let step = get_first_step(&pool, task.task_uuid).await?;

    // STEP 2: Validate initial state
    pool.assert_step_ready(step.workflow_step_uuid).await?;

    // STEP 3: Execute step
    let result = orchestrator.execute_step(&step, true, 1000).await?;
    assert!(result.success);

    // STEP 4: Validate final state
    pool.assert_step_complete(step.workflow_step_uuid).await?;

    tracing::info!("✅ Basic lifecycle validation complete");
    Ok(())
}
```

### Pattern 2: Error and Retry Validation

```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_error_retry_validation(pool: PgPool) -> Result<()> {
    tracing::info!("🔄 Testing error and retry behavior");

    let orchestrator = TestOrchestrator::new(pool.clone());
    let task = orchestrator.create_simple_task("test", "retry_validation").await?;
    let step = get_first_step(&pool, task.task_uuid).await?;

    // STEP 1: Simulate retryable error
    StepErrorSimulator::simulate_execution_error(
        &pool,
        &step,
        1 // attempt number
    ).await?;

    // STEP 2: Validate retry behavior
    pool.assert_step_retry_behavior(
        step.workflow_step_uuid,
        1,    // expected attempts
        None, // no custom backoff
        true  // still retry eligible
    ).await?;

    tracing::info!("✅ Error retry validation complete");
    Ok(())
}
```

### Pattern 3: Complex Dependency Validation

```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_dependency_validation(pool: PgPool) -> Result<()> {
    tracing::info!("🔗 Testing dependency relationships");

    let orchestrator = TestOrchestrator::new(pool.clone());

    // Create diamond pattern workflow
    let task = create_diamond_workflow_task(&orchestrator).await?;
    let steps = get_task_steps(&pool, task.task_uuid).await?;

    // Execute start step
    let result = orchestrator.execute_step(&steps[0], true, 1000).await?;
    assert!(result.success);

    // Fail one branch
    StepErrorSimulator::simulate_validation_error(
        &pool,
        &steps[1],
        "dependency_test_error"
    ).await?;

    // Complete other branch
    let result = orchestrator.execute_step(&steps[2], true, 1000).await?;
    assert!(result.success);

    // Validate convergence step is blocked
    pool.assert_step_blocked(steps[3].workflow_step_uuid).await?;

    tracing::info!("✅ Dependency validation complete");
    Ok(())
}
```

## Advanced Assertion Traits

### TaskAssertions Trait Usage

```rust
use common::lifecycle_test_helpers::{TaskAssertions, TaskStepDistribution};

// Task completion validation
pool.assert_task_complete(task_uuid).await?;

// Task error state validation
pool.assert_task_error(task_uuid, 2).await?; // 2 error steps

// Complex step distribution validation
pool.assert_task_step_distribution(
    task_uuid,
    TaskStepDistribution {
        total_steps: 4,
        completed_steps: 2,
        failed_steps: 1,
        ready_steps: 0,
        pending_steps: 1,
        in_progress_steps: 0,
        error_steps: 1,
    }
).await?;

// Execution status validation
pool.assert_task_execution_status(
    task_uuid,
    ExecutionStatus::BlockedByFailures,
    Some(RecommendedAction::HandleFailures)
).await?;

// Completion percentage validation
pool.assert_task_completion_percentage(task_uuid, 75.0, 5.0).await?;
```

### StepAssertions Trait Usage

```rust
use common::lifecycle_test_helpers::StepAssertions;

// Basic step state validations
pool.assert_step_ready(step_uuid).await?;
pool.assert_step_complete(step_uuid).await?;
pool.assert_step_blocked(step_uuid).await?;

// Retry behavior validation
pool.assert_step_retry_behavior(
    step_uuid,
    3,        // expected attempts
    Some(30), // custom backoff seconds
    false     // not retry eligible (exhausted)
).await?;

// Dependency validation
pool.assert_step_dependencies_satisfied(step_uuid, true).await?;

// State transition sequence validation
pool.assert_step_state_sequence(
    step_uuid,
    vec!["Pending".to_string(), "InProgress".to_string(), "Complete".to_string()]
).await?;

// Permanent failure validation
pool.assert_step_failed_permanently(step_uuid).await?;

// Waiting for retry with specific time
let retry_time = chrono::Utc::now() + chrono::Duration::seconds(60);
pool.assert_step_waiting(step_uuid, retry_time).await?;
```

## Template-Based Testing

### ErrorPattern Configuration

```rust
use common::lifecycle_test_helpers::{ErrorPattern, TemplateTestRunner};

// Simple patterns
let success_pattern = ErrorPattern::AllSuccess;
let first_fail_pattern = ErrorPattern::FirstStepFails { retryable: true };
let last_fail_pattern = ErrorPattern::LastStepFails { permanently: false };

// Advanced patterns
let targeted_pattern = ErrorPattern::MiddleStepFails {
    step_name: "process_payment".to_string(),
    attempts_before_success: 3
};

let dependency_pattern = ErrorPattern::DependencyBlockage {
    blocked_step: "finalize_order".to_string(),
    blocking_step: "validate_payment".to_string()
};

// Custom pattern with full control
let custom_pattern = ErrorPattern::Custom {
    step_configs: {
        let mut configs = HashMap::new();
        configs.insert("critical_step".to_string(), StepErrorConfig {
            error_type: StepErrorType::ExternalServiceError,
            attempts_before_success: Some(5),
            custom_backoff_seconds: Some(120),
            permanently_fails: false,
        });
        configs
    }
};
```

### Template Runner Usage

```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_template_patterns(pool: PgPool) -> Result<()> {
    let template_runner = TemplateTestRunner::new(pool.clone()).await?;

    // Test single pattern
    let summary = template_runner.run_template_with_errors(
        "order_fulfillment.yaml",
        ErrorPattern::FirstStepFails { retryable: true }
    ).await?;

    assert!(summary.sql_validations_passed > 0);
    assert_eq!(summary.sql_validations_failed, 0);

    // Test all patterns automatically
    let summaries = template_runner
        .run_template_with_all_patterns("linear_workflow.yaml")
        .await?;

    for summary in summaries {
        tracing::info!(
            pattern = summary.error_pattern,
            execution_time = summary.execution_time_ms,
            validations = summary.sql_validations_passed,
            "Pattern execution complete"
        );
    }

    Ok(())
}
```

## SQL Function Integration

### Direct SQL Function Testing

```rust
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_direct_sql_functions(pool: PgPool) -> Result<()> {
    // Test get_step_readiness_status
    let step_status = sqlx::query!(
        "SELECT ready_for_execution, dependencies_satisfied, retry_eligible, attempts,
                backoff_request_seconds, next_retry_at
         FROM get_step_readiness_status($1)",
        step_uuid
    )
    .fetch_one(&pool)
    .await?;

    // Validate individual fields
    assert_eq!(step_status.ready_for_execution, Some(true));
    assert_eq!(step_status.dependencies_satisfied, Some(true));
    assert_eq!(step_status.retry_eligible, Some(false));
    assert_eq!(step_status.attempts, Some(0));

    // Test get_task_execution_context
    let task_context = sqlx::query!(
        "SELECT total_steps, completed_steps, failed_steps, ready_steps,
                pending_steps, in_progress_steps, error_steps,
                completion_percentage, execution_status, recommended_action,
                blocked_by_errors
         FROM get_task_execution_context($1)",
        task_uuid
    )
    .fetch_one(&pool)
    .await?;

    // Validate task aggregations
    assert!(task_context.total_steps.unwrap_or(0) > 0);
    assert_eq!(task_context.completed_steps, Some(0));
    assert_eq!(task_context.failed_steps, Some(0));

    Ok(())
}
```

### Integrated SQL Validation Pattern

```rust
// The standard pattern used throughout the framework
async fn validate_integrated_sql_behavior(
    pool: &PgPool,
    task_uuid: Uuid,
    step_uuid: Uuid
) -> Result<()> {
    // STEP 1: Execute lifecycle action
    StepErrorSimulator::simulate_execution_error(pool, step, 2).await?;

    // STEP 2: Immediately validate SQL functions
    SqlLifecycleAssertion::assert_step_scenario(
        pool,
        task_uuid,
        step_uuid,
        ExpectedStepState {
            state: "Error".to_string(),
            ready_for_execution: false,
            dependencies_satisfied: true,
            retry_eligible: true,
            attempts: 2,
            next_retry_at: Some(calculate_expected_retry_time()),
            backoff_request_seconds: None,
            retry_limit: 3,
        }
    ).await?;

    // STEP 3: Document the relationship
    tracing::info!(
        lifecycle_action = "simulate_execution_error",
        sql_result = "retry_eligible=true, attempts=2",
        "✅ INTEGRATION: Lifecycle → SQL alignment verified"
    );

    Ok(())
}
```

## Example Test Executions

### Running Individual Tests

```bash
# Run specific test with detailed output
RUST_LOG=info cargo test --test complex_retry_scenarios \
    test_cascading_retries_with_dependencies -- --nocapture

# Run all lifecycle tests
cargo test --all-features --test '*lifecycle*' -- --nocapture

# Run with specific environment
TASKER_ENV=test cargo test --test step_retry_lifecycle_tests -- --nocapture
```

### Running Test Suites

```bash
# Run comprehensive validation
cargo test --test sql_function_integration_validation -- --nocapture

# Run complex scenarios
cargo test --test complex_retry_scenarios -- --nocapture

# Run task finalization tests
cargo test --test task_finalization_error_scenarios -- --nocapture
```

## Tracing Output Examples

### Successful Test Execution

```log
2025-01-15T10:30:45.123Z INFO test_cascading_retries_with_dependencies:
🧪 Testing cascading retries with diamond dependency pattern

2025-01-15T10:30:45.125Z INFO test_cascading_retries_with_dependencies:
🏗️ Creating diamond workflow: Start → BranchA/BranchB → Convergence

2025-01-15T10:30:45.145Z INFO test_cascading_retries_with_dependencies:
📋 STEP 1: Executing start step successfully
step_uuid=01JGJX7K8QMRNP4W2X3Y5Z6ABC

2025-01-15T10:30:45.167Z INFO test_cascading_retries_with_dependencies:
🔄 STEP 2: Simulating BranchA failure (attempt 1)
step_uuid=01JGJX7K8RMRNP4W2X3Y5Z6DEF
error_type="ExecutionError" retryable=true

2025-01-15T10:30:45.189Z INFO test_cascading_retries_with_dependencies:
✅ STEP ASSERTION: Retry behavior matches expectations
step_uuid=01JGJX7K8RMRNP4W2X3Y5Z6DEF
attempts=1 backoff=null retry_eligible=true

2025-01-15T10:30:45.201Z INFO test_cascading_retries_with_dependencies:
🔄 STEP 3: BranchA retry attempt (attempt 2)
step_uuid=01JGJX7K8RMRNP4W2X3Y5Z6DEF

2025-01-15T10:30:45.223Z INFO test_cascading_retries_with_dependencies:
✅ STEP ASSERTION: Step completed successfully
step_uuid=01JGJX7K8RMRNP4W2X3Y5Z6DEF

2025-01-15T10:30:45.245Z INFO test_cascading_retries_with_dependencies:
❌ STEP 4: Simulating BranchB permanent failure
step_uuid=01JGJX7K8SMRNP4W2X3Y5Z6GHI
error_type="ValidationError" retryable=false

2025-01-15T10:30:45.267Z INFO test_cascading_retries_with_dependencies:
✅ STEP ASSERTION: Step failed permanently (not retryable)
step_uuid=01JGJX7K8SMRNP4W2X3Y5Z6GHI

2025-01-15T10:30:45.289Z INFO test_cascading_retries_with_dependencies:
🚫 STEP 5: Validating Convergence step is blocked
step_uuid=01JGJX7K8TMRNP4W2X3Y5Z6JKL

2025-01-15T10:30:45.301Z INFO test_cascading_retries_with_dependencies:
✅ STEP ASSERTION: Step blocked by dependencies
step_uuid=01JGJX7K8TMRNP4W2X3Y5Z6JKL

2025-01-15T10:30:45.323Z INFO test_cascading_retries_with_dependencies:
📊 TASK ASSERTION: Step distribution matches expectations
task_uuid=01JGJX7K8PMRNP4W2X3Y5Z6MNO
total=4 completed=2 failed=0 ready=0 pending=0 in_progress=0 error=2

2025-01-15T10:30:45.345Z INFO test_cascading_retries_with_dependencies:
✅ INTEGRATION: Lifecycle → SQL alignment verified
lifecycle_action="cascading_retry_with_dependency_blocking"
sql_result="blocked_by_errors=true, error_steps=2"

2025-01-15T10:30:45.356Z INFO test_cascading_retries_with_dependencies:
🧪 CASCADING RETRY TEST COMPLETE: Diamond pattern with mixed outcomes validated
```

### Error Pattern Testing Output

```log
2025-01-15T10:35:12.123Z INFO test_template_runner_all_patterns:
🎭 TEMPLATE DEMO: All error patterns with multiple templates

2025-01-15T10:35:12.145Z INFO test_template_runner_all_patterns:
📋 Testing template with all error patterns
template="linear_workflow.yaml"

2025-01-15T10:35:12.167Z INFO template_runner:
🎭 TEMPLATE TEST: Starting parameterized test execution
template_path="linear_workflow.yaml"
error_pattern=r#"AllSuccess"#

2025-01-15T10:35:12.234Z INFO template_runner:
🎭 TEMPLATE TEST: Execution complete
template_path="linear_workflow.yaml"
execution_time_ms=67
successful_steps=4 failed_steps=0 retried_steps=0
final_state="Complete"
validations_passed=12 validations_failed=0

2025-01-15T10:35:12.256Z INFO template_runner:
🎭 TEMPLATE TEST: Starting parameterized test execution
template_path="linear_workflow.yaml"
error_pattern=r#"FirstStepFails { retryable: true }"#

2025-01-15T10:35:12.334Z INFO template_runner:
📋 TEMPLATE: Simulated retryable error
step_name="initialize" attempt=1

2025-01-15T10:35:12.356Z INFO template_runner:
📋 TEMPLATE: Simulated retryable error
step_name="initialize" attempt=2

2025-01-15T10:35:12.423Z INFO template_runner:
🎭 TEMPLATE TEST: Execution complete
template_path="linear_workflow.yaml"
execution_time_ms=167
successful_steps=4 failed_steps=0 retried_steps=1
final_state="Complete"
validations_passed=15 validations_failed=0

2025-01-15T10:35:12.445Z INFO test_template_runner_all_patterns:
📊 Template pattern result
template="linear_workflow.yaml" pattern_index=0
pattern="AllSuccess" execution_time_ms=67
final_state="Complete" total_validations=12 success_rate="100.0%"

2025-01-15T10:35:12.467Z INFO test_template_runner_all_patterns:
📊 Template pattern result
template="linear_workflow.yaml" pattern_index=1
pattern=r#"FirstStepFails { retryable: true }"# execution_time_ms=167
final_state="Complete" total_validations=15 success_rate="100.0%"
```

### SQL Function Validation Output

```log
2025-01-15T10:40:30.123Z INFO test_comprehensive_sql_function_integration:
🔍 SQL INTEGRATION: Starting comprehensive validation across all scenarios

2025-01-15T10:40:30.145Z INFO test_comprehensive_sql_function_integration:
📋 SCENARIO 1: Basic lifecycle progression validation

2025-01-15T10:40:30.167Z INFO validate_initial_state:
✅ Initial state validation passed

2025-01-15T10:40:30.189Z INFO validate_step_completion:
✅ Step completion validation passed
step_uuid=01JGJX7M8QMRNP4W2X3Y5Z6PQR

2025-01-15T10:40:30.201Z INFO test_comprehensive_sql_function_integration:
✅ SCENARIO 1: Basic lifecycle validation complete
scenario="basic_lifecycle" validations=2

2025-01-15T10:40:30.223Z INFO test_comprehensive_sql_function_integration:
🔄 SCENARIO 2: Error handling and retry behavior validation

2025-01-15T10:40:30.245Z INFO validate_retry_behavior:
✅ Retry behavior validation passed
step_uuid=01JGJX7M8RMRNP4W2X3Y5Z6STU
attempts=1 backoff=Some(5) retry_eligible=true

2025-01-15T10:40:30.267Z INFO test_comprehensive_sql_function_integration:
✅ SCENARIO 2: Error and retry validation complete
scenario="error_retry" validations=1

2025-01-15T10:40:30.289Z INFO test_comprehensive_sql_function_integration:
🎯 FINAL VALIDATION: Comprehensive results summary
total_validations=25 successful_validations=25
success_rate="100.00%" scenarios_tested=6

2025-01-15T10:40:30.301Z INFO test_comprehensive_sql_function_integration:
🔍 SQL INTEGRATION VALIDATION COMPLETE: All scenarios validated successfully
```

## Best Practices

### 1. Always Use Integrated Validation Pattern

```rust
// ✅ GOOD: Integrated lifecycle + SQL validation
async fn test_step_retry_behavior(pool: PgPool) -> Result<()> {
    // Exercise lifecycle
    StepErrorSimulator::simulate_execution_error(pool, step, 1).await?;

    // Immediately validate SQL functions
    pool.assert_step_retry_behavior(step_uuid, 1, None, true).await?;

    // Document relationship
    tracing::info!("✅ INTEGRATION: Retry behavior alignment verified");
    Ok(())
}

// ❌ BAD: Testing SQL functions in isolation
async fn test_sql_only(pool: PgPool) -> Result<()> {
    // Directly manipulating database state
    sqlx::query!("UPDATE steps SET attempts = 3").execute(pool).await?;

    // This doesn't prove the integration works
    let status = sqlx::query!("SELECT * FROM get_step_readiness_status($1)", uuid)
        .fetch_one(pool).await?;
    Ok(())
}
```

### 2. Use Structured Tracing

```rust
// ✅ GOOD: Structured tracing with context
tracing::info!(
    step_uuid = %step.workflow_step_uuid,
    attempts = expected_attempts,
    backoff = ?expected_backoff,
    retry_eligible = expected_retry_eligible,
    "✅ STEP ASSERTION: Retry behavior matches expectations"
);

// ❌ BAD: Unstructured logging
println!("Step retry test passed");
```

### 3. Test Multiple Scenarios

```rust
// ✅ GOOD: Comprehensive scenario coverage
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_complete_retry_scenarios(pool: PgPool) -> Result<()> {
    // Test retryable error
    test_retryable_error_scenario(&pool).await?;

    // Test non-retryable error
    test_non_retryable_error_scenario(&pool).await?;

    // Test retry exhaustion
    test_retry_exhaustion_scenario(&pool).await?;

    // Test custom backoff
    test_custom_backoff_scenario(&pool).await?;

    Ok(())
}
```

### 4. Validate State Transitions

```rust
// ✅ GOOD: Validate complete state transition sequence
pool.assert_step_state_sequence(
    step_uuid,
    vec![
        "Pending".to_string(),
        "InProgress".to_string(),
        "Error".to_string(),
        "WaitingForRetry".to_string(),
        "Ready".to_string(),
        "Complete".to_string()
    ]
).await?;
```

### 5. Use Assertion Traits for Readability

```rust
// ✅ GOOD: Clear, readable assertions
pool.assert_task_complete(task_uuid).await?;
pool.assert_step_failed_permanently(step_uuid).await?;

// ❌ BAD: Manual SQL queries everywhere
let task_status = sqlx::query!("SELECT ...").fetch_one(pool).await?;
assert_eq!(task_status.some_field, Some("Complete"));
```

## Troubleshooting

### Common Issues

#### 1. Assertion Failures

```log
Error: Task 01JGJX... assertion failed: expected Complete, found Processing

// Solution: Ensure lifecycle actions complete before asserting
tokio::time::sleep(Duration::from_millis(100)).await;
pool.assert_task_complete(task_uuid).await?;
```

#### 2. SQL Function Mismatches

```log
Error: Step 01JGJX... retry assertion failed: attempts expected 2, got Some(1)

// Solution: Verify error simulator is configured correctly
StepErrorSimulator::simulate_execution_error(pool, step, 2).await?; // 2 attempts
```

#### 3. State Machine Violations

```log
Error: Cannot transition from Complete to InProgress

// Solution: Use proper orchestration framework, not direct DB manipulation
let result = orchestrator.execute_step(step, true, 1000).await?;
// Don't: sqlx::query!("UPDATE steps SET state = 'InProgress'").execute(pool).await?;
```

#### 4. Template Loading Issues

```log
Error: Template 'nonexistent.yaml' not found

// Solution: Ensure template exists in correct directory
// templates should be in tests/fixtures/task_templates/rust/
```

### Debugging Techniques

#### 1. Enable Detailed Tracing

```bash
RUST_LOG=debug cargo test test_name -- --nocapture
```

#### 2. Inspect SQL Function Results Directly

```rust
let step_status = sqlx::query!(
    "SELECT * FROM get_step_readiness_status($1)",
    step_uuid
)
.fetch_one(&pool)
.await?;

tracing::debug!("Step status: {:?}", step_status);
```

#### 3. Validate Test Prerequisites

```rust
// Ensure test setup is correct
assert_eq!(steps.len(), 4, "Test requires 4 steps");
assert_eq!(task.namespace, "expected_namespace");
```

#### 4. Use Incremental Validation

```rust
// Validate after each step
orchestrator.execute_step(&step1, true, 1000).await?;
pool.assert_step_complete(step1.workflow_step_uuid).await?;

orchestrator.execute_step(&step2, false, 1000).await?;
pool.assert_step_retry_behavior(step2.workflow_step_uuid, 1, None, true).await?;
```

## Migration from Old Tests

### Before (Direct Database Manipulation)

```rust
// ❌ OLD: Bypassing orchestration framework
async fn test_task_finalization_old(pool: PgPool) -> Result<()> {
    // Direct database manipulation
    sqlx::query!("UPDATE tasks SET state = 'Error'").execute(&pool).await?;
    sqlx::query!("UPDATE steps SET state = 'Error'").execute(&pool).await?;

    // Test SQL functions in isolation
    let context = get_task_execution_context(&pool, task_uuid).await?;
    assert_eq!(context.execution_status, ExecutionStatus::Error);

    Ok(())
}
```

### After (Integrated Framework)

```rust
// ✅ NEW: Using integrated framework
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_task_finalization_new(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 Testing task finalization with integrated approach");

    // Use orchestration framework
    let orchestrator = TestOrchestrator::new(pool.clone());
    let task = orchestrator.create_simple_task("test", "finalization").await?;
    let step = get_first_step(&pool, task.task_uuid).await?;

    // Create error state through framework
    StepErrorSimulator::simulate_validation_error(
        &pool,
        &step,
        "finalization_test_error"
    ).await?;

    // Immediately validate SQL functions
    pool.assert_step_failed_permanently(step.workflow_step_uuid).await?;
    pool.assert_task_error(task.task_uuid, 1).await?;

    tracing::info!("✅ INTEGRATION: Finalization behavior verified");
    Ok(())
}
```

This comprehensive guide demonstrates the power and flexibility of the lifecycle testing framework, providing developers with the tools needed to validate complex workflow behavior while maintaining confidence in the system's correctness.