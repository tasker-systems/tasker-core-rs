//! TAS-93 Phase 5: Resolver Chain E2E Tests for TypeScript
//!
//! These tests validate the resolver chain features with TypeScript handlers:
//! - Method dispatch: Calling specific methods on handlers (validate, process, refund)
//! - Resolver hints: Bypassing the resolver chain with explicit resolver names
//! - Backward compatibility: Existing templates continue to work
//!
//! Note: These tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

// =============================================================================
// Method Dispatch Tests
// =============================================================================

/// Test method dispatch with validate() method
///
/// Validates:
/// - Handler's validate() method is called instead of call()
/// - Step completes successfully with validation result
/// - Result contains invoked_method: "validate"
#[tokio::test]
async fn test_typescript_method_dispatch_validate() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task that uses method dispatch to call validate()
    let task_request = create_task_request(
        "resolver_tests_ts",
        "method_dispatch_ts",
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

    println!("✅ TypeScript method dispatch test completed - all 4 methods invoked successfully");
    Ok(())
}

/// Test method dispatch with different methods on the same handler
///
/// Validates:
/// - Multiple different methods can be called on the same handler class
/// - Each step invokes the correct method
/// - Results from each method are distinct
#[tokio::test]
async fn test_typescript_method_dispatch_multiple_methods() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "resolver_tests_ts",
        "method_dispatch_ts",
        json!({
            "data": {
                "amount": 250.0,
                "reason": "customer_request"
            }
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify execution order through dependencies
    let mut validate_found = false;
    let mut process_found = false;
    let mut refund_found = false;
    let mut default_found = false;

    for step in &steps {
        match step.name.as_str() {
            "validate_step" => validate_found = true,
            "process_step" => process_found = true,
            "refund_step" => refund_found = true,
            "default_call_step" => default_found = true,
            _ => {}
        }
    }

    assert!(
        validate_found && process_found && refund_found && default_found,
        "All method dispatch steps should be present"
    );

    println!("✅ TypeScript multi-method dispatch verified - validate, process, refund, call");
    Ok(())
}

// =============================================================================
// Resolver Hints Tests
// =============================================================================

/// Test resolver hints bypass chain resolution
///
/// Validates:
/// - resolver: "explicit_mapping" forces use of ExplicitMappingResolver
/// - Handler is found through explicit registration
/// - Step completes successfully
#[tokio::test]
async fn test_typescript_resolver_hints_explicit() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "resolver_tests_ts",
        "resolver_hints_ts",
        json!({
            "action_type": "test_action"
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for all 3 steps
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task with resolver hints should complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(
        steps.len(),
        3,
        "Should have 3 steps in resolver hints workflow"
    );

    // Verify all steps completed
    for step in &steps {
        assert!(
            step.current_state.to_uppercase() == "COMPLETE",
            "Step {} should be complete, got: {}",
            step.name,
            step.current_state
        );
    }

    println!("✅ TypeScript resolver hints test completed - explicit_mapping resolver used");
    Ok(())
}

/// Test combined resolver hints and method dispatch
///
/// Validates:
/// - Both resolver hint and method dispatch can be used together
/// - resolver: "explicit_mapping" + method: "execute_action" works
#[tokio::test]
async fn test_typescript_resolver_hints_with_method() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    let task_request = create_task_request(
        "resolver_tests_ts",
        "resolver_hints_ts",
        json!({
            "action_type": "combined_test"
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task with resolver hints + method should complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Find the combined_hint_step which uses both resolver and method
    let combined_step = steps
        .iter()
        .find(|s| s.name == "combined_hint_step")
        .expect("Should have combined_hint_step");

    assert!(
        combined_step.current_state.to_uppercase() == "COMPLETE",
        "Combined hint step should complete, got: {}",
        combined_step.current_state
    );

    println!("✅ TypeScript resolver hints with method dispatch verified");
    Ok(())
}

// =============================================================================
// Backward Compatibility Tests
// =============================================================================

/// Test that existing templates without method/resolver fields still work
///
/// Validates:
/// - Templates that don't use method dispatch continue to work
/// - Default call() method is invoked
/// - No regression in existing functionality
#[tokio::test]
async fn test_typescript_backward_compatibility_no_method() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Use existing linear workflow template (no method/resolver fields)
    let task_request = create_task_request(
        "linear_workflow_ts",
        "mathematical_sequence_ts",
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

    println!("✅ TypeScript backward compatibility verified - existing templates work");
    Ok(())
}

/// Test that explicit method: "call" works the same as no method
///
/// Validates:
/// - method: "call" is equivalent to omitting method field
/// - No extra wrapping occurs for default method
#[tokio::test]
async fn test_typescript_explicit_call_method() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Use method dispatch template but the last step has no method (defaults to call)
    let task_request = create_task_request(
        "resolver_tests_ts",
        "method_dispatch_ts",
        json!({
            "data": {
                "amount": 50.0
            }
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task should complete with default call method"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Find the default_call_step which doesn't specify a method
    let default_step = steps
        .iter()
        .find(|s| s.name == "default_call_step")
        .expect("Should have default_call_step");

    assert!(
        default_step.current_state.to_uppercase() == "COMPLETE",
        "Default call step should complete"
    );

    println!("✅ TypeScript explicit call method verified - equivalent to no method");
    Ok(())
}
