//! System command handlers for the Tasker CLI

use tasker_client::{
    ClientConfig, ClientResult, OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient,
    WorkerApiConfig,
};

use crate::SystemCommands;

pub async fn handle_system_command(cmd: SystemCommands, config: &ClientConfig) -> ClientResult<()> {
    match cmd {
        SystemCommands::Health {
            orchestration,
            workers,
        } => {
            if orchestration || !workers {
                println!("Checking orchestration health...");

                let orchestration_config = OrchestrationApiConfig {
                    base_url: config.orchestration.base_url.clone(),
                    timeout_ms: config.orchestration.timeout_ms,
                    max_retries: config.orchestration.max_retries,
                    auth: config.orchestration.resolve_web_auth_config(),
                };

                let orch_client = OrchestrationApiClient::new(orchestration_config)?;

                // Check basic health
                match orch_client.get_basic_health().await {
                    Ok(health) => {
                        println!("  ✓ Orchestration service is healthy: {}", health.status);
                    }
                    Err(e) => {
                        println!("  ✗ Orchestration service health check failed: {}", e);
                    }
                }

                // Get detailed health if available
                match orch_client.get_detailed_health().await {
                    Ok(detailed) => {
                        println!("  ✓ Detailed orchestration health:");
                        println!(
                            "    Status: {} | Environment: {} | Version: {}",
                            detailed.status, detailed.info.environment, detailed.info.version
                        );
                        println!(
                            "    Operational state: {} | Circuit breaker: {}",
                            detailed.info.operational_state, detailed.info.circuit_breaker_state
                        );
                        println!(
                            "    DB pools - Web: {}, Orchestration: {}",
                            detailed.info.web_database_pool_size,
                            detailed.info.orchestration_database_pool_size
                        );

                        if !detailed.checks.is_empty() {
                            println!("    Health checks:");
                            for (check_name, check_result) in detailed.checks {
                                let status_icon = if check_result.status == "healthy" {
                                    "✓"
                                } else {
                                    "✗"
                                };
                                println!(
                                    "      {} {}: {} ({}ms)",
                                    status_icon,
                                    check_name,
                                    check_result.status,
                                    check_result.duration_ms
                                );
                                if let Some(message) = check_result.message {
                                    println!("        {}", message);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("  Could not get detailed health info: {}", e);
                    }
                }
            }

            if workers || !orchestration {
                println!("\nChecking worker health...");

                let worker_config = WorkerApiConfig {
                    base_url: config.worker.base_url.clone(),
                    timeout_ms: config.worker.timeout_ms,
                    max_retries: config.worker.max_retries,
                    auth: config.worker.resolve_web_auth_config(),
                };

                let worker_client = WorkerApiClient::new(worker_config)?;

                // Check basic worker service health
                match worker_client.health_check().await {
                    Ok(health) => {
                        println!("  ✓ Worker service is healthy: {}", health.status);
                        println!("    Worker ID: {}", health.worker_id);
                    }
                    Err(e) => {
                        println!("  ✗ Worker service health check failed: {}", e);
                    }
                }

                // Get detailed worker health
                match worker_client.get_detailed_health().await {
                    Ok(health) => {
                        println!("  ✓ Worker detailed health:");
                        println!(
                            "    Status: {} | Version: {} | Uptime: {}s",
                            health.status,
                            health.system_info.version,
                            health.system_info.uptime_seconds
                        );
                        println!(
                            "    Worker type: {} | Environment: {}",
                            health.system_info.worker_type, health.system_info.environment
                        );
                        println!(
                            "    Namespaces: {}",
                            health.system_info.supported_namespaces.join(", ")
                        );

                        if !health.checks.is_empty() {
                            println!("    Health checks:");
                            for (check_name, check_result) in &health.checks {
                                let status_icon = if check_result.status == "healthy" {
                                    "✓"
                                } else {
                                    "✗"
                                };
                                println!(
                                    "      {} {}: {} ({}ms)",
                                    status_icon,
                                    check_name,
                                    check_result.status,
                                    check_result.duration_ms
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("  ✗ Could not get detailed worker health: {}", e);
                    }
                }
            }

            if !orchestration && !workers {
                println!(
                    "\nOverall system health: Both orchestration and worker services checked above"
                );
            }
        }
        SystemCommands::Info => {
            println!("Tasker System Information");
            println!("================================");
            println!("CLI Version: {}", env!("CARGO_PKG_VERSION"));
            println!("Build Target: {}", std::env::consts::ARCH);
            println!();

            println!("Configuration:");
            println!("  Orchestration API: {}", config.orchestration.base_url);
            println!("  Worker API: {}", config.worker.base_url);
            println!("  Request timeout: {}ms", config.orchestration.timeout_ms);
            println!("  Max retries: {}", config.orchestration.max_retries);
            println!();

            // Try to get version info from services
            println!("🔗 Service Information:");

            // Orchestration service info
            let orchestration_config = OrchestrationApiConfig {
                base_url: config.orchestration.base_url.clone(),
                timeout_ms: config.orchestration.timeout_ms,
                max_retries: config.orchestration.max_retries,
                auth: config.orchestration.resolve_web_auth_config(),
            };

            if let Ok(orch_client) = OrchestrationApiClient::new(orchestration_config) {
                match orch_client.get_detailed_health().await {
                    Ok(health) => {
                        println!(
                            "  Orchestration: {} v{} ({})",
                            health.status, health.info.version, health.info.environment
                        );
                        println!("    Operational state: {}", health.info.operational_state);
                        println!(
                            "    Database pools: Web={}, Orch={}",
                            health.info.web_database_pool_size,
                            health.info.orchestration_database_pool_size
                        );
                    }
                    Err(_) => {
                        println!("  Orchestration: Unable to retrieve service info");
                    }
                }
            } else {
                println!("  Orchestration: Configuration error");
            }

            // Worker service info
            let worker_config = WorkerApiConfig {
                base_url: config.worker.base_url.clone(),
                timeout_ms: config.worker.timeout_ms,
                max_retries: config.worker.max_retries,
                auth: config.worker.resolve_web_auth_config(),
            };

            if let Ok(worker_client) = WorkerApiClient::new(worker_config) {
                match worker_client.get_detailed_health().await {
                    Ok(health) => {
                        println!(
                            "  Worker: {} v{} ({})",
                            health.status,
                            health.system_info.version,
                            health.system_info.environment
                        );
                        println!(
                            "    Worker type: {} | Uptime: {}s",
                            health.system_info.worker_type, health.system_info.uptime_seconds
                        );
                        println!(
                            "    Supported namespaces: {}",
                            health.system_info.supported_namespaces.join(", ")
                        );
                    }
                    Err(_) => {
                        println!("  Worker: Unable to retrieve worker info");
                    }
                }
            } else {
                println!("  Worker: Configuration error");
            }
        }
    }
    Ok(())
}
