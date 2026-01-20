//! # TAS-73: Cross-Instance Consistency Tests
//!
//! Validates that state is consistent when querying tasks across
//! different orchestration instances.

use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use crate::common::integration_test_utils::create_task_request;
use crate::common::multi_instance_test_manager::MultiInstanceTestManager;

/// Test state consistency across orchestration instances
///
/// Creates a task through one instance and verifies it can be
/// read consistently through all instances.
#[tokio::test]
async fn test_state_consistency_across_instances() -> Result<()> {
    println!("\n🧪 Testing state consistency across instances...");

    let manager = MultiInstanceTestManager::setup_from_env().await?;
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    let instance_count = manager.orchestration_count();
    println!("   Cluster has {} orchestration instances", instance_count);

    // Create a task
    let request = create_task_request(
        "rust_e2e_linear",
        "mathematical_sequence",
        json!({"even_number": 42}),
    );

    let responses = manager.create_tasks_concurrent(vec![request]).await?;
    let task_uuid = uuid::Uuid::parse_str(&responses[0].task_uuid)?;

    println!("   Created task: {}", task_uuid);

    // Wait for task to complete
    let timeout = Duration::from_secs(60);
    let completed = manager.wait_for_task_completion(task_uuid, timeout).await?;
    println!("   Task completed with status: {}", completed.status);

    // Verify consistency across all instances
    println!(
        "   Verifying state across all {} instances...",
        instance_count
    );
    manager.verify_task_consistency(task_uuid).await?;

    // Query task from each instance directly and compare
    let clients = manager.cluster.all_clients();
    let mut statuses = Vec::new();

    for (i, client) in clients.iter().enumerate() {
        let task = client.get_task(task_uuid).await?;
        statuses.push(task.status.clone());
        println!(
            "      Instance {}: status='{}', name='{}'",
            i + 1,
            task.status,
            task.name
        );
    }

    // Verify all instances report the same status
    let first_status = &statuses[0];
    for (i, status) in statuses.iter().enumerate().skip(1) {
        assert_eq!(
            status,
            first_status,
            "Instance {} reports different status '{}' vs '{}'",
            i + 1,
            status,
            first_status
        );
    }

    println!("   ✅ All instances report consistent state");
    println!("🎉 State consistency test passed!");

    Ok(())
}

/// Test eventual consistency after concurrent modifications
///
/// Creates multiple tasks concurrently and verifies they all
/// reach consistent state across instances.
#[tokio::test]
async fn test_eventual_consistency_after_concurrent_operations() -> Result<()> {
    println!("\n🧪 Testing eventual consistency after concurrent operations...");

    let manager = MultiInstanceTestManager::setup_from_env().await?;
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    // Create several tasks concurrently
    let task_count = 5;
    let requests: Vec<_> = (0..task_count)
        .map(|i| {
            create_task_request(
                "rust_e2e_linear",
                "mathematical_sequence",
                json!({"even_number": i * 2 + 100}),
            )
        })
        .collect();

    println!("   Creating {} tasks concurrently...", task_count);
    let responses = manager.create_tasks_concurrent(requests).await?;

    let task_uuids: Vec<_> = responses
        .iter()
        .map(|r| uuid::Uuid::parse_str(&r.task_uuid).unwrap())
        .collect();

    // Wait for all to complete
    let timeout = Duration::from_secs(120);
    println!("   Waiting for all tasks to complete...");
    manager
        .wait_for_tasks_completion(task_uuids.clone(), timeout)
        .await?;

    // Small delay to ensure any pending state propagation
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify consistency for all tasks
    println!("   Verifying consistency for all {} tasks...", task_count);
    for (i, task_uuid) in task_uuids.iter().enumerate() {
        manager.verify_task_consistency(*task_uuid).await?;
        println!("      Task {}: consistent ✓", i + 1);
    }

    println!("   ✅ All tasks eventually consistent across instances");
    println!("🎉 Eventual consistency test passed!");

    Ok(())
}

/// Test read-after-write consistency
///
/// Verifies that immediately after creating a task through one instance,
/// it can be read through any other instance.
#[tokio::test]
async fn test_read_after_write_consistency() -> Result<()> {
    println!("\n🧪 Testing read-after-write consistency...");

    let manager = MultiInstanceTestManager::setup_from_env().await?;
    manager.wait_for_healthy(Duration::from_secs(30)).await?;

    let instance_count = manager.orchestration_count();
    if instance_count < 2 {
        println!("   ⚠️ Skipping: need at least 2 instances");
        return Ok(());
    }

    // Create a task (will go to first instance via round-robin)
    let request = create_task_request(
        "rust_e2e_linear",
        "mathematical_sequence",
        json!({"even_number": 256}),
    );

    println!("   Creating task through cluster...");
    let responses = manager.create_tasks_concurrent(vec![request]).await?;
    let task_uuid = uuid::Uuid::parse_str(&responses[0].task_uuid)?;

    println!("   Task created: {}", task_uuid);

    // Immediately read from ALL instances
    println!("   Immediately reading from all instances...");
    let clients = manager.cluster.all_clients();

    for (i, client) in clients.iter().enumerate() {
        match client.get_task(task_uuid).await {
            Ok(task) => {
                println!(
                    "      Instance {}: found task, status='{}'",
                    i + 1,
                    task.status
                );
            }
            Err(e) => {
                panic!(
                    "Instance {} could not read task immediately after creation: {}",
                    i + 1,
                    e
                );
            }
        }
    }

    println!("   ✅ Task readable from all instances immediately");
    println!("🎉 Read-after-write consistency test passed!");

    Ok(())
}
