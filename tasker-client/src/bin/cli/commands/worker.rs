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
            // List templates instead of workers (workers don't have a registry)
            println!("Listing worker templates and capabilities");
            if let Some(ref ns) = namespace {
                println!("Namespace filter: {}", ns);
            }

            match client.list_templates(None).await {
                Ok(response) => {
                    println!("✓ Worker service information:");
                    println!(
                        "  Supported namespaces: {}",
                        response.supported_namespaces.join(", ")
                    );
                    println!("  Cached templates: {}", response.template_count);
                    println!(
                        "  Worker capabilities: {}",
                        response.worker_capabilities.join(", ")
                    );

                    if let Some(cache_stats) = response.cache_stats {
                        println!("\n  Cache statistics:");
                        println!("    Total cached: {}", cache_stats.total_cached);
                        println!("    Cache hits: {}", cache_stats.cache_hits);
                        println!("    Cache misses: {}", cache_stats.cache_misses);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to get worker info: {}", e);
                    return Err(e.into());
                }
            }
        }
        WorkerCommands::Status { worker_id: _ } => {
            // Get worker detailed health status (worker_id is ignored - single worker per service)
            println!("Getting worker status...");

            match client.get_detailed_health().await {
                Ok(response) => {
                    println!("✓ Worker status:");
                    println!("  Worker ID: {}", response.worker_id);
                    println!("  Status: {}", response.status);
                    println!(
                        "  Version: {} ({})",
                        response.system_info.version, response.system_info.environment
                    );
                    println!("  Uptime: {} seconds", response.system_info.uptime_seconds);
                    println!("  Worker type: {}", response.system_info.worker_type);
                    println!(
                        "  DB pool size: {}",
                        response.system_info.database_pool_size
                    );
                    println!(
                        "  Command processor: {}",
                        if response.system_info.command_processor_active {
                            "active"
                        } else {
                            "inactive"
                        }
                    );
                    println!(
                        "  Namespaces: {}",
                        response.system_info.supported_namespaces.join(", ")
                    );

                    if !response.checks.is_empty() {
                        println!("\n  Health checks:");
                        for (check_name, check_result) in &response.checks {
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
                    eprintln!("✗ Failed to get worker status: {}", e);
                    return Err(e.into());
                }
            }
        }
        WorkerCommands::Health {
            all: _,
            worker_id: _,
        } => {
            // Check worker health (--all and worker_id ignored - single worker per service)
            println!("Checking worker health...");

            // Basic health check first
            match client.health_check().await {
                Ok(basic) => {
                    println!("✓ Worker basic health: {}", basic.status);
                    println!("  Worker ID: {}", basic.worker_id);
                }
                Err(e) => {
                    eprintln!("✗ Worker health check failed: {}", e);
                    return Err(e.into());
                }
            }

            // Detailed health check
            match client.get_detailed_health().await {
                Ok(response) => {
                    println!("\n✓ Detailed worker health:");
                    println!(
                        "  Status: {} | Timestamp: {}",
                        response.status, response.timestamp
                    );

                    println!("\n  System info:");
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
                        println!("\n  Health checks:");
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
                    eprintln!("  Could not get detailed health info: {}", e);
                }
            }
        }
    }
    Ok(())
}
