//! Task command handlers for the Tasker CLI

use std::str::FromStr;

use tasker_client::{ClientConfig, ClientResult, OrchestrationApiClient, OrchestrationApiConfig};
use tasker_shared::models::core::{task::TaskListQuery, task_request::TaskRequest};
use tasker_shared::types::api::orchestration::{ManualCompletionData, StepManualAction};
use uuid::Uuid;

use crate::TaskCommands;

pub async fn handle_task_command(cmd: TaskCommands, config: &ClientConfig) -> ClientResult<()> {
    let orchestration_config = OrchestrationApiConfig {
        base_url: config.orchestration.base_url.clone(),
        timeout_ms: config.orchestration.timeout_ms,
        max_retries: config.orchestration.max_retries,
        auth: None, // TODO: Convert auth_token to WebAuthConfig when needed
    };

    let client = OrchestrationApiClient::new(orchestration_config)?;

    match cmd {
        TaskCommands::Create {
            namespace,
            name,
            version,
            input,
            description: _,
            priority,
            correlation_id,
        } => {
            println!("Creating task: {}/{} v{}", namespace, name, version);

            // Parse input as JSON
            let context: serde_json::Value = serde_json::from_str(&input).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid JSON input: {}", e))
            })?;

            let correlation_id = correlation_id.unwrap_or_else(|| Uuid::now_v7().to_string());
            let correlation_id = Uuid::from_str(correlation_id.as_str()).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid correlation ID: {}", e))
            })?;

            let task_request = TaskRequest {
                namespace,
                name,
                version,
                context,
                initiator: "tasker-cli".to_string(),
                source_system: "cli".to_string(),
                reason: "CLI task creation".to_string(),
                tags: Vec::new(),

                requested_at: chrono::Utc::now().naive_utc(),
                options: None,
                priority: Some(priority as i32),
                correlation_id,
                parent_correlation_id: None,
            };

            match client.create_task(task_request).await {
                Ok(response) => {
                    println!("✓ Task created successfully!");
                    println!("  Task UUID: {}", response.task_uuid);
                    println!("  Status: {}", response.status);
                    println!("  Steps: {}", response.step_count);
                    println!("  Created at: {}", response.created_at);
                    if let Some(completion) = response.estimated_completion {
                        println!("  Estimated completion: {}", completion);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to create task: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::Get { task_id } => {
            println!("Getting task: {}", task_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid UUID: {}", e))
            })?;

            match client.get_task(task_uuid).await {
                Ok(response) => {
                    println!("✓ Task details:");
                    println!("  UUID: {}", response.task_uuid);
                    println!("  Name: {}", response.name);
                    println!("  Namespace: {}", response.namespace);
                    println!("  Version: {}", response.version);
                    println!("  Status: {}", response.status);
                    if let Some(priority) = response.priority {
                        println!("  Priority: {}", priority);
                    }
                    println!("  Created: {}", response.created_at);
                    println!("  Updated: {}", response.updated_at);
                    if let Some(completed) = response.completed_at {
                        println!("  Completed: {}", completed);
                    }
                    println!(
                        "  Steps: {}/{} completed",
                        response.completed_steps, response.total_steps
                    );
                    println!("  Progress: {:.1}%", response.completion_percentage);
                    println!("  Health: {}", response.health_status);
                    println!("  Recommended action: {}", response.recommended_action);
                    println!("  Correlation ID: {}", response.correlation_id);
                }
                Err(e) => {
                    eprintln!("✗ Failed to get task: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::List {
            namespace,
            status,
            limit,
        } => {
            println!("Listing tasks (limit: {})", limit);

            let query = TaskListQuery {
                page: 1,
                per_page: limit,
                namespace,
                status,
                initiator: None,
                source_system: None,
            };

            match client.list_tasks(&query).await {
                Ok(response) => {
                    println!(
                        "✓ Found {} tasks (page {} of {})",
                        response.tasks.len(),
                        response.pagination.page,
                        response.pagination.total_pages
                    );
                    println!("  Total: {} tasks\n", response.pagination.total_count);

                    for task in response.tasks {
                        println!(
                            "  • {} - {}/{} v{}",
                            task.task_uuid, task.namespace, task.name, task.version
                        );
                        println!(
                            "    Status: {} | Progress: {:.1}% | Health: {}",
                            task.status, task.completion_percentage, task.health_status
                        );
                        println!(
                            "    Created: {} | Steps: {}/{}",
                            task.created_at, task.completed_steps, task.total_steps
                        );
                        if let Some(ref tags) = task.tags {
                            if !tags.is_empty() {
                                println!("    Tags: {}", tags.join(", "));
                            }
                        }
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list tasks: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::Cancel { task_id } => {
            println!("Canceling task: {}", task_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid UUID: {}", e))
            })?;

            match client.cancel_task(task_uuid).await {
                Ok(()) => {
                    println!("✓ Task {} has been canceled successfully", task_id);
                }
                Err(e) => {
                    eprintln!("✗ Failed to cancel task: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::Steps { task_id } => {
            println!("Listing workflow steps for task: {}", task_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid UUID: {}", e))
            })?;

            match client.list_task_steps(task_uuid).await {
                Ok(steps) => {
                    println!("\n✓ Found {} workflow steps:\n", steps.len());
                    for step in steps {
                        println!("  Step: {} ({})", step.name, step.step_uuid);
                        println!("    State: {}", step.current_state);
                        println!(
                            "    Dependencies satisfied: {}",
                            step.dependencies_satisfied
                        );
                        println!("    Ready for execution: {}", step.ready_for_execution);
                        println!("    Attempts: {}/{}", step.attempts, step.max_attempts);
                        if step.retry_eligible {
                            println!("    ⚠ Retry eligible");
                        }
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list steps: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::Step { task_id, step_id } => {
            println!("Getting step details: {} for task: {}", step_id, task_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid task UUID: {}", e))
            })?;

            let step_uuid = Uuid::parse_str(&step_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid step UUID: {}", e))
            })?;

            match client.get_step(task_uuid, step_uuid).await {
                Ok(step) => {
                    println!("\n✓ Step Details:\n");
                    println!("  UUID: {}", step.step_uuid);
                    println!("  Name: {}", step.name);
                    println!("  State: {}", step.current_state);
                    println!("  Dependencies satisfied: {}", step.dependencies_satisfied);
                    println!("  Ready for execution: {}", step.ready_for_execution);
                    println!("  Retry eligible: {}", step.retry_eligible);
                    println!("  Attempts: {}/{}", step.attempts, step.max_attempts);
                    if let Some(last_failure) = step.last_failure_at {
                        println!("  Last failure: {}", last_failure);
                    }
                    if let Some(next_retry) = step.next_retry_at {
                        println!("  Next retry: {}", next_retry);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to get step: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::ResetStep {
            task_id,
            step_id,
            reason,
            reset_by,
        } => {
            println!("Resetting step {} for retry...", step_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid task UUID: {}", e))
            })?;

            let step_uuid = Uuid::parse_str(&step_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid step UUID: {}", e))
            })?;

            let action = StepManualAction::ResetForRetry {
                reason: reason.clone(),
                reset_by: reset_by.clone(),
            };

            match client
                .resolve_step_manually(task_uuid, step_uuid, action)
                .await
            {
                Ok(step) => {
                    println!("\n✓ Step reset successfully!");
                    println!("  New state: {}", step.current_state);
                    println!("  Reason: {}", reason);
                    println!("  Reset by: {}", reset_by);
                }
                Err(e) => {
                    eprintln!("✗ Failed to reset step: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::ResolveStep {
            task_id,
            step_id,
            reason,
            resolved_by,
        } => {
            println!("Manually resolving step {}...", step_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid task UUID: {}", e))
            })?;

            let step_uuid = Uuid::parse_str(&step_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid step UUID: {}", e))
            })?;

            let action = StepManualAction::ResolveManually {
                reason: reason.clone(),
                resolved_by: resolved_by.clone(),
            };

            match client
                .resolve_step_manually(task_uuid, step_uuid, action)
                .await
            {
                Ok(step) => {
                    println!("\n✓ Step resolved manually!");
                    println!("  New state: {}", step.current_state);
                    println!("  Reason: {}", reason);
                    println!("  Resolved by: {}", resolved_by);
                }
                Err(e) => {
                    eprintln!("✗ Failed to resolve step: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::CompleteStep {
            task_id,
            step_id,
            result,
            metadata,
            reason,
            completed_by,
        } => {
            println!("Manually completing step {} with results...", step_id);

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid task UUID: {}", e))
            })?;

            let step_uuid = Uuid::parse_str(&step_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid step UUID: {}", e))
            })?;

            // Parse result JSON
            let result_value: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid result JSON: {}", e))
            })?;

            // Parse optional metadata JSON
            let metadata_value: Option<serde_json::Value> = if let Some(meta) = metadata {
                Some(serde_json::from_str(&meta).map_err(|e| {
                    tasker_client::ClientError::InvalidInput(format!(
                        "Invalid metadata JSON: {}",
                        e
                    ))
                })?)
            } else {
                None
            };

            let completion_data = ManualCompletionData {
                result: result_value,
                metadata: metadata_value,
            };

            let action = StepManualAction::CompleteManually {
                completion_data,
                reason: reason.clone(),
                completed_by: completed_by.clone(),
            };

            match client
                .resolve_step_manually(task_uuid, step_uuid, action)
                .await
            {
                Ok(step) => {
                    println!("\n✓ Step completed manually with results!");
                    println!("  New state: {}", step.current_state);
                    println!("  Reason: {}", reason);
                    println!("  Completed by: {}", completed_by);
                }
                Err(e) => {
                    eprintln!("✗ Failed to complete step: {}", e);
                    return Err(e.into());
                }
            }
        }
        TaskCommands::StepAudit { task_id, step_id } => {
            println!(
                "Getting audit history for step: {} (task: {})",
                step_id, task_id
            );

            let task_uuid = Uuid::parse_str(&task_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid task UUID: {}", e))
            })?;

            let step_uuid = Uuid::parse_str(&step_id).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid step UUID: {}", e))
            })?;

            match client.get_step_audit_history(task_uuid, step_uuid).await {
                Ok(audit_records) => {
                    if audit_records.is_empty() {
                        println!("\n⚠ No audit records found for this step");
                        println!(
                            "  Note: Audit records are created when step results are persisted"
                        );
                    } else {
                        println!("\n✓ Found {} audit record(s):\n", audit_records.len());
                        for (i, audit) in audit_records.iter().enumerate() {
                            println!("  Audit Record #{}", i + 1);
                            println!("    UUID: {}", audit.audit_uuid);
                            println!(
                                "    Step: {} ({})",
                                audit.step_name, audit.workflow_step_uuid
                            );
                            println!(
                                "    Success: {}",
                                if audit.success { "✓ Yes" } else { "✗ No" }
                            );
                            println!("    Recorded at: {}", audit.recorded_at);
                            println!(
                                "    Transition: {} → {}",
                                audit.from_state.as_deref().unwrap_or("(none)"),
                                audit.to_state
                            );
                            if let Some(ref worker) = audit.worker_uuid {
                                println!("    Worker UUID: {}", worker);
                            }
                            if let Some(ref correlation) = audit.correlation_id {
                                println!("    Correlation ID: {}", correlation);
                            }
                            if let Some(time_ms) = audit.execution_time_ms {
                                println!("    Execution time: {}ms", time_ms);
                            }
                            println!();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to get audit history: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}
