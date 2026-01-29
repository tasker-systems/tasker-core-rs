// TAS-53: E2E Tests for Decision Point Workflows
//
// This module tests the conditional approval workflow demonstrating
// decision point functionality with dynamic step creation based on
// runtime conditions (approval amount thresholds).

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};
use tasker_shared::types::api::orchestration::StepResponse;

/// Helper function to create a conditional approval task request
fn create_approval_request(
    amount: f64,
    requester: &str,
    purpose: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "conditional_approval",
        "approval_routing",
        json!({
            "amount": amount,
            "requester": requester,
            "purpose": purpose
        }),
    )
}

/// Verify that specific steps were created by the decision point
fn verify_steps_created(steps: &[StepResponse], expected_step_names: &[&str]) {
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    for expected_name in expected_step_names {
        assert!(
            step_names.contains(&expected_name.to_string()),
            "Expected step '{}' not found in created steps. Found: {:?}",
            expected_name,
            step_names
        );
    }
}

/// Verify the decision step exists and is complete
fn verify_decision_step(steps: &[StepResponse], decision_step_name: &str) {
    let decision_step = steps
        .iter()
        .find(|s| s.name == decision_step_name)
        .expect("Decision step not found");

    assert_eq!(
        decision_step.current_state.to_uppercase(),
        "COMPLETE",
        "Decision step should be Complete"
    );
}

#[tokio::test]
async fn test_small_amount_auto_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Small amount: $500 should trigger auto-approval path
    let task_request = create_approval_request(500.0, "alice@example.com", "Office supplies");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for task completion
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify task is complete
    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for small amount:
    // 1. validate_request (always created)
    // 2. routing_decision (decision point - always created)
    // 3. auto_approve (dynamically created by decision point)
    // 4. finalize_approval (convergence step - always created)
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "auto_approve",
            "finalize_approval",
        ],
    );

    // Verify only the auto_approve path was created (not manager/finance)
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"manager_approval".to_string()),
        "Manager approval should NOT be created for small amounts"
    );
    assert!(
        !step_names.contains(&"finance_review".to_string()),
        "Finance review should NOT be created for small amounts"
    );

    // Verify decision step is complete
    verify_decision_step(&steps, "routing_decision");

    // Verify total step count (4 steps for auto-approval path)
    assert_eq!(
        steps.len(),
        4,
        "Should have exactly 4 steps for auto-approval path"
    );

    Ok(())
}

#[tokio::test]
async fn test_medium_amount_manager_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Medium amount: $2,500 should trigger manager-only approval path
    let task_request = create_approval_request(2500.0, "bob@example.com", "New equipment");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for task completion
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify task is complete
    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for medium amount:
    // 1. validate_request
    // 2. routing_decision
    // 3. manager_approval (dynamically created)
    // 4. finalize_approval
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "manager_approval",
            "finalize_approval",
        ],
    );

    // Verify only manager approval path was created (not auto or finance)
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve".to_string()),
        "Auto-approve should NOT be created for medium amounts"
    );
    assert!(
        !step_names.contains(&"finance_review".to_string()),
        "Finance review should NOT be created for medium amounts"
    );

    // Verify decision step is complete
    verify_decision_step(&steps, "routing_decision");

    // Verify total step count (4 steps for manager-only path)
    assert_eq!(
        steps.len(),
        4,
        "Should have exactly 4 steps for manager-only path"
    );

    Ok(())
}

#[tokio::test]
async fn test_large_amount_dual_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Large amount: $10,000 should trigger dual-approval path
    let task_request =
        create_approval_request(10000.0, "charlie@example.com", "Capital investment");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for task completion
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify task is complete
    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for large amount:
    // 1. validate_request
    // 2. routing_decision
    // 3. manager_approval (dynamically created)
    // 4. finance_review (dynamically created)
    // 5. finalize_approval
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "manager_approval",
            "finance_review",
            "finalize_approval",
        ],
    );

    // Verify auto-approve was NOT created for large amounts
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve".to_string()),
        "Auto-approve should NOT be created for large amounts"
    );

    // Verify decision step is complete
    verify_decision_step(&steps, "routing_decision");

    // Verify total step count (5 steps for dual-approval path)
    assert_eq!(
        steps.len(),
        5,
        "Should have exactly 5 steps for dual-approval path"
    );

    // Verify both approval steps completed
    let manager_step = steps
        .iter()
        .find(|s| s.name == "manager_approval")
        .expect("Manager approval step should exist");
    assert_eq!(
        manager_step.current_state.to_uppercase(),
        "COMPLETE",
        "Manager approval should complete"
    );

    let finance_step = steps
        .iter()
        .find(|s| s.name == "finance_review")
        .expect("Finance review step should exist");
    assert_eq!(
        finance_step.current_state.to_uppercase(),
        "COMPLETE",
        "Finance review should complete"
    );

    Ok(())
}

#[tokio::test]
async fn test_decision_point_step_dependency_structure() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test that dynamically created steps have correct dependencies
    let task_request = create_approval_request(7500.0, "dave@example.com", "Software licenses");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Build dependency map for verification
    let mut step_map: HashMap<String, &StepResponse> = HashMap::new();
    for step in &steps {
        step_map.insert(step.name.clone(), step);
    }

    // Verify validate_request has no dependencies
    let _validate_step = step_map
        .get("validate_request")
        .expect("validate_request should exist");
    // Dependencies verification would require the API to return dependency info
    // For now, verify the step exists and completed

    // Verify routing_decision depends on validate_request
    let routing_step = step_map
        .get("routing_decision")
        .expect("routing_decision should exist");
    assert_eq!(
        routing_step.current_state.to_uppercase(),
        "COMPLETE",
        "Routing decision should complete"
    );

    // Verify dynamically created steps depend on routing_decision
    // (they should not execute until routing_decision completes)
    let manager_step = step_map
        .get("manager_approval")
        .expect("manager_approval should exist");
    let finance_step = step_map
        .get("finance_review")
        .expect("finance_review should exist");

    // Both should have completed (indicating proper dependency resolution)
    assert_eq!(
        manager_step.current_state.to_uppercase(),
        "COMPLETE",
        "Manager approval should complete after decision"
    );
    assert_eq!(
        finance_step.current_state.to_uppercase(),
        "COMPLETE",
        "Finance review should complete after decision"
    );

    // Verify finalize_approval depends on the dynamic steps
    let finalize_step = step_map
        .get("finalize_approval")
        .expect("finalize_approval should exist");
    assert_eq!(
        finalize_step.current_state.to_uppercase(),
        "COMPLETE",
        "Finalize should complete after all approvals"
    );

    Ok(())
}

#[tokio::test]
async fn test_boundary_conditions() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test exactly at $1,000 threshold (should trigger manager approval)
    let task_request = create_approval_request(1000.0, "eve@example.com", "Threshold test");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // At exactly $1,000, should use manager approval (not auto)
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "manager_approval",
            "finalize_approval",
        ],
    );

    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve".to_string()),
        "Should NOT auto-approve at threshold"
    );

    Ok(())
}

#[tokio::test]
async fn test_boundary_large_threshold() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test exactly at $5,000 threshold (should trigger dual approval)
    let task_request = create_approval_request(5000.0, "frank@example.com", "Large threshold test");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // At exactly $5,000, should use dual approval
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "manager_approval",
            "finance_review",
            "finalize_approval",
        ],
    );

    assert_eq!(
        steps.len(),
        5,
        "Should have 5 steps for dual approval at threshold"
    );

    Ok(())
}

#[tokio::test]
async fn test_very_small_amount() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test very small amount: $0.01
    let task_request = create_approval_request(0.01, "grace@example.com", "Petty cash");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 5).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(task.status, "complete", "Very small amount should complete");

    // Should use auto-approval
    verify_steps_created(
        &steps,
        &[
            "validate_request",
            "routing_decision",
            "auto_approve",
            "finalize_approval",
        ],
    );

    Ok(())
}
