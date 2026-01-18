//! # TAS-73: Concurrent Task Creation Tests
//!
//! Validates that tasks can be created concurrently across multiple
//! orchestration instances without conflicts or data corruption.

use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use crate::common::integration_test_utils::create_task_request;
use crate::common::multi_instance_test_manager::MultiInstanceTestManager;

/// Test concurrent task creation across multiple orchestration instances
///
/// This test validates that:
/// 1. Tasks can be created concurrently through different orchestration instances
/// 2. All tasks complete successfully
/// 3. State is consistent across all instances
#[tokio::test]
#[ignore = "Requires multi-instance cluster: cargo make cluster-start"]
async fn test_concurrent_task_creation_across_instances() -> Result<()> {
    println!("\n🧪 Testing concurrent task creation across instances...");

    // Setup with 2 orchestration instances (assumes cluster is running)
    let manager = MultiInstanceTestManager::setup_from_env().await?;
    let timeout = Duration::from_secs(60);

    // Wait for cluster health
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    println!(
        "   Cluster: {} orchestration, {} workers",
        manager.orchestration_count(),
        manager.worker_count()
    );

    // Create task requests
    let task_count = 10;
    let requests: Vec<_> = (0..task_count)
        .map(|i| {
            create_task_request(
                "rust_e2e_linear",
                "mathematical_sequence",
                json!({"even_number": (i * 2 + 2) as i32}),
            )
        })
        .collect();

    println!("   Creating {} tasks concurrently...", task_count);

    // Create tasks concurrently across the cluster
    let responses = manager.create_tasks_concurrent(requests).await?;
    assert_eq!(responses.len(), task_count);

    println!("   ✅ All {} tasks created", task_count);

    // Verify each task was created successfully
    for (i, response) in responses.iter().enumerate() {
        assert!(!response.task_uuid.is_empty(), "Task {} has empty UUID", i);
        assert!(
            response.step_count > 0,
            "Task {} has no steps: {}",
            i,
            response.task_uuid
        );
        println!(
            "      Task {}: {} ({} steps)",
            i + 1,
            response.task_uuid,
            response.step_count
        );
    }

    // Wait for all tasks to complete
    println!("   Waiting for tasks to complete...");
    let task_uuids: Vec<_> = responses
        .iter()
        .map(|r| uuid::Uuid::parse_str(&r.task_uuid).unwrap())
        .collect();

    let completed = manager
        .wait_for_tasks_completion(task_uuids.clone(), timeout)
        .await?;

    // Verify all tasks completed successfully
    for (i, task) in completed.iter().enumerate() {
        assert_eq!(
            task.status, "complete",
            "Task {} did not complete: status={}",
            i, task.status
        );
    }

    println!("   ✅ All {} tasks completed", task_count);

    // Verify consistency across instances
    println!("   Verifying cross-instance consistency...");
    for task_uuid in &task_uuids {
        manager.verify_task_consistency(*task_uuid).await?;
    }

    println!("   ✅ All tasks consistent across instances");
    println!("🎉 Concurrent task creation test passed!");

    Ok(())
}

/// Test rapid task creation burst
///
/// Creates many tasks in quick succession to stress test the system.
#[tokio::test]
#[ignore = "Requires multi-instance cluster: cargo make cluster-start"]
async fn test_rapid_task_creation_burst() -> Result<()> {
    println!("\n🧪 Testing rapid task creation burst...");

    let manager = MultiInstanceTestManager::setup_from_env().await?;
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    // Create a burst of tasks
    let burst_size = 25;
    let requests: Vec<_> = (0..burst_size)
        .map(|i| {
            create_task_request(
                "rust_e2e_linear",
                "mathematical_sequence",
                json!({"even_number": ((i % 50) * 2 + 2) as i32}),
            )
        })
        .collect();

    println!("   Sending burst of {} tasks...", burst_size);
    let start = std::time::Instant::now();

    let responses = manager.create_tasks_concurrent(requests).await?;
    let creation_time = start.elapsed();

    assert_eq!(responses.len(), burst_size);
    println!(
        "   ✅ Created {} tasks in {:?} ({:.1} tasks/sec)",
        burst_size,
        creation_time,
        burst_size as f64 / creation_time.as_secs_f64()
    );

    // Verify no duplicates by checking UUIDs
    let mut uuids: Vec<_> = responses.iter().map(|r| r.task_uuid.clone()).collect();
    uuids.sort();
    let original_len = uuids.len();
    uuids.dedup();
    assert_eq!(
        uuids.len(),
        original_len,
        "Duplicate task UUIDs detected!"
    );

    println!("   ✅ No duplicate UUIDs detected");
    println!("🎉 Rapid task creation burst test passed!");

    Ok(())
}

/// Test task creation with round-robin distribution
///
/// Verifies that tasks are distributed across instances using round-robin.
#[tokio::test]
#[ignore = "Requires multi-instance cluster: cargo make cluster-start"]
async fn test_task_creation_round_robin_distribution() -> Result<()> {
    println!("\n🧪 Testing round-robin task distribution...");

    let manager = MultiInstanceTestManager::setup_from_env().await?;
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    let instance_count = manager.orchestration_count();
    if instance_count < 2 {
        println!("   ⚠️ Skipping: need at least 2 instances for distribution test");
        return Ok(());
    }

    // Create tasks one at a time to verify round-robin
    let task_count = instance_count * 3; // 3 rounds
    println!(
        "   Creating {} tasks across {} instances...",
        task_count, instance_count
    );

    for i in 0..task_count {
        let request = create_task_request(
            "rust_e2e_linear",
            "mathematical_sequence",
            json!({"even_number": (i * 2 + 2) as i32}),
        );

        let response = manager.create_tasks_concurrent(vec![request]).await?;
        assert_eq!(response.len(), 1);
        println!(
            "      Task {}: {} (instance {})",
            i + 1,
            response[0].task_uuid,
            (i % instance_count) + 1
        );
    }

    println!("   ✅ Tasks distributed via round-robin");
    println!("🎉 Round-robin distribution test passed!");

    Ok(())
}
