//! Worker command handlers for the Tasker CLI

use tasker_client::{ClientConfig, ClientResult, WorkerApiClient, WorkerApiConfig};

use crate::WorkerCommands;

pub async fn handle_worker_command(cmd: WorkerCommands, config: &ClientConfig) -> ClientResult<()> {
    let worker_config = WorkerApiConfig {
        base_url: config.worker.base_url.clone(),
        timeout_ms: config.worker.timeout_ms,
        max_retries: config.worker.max_retries,
        auth: None, // TODO: Convert auth_token to WebAuthConfig when needed
    };

    let client = WorkerApiClient::new(worker_config)?;

    match cmd {
        WorkerCommands::List { namespace } => {
            println!("Listing workers");
            if let Some(ref ns) = namespace {
                println!("Namespace filter: {}", ns);
            }

            match client.list_workers(namespace.as_deref()).await {
                Ok(response) => {
                    println!(
                        "✓ Found {} workers (total: {}, active: {})",
                        response.workers.len(),
                        response.total_count,
                        response.active_count
                    );
                    println!("  Timestamp: {}\n", response.timestamp);

                    for worker in response.workers {
                        println!("  • Worker: {}", worker.worker_id);
                        println!(
                            "    Status: {} | Version: {}",
                            worker.status, worker.version
                        );
                        println!("    Namespaces: {}", worker.namespaces.join(", "));
                        println!("    Last seen: {}", worker.last_seen);
                        println!();
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to list workers: {}", e);
                    return Err(e.into());
                }
            }
        }
        WorkerCommands::Status { worker_id } => {
            println!("Getting worker status: {}", worker_id);

            match client.get_worker_status(&worker_id).await {
                Ok(response) => {
                    println!("✓ Worker status:");
                    println!("  Worker ID: {}", response.worker_id);
                    println!("  Status: {}", response.status);
                    println!("  Version: {} ({})", response.version, response.environment);
                    println!("  Uptime: {} seconds", response.uptime_seconds);
                    println!("  Namespaces: {}", response.namespaces.join(", "));
                    println!(
                        "  Tasks: {} current, {} completed, {} failed",
                        response.current_tasks, response.completed_tasks, response.failed_tasks
                    );
                    if let Some(last_activity) = response.last_activity {
                        println!("  Last activity: {}", last_activity);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to get worker status: {}", e);
                    return Err(e.into());
                }
            }
        }
        WorkerCommands::Health { all, worker_id } => {
            if all {
                println!("Checking health of all workers");
                // Get all workers first, then check health of each
                match client.list_workers(None).await {
                    Ok(worker_list) => {
                        println!(
                            "✓ Checking health of {} workers\n",
                            worker_list.workers.len()
                        );

                        for worker in worker_list.workers {
                            println!("Worker: {}", worker.worker_id);
                            match client.worker_health(&worker.worker_id).await {
                                Ok(health) => {
                                    println!(
                                        "  ✓ Status: {} | Version: {}",
                                        health.status, health.system_info.version
                                    );
                                    println!(
                                        "  Environment: {} | Uptime: {}s",
                                        health.system_info.environment,
                                        health.system_info.uptime_seconds
                                    );
                                    println!(
                                        "  Worker type: {} | DB pool: {}",
                                        health.system_info.worker_type,
                                        health.system_info.database_pool_size
                                    );
                                    println!(
                                        "  Command processor: {} | Namespaces: {}",
                                        health.system_info.command_processor_active,
                                        health.system_info.supported_namespaces.join(", ")
                                    );

                                    // Show individual health checks
                                    if !health.checks.is_empty() {
                                        println!("  Health checks:");
                                        for (check_name, check_result) in &health.checks {
                                            let status_icon = if check_result.status == "healthy" {
                                                "✓"
                                            } else {
                                                "✗"
                                            };
                                            println!(
                                                "    {} {}: {} ({}ms)",
                                                status_icon,
                                                check_name,
                                                check_result.status,
                                                check_result.duration_ms
                                            );
                                            if let Some(ref msg) = check_result.message {
                                                println!("      {}", msg);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("  ✗ Health check failed: {}", e);
                                }
                            }
                            println!();
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to get worker list: {}", e);
                        return Err(e.into());
                    }
                }
            } else if let Some(id) = worker_id {
                println!("Checking health of worker: {}", id);

                match client.worker_health(&id).await {
                    Ok(response) => {
                        println!("✓ Worker health:");
                        println!("  Worker ID: {}", response.worker_id);
                        println!(
                            "  Status: {} | Timestamp: {}",
                            response.status, response.timestamp
                        );

                        println!("  System info:");
                        println!(
                            "    Version: {} | Environment: {}",
                            response.system_info.version, response.system_info.environment
                        );
                        println!(
                            "    Uptime: {} seconds | Worker type: {}",
                            response.system_info.uptime_seconds, response.system_info.worker_type
                        );
                        println!(
                            "    DB pool size: {} | Command processor: {}",
                            response.system_info.database_pool_size,
                            response.system_info.command_processor_active
                        );
                        println!(
                            "    Supported namespaces: {}",
                            response.system_info.supported_namespaces.join(", ")
                        );

                        if !response.checks.is_empty() {
                            println!("  Health checks:");
                            for (check_name, check_result) in response.checks {
                                let status_icon = if check_result.status == "healthy" {
                                    "✓"
                                } else {
                                    "✗"
                                };
                                println!(
                                    "    {} {}: {} ({}ms) - last checked: {}",
                                    status_icon,
                                    check_name,
                                    check_result.status,
                                    check_result.duration_ms,
                                    check_result.last_checked
                                );
                                if let Some(message) = check_result.message {
                                    println!("      {}", message);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to check worker health: {}", e);
                        return Err(e.into());
                    }
                }
            } else {
                eprintln!("✗ Please specify either --all or provide a worker ID");
                return Err(tasker_client::ClientError::InvalidInput(
                    "Either --all flag or worker ID required".to_string(),
                ));
            }
        }
    }
    Ok(())
}
