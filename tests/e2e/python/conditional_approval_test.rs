// Rust guideline compliant 2025-12-16
//! Python Conditional Approval E2E Tests
//!
//! Tests the conditional approval workflow demonstrating decision point
//! functionality with dynamic step creation based on runtime conditions
//! (approval amount thresholds) using Python handlers.
//!
//! Workflow Pattern:
//! 1. validate_request_py: Validate approval request data
//! 2. routing_decision_py: DECISION POINT - route based on amount
//!    - < $1,000: auto_approve_py
//!    - $1,000-$4,999: manager_approval_py
//!    - >= $5,000: manager_approval_py + finance_review_py
//! 3. finalize_approval_py: Convergence step processing all approvals
//!
//! Note: These tests require the Python worker to be running on port 8083.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};
use tasker_shared::types::api::orchestration::StepResponse;

/// Create a conditional approval task request for Python handlers.
fn create_approval_request(
    amount: f64,
    requester: &str,
    purpose: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "conditional_approval_py",
        "approval_routing_py",
        json!({
            "amount": amount,
            "requester": requester,
            "purpose": purpose
        }),
    )
}

/// Verify that specific steps were created by the decision point.
fn verify_steps_created(steps: &[StepResponse], expected_step_names: &[&str]) {
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    for expected_name in expected_step_names {
        assert!(
            step_names.contains(&(*expected_name).to_string()),
            "Expected step '{}' not found in created steps. Found: {:?}",
            expected_name,
            step_names
        );
    }
}

/// Verify the decision step exists and is complete.
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

/// Test small amount triggers auto-approval path with Python handlers.
///
/// Validates:
/// - Small amounts (< $1,000) route to auto_approve_py
/// - manager_approval_py and finance_review_py are NOT created
/// - Task completes successfully with 4 steps
#[tokio::test]
async fn test_python_small_amount_auto_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Small amount: $500 should trigger auto-approval path
    let task_request = create_approval_request(500.0, "alice@example.com", "Office supplies");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for task completion
    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Verify task is complete
    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for small amount
    verify_steps_created(
        &steps,
        &[
            "validate_request_py",
            "routing_decision_py",
            "auto_approve_py",
            "finalize_approval_py",
        ],
    );

    // Verify only the auto_approve path was created
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"manager_approval_py".to_string()),
        "Manager approval should NOT be created for small amounts"
    );
    assert!(
        !step_names.contains(&"finance_review_py".to_string()),
        "Finance review should NOT be created for small amounts"
    );

    // Verify decision step is complete
    verify_decision_step(&steps, "routing_decision_py");

    // Verify total step count (4 steps for auto-approval path)
    assert_eq!(
        steps.len(),
        4,
        "Should have exactly 4 steps for auto-approval path"
    );

    println!("✅ Python small amount auto-approval test passed");
    Ok(())
}

/// Test medium amount triggers manager-only approval path.
///
/// Validates:
/// - Medium amounts ($1,000-$4,999) route to manager_approval_py only
/// - auto_approve_py and finance_review_py are NOT created
/// - Task completes successfully with 4 steps
#[tokio::test]
async fn test_python_medium_amount_manager_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Medium amount: $2,500 should trigger manager-only approval path
    let task_request = create_approval_request(2500.0, "bob@example.com", "New equipment");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for medium amount
    verify_steps_created(
        &steps,
        &[
            "validate_request_py",
            "routing_decision_py",
            "manager_approval_py",
            "finalize_approval_py",
        ],
    );

    // Verify only manager approval path was created
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve_py".to_string()),
        "Auto-approve should NOT be created for medium amounts"
    );
    assert!(
        !step_names.contains(&"finance_review_py".to_string()),
        "Finance review should NOT be created for medium amounts"
    );

    verify_decision_step(&steps, "routing_decision_py");

    assert_eq!(
        steps.len(),
        4,
        "Should have exactly 4 steps for manager-only path"
    );

    println!("✅ Python medium amount manager-approval test passed");
    Ok(())
}

/// Test large amount triggers dual-approval path.
///
/// Validates:
/// - Large amounts (>= $5,000) route to manager_approval_py + finance_review_py
/// - auto_approve_py is NOT created
/// - Task completes successfully with 5 steps
#[tokio::test]
async fn test_python_large_amount_dual_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Large amount: $10,000 should trigger dual-approval path
    let task_request =
        create_approval_request(10000.0, "charlie@example.com", "Capital investment");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(task.status, "complete", "Task should complete successfully");

    // Expected steps for large amount
    verify_steps_created(
        &steps,
        &[
            "validate_request_py",
            "routing_decision_py",
            "manager_approval_py",
            "finance_review_py",
            "finalize_approval_py",
        ],
    );

    // Verify auto-approve was NOT created
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve_py".to_string()),
        "Auto-approve should NOT be created for large amounts"
    );

    verify_decision_step(&steps, "routing_decision_py");

    // Verify total step count (5 steps for dual-approval path)
    assert_eq!(
        steps.len(),
        5,
        "Should have exactly 5 steps for dual-approval path"
    );

    // Verify both approval steps completed
    let manager_step = steps
        .iter()
        .find(|s| s.name == "manager_approval_py")
        .expect("Manager approval step should exist");
    assert_eq!(
        manager_step.current_state.to_uppercase(),
        "COMPLETE",
        "Manager approval should complete"
    );

    let finance_step = steps
        .iter()
        .find(|s| s.name == "finance_review_py")
        .expect("Finance review step should exist");
    assert_eq!(
        finance_step.current_state.to_uppercase(),
        "COMPLETE",
        "Finance review should complete"
    );

    println!("✅ Python large amount dual-approval test passed");
    Ok(())
}

/// Test boundary condition at $1,000 threshold.
///
/// Validates:
/// - Exactly $1,000 triggers manager approval (not auto-approve)
#[tokio::test]
async fn test_python_boundary_small_threshold() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test exactly at $1,000 threshold (should trigger manager approval)
    let task_request = create_approval_request(1000.0, "eve@example.com", "Threshold test");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // At exactly $1,000, should use manager approval (not auto)
    verify_steps_created(
        &steps,
        &[
            "validate_request_py",
            "routing_decision_py",
            "manager_approval_py",
            "finalize_approval_py",
        ],
    );

    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        !step_names.contains(&"auto_approve_py".to_string()),
        "Should NOT auto-approve at threshold"
    );

    println!("✅ Python boundary $1,000 threshold test passed");
    Ok(())
}

/// Test boundary condition at $5,000 threshold.
///
/// Validates:
/// - Exactly $5,000 triggers dual approval
#[tokio::test]
async fn test_python_boundary_large_threshold() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test exactly at $5,000 threshold (should trigger dual approval)
    let task_request = create_approval_request(5000.0, "frank@example.com", "Large threshold test");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // At exactly $5,000, should use dual approval
    verify_steps_created(
        &steps,
        &[
            "validate_request_py",
            "routing_decision_py",
            "manager_approval_py",
            "finance_review_py",
            "finalize_approval_py",
        ],
    );

    assert_eq!(
        steps.len(),
        5,
        "Should have 5 steps for dual approval at threshold"
    );

    println!("✅ Python boundary $5,000 threshold test passed");
    Ok(())
}

/// Test very small amount still uses auto-approval.
///
/// Validates:
/// - Very small amounts ($0.01) route to auto_approve_py
#[tokio::test]
async fn test_python_very_small_amount() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Test very small amount: $0.01
    let task_request = create_approval_request(0.01, "grace@example.com", "Petty cash");

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 10).await?;

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
            "validate_request_py",
            "routing_decision_py",
            "auto_approve_py",
            "finalize_approval_py",
        ],
    );

    println!("✅ Python very small amount test passed");
    Ok(())
}
