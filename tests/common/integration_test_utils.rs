//! Helper functions for integration tests
//!

#![allow(dead_code)]

use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use tasker_client::OrchestrationApiClient;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::models::orchestration::execution_status::ExecutionStatus;

/// Get appropriate timeout for task completion based on environment
///
/// In CI environments (GitHub Actions, etc.), tasks take longer due to:
/// - Shared CPU resources
/// - All services running natively (not isolated in Docker)
/// - Higher contention for database connections
///
/// Returns:
/// - 15 seconds in CI (detected via CI environment variable)
/// - 5 seconds locally (fast feedback for development)
pub fn get_task_completion_timeout() -> u64 {
    if std::env::var("CI").is_ok() {
        15 // CI environment - allow more time for shared resources
    } else {
        5 // Local development - keep fast feedback
    }
}

/// Helper to create a TaskRequest matching CLI usage
pub fn create_task_request(
    namespace: &str,
    name: &str,
    input_context: serde_json::Value,
) -> TaskRequest {
    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context: input_context,
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        initiator: "integration-test".to_string(),
        source_system: "test".to_string(),
        reason: "Integration test execution".to_string(),
        tags: vec!["integration-test".to_string()],

        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
    }
}

/// Wait for task completion by polling task status
pub async fn wait_for_task_completion(
    client: &OrchestrationApiClient,
    task_uuid: &str,
    max_wait_seconds: u64,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let max_duration = Duration::from_secs(max_wait_seconds);

    println!(
        "⏳ Waiting for task {} to complete (max {}s)...",
        task_uuid, max_wait_seconds
    );

    while start_time.elapsed() < max_duration {
        let uuid = Uuid::parse_str(task_uuid)?;
        match client.get_task(uuid).await {
            Ok(task_response) => {
                let execution_status = task_response.execution_status_typed();
                println!(
                    "   Task execution status: {} ({})",
                    task_response.execution_status, task_response.status
                );

                match execution_status {
                    ExecutionStatus::AllComplete => {
                        println!("✅ Task completed successfully!");
                        return Ok(());
                    }
                    ExecutionStatus::BlockedByFailures => {
                        return Err(anyhow::anyhow!(
                            "Task blocked by failures that cannot be retried: {}",
                            task_response.execution_status
                        ));
                    }
                    ExecutionStatus::HasReadySteps
                    | ExecutionStatus::Processing
                    | ExecutionStatus::WaitingForDependencies => {
                        // Still processing, continue polling
                        sleep(Duration::from_secs(2)).await;
                    }
                }
            }
            Err(e) => {
                println!("   Error polling task status: {}", e);
                sleep(Duration::from_secs(2)).await;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Task did not complete within {}s",
        max_wait_seconds
    ))
}

/// Wait for task to fail (reach error or blocked state)
///
/// This is useful for testing error scenarios where we expect the task to fail.
/// Returns Ok(()) when task reaches BlockedByFailures state.
pub async fn wait_for_task_failure(
    client: &OrchestrationApiClient,
    task_uuid: &str,
    max_wait_seconds: u64,
) -> Result<()> {
    let start_time = std::time::Instant::now();
    let max_duration = Duration::from_secs(max_wait_seconds);

    println!(
        "⏳ Waiting for task {} to fail (max {}s)...",
        task_uuid, max_wait_seconds
    );

    while start_time.elapsed() < max_duration {
        let uuid = Uuid::parse_str(task_uuid)?;
        match client.get_task(uuid).await {
            Ok(task_response) => {
                let execution_status = task_response.execution_status_typed();
                println!(
                    "   Task execution status: {} ({})",
                    task_response.execution_status, task_response.status
                );

                match execution_status {
                    ExecutionStatus::BlockedByFailures => {
                        println!("✅ Task failed as expected (blocked by failures)!");
                        return Ok(());
                    }
                    ExecutionStatus::AllComplete => {
                        return Err(anyhow::anyhow!(
                            "Task completed successfully but was expected to fail"
                        ));
                    }
                    ExecutionStatus::HasReadySteps
                    | ExecutionStatus::Processing
                    | ExecutionStatus::WaitingForDependencies => {
                        // Still processing, continue polling
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            Err(e) => {
                println!("   Error polling task status: {}", e);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Task did not fail within {}s",
        max_wait_seconds
    ))
}
