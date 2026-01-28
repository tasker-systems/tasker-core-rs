//! # End-to-End Conditional Approval Rust Workflow Integration Test
//!
//! This integration test validates the conditional approval workflow pattern using native Rust handlers by:
//! 1. Connecting to running Docker Compose services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute conditional approval tasks
//! 3. Testing TAS-53 decision point routing with dynamic step creation
//! 4. Validating YAML configuration from tests/fixtures/task_templates/rust/conditional_approval_rust.yaml
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Conditional Approval Pattern:
//! 1. validate_request: Validates the approval request
//! 2. routing_decision: DECISION POINT that routes based on amount:
//!    - amount < $1,000: auto_approve only
//!    - amount < $5,000: manager_approval only
//!    - amount >= $5,000: manager_approval + finance_review (dual approval)
//! 3. finalize_approval: Convergence step that collects all approval results

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{
    create_task_request, get_task_completion_timeout, wait_for_task_completion,
};

#[tokio::test]
async fn test_conditional_approval_small_amount() -> Result<()> {
    println!("🚀 Starting Conditional Approval (Small Amount) Test");
    println!("   Amount: $500 (should route to auto_approve only)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create conditional approval task with small amount
    println!("\n🎯 Creating conditional approval task (small amount)...");
    let task_request = create_task_request(
        "conditional_approval_rust",
        "approval_routing",
        json!({
            "amount": 500,
            "requester": "john.doe",
            "purpose": "Office supplies purchase"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected route: auto_approve only (amount < $1,000)");

    // Monitor task execution
    println!("\n⏱️ Monitoring conditional approval execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying conditional approval results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Conditional approval execution should be complete"
    );
    println!(
        "✅ Task execution status: {} (overall status: {})",
        final_task.execution_status, final_task.status
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify expected steps for small amount: validate_request, routing_decision, auto_approve, finalize_approval
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"validate_request"),
        "Should have validate_request step"
    );
    assert!(
        step_names.contains(&"routing_decision"),
        "Should have routing_decision step"
    );
    assert!(
        step_names.contains(&"auto_approve"),
        "Should have auto_approve step for small amount"
    );
    assert!(
        step_names.contains(&"finalize_approval"),
        "Should have finalize_approval step"
    );

    // Should NOT have manager_approval or finance_review for small amounts
    assert!(
        !step_names.contains(&"manager_approval"),
        "Should NOT have manager_approval for small amount"
    );
    assert!(
        !step_names.contains(&"finance_review"),
        "Should NOT have finance_review for small amount"
    );

    println!("\n🎉 Conditional Approval (Small Amount) Test PASSED!");
    println!("✅ Auto-approval routing: Working");
    println!("✅ Decision point step creation: Working");
    println!("✅ Rust handlers (conditional_approval_rust): Working");

    Ok(())
}

#[tokio::test]
async fn test_conditional_approval_medium_amount() -> Result<()> {
    println!("🚀 Starting Conditional Approval (Medium Amount) Test");
    println!("   Amount: $3,000 (should route to manager_approval only)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating conditional approval task (medium amount)...");
    let task_request = create_task_request(
        "conditional_approval_rust",
        "approval_routing",
        json!({
            "amount": 3000,
            "requester": "jane.smith",
            "purpose": "Equipment upgrade"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected route: manager_approval only (amount < $5,000)");

    // Monitor task execution
    println!("\n⏱️ Monitoring conditional approval execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying conditional approval results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Conditional approval execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify expected steps for medium amount
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"validate_request"),
        "Should have validate_request step"
    );
    assert!(
        step_names.contains(&"routing_decision"),
        "Should have routing_decision step"
    );
    assert!(
        step_names.contains(&"manager_approval"),
        "Should have manager_approval step for medium amount"
    );
    assert!(
        step_names.contains(&"finalize_approval"),
        "Should have finalize_approval step"
    );

    // Should NOT have auto_approve or finance_review for medium amounts
    assert!(
        !step_names.contains(&"auto_approve"),
        "Should NOT have auto_approve for medium amount"
    );
    assert!(
        !step_names.contains(&"finance_review"),
        "Should NOT have finance_review for medium amount"
    );

    println!("\n🎉 Conditional Approval (Medium Amount) Test PASSED!");
    println!("✅ Manager-only approval routing: Working");
    println!("✅ Decision point step creation: Working");

    Ok(())
}

#[tokio::test]
async fn test_conditional_approval_large_amount() -> Result<()> {
    println!("🚀 Starting Conditional Approval (Large Amount) Test");
    println!("   Amount: $10,000 (should route to manager_approval + finance_review)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating conditional approval task (large amount)...");
    let task_request = create_task_request(
        "conditional_approval_rust",
        "approval_routing",
        json!({
            "amount": 10000,
            "requester": "bob.johnson",
            "purpose": "Major infrastructure investment"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected route: manager_approval + finance_review (amount >= $5,000)");

    // Monitor task execution
    println!("\n⏱️ Monitoring conditional approval execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying conditional approval results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Conditional approval execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify expected steps for large amount (dual approval)
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"validate_request"),
        "Should have validate_request step"
    );
    assert!(
        step_names.contains(&"routing_decision"),
        "Should have routing_decision step"
    );
    assert!(
        step_names.contains(&"manager_approval"),
        "Should have manager_approval step for large amount"
    );
    assert!(
        step_names.contains(&"finance_review"),
        "Should have finance_review step for large amount"
    );
    assert!(
        step_names.contains(&"finalize_approval"),
        "Should have finalize_approval step"
    );

    // Should NOT have auto_approve for large amounts
    assert!(
        !step_names.contains(&"auto_approve"),
        "Should NOT have auto_approve for large amount"
    );

    // Verify all steps completed
    for step in &steps {
        assert_eq!(
            step.current_state.to_ascii_uppercase(),
            "COMPLETE",
            "Step {} should be completed",
            step.name
        );
        println!("   ✅ Step: {} - {}", step.name, step.current_state);
    }

    println!("\n🎉 Conditional Approval (Large Amount) Test PASSED!");
    println!("✅ Dual approval routing: Working");
    println!("✅ Decision point parallel step creation: Working");
    println!("✅ Deferred finalization with dynamic dependencies: Working");

    Ok(())
}

/// Test API validation without full execution
#[tokio::test]
async fn test_conditional_approval_api_validation() -> Result<()> {
    println!("🔧 Testing Conditional Approval Rust API Validation");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Test task creation with valid data
    let task_request = create_task_request(
        "conditional_approval_rust",
        "approval_routing",
        json!({
            "amount": 1500,
            "requester": "test.user",
            "purpose": "Test approval"
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    assert!(!task_response.task_uuid.is_empty());
    println!(
        "✅ Conditional approval API creation working: {}",
        task_response.task_uuid
    );

    // Test task retrieval
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );
    println!("✅ Conditional approval retrieval API working");

    // Verify expected step count for decision point workflow
    // At initialization, only steps up to and including decision points are created:
    // 1. validate_request (root step)
    // 2. routing_decision (decision point)
    //
    // NOT created at initialization:
    // - finalize_approval (deferred convergence - depends on dynamic children)
    // - auto_approve, manager_approval, finance_review (dynamic children of decision)
    //
    // The workflow graph segmentation logic correctly identifies finalize_approval
    // as a DESCENDANT of the decision point (transitively through its dependencies)
    // and excludes it from the initial set.
    println!(
        "   Task has {} steps at initialization (expecting 2: up to decision boundary)",
        task_response.total_steps
    );
    assert_eq!(
        task_response.total_steps, 2,
        "Decision point workflow should have exactly 2 steps at initialization (validate_request, routing_decision). \
        Deferred convergence step and dynamic children are created at runtime when decision executes. Got: {}",
        task_response.total_steps
    );
    println!(
        "✅ Conditional approval step count validation: {} steps at initialization \
        (deferred convergence and dynamic children created at runtime)",
        task_response.total_steps
    );

    println!("\n🎉 Conditional Approval API Validation Test PASSED!");
    println!("✅ Conditional approval task creation: Working");
    println!("✅ API validation: Working");
    println!("✅ tasker-client integration: Working");

    Ok(())
}
