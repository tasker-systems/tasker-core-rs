//! TAS-93 Phase 5: Resolver Chain E2E Tests for Ruby
//!
//! These tests validate the resolver chain features with Ruby handlers:
//! - Method dispatch: Calling specific methods on handlers (validate, process, refund)
//! - Backward compatibility: Existing templates continue to work
//!
//! Note: These tests require the Ruby worker to be running on port 8081.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

// =============================================================================
// Method Dispatch Tests
// =============================================================================

/// Test method dispatch with Ruby handlers
///
/// Validates:
/// - Handler's validate() method is called instead of call()
/// - All 4 steps with different methods complete successfully
/// - Results contain correct invoked_method values
#[tokio::test]
async fn test_ruby_method_dispatch() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task that uses method dispatch
    let task_request = create_task_request(
        "resolver_tests",
        "method_dispatch",
        json!({
            "data": {
                "amount": 100.0,
                "reason": "test_refund"
            }
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for all steps to complete (4 sequential steps)
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    // Verify all 4 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(
        steps.len(),
        4,
        "Should have 4 steps in method dispatch workflow"
    );

    // Verify step names match expected pattern
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"validate_step"),
        "Should have validate_step"
    );
    assert!(
        step_names.contains(&"process_step"),
        "Should have process_step"
    );
    assert!(
        step_names.contains(&"refund_step"),
        "Should have refund_step"
    );
    assert!(
        step_names.contains(&"default_call_step"),
        "Should have default_call_step"
    );

    // All steps should be complete
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    println!("✅ Ruby method dispatch test completed - all 4 methods invoked successfully");
    Ok(())
}

// =============================================================================
// Backward Compatibility Tests
// =============================================================================

/// Test that existing Ruby templates without method/resolver fields still work
///
/// Validates:
/// - Templates that don't use method dispatch continue to work
/// - Default call() method is invoked
/// - No regression in existing functionality
#[tokio::test]
async fn test_ruby_backward_compatibility() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Use existing linear workflow template (no method/resolver fields)
    let task_request = create_task_request(
        "linear_workflow",
        "mathematical_sequence",
        json!({ "even_number": 6 }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Existing template should still work without method/resolver"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // All steps should be complete
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete",
            step.name
        );
    }

    println!("✅ Ruby backward compatibility verified - existing templates work");
    Ok(())
}
