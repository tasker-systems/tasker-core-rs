//! # End-to-End Diamond-Decision-Batch Combined Workflow Integration Test
//!
//! This integration test validates the composition of three workflow patterns using native Rust handlers:
//! 1. **Diamond pattern**: Parallel branches with convergence
//! 2. **Decision points**: Dynamic step creation based on runtime conditions
//! 3. **Batch processing**: Cursor-based parallel batch workers with deferred convergence
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! Combined Workflow Structure:
//! ```text
//! ddb_diamond_start
//!     ├─→ branch_evens (filter even numbers)
//!     └─→ branch_odds (filter odd numbers)
//!             └─→ ddb_routing_decision (decision point)
//!                     ├─→ even_batch_analyzer (batchable)
//!                     │       ├─→ process_even_batch_001 (batch_worker)
//!                     │       └─→ process_even_batch_N (batch_worker)
//!                     │               └─→ aggregate_even_results (deferred_convergence)
//!                     │
//!                     └─→ odd_batch_analyzer (batchable)
//!                             ├─→ process_odd_batch_001 (batch_worker)
//!                             └─→ process_odd_batch_N (batch_worker)
//!                                     └─→ aggregate_odd_results (deferred_convergence)
//! ```
//!
//! This demonstrates advanced workflow composition capabilities:
//! - Parallel execution (diamond branches)
//! - Conditional routing (decision point choosing even vs odd path)
//! - Dynamic parallelism (batch workers created at runtime)
//! - Convergence patterns (deferred steps waiting for dynamic children)

use anyhow::Result;
use serde_json::json;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{
    create_task_request, get_task_completion_timeout, wait_for_task_completion,
};

#[tokio::test]
async fn test_diamond_decision_batch_even_dominant() -> Result<()> {
    println!("🚀 Starting Diamond-Decision-Batch (Even Dominant) Test");
    println!("   Numbers: [2, 4, 6, 8, 10, 12] (6 evens, 0 odds)");
    println!("   Expected: Route to even_batch_analyzer → create even batch workers");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    if let Some(ref worker_url) = manager.worker_url {
        println!("   Worker: {}", worker_url);
    }

    // Create combined workflow task with even-dominant numbers
    println!("\n🎯 Creating diamond-decision-batch task (even dominant)...");
    let task_request = create_task_request(
        "combined_workflow_test",
        "diamond_decision_batch_processor",
        json!({
            "numbers": [2, 4, 6, 8, 10, 12],
            "batch_size": 2  // Should create 3 workers (6/2 = 3)
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected path: Diamond → Decision → Even batches");

    // Monitor task execution
    println!("\n⏱️ Monitoring combined workflow execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying combined workflow results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Combined workflow execution should be complete"
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

    // Verify diamond pattern steps
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"ddb_diamond_start"),
        "Should have ddb_diamond_start step"
    );
    assert!(
        step_names.contains(&"branch_evens"),
        "Should have branch_evens step"
    );
    assert!(
        step_names.contains(&"branch_odds"),
        "Should have branch_odds step"
    );

    // Verify decision point step
    assert!(
        step_names.contains(&"ddb_routing_decision"),
        "Should have ddb_routing_decision step"
    );

    // Verify even path was chosen (even dominant)
    assert!(
        step_names.contains(&"even_batch_analyzer"),
        "Should have even_batch_analyzer step (even dominant path)"
    );
    assert!(
        step_names.contains(&"aggregate_even_results"),
        "Should have aggregate_even_results step"
    );

    // Verify odd path was NOT chosen
    assert!(
        !step_names.contains(&"odd_batch_analyzer"),
        "Should NOT have odd_batch_analyzer step (even dominant)"
    );
    assert!(
        !step_names.contains(&"aggregate_odd_results"),
        "Should NOT have aggregate_odd_results step (even dominant)"
    );

    // Count even batch workers
    let even_batch_workers = step_names
        .iter()
        .filter(|name| name.starts_with("process_even_batch_"))
        .count();
    println!("   ✅ Created {} even batch workers", even_batch_workers);
    assert!(
        even_batch_workers >= 3,
        "Should have at least 3 even batch workers (6 numbers / batch_size 2)"
    );

    // Verify no odd batch workers
    let odd_batch_workers = step_names
        .iter()
        .filter(|name| name.starts_with("process_odd_batch_"))
        .count();
    assert_eq!(
        odd_batch_workers, 0,
        "Should have NO odd batch workers (even dominant)"
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

    println!("\n🎉 Diamond-Decision-Batch (Even Dominant) Test PASSED!");
    println!("✅ Diamond parallel branches: Working");
    println!("✅ Decision point routing (even path): Working");
    println!("✅ Even batch processing: Working");
    println!("✅ Combined workflow composition: Working");

    Ok(())
}

#[tokio::test]
async fn test_diamond_decision_batch_odd_dominant() -> Result<()> {
    println!("🚀 Starting Diamond-Decision-Batch (Odd Dominant) Test");
    println!("   Numbers: [1, 3, 5, 7, 9, 11, 2, 4] (6 odds, 2 evens)");
    println!("   Expected: Route to odd_batch_analyzer → create odd batch workers");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating diamond-decision-batch task (odd dominant)...");
    let task_request = create_task_request(
        "combined_workflow_test",
        "diamond_decision_batch_processor",
        json!({
            "numbers": [1, 3, 5, 7, 9, 11, 2, 4],
            "batch_size": 2  // Should create 3 workers (6/2 = 3)
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected path: Diamond → Decision → Odd batches");

    // Monitor task execution
    println!("\n⏱️ Monitoring combined workflow execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying combined workflow results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Combined workflow execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify odd path was chosen (odd dominant)
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"odd_batch_analyzer"),
        "Should have odd_batch_analyzer step (odd dominant path)"
    );
    assert!(
        step_names.contains(&"aggregate_odd_results"),
        "Should have aggregate_odd_results step"
    );

    // Verify even path was NOT chosen
    assert!(
        !step_names.contains(&"even_batch_analyzer"),
        "Should NOT have even_batch_analyzer step (odd dominant)"
    );
    assert!(
        !step_names.contains(&"aggregate_even_results"),
        "Should NOT have aggregate_even_results step (odd dominant)"
    );

    // Count odd batch workers
    let odd_batch_workers = step_names
        .iter()
        .filter(|name| name.starts_with("process_odd_batch_"))
        .count();
    println!("   ✅ Created {} odd batch workers", odd_batch_workers);
    assert!(
        odd_batch_workers >= 3,
        "Should have at least 3 odd batch workers (6 numbers / batch_size 2)"
    );

    // Verify no even batch workers
    let even_batch_workers = step_names
        .iter()
        .filter(|name| name.starts_with("process_even_batch_"))
        .count();
    assert_eq!(
        even_batch_workers, 0,
        "Should have NO even batch workers (odd dominant)"
    );

    println!("\n🎉 Diamond-Decision-Batch (Odd Dominant) Test PASSED!");
    println!("✅ Decision point routing (odd path): Working");
    println!("✅ Odd batch processing: Working");
    println!("✅ Alternative path selection: Working");

    Ok(())
}

#[tokio::test]
async fn test_diamond_decision_batch_balanced() -> Result<()> {
    println!("🚀 Starting Diamond-Decision-Batch (Balanced) Test");
    println!("   Numbers: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] (5 evens, 5 odds - equal)");
    println!("   Expected: Route to even_batch_analyzer (default when equal)");

    let manager = IntegrationTestManager::setup().await?;

    println!("\n🎯 Creating diamond-decision-batch task (balanced)...");
    let task_request = create_task_request(
        "combined_workflow_test",
        "diamond_decision_batch_processor",
        json!({
            "numbers": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            "batch_size": 3
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: Even path (default when counts are equal)");

    // Monitor task execution
    println!("\n⏱️ Monitoring combined workflow execution...");
    let timeout = get_task_completion_timeout();
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    // Verify final results
    println!("\n🔍 Verifying combined workflow results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let final_task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        final_task.is_execution_complete(),
        "Combined workflow execution should be complete"
    );

    // Get workflow steps
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;
    println!("✅ Retrieved {} workflow steps", steps.len());

    // Verify even path was chosen (default when equal)
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        step_names.contains(&"even_batch_analyzer"),
        "Should have even_batch_analyzer step (default when counts equal)"
    );

    println!("\n🎉 Diamond-Decision-Batch (Balanced) Test PASSED!");
    println!("✅ Decision point default routing: Working");
    println!("✅ Edge case handling: Working");

    Ok(())
}

/// Test API validation without full execution
#[tokio::test]
async fn test_diamond_decision_batch_api_validation() -> Result<()> {
    println!("🔧 Testing Diamond-Decision-Batch API Validation");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Test task creation with valid data
    let task_request = create_task_request(
        "combined_workflow_test",
        "diamond_decision_batch_processor",
        json!({
            "numbers": [2, 4, 6, 8],
            "batch_size": 2
        }),
    );

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    assert!(!task_response.task_uuid.is_empty());
    println!(
        "✅ Diamond-decision-batch API creation working: {}",
        task_response.task_uuid
    );

    // Test task retrieval
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let retrieved_task = manager.orchestration_client.get_task(task_uuid).await?;
    assert_eq!(
        retrieved_task.task_uuid.to_string(),
        task_response.task_uuid
    );
    println!("✅ Diamond-decision-batch retrieval API working");

    // Verify expected step count for combined workflow
    // At initialization, only steps up to and including decision points are created:
    // 1. ddb_diamond_start (root step)
    // 2. branch_evens (parallel branch)
    // 3. branch_odds (parallel branch)
    // 4. ddb_routing_decision (decision point)
    //
    // NOT created at initialization:
    // - even_batch_analyzer, odd_batch_analyzer (dynamic children of decision)
    // - process_even_batch_*, process_odd_batch_* (dynamic batch workers)
    // - aggregate_even_results, aggregate_odd_results (deferred convergence)
    println!(
        "   Task has {} steps at initialization (expecting 4: diamond + decision)",
        task_response.step_count
    );
    assert_eq!(
        task_response.step_count, 4,
        "Combined workflow should have exactly 4 steps at initialization \
        (ddb_diamond_start, branch_evens, branch_odds, ddb_routing_decision). \
        Batchable, batch workers, and deferred convergence are created at runtime. Got: {}",
        task_response.step_count
    );
    println!(
        "✅ Diamond-decision-batch step count validation: {} steps at initialization \
        (dynamic steps created at runtime)",
        task_response.step_count
    );

    println!("\n🎉 Diamond-Decision-Batch API Validation Test PASSED!");
    println!("✅ Combined workflow task creation: Working");
    println!("✅ API validation: Working");
    println!("✅ tasker-client integration: Working");
    println!("✅ Workflow pattern composition: Working");

    Ok(())
}
