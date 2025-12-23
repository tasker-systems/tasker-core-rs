//! TypeScript Conditional Approval E2E Tests
//!
//! TAS-53: Dynamic Workflows with Decision Points
//!
//! These tests validate conditional approval routing with TypeScript handlers
//! through the orchestration API.
//!
//! Approval Routing Logic:
//! - Amount < $1,000: Auto-approve (4 steps: validate → routing → auto_approve → finalize)
//! - Amount $1,000-$4,999: Manager approval (4 steps: validate → routing → manager → finalize)
//! - Amount >= $5,000: Dual approval (5 steps: validate → routing → manager + finance → finalize)
//!
//! Note: These tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Test auto-approval for small amounts (< $1,000)
///
/// Validates:
/// - Routing decision creates auto_approve step only
/// - Task completes with 4 steps
/// - All steps reach COMPLETE state
#[tokio::test]
async fn test_typescript_small_amount_auto_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with small amount ($500)
    let task_request = create_task_request(
        "conditional_approval_ts",
        "approval_routing_ts",
        json!({
            "amount": 500.0,
            "requester": "test_user",
            "purpose": "Small purchase"
        }),
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
        "Task should be complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Should have 4 steps: validate, routing_decision, auto_approve, finalize
    assert_eq!(
        steps.len(),
        4,
        "Should have 4 steps for small amount auto-approval"
    );

    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"validate_request_ts"),
        "Should have validate_request_ts"
    );
    assert!(
        step_names.contains(&"routing_decision_ts"),
        "Should have routing_decision_ts"
    );
    assert!(
        step_names.contains(&"auto_approve_ts"),
        "Should have auto_approve_ts"
    );
    assert!(
        step_names.contains(&"finalize_approval_ts"),
        "Should have finalize_approval_ts"
    );

    // All steps should be complete
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ TypeScript small amount auto-approval completed");
    Ok(())
}

/// Test manager approval for medium amounts ($1,000-$4,999)
///
/// Validates:
/// - Routing decision creates manager_approval step only
/// - Task completes with 4 steps
#[tokio::test]
async fn test_typescript_medium_amount_manager_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with medium amount ($2,500)
    let task_request = create_task_request(
        "conditional_approval_ts",
        "approval_routing_ts",
        json!({
            "amount": 2500.0,
            "requester": "test_user",
            "purpose": "Medium purchase"
        }),
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
        "Task should be complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Should have 4 steps: validate, routing_decision, manager_approval, finalize
    assert_eq!(
        steps.len(),
        4,
        "Should have 4 steps for medium amount manager approval"
    );

    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"manager_approval_ts"),
        "Should have manager_approval_ts"
    );

    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ TypeScript medium amount manager approval completed");
    Ok(())
}

/// Test dual approval for large amounts (>= $5,000)
///
/// Validates:
/// - Routing decision creates both manager_approval and finance_review steps
/// - Task completes with 5 steps
#[tokio::test]
async fn test_typescript_large_amount_dual_approval() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Create task with large amount ($10,000)
    let task_request = create_task_request(
        "conditional_approval_ts",
        "approval_routing_ts",
        json!({
            "amount": 10000.0,
            "requester": "test_user",
            "purpose": "Large purchase"
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
        "Task should be complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Should have 5 steps: validate, routing_decision, manager_approval, finance_review, finalize
    assert_eq!(
        steps.len(),
        5,
        "Should have 5 steps for large amount dual approval"
    );

    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"manager_approval_ts"),
        "Should have manager_approval_ts"
    );
    assert!(
        step_names.contains(&"finance_review_ts"),
        "Should have finance_review_ts"
    );

    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    println!("✅ TypeScript large amount dual approval completed");
    Ok(())
}

/// Test boundary condition: exactly $1,000
///
/// Validates:
/// - Amount exactly at threshold routes correctly
#[tokio::test]
async fn test_typescript_boundary_small_threshold() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Exactly $1,000 - should be medium (manager approval)
    let task_request = create_task_request(
        "conditional_approval_ts",
        "approval_routing_ts",
        json!({
            "amount": 1000.0,
            "requester": "test_user",
            "purpose": "Boundary test"
        }),
    );

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

    // Should have manager approval (not auto-approve)
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"manager_approval_ts"),
        "Exactly $1,000 should route to manager approval"
    );
    assert!(
        !step_names.contains(&"auto_approve_ts"),
        "Exactly $1,000 should NOT have auto-approve"
    );

    println!("✅ TypeScript boundary test ($1,000) completed");
    Ok(())
}

/// Test boundary condition: exactly $5,000
///
/// Validates:
/// - Amount exactly at large threshold routes correctly
#[tokio::test]
async fn test_typescript_boundary_large_threshold() -> Result<()> {
    let manager = IntegrationTestManager::setup().await?;

    // Exactly $5,000 - should be large (dual approval)
    let task_request = create_task_request(
        "conditional_approval_ts",
        "approval_routing_ts",
        json!({
            "amount": 5000.0,
            "requester": "test_user",
            "purpose": "Boundary test"
        }),
    );

    let response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(&manager.orchestration_client, &response.task_uuid, 15).await?;

    let task_uuid = Uuid::parse_str(&response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    // Should have both manager and finance (dual approval)
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"manager_approval_ts"),
        "Exactly $5,000 should have manager approval"
    );
    assert!(
        step_names.contains(&"finance_review_ts"),
        "Exactly $5,000 should have finance review"
    );

    println!("✅ TypeScript boundary test ($5,000) completed");
    Ok(())
}
