//! Helper functions for integration tests
//!

use anyhow::Result;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

use tasker_client::OrchestrationApiClient;
use tasker_shared::models::core::{task::TaskListQuery, task_request::TaskRequest};
use tasker_shared::models::orchestration::execution_status::ExecutionStatus;

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
        status: "PENDING".to_string(),
        initiator: "integration-test".to_string(),
        source_system: "test".to_string(),
        reason: "Integration test execution".to_string(),
        complete: false,
        tags: vec!["integration-test".to_string()],
        bypass_steps: vec![],
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
