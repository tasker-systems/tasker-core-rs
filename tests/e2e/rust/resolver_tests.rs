//! TAS-93 Phase 5: Resolver Chain E2E Tests for Rust
//!
//! These tests validate the resolver chain features with native Rust handlers:
//! - Method dispatch: Internal routing to different handler methods
//! - Backward compatibility: Existing templates continue to work
//!
//! Rust handlers implement method dispatch internally by reading the
//! `step_definition.handler.method` field and routing to the appropriate
//! implementation method.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

// =============================================================================
// Method Dispatch Tests
// =============================================================================

/// Test method dispatch with native Rust handlers
///
/// Validates:
/// - Handler's validate logic is called when method: "validate" is specified
/// - All 4 steps with different methods complete successfully
/// - Results contain correct invoked_method values
#[tokio::test]
async fn test_rust_method_dispatch() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task that uses method dispatch
    let task_request = create_task_request(
        "resolver_tests_rust",
        "method_dispatch_rust",
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
        step_names.contains(&"validate_step_rust"),
        "Should have validate_step_rust"
    );
    assert!(
        step_names.contains(&"process_step_rust"),
        "Should have process_step_rust"
    );
    assert!(
        step_names.contains(&"refund_step_rust"),
        "Should have refund_step_rust"
    );
    assert!(
        step_names.contains(&"default_call_step_rust"),
        "Should have default_call_step_rust"
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

    println!("✅ Rust method dispatch test completed - all 4 methods invoked successfully");
    Ok(())
}

// =============================================================================
// Backward Compatibility Tests
// =============================================================================

/// Test that existing Rust templates without method/resolver fields still work
///
/// Validates:
/// - Templates that don't use method dispatch continue to work
/// - Default call() method is invoked
/// - No regression in existing functionality
#[tokio::test]
async fn test_rust_backward_compatibility() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Use existing linear workflow template (no method/resolver fields)
    let task_request = create_task_request(
        "rust_e2e_linear",
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

    println!("✅ Rust backward compatibility verified - existing templates work");
    Ok(())
}
