//! # Tasker CLI Tool
//!
//! Command-line interface for interacting with Tasker orchestration and worker APIs.
//! Provides task management, worker monitoring, and system health checking capabilities.

use clap::{Parser, Subcommand};
use tasker_client::{
    ClientConfig, ClientResult, OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient,
    WorkerApiConfig,
};
use tasker_shared::models::core::{task::TaskListQuery, task_request::TaskRequest};
use tracing::info;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "tasker-cli")]
#[command(about = "Command-line interface for Tasker orchestration system")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Configuration file path (default: ~/.tasker/config.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Verbose output level (use multiple times for more verbosity)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Output format
    #[arg(long, default_value = "table")]
    format: String,

    /// Subcommands
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Task management operations
    #[command(subcommand)]
    Task(TaskCommands),

    /// Worker management operations
    #[command(subcommand)]
    Worker(WorkerCommands),

    /// System-level operations
    #[command(subcommand)]
    System(SystemCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// Create a new task
    Create {
        /// Task namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,
        /// Task name
        #[arg(long)]
        name: String,
        /// Task version (default: 1.0.0)
        #[arg(short, long, default_value = "1.0.0")]
        version: String,
        /// Task context as JSON string
        #[arg(short, long)]
        input: String,
        /// Task description
        #[arg(short, long)]
        description: Option<String>,
        /// Priority (1-10, default: 5)
        #[arg(short, long, default_value = "5")]
        priority: u8,
    },
    /// Get task details by UUID
    Get {
        /// Task UUID
        #[arg(value_name = "UUID")]
        task_id: String,
    },
    /// List tasks with optional filters
    List {
        /// Filter by namespace
        #[arg(short, long)]
        namespace: Option<String>,
        /// Filter by status
        #[arg(short, long)]
        status: Option<String>,
        /// Limit number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
    },
    /// Cancel a task
    Cancel {
        /// Task UUID to cancel
        #[arg(value_name = "UUID")]
        task_id: String,
    },
}

#[derive(Subcommand)]
pub enum WorkerCommands {
    /// List workers
    List {
        /// Filter by namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },
    /// Get worker status
    Status {
        /// Worker ID
        #[arg(value_name = "WORKER_ID")]
        worker_id: String,
    },
    /// Check worker health
    Health {
        /// Check all workers
        #[arg(short, long)]
        all: bool,
        /// Specific worker ID
        #[arg(value_name = "WORKER_ID")]
        worker_id: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum SystemCommands {
    /// System health check
    Health {
        /// Check orchestration health
        #[arg(short, long)]
        orchestration: bool,
        /// Check workers health
        #[arg(short, long)]
        workers: bool,
    },
    /// System information
    Info,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Generate default configuration file
    Init {
        /// Configuration file path
        #[arg(short, long)]
        path: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ClientResult<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity level
    let log_level = match cli.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    // Load configuration
    let config = if let Some(config_path) = cli.config {
        ClientConfig::load_from_file(std::path::Path::new(&config_path))?
    } else {
        ClientConfig::load()?
    };

    info!("Tasker CLI starting with configuration: {:?}", config);

    // Execute command
    match cli.command {
        Commands::Task(task_cmd) => handle_task_command(task_cmd, &config).await,
        Commands::Worker(worker_cmd) => handle_worker_command(worker_cmd, &config).await,
        Commands::System(system_cmd) => handle_system_command(system_cmd, &config).await,
        Commands::Config(config_cmd) => handle_config_command(config_cmd, &config).await,
    }
}

async fn handle_task_command(cmd: TaskCommands, config: &ClientConfig) -> ClientResult<()> {
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
        } => {
            println!("Creating task: {}/{} v{}", namespace, name, version);

            // Parse input as JSON
            let context: serde_json::Value = serde_json::from_str(&input).map_err(|e| {
                tasker_client::ClientError::InvalidInput(format!("Invalid JSON input: {}", e))
            })?;

            let task_request = TaskRequest {
                namespace,
                name,
                version,
                context,
                status: "PENDING".to_string(),
                initiator: "tasker-cli".to_string(),
                source_system: "cli".to_string(),
                reason: "CLI task creation".to_string(),
                complete: false,
                tags: Vec::new(),
                bypass_steps: Vec::new(),
                requested_at: chrono::Utc::now().naive_utc(),
                options: None,
                priority: Some(priority as i32),
                claim_timeout_seconds: None,
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
                per_page: limit as u32,
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
    }
    Ok(())
}

async fn handle_worker_command(cmd: WorkerCommands, config: &ClientConfig) -> ClientResult<()> {
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

async fn handle_system_command(cmd: SystemCommands, config: &ClientConfig) -> ClientResult<()> {
    match cmd {
        SystemCommands::Health {
            orchestration,
            workers,
        } => {
            if orchestration || (!orchestration && !workers) {
                println!("🔍 Checking orchestration health...");

                let orchestration_config = OrchestrationApiConfig {
                    base_url: config.orchestration.base_url.clone(),
                    timeout_ms: config.orchestration.timeout_ms,
                    max_retries: config.orchestration.max_retries,
                    auth: None,
                };

                let orch_client = OrchestrationApiClient::new(orchestration_config)?;

                // Check basic health
                match orch_client.health_check().await {
                    Ok(()) => {
                        println!("  ✓ Orchestration service is healthy");
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
                        println!("  ⚠ Could not get detailed health info: {}", e);
                    }
                }
            }

            if workers || (!orchestration && !workers) {
                println!("\n🔍 Checking worker health...");

                let worker_config = WorkerApiConfig {
                    base_url: config.worker.base_url.clone(),
                    timeout_ms: config.worker.timeout_ms,
                    max_retries: config.worker.max_retries,
                    auth: None,
                };

                let worker_client = WorkerApiClient::new(worker_config)?;

                // Check basic worker service health
                match worker_client.health_check().await {
                    Ok(()) => {
                        println!("  ✓ Worker service is healthy");
                    }
                    Err(e) => {
                        println!("  ✗ Worker service health check failed: {}", e);
                    }
                }

                // Get worker instances
                match worker_client.list_workers(None).await {
                    Ok(worker_list) => {
                        println!(
                            "  ✓ Found {} worker instances (active: {})",
                            worker_list.total_count, worker_list.active_count
                        );

                        for worker in worker_list.workers.iter().take(5) {
                            // Show first 5
                            match worker_client.worker_health(&worker.worker_id).await {
                                Ok(health) => {
                                    println!(
                                        "    ✓ {}: {} | {} | {}s uptime",
                                        worker.worker_id,
                                        health.status,
                                        health.system_info.version,
                                        health.system_info.uptime_seconds
                                    );
                                }
                                Err(_) => {
                                    println!("    ✗ {}: Health check failed", worker.worker_id);
                                }
                            }
                        }

                        if worker_list.workers.len() > 5 {
                            println!("    ... and {} more workers", worker_list.workers.len() - 5);
                        }
                    }
                    Err(e) => {
                        println!("  ✗ Could not get worker list: {}", e);
                    }
                }
            }

            if !orchestration && !workers {
                println!("\n🎯 Overall system health: Both orchestration and worker services checked above");
            }
        }
        SystemCommands::Info => {
            println!("📊 Tasker System Information");
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
                auth: None,
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
                auth: None,
            };

            if let Ok(worker_client) = WorkerApiClient::new(worker_config) {
                match worker_client.list_workers(None).await {
                    Ok(worker_list) => {
                        println!(
                            "  Workers: {} total, {} active",
                            worker_list.total_count, worker_list.active_count
                        );

                        if let Some(first_worker) = worker_list.workers.first() {
                            if let Ok(health) =
                                worker_client.worker_health(&first_worker.worker_id).await
                            {
                                println!(
                                    "    Sample worker: {} v{} ({})",
                                    health.system_info.worker_type,
                                    health.system_info.version,
                                    health.system_info.environment
                                );
                                println!(
                                    "    Supported namespaces: {}",
                                    health.system_info.supported_namespaces.join(", ")
                                );
                            }
                        }
                    }
                    Err(_) => {
                        println!("  Workers: Unable to retrieve worker info");
                    }
                }
            } else {
                println!("  Workers: Configuration error");
            }
        }
    }
    Ok(())
}

async fn handle_config_command(cmd: ConfigCommands, config: &ClientConfig) -> ClientResult<()> {
    match cmd {
        ConfigCommands::Show => {
            println!("Current Configuration:");
            println!("{}", toml::to_string_pretty(config).unwrap());
        }
        ConfigCommands::Init { path } => {
            let config_path = if let Some(p) = path {
                std::path::PathBuf::from(p)
            } else {
                ClientConfig::default_config_path()?
            };

            let default_config = ClientConfig::default();
            default_config.save_to_file(&config_path)?;
            println!(
                "Default configuration written to: {}",
                config_path.display()
            );
        }
    }
    Ok(())
}
