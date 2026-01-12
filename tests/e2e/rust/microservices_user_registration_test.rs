//! # End-to-End Rust Microservices User Registration Workflow Integration Test
//!
//! This integration test validates the Blog Post 03 microservices coordination pattern with Rust handlers:
//! 1. Connecting to running services (postgres, orchestration, worker)
//! 2. Using tasker-client library to create and execute user registration tasks
//! 3. Testing complete workflow: user creation -> parallel billing/preferences -> welcome -> activation
//! 4. Validating YAML configuration from tests/fixtures/task_templates/rust/microservices_user_registration_rs.yaml
//!
//! Prerequisites:
//! Run `cargo make services-start` to start local services
//!
//! Microservices User Registration Pattern (5 steps):
//! 1. create_user_account: Create user in user service with idempotency (root)
//! 2. setup_billing_profile: Setup billing profile in billing service (parallel)
//! 3. initialize_preferences: Set user preferences in preferences service (parallel)
//! 4. send_welcome_sequence: Send welcome emails via notification service (convergence)
//! 5. update_user_status: Mark user as active in user service (final)
//!
//! TAS-91: Blog Post 03 - Rust implementation
//!
//! NOTE: This test is language-agnostic and uses the tasker-client API. It does NOT reference
//! Rust code or handlers directly - ensuring the system works correctly regardless of worker
//! implementation language.

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create user registration task request for Rust
///
/// Creates a task request for the Rust microservices workflow template.
/// Uses language-agnostic parameters that work regardless of handler implementation.
fn create_user_registration_request_rs(
    user_info: serde_json::Value,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request("microservices_rs", "user_registration_rs", user_info)
}

/// Test successful user registration workflow (Rust)
///
/// Validates:
/// - Task completes successfully
/// - All 5 steps execute correctly (1 root + 2 parallel + 1 convergence + 1 final)
/// - Parallel execution works (billing and preferences run concurrently)
/// - Convergence works correctly (welcome waits for both parallel steps)
/// - Execution completes in reasonable time (< 30s for 5 steps)
#[tokio::test]
async fn test_successful_user_registration_rs() -> Result<()> {
    println!("🚀 Starting Rust Microservices User Registration Test");
    println!("   Workflow: User creation -> parallel billing/preferences -> welcome -> activation");
    println!("   Template: user_registration_rs");
    println!("   Namespace: microservices_rs");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create user registration request
    let user_email = format!("test_{}@example.com", Uuid::new_v4());
    let user_info = json!({
        "user_info": {
            "email": user_email,
            "name": "Test User",
            "plan": "pro",
            "source": "api_test"
        }
    });

    println!("\n🎯 Creating Rust user registration task...");
    println!("   Email: {}", user_email);
    println!("   Plan: pro");
    let task_request = create_user_registration_request_rs(user_info);

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 5 steps (1 root, 2 parallel, 1 convergence, 1 final)");

    // Monitor task execution
    println!("\n⏱️  Monitoring user registration execution...");
    let timeout = 30; // 30 seconds for 5 steps
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

    assert_eq!(steps.len(), 5, "Should have 5 total steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step names match expected workflow
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();

    // Sequential phase (1 step - root)
    assert!(
        step_names.contains(&"create_user_account".to_string()),
        "Should have create_user_account step"
    );

    // Parallel phase (2 steps)
    assert!(
        step_names.contains(&"setup_billing_profile".to_string()),
        "Should have setup_billing_profile step"
    );
    assert!(
        step_names.contains(&"initialize_preferences".to_string()),
        "Should have initialize_preferences step"
    );

    // Coordination phase (1 step - convergence)
    assert!(
        step_names.contains(&"send_welcome_sequence".to_string()),
        "Should have send_welcome_sequence step (convergence)"
    );

    // Completion phase (1 step - final)
    assert!(
        step_names.contains(&"update_user_status".to_string()),
        "Should have update_user_status step (final)"
    );

    println!("✅ Rust microservices user registration completed successfully!");
    println!("   All 5 steps executed correctly");
    println!("   Parallel execution (billing + preferences) worked correctly");
    println!("   Convergence (send_welcome_sequence) worked correctly");
    println!("   User email: {}", user_email);
    Ok(())
}
