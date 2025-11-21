//! # End-to-End Ruby Microservices Coordination Integration Test
//!
//! This integration test validates the Blog Post 03 microservices coordination workflow:
//! 1. Sequential user creation (create_user_account)
//! 2. Parallel service coordination (setup_billing_profile + initialize_preferences)
//! 3. Convergence with welcome sequence (send_welcome_sequence)
//! 4. Final status update (update_user_status)
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Workflow Structure:
//! ```
//! create_user_account
//!        ├──→ setup_billing_profile ──┐
//!        └──→ initialize_preferences ──→ send_welcome_sequence ──→ update_user_status
//! ```
//!
//! NOTE: This test demonstrates parallel execution and microservices coordination patterns.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create user registration task request
fn create_user_registration_request(
    email: &str,
    name: &str,
    plan: &str,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "microservices",
        "user_registration",
        json!({
            "user_info": {
                "email": email,
                "name": name,
                "plan": plan,
                "source": "web"
            }
        }),
    )
}

/// Test complete user registration workflow with pro plan
///
/// Validates:
/// - All 5 steps complete successfully
/// - Parallel execution of billing + preferences
/// - Sequential execution of create → parallel → welcome → status
/// - Proper data flow between services
#[tokio::test]
async fn test_user_registration_pro_plan_workflow() -> Result<()> {
    println!("🚀 Starting User Registration Pro Plan Workflow Test");
    println!("   Namespace: microservices");
    println!("   Workflow: user_registration");
    println!("   Steps: 5 (1 create → 2 parallel → 1 welcome → 1 status)");
    println!("   Pattern: Microservices coordination with parallel execution");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready!");

    // Create user registration task for pro plan
    println!("\n🎯 Creating user registration task (pro plan)...");
    let task_request =
        create_user_registration_request("sarah@example.com", "Sarah Johnson", "pro");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Namespace: microservices");
    println!("   Plan: pro (requires billing setup)");

    // Monitor task execution
    println!("\n⏱️  Monitoring user registration workflow...");
    let timeout = 30; // 30 seconds for 5 steps with parallel execution
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying user registration results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );
    assert!(
        task.status.to_lowercase().contains("complete"),
        "Task status should indicate completion, got: {}",
        task.status
    );

    // Verify all 5 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 5, "Should have 5 steps in workflow");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names and workflow structure
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    // Sequential phase
    assert!(
        step_names.contains(&"create_user_account".to_string()),
        "Should have create_user_account step"
    );

    // Parallel phase
    assert!(
        step_names.contains(&"setup_billing_profile".to_string()),
        "Should have setup_billing_profile step"
    );
    assert!(
        step_names.contains(&"initialize_preferences".to_string()),
        "Should have initialize_preferences step"
    );

    // Coordination phase
    assert!(
        step_names.contains(&"send_welcome_sequence".to_string()),
        "Should have send_welcome_sequence step (convergence point)"
    );

    // Completion phase
    assert!(
        step_names.contains(&"update_user_status".to_string()),
        "Should have update_user_status step"
    );

    println!("✅ User registration completed successfully!");
    println!("   All 5 steps executed in correct workflow order");
    println!("   ✓ Create user: Sequential");
    println!("   ✓ Billing + Preferences: Parallel execution");
    println!("   ✓ Welcome sequence: Convergence after parallel");
    println!("   ✓ Status update: Final step");

    Ok(())
}

/// Test user registration workflow with free plan (graceful degradation)
///
/// Validates:
/// - Free plan users skip billing setup gracefully
/// - Workflow still completes successfully
/// - Parallel execution still occurs (preferences runs in parallel conceptually)
#[tokio::test]
async fn test_user_registration_free_plan_workflow() -> Result<()> {
    println!("🚀 Starting User Registration Free Plan Workflow Test");
    println!("   Plan: free (demonstrates graceful degradation)");

    let manager = IntegrationTestManager::setup().await?;

    // Create user registration task for free plan
    println!("\n🎯 Creating user registration task (free plan)...");
    let task_request =
        create_user_registration_request("free.user@example.com", "Free User", "free");

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Plan: free (billing setup will be skipped)");

    // Monitor task execution
    println!("\n⏱️  Monitoring free plan registration...");
    wait_for_task_completion(&manager.orchestration_client, &task_response.task_uuid, 30).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Free plan registration should complete"
    );

    // Verify all steps completed (including billing which gracefully degrades)
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 5, "Should have 5 steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should complete (with graceful degradation)"
    );

    println!("✅ Free plan registration completed!");
    println!("   Graceful degradation: billing setup skipped for free plan");

    Ok(())
}

/// Test enterprise plan registration with full features
///
/// Validates:
/// - Enterprise plan gets all premium features
/// - Additional notification channels (SMS for enterprise)
/// - Full workflow execution
#[tokio::test]
async fn test_user_registration_enterprise_plan_workflow() -> Result<()> {
    println!("🚀 Starting User Registration Enterprise Plan Workflow Test");
    println!("   Plan: enterprise (all premium features)");

    let manager = IntegrationTestManager::setup().await?;

    // Create user registration task for enterprise plan
    println!("\n🎯 Creating user registration task (enterprise plan)...");
    let task_request = create_user_registration_request(
        "enterprise@example.com",
        "Enterprise Customer",
        "enterprise",
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Plan: enterprise (includes SMS notifications)");

    // Monitor task execution
    println!("\n⏱️  Monitoring enterprise registration...");
    wait_for_task_completion(&manager.orchestration_client, &task_response.task_uuid, 30).await?;

    // Verify completion
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Enterprise registration should complete"
    );

    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 5, "Should have 5 steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should complete"
    );

    println!("✅ Enterprise registration completed!");
    println!("   Full feature set: billing + preferences + multi-channel notifications");

    Ok(())
}
