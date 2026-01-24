//! DLQ (Dead Letter Queue) command handlers for the Tasker CLI

use tasker_client::{ClientConfig, ClientResult, OrchestrationApiClient, OrchestrationApiConfig};
use uuid::Uuid;

use crate::DlqCommands;

pub async fn handle_dlq_command(cmd: DlqCommands, config: &ClientConfig) -> ClientResult<()> {
    let orchestration_config = OrchestrationApiConfig {
        base_url: config.orchestration.base_url.clone(),
        timeout_ms: config.orchestration.timeout_ms,
        max_retries: config.orchestration.max_retries,
        auth: config.orchestration.resolve_web_auth_config(),
    };

    let client = OrchestrationApiClient::new(orchestration_config)?;

    match cmd {
        DlqCommands::List {
            status,
            limit,
            offset,
        } => {
            println!("Listing DLQ entries (limit: {}, offset: {})", limit, offset);

            // Parse status if provided
            let resolution_status = if let Some(status_str) = status {
                Some(match status_str.as_str() {
                    "pending" => tasker_shared::models::orchestration::DlqResolutionStatus::Pending,
                    "manually_resolved" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::ManuallyResolved
                    }
                    "permanently_failed" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::PermanentlyFailed
                    }
                    "cancelled" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::Cancelled
                    }
                    _ => {
                        eprintln!(
                            "✗ Invalid status '{}'. Valid: pending, manually_resolved, permanently_failed, cancelled",
                            status_str
                        );
                        return Err(tasker_client::ClientError::InvalidInput(format!(
                            "Invalid status: {}",
                            status_str
                        )));
                    }
                })
            } else {
                None
            };

            let params = tasker_shared::models::orchestration::DlqListParams {
                resolution_status,
                limit,
                offset,
            };

            match client.list_dlq_entries(Some(&params)).await {
                Ok(entries) => {
                    println!("✓ Found {} DLQ entries\n", entries.len());

                    for entry in entries {
                        println!("  • DLQ Entry: {}", entry.dlq_entry_uuid);
                        println!("    Task UUID: {}", entry.task_uuid);
                        println!("    Reason: {:?}", entry.dlq_reason);
                        println!("    Original state: {}", entry.original_state);
                        println!("    Resolution status: {:?}", entry.resolution_status);
                        println!("    DLQ timestamp: {}", entry.dlq_timestamp);
                        if let Some(ref notes) = entry.resolution_notes {
                            println!("    Notes: {}", notes);
                        }
                        if let Some(ref resolved_by) = entry.resolved_by {
                            println!("    Resolved by: {}", resolved_by);
                        }
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list DLQ entries: {}", e);
                    return Err(e.into());
                }
            }
        }
        DlqCommands::Get { task_uuid } => {
            println!("Getting DLQ entry for task: {}", task_uuid);

            let task_uuid = Uuid::parse_str(&task_uuid).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid UUID: {}", e))
            })?;

            match client.get_dlq_entry(task_uuid).await {
                Ok(entry) => {
                    println!("✓ DLQ Entry Details:\n");
                    println!("  DLQ Entry UUID: {}", entry.dlq_entry_uuid);
                    println!("  Task UUID: {}", entry.task_uuid);
                    println!("  Reason: {:?}", entry.dlq_reason);
                    println!("  Original state: {}", entry.original_state);
                    println!("  Resolution status: {:?}", entry.resolution_status);
                    println!("  DLQ timestamp: {}", entry.dlq_timestamp);

                    if let Some(resolution_ts) = entry.resolution_timestamp {
                        println!("  Resolution timestamp: {}", resolution_ts);
                    }
                    if let Some(ref notes) = entry.resolution_notes {
                        println!("  Resolution notes: {}", notes);
                    }
                    if let Some(ref resolved_by) = entry.resolved_by {
                        println!("  Resolved by: {}", resolved_by);
                    }

                    println!("\n  Task Snapshot:");
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&entry.task_snapshot).unwrap()
                    );
                }
                Err(e) => {
                    eprintln!("✗ Failed to get DLQ entry: {}", e);
                    return Err(e.into());
                }
            }
        }
        DlqCommands::Update {
            dlq_entry_uuid,
            status,
            notes,
            resolved_by,
        } => {
            println!("Updating DLQ investigation: {}", dlq_entry_uuid);

            let dlq_entry_uuid = Uuid::parse_str(&dlq_entry_uuid).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid UUID: {}", e))
            })?;

            // Parse status if provided
            let resolution_status = if let Some(status_str) = status {
                Some(match status_str.as_str() {
                    "pending" => tasker_shared::models::orchestration::DlqResolutionStatus::Pending,
                    "manually_resolved" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::ManuallyResolved
                    }
                    "permanently_failed" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::PermanentlyFailed
                    }
                    "cancelled" => {
                        tasker_shared::models::orchestration::DlqResolutionStatus::Cancelled
                    }
                    _ => {
                        eprintln!(
                            "✗ Invalid status '{}'. Valid: pending, manually_resolved, permanently_failed, cancelled",
                            status_str
                        );
                        return Err(tasker_client::ClientError::InvalidInput(format!(
                            "Invalid status: {}",
                            status_str
                        )));
                    }
                })
            } else {
                None
            };

            let update = tasker_shared::models::orchestration::DlqInvestigationUpdate {
                resolution_status,
                resolution_notes: notes,
                resolved_by,
                metadata: None,
            };

            match client
                .update_dlq_investigation(dlq_entry_uuid, update)
                .await
            {
                Ok(()) => {
                    println!("✓ DLQ investigation updated successfully");
                }
                Err(e) => {
                    eprintln!("✗ Failed to update DLQ investigation: {}", e);
                    return Err(e.into());
                }
            }
        }
        DlqCommands::Stats => {
            println!("Getting DLQ statistics...\n");

            match client.get_dlq_stats().await {
                Ok(stats) => {
                    println!("✓ DLQ Statistics by Reason:\n");

                    for stat in stats {
                        println!("  Reason: {:?}", stat.dlq_reason);
                        println!("    Total entries: {}", stat.total_entries);
                        println!("    Pending: {}", stat.pending);
                        println!("    Manually resolved: {}", stat.manually_resolved);
                        println!("    Permanent failures: {}", stat.permanent_failures);

                        if let Some(oldest) = stat.oldest_entry {
                            println!("    Oldest entry: {}", oldest);
                        }
                        if let Some(newest) = stat.newest_entry {
                            println!("    Newest entry: {}", newest);
                        }
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to get DLQ statistics: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    Ok(())
}
