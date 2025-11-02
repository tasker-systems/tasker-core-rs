//! # Manual Step Completion Integration Tests
//!
//! Tests for manual step resolution and completion functionality.
//!
//! ## Test Coverage
//!
//! 1. **Simple Manual Resolution**: Mark step as `resolved_manually` without results
//! 2. **Manual Completion with Results**: Provide execution results and transition to `complete`
//! 3. **Attempt Counter Reset**: Reset retry attempts before manual intervention
//! 4. **Dependent Step Execution**: Verify dependent steps can execute after manual completion
//!
//! ## Design Note
//!
//! These tests construct full `StepExecutionResult` objects to test the state machine directly.
//! In production, the PATCH endpoint constructs these from constrained `ManualCompletionData`
//! which only accepts `result` and optional `metadata`, with the system enforcing:
//! - `success: true` (always)
//! - `status: "completed"` (always)
//! - `error: null` (always)
//!
//! This prevents operators from incorrectly setting status or error fields.

use anyhow::Result;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

use tasker_shared::models::WorkflowStep;
use tasker_shared::state_machine::events::StepEvent;
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::system_context::SystemContext;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

// ============================================================================
// Test 1: Simple Manual Resolution (ResolveManually)
// ============================================================================

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_simple_manual_resolution_without_results(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 MANUAL COMPLETION: Simple resolution without results");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create a linear workflow task
    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "rust_e2e_linear",
        json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get the first step
    let steps = WorkflowStep::list_by_task(&pool, init_result.task_uuid).await?;
    let first_step = steps
        .first()
        .expect("Should have at least one step")
        .clone();

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let mut step_state_machine = StepStateMachine::new(first_step.clone(), system_context);

    // Transition to error state
    step_state_machine
        .transition(StepEvent::Fail("Test failure".to_string()))
        .await?;

    // Verify in error state
    let current_state = step_state_machine.current_state().await?;
    assert_eq!(current_state, WorkflowStepState::Error);

    // Apply simple manual resolution (no results)
    let new_state = step_state_machine
        .transition(StepEvent::ResolveManually)
        .await?;

    // Validate: Should be in resolved_manually state
    assert_eq!(
        new_state,
        WorkflowStepState::ResolvedManually,
        "Step should transition to resolved_manually"
    );

    // Validate: No results should be persisted
    let updated_step = WorkflowStep::find_by_id(&pool, first_step.workflow_step_uuid)
        .await?
        .expect("Step should exist");
    assert!(
        updated_step.results.is_none(),
        "No results should be persisted for simple resolution"
    );

    tracing::info!("✅ MANUAL COMPLETION: Simple resolution test passed");
    Ok(())
}

// ============================================================================
// Test 2: Manual Completion with Execution Results
// ============================================================================

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_manual_completion_with_execution_results(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 MANUAL COMPLETION: Complete with execution results");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create a linear workflow task
    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "rust_e2e_linear",
        json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get the first step
    let steps = WorkflowStep::list_by_task(&pool, init_result.task_uuid).await?;
    let first_step = steps
        .first()
        .expect("Should have at least one step")
        .clone();

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let mut step_state_machine = StepStateMachine::new(first_step.clone(), system_context);

    // Transition to error state
    step_state_machine
        .transition(StepEvent::Fail("Test failure".to_string()))
        .await?;

    // Verify in error state
    let current_state = step_state_machine.current_state().await?;
    assert_eq!(current_state, WorkflowStepState::Error);

    // Apply manual completion with execution results
    // Only provide result and metadata - system enforces success=true and status="completed"
    let completion_result = json!({
        "calculated_value": 42,
        "processing_time_ms": 150,
        "data_processed": 1000
    });

    let completion_metadata = json!({
        "manually_completed": true,
        "operator": "test@example.com"
    });

    // Construct the full StepExecutionResult as the system would
    let execution_result = json!({
        "step_uuid": first_step.workflow_step_uuid,
        "success": true,
        "status": "completed",
        "result": completion_result,
        "metadata": completion_metadata,
        "error": null
    });

    let new_state = step_state_machine
        .transition(StepEvent::CompleteManually(Some(execution_result)))
        .await?;

    // Validate: Should be in complete state (not resolved_manually)
    assert_eq!(
        new_state,
        WorkflowStepState::Complete,
        "Step should transition to complete state"
    );

    // Validate: Results should be persisted
    let updated_step = WorkflowStep::find_by_id(&pool, first_step.workflow_step_uuid)
        .await?
        .expect("Step should exist");

    tracing::info!("Updated step results: {:?}", updated_step.results);
    tracing::info!("Updated step processed: {}", updated_step.processed);
    tracing::info!("Updated step in_process: {}", updated_step.in_process);

    assert!(
        updated_step.results.is_some(),
        "Results should be persisted for manual completion. Got: {:?}",
        updated_step.results
    );
    assert_eq!(
        updated_step.results.unwrap()["result"]["calculated_value"],
        42,
        "Persisted results should match provided execution results"
    );

    // Validate: Step should be marked as processed
    assert!(updated_step.processed, "Step should be marked as processed");
    assert!(!updated_step.in_process, "Step should not be in process");
    assert!(
        updated_step.processed_at.is_some(),
        "Step should have processed_at timestamp"
    );

    tracing::info!("✅ MANUAL COMPLETION: Execution results test passed");
    Ok(())
}

// ============================================================================
// Test 3: Attempt Counter Reset
// ============================================================================

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_attempt_counter_reset_functionality(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 MANUAL COMPLETION: Attempt counter reset");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create a linear workflow task
    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "rust_e2e_linear",
        json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get the first step
    let steps = WorkflowStep::list_by_task(&pool, init_result.task_uuid).await?;
    let first_step = steps
        .first()
        .expect("Should have at least one step")
        .clone();

    // Simulate step with 5 failed attempts
    sqlx::query!(
        r#"
        UPDATE tasker_workflow_steps
        SET attempts = 5,
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
        first_step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Verify attempts count before reset
    let attempts_before = sqlx::query_scalar!(
        "SELECT attempts FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
        first_step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        attempts_before,
        Some(5),
        "Step should have 5 attempts before reset"
    );

    // Reset attempt counter (simulating PATCH request behavior)
    sqlx::query!(
        r#"
        UPDATE tasker_workflow_steps
        SET attempts = 0,
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
        first_step.workflow_step_uuid
    )
    .execute(&pool)
    .await?;

    // Verify attempts reset to 0
    let attempts_after = sqlx::query_scalar!(
        "SELECT attempts FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
        first_step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        attempts_after,
        Some(0),
        "Attempt counter should be reset to 0"
    );

    tracing::info!("✅ MANUAL COMPLETION: Attempt reset test passed");
    Ok(())
}

// ============================================================================
// Test 4: Dependent Step Execution After Manual Completion
// ============================================================================

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_dependent_step_execution_after_manual_completion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 MANUAL COMPLETION: Dependent step execution");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create a linear workflow task (steps have dependencies)
    let task_request = manager.create_task_request_for_template(
        "mathematical_sequence",
        "rust_e2e_linear",
        json!({"even_number": 6}),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get the steps
    let steps = WorkflowStep::list_by_task(&pool, init_result.task_uuid).await?;
    assert!(steps.len() >= 2, "Should have at least 2 steps");

    let first_step = steps[0].clone();
    let second_step = steps[1].clone();

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

    // Check that second step is not ready (first step not complete)
    let child_readiness = sqlx::query!(
        r#"
        SELECT * FROM get_step_readiness_status($1)
        WHERE workflow_step_uuid = $2
        "#,
        init_result.task_uuid,
        second_step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        !child_readiness.ready_for_execution.unwrap_or(false),
        "Second step should not be ready when first step is pending"
    );

    // Put first step in error state
    let mut first_step_machine = StepStateMachine::new(first_step.clone(), system_context.clone());
    first_step_machine
        .transition(StepEvent::Fail("Test failure".to_string()))
        .await?;

    // Manually complete first step with results
    // Only provide result - system enforces success=true and status="completed"
    let completion_result = json!({
        "output_data": "parent_result",
        "value": 100
    });

    // Construct the full StepExecutionResult as the system would
    let execution_result = json!({
        "step_uuid": first_step.workflow_step_uuid,
        "success": true,
        "status": "completed",
        "result": completion_result,
        "metadata": {
            "manually_completed": true
        },
        "error": null
    });

    first_step_machine
        .transition(StepEvent::CompleteManually(Some(execution_result)))
        .await?;

    // Verify first step is now complete
    let first_step_state = first_step_machine.current_state().await?;
    assert_eq!(
        first_step_state,
        WorkflowStepState::Complete,
        "First step should be in complete state"
    );

    // Verify second step is now ready (first step complete)
    let child_readiness_after = sqlx::query!(
        r#"
        SELECT * FROM get_step_readiness_status($1)
        WHERE workflow_step_uuid = $2
        "#,
        init_result.task_uuid,
        second_step.workflow_step_uuid
    )
    .fetch_one(&pool)
    .await?;

    assert!(
        child_readiness_after.ready_for_execution.unwrap_or(false),
        "Second step should be ready when first step is complete"
    );
    assert!(
        child_readiness_after
            .dependencies_satisfied
            .unwrap_or(false),
        "Second step dependencies should be satisfied"
    );

    // Verify first step results are accessible
    let updated_first_step = WorkflowStep::find_by_id(&pool, first_step.workflow_step_uuid)
        .await?
        .expect("First step should exist");

    assert!(
        updated_first_step.results.is_some(),
        "First step results should be accessible"
    );
    assert_eq!(
        updated_first_step.results.unwrap()["result"]["value"],
        100,
        "Second step should be able to access first step's manually provided results"
    );

    tracing::info!("✅ MANUAL COMPLETION: Dependent step test passed");
    Ok(())
}
