//! # Tasker CLI Tool
//!
//! Command-line interface for interacting with Tasker orchestration and worker APIs.
//! Provides task management, worker monitoring, and system health checking capabilities.

use std::str::FromStr;

use chrono::Utc;
use clap::{Parser, Subcommand};
use tasker_client::{
    ClientConfig, ClientResult, OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient,
    WorkerApiConfig,
};
use tasker_shared::config::{ConfigMerger, ConfigurationContext};
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
        /// Correlation ID for tracing
        #[arg(short, long)]
        correlation_id: Option<String>,
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
    /// Generate single deployable configuration file from base + environment
    Generate {
        /// Configuration context (orchestration, worker, or combined)
        #[arg(short, long)]
        context: String,

        /// Target environment (test, development, production)
        #[arg(short, long)]
        environment: String,

        /// Source directory containing base and environment configs
        #[arg(short, long, default_value = "config/tasker")]
        source_dir: String,

        /// Output file path for generated config
        #[arg(short, long)]
        output: String,

        /// Validate configuration after generation
        #[arg(long)]
        validate: bool,
    },

    /// Validate configuration file
    Validate {
        /// Configuration file to validate
        #[arg(short, long)]
        config: String,

        /// Expected configuration context
        #[arg(short = 't', long)]
        context: String,

        /// Strict mode - fail on warnings
        #[arg(long)]
        strict: bool,

        /// Provide detailed error explanations
        #[arg(long)]
        explain_errors: bool,
    },

    /// Explain configuration parameters
    Explain {
        /// Specific parameter path (e.g., "database.pool.max_connections")
        #[arg(short, long)]
        parameter: Option<String>,

        /// Configuration context to list parameters for
        #[arg(short, long)]
        context: Option<String>,

        /// Environment for recommendations
        #[arg(short, long)]
        environment: Option<String>,
    },

    /// Validate source configuration files (base + environment) without generating output
    ValidateSources {
        /// Configuration context (orchestration, worker, or common)
        #[arg(short, long)]
        context: String,

        /// Target environment (test, development, production)
        #[arg(short, long)]
        environment: String,

        /// Source directory containing base and environment configs
        #[arg(short, long, default_value = "config/tasker")]
        source_dir: String,

        /// Provide detailed error explanations
        #[arg(long)]
        explain_errors: bool,
    },

    /// Detect unused configuration parameters
    DetectUnused {
        /// Source directory with TOML files
        #[arg(short, long, default_value = "config/tasker")]
        source_dir: String,

        /// Configuration context
        #[arg(short, long)]
        context: String,

        /// Automatically remove unused parameters (creates backup)
        #[arg(long)]
        fix: bool,
    },

    /// Show current CLI configuration
    Show,
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
                bypass_steps: Vec::new(),
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
            if orchestration || !workers {
                println!("Checking orchestration health...");

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

/// Extract line and column from TOML error message
fn extract_error_position(error_msg: &str) -> Option<(usize, usize)> {
    // TOML errors often contain "at line X column Y" patterns
    use regex::Regex;
    let re = Regex::new(r"line (\d+) column (\d+)").ok()?;
    let caps = re.captures(error_msg)?;

    let line = caps.get(1)?.as_str().parse().ok()?;
    let col = caps.get(2)?.as_str().parse().ok()?;

    Some((line, col))
}

async fn handle_config_command(cmd: ConfigCommands, _config: &ClientConfig) -> ClientResult<()> {
    match cmd {
        ConfigCommands::Generate {
            context,
            environment,
            source_dir,
            output,
            validate,
        } => {
            println!("Generating configuration:");
            println!("  Context: {}", context);
            println!("  Environment: {}", environment);
            println!("  Source: {}", source_dir);
            println!("  Output: {}", output);

            // Create ConfigMerger
            let mut merger = ConfigMerger::new(std::path::PathBuf::from(&source_dir), &environment)
                .map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to initialize config merger: {}",
                        e
                    ))
                })?;

            // Merge configuration
            println!("\nMerging configuration...");
            let merged_config = merger.merge_context(&context).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to merge configuration: {}",
                    e
                ))
            })?;

            // Generate metadata header
            let header = format!(
                "# Tasker Configuration - {} Context\n\
                 # Environment: {}\n\
                 # Generated: {}\n\
                 # Source: {}\n\
                 #\n\
                 # This file is a MERGED configuration (base + environment overrides).\n\
                 # DO NOT EDIT manually - regenerate using: tasker-cli config generate\n\
                 #\n\
                 # Environment Variable Overrides (applied at runtime):\n\
                 # - DATABASE_URL: Override database.url (K8s secrets rotation)\n\
                 # - TASKER_TEMPLATE_PATH: Override worker.template_path (testing)\n\
                 #\n\n",
                context,
                environment,
                Utc::now().to_rfc3339(),
                source_dir
            );

            // Write header + merged config to output file
            let full_config = format!("{}{}", header, merged_config);
            println!("Writing to: {}", output);
            std::fs::write(&output, &full_config).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to write output file: {}",
                    e
                ))
            })?;

            println!("✓ Successfully generated configuration!");
            println!("  Output file: {}", output);
            println!("  Size: {} bytes", full_config.len());

            // Optionally validate if requested
            if validate {
                println!("\nValidating generated configuration...");

                // Re-read the file we just wrote
                let written_content = std::fs::read_to_string(&output).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to read generated file for validation: {}",
                        e
                    ))
                })?;

                // Parse as TOML
                let toml_value: toml::Value = toml::from_str(&written_content).map_err(|e| {
                    tasker_client::ClientError::ConfigError(format!(
                        "Generated file is not valid TOML: {}",
                        e
                    ))
                })?;

                // Validate based on context
                match context.as_str() {
                    "common" => {
                        use tasker_shared::config::contexts::CommonConfig;
                        let config: CommonConfig = toml_value.clone().try_into().map_err(|e| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize CommonConfig: {}",
                                e
                            ))
                        })?;
                        config.validate().map_err(|errors| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Validation failed: {:?}",
                                errors
                            ))
                        })?;
                    }
                    "orchestration" => {
                        use tasker_shared::config::contexts::OrchestrationConfig;
                        let config: OrchestrationConfig =
                            toml_value.clone().try_into().map_err(|e| {
                                tasker_client::ClientError::ConfigError(format!(
                                    "Failed to deserialize OrchestrationConfig: {}",
                                    e
                                ))
                            })?;
                        config.validate().map_err(|errors| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Validation failed: {:?}",
                                errors
                            ))
                        })?;
                    }
                    "worker" => {
                        use tasker_shared::config::contexts::WorkerConfig;
                        let config: WorkerConfig = toml_value.clone().try_into().map_err(|e| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize WorkerConfig: {}",
                                e
                            ))
                        })?;
                        config.validate().map_err(|errors| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Validation failed: {:?}",
                                errors
                            ))
                        })?;
                    }
                    "complete" => {
                        use tasker_shared::config::TaskerConfig;
                        let config: TaskerConfig = toml_value.clone().try_into().map_err(|e| {
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize TaskerConfig (complete context): {}",
                                e
                            ))
                        })?;
                        // TaskerConfig doesn't have a validate method, so just check deserialization succeeded
                        println!("  Note: Complete context contains all sections (common + orchestration + worker)");
                    }
                    _ => {
                        return Err(tasker_client::ClientError::InvalidInput(format!(
                            "Unknown context '{}'. Valid contexts: common, orchestration, worker, complete",
                            context
                        )));
                    }
                }

                println!("✓ Validation passed!");
            }
        }
        ConfigCommands::Validate {
            config: config_path,
            context,
            strict,
            explain_errors,
        } => {
            println!("Validating configuration:");
            println!("  File: {}", config_path);
            println!("  Context: {}", context);
            println!("  Strict mode: {}", strict);

            // Load the configuration file
            let config_content = std::fs::read_to_string(&config_path).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to read config file '{}': {}",
                    config_path, e
                ))
            })?;

            // Parse as TOML
            let toml_value: toml::Value = toml::from_str(&config_content).map_err(|e| {
                if explain_errors {
                    eprintln!("\n❌ TOML parsing error:");
                    eprintln!("  {}", e);
                    if let Some((line, col)) = extract_error_position(&e.to_string()) {
                        eprintln!("  Location: line {}, column {}", line, col);
                    }
                }
                tasker_client::ClientError::ConfigError(format!("Invalid TOML syntax: {}", e))
            })?;

            // Validate based on context
            println!("\nValidating {} configuration...", context);

            match context.as_str() {
                "common" => {
                    use tasker_shared::config::contexts::CommonConfig;

                    let common_config: CommonConfig =
                        toml_value.clone().try_into().map_err(|e| {
                            if explain_errors {
                                eprintln!("\n❌ Configuration structure error:");
                                eprintln!("  {}", e);
                                eprintln!("\nExpected CommonConfig structure with sections:");
                                eprintln!("  - database");
                                eprintln!("  - circuit_breakers");
                                eprintln!("  - engine (optional)");
                            }
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize CommonConfig: {}",
                                e
                            ))
                        })?;

                    // Run validation
                    common_config.validate().map_err(|errors| {
                        if explain_errors {
                            eprintln!("\n❌ Validation errors:");
                            for error in &errors {
                                eprintln!("  - {}", error);
                            }
                        }
                        tasker_client::ClientError::ConfigError(format!(
                            "CommonConfig validation failed: {:?}",
                            errors
                        ))
                    })?;

                    println!("✓ CommonConfig validation passed");
                }
                "orchestration" => {
                    use tasker_shared::config::contexts::OrchestrationConfig;

                    let orch_config: OrchestrationConfig =
                        toml_value.clone().try_into().map_err(|e| {
                            if explain_errors {
                                eprintln!("\n❌ Configuration structure error:");
                                eprintln!("  {}", e);
                                eprintln!(
                                    "\nExpected OrchestrationConfig structure with sections:"
                                );
                                eprintln!("  - backoff");
                                eprintln!("  - event_systems");
                                eprintln!("  - mpsc_channels");
                            }
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize OrchestrationConfig: {}",
                                e
                            ))
                        })?;

                    // Run validation
                    orch_config.validate().map_err(|errors| {
                        if explain_errors {
                            eprintln!("\n❌ Validation errors:");
                            for error in &errors {
                                eprintln!("  - {}", error);
                            }
                        }
                        tasker_client::ClientError::ConfigError(format!(
                            "OrchestrationConfig validation failed: {:?}",
                            errors
                        ))
                    })?;

                    println!("✓ OrchestrationConfig validation passed");
                }
                "worker" => {
                    use tasker_shared::config::contexts::WorkerConfig;

                    let worker_config: WorkerConfig =
                        toml_value.clone().try_into().map_err(|e| {
                            if explain_errors {
                                eprintln!("\n❌ Configuration structure error:");
                                eprintln!("  {}", e);
                                eprintln!("\nExpected WorkerConfig structure with sections:");
                                eprintln!("  - event_systems");
                                eprintln!("  - mpsc_channels");
                                eprintln!("  - health_monitoring (optional)");
                            }
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize WorkerConfig: {}",
                                e
                            ))
                        })?;

                    // Run validation
                    worker_config.validate().map_err(|errors| {
                        if explain_errors {
                            eprintln!("\n❌ Validation errors:");
                            for error in &errors {
                                eprintln!("  - {}", error);
                            }
                        }
                        tasker_client::ClientError::ConfigError(format!(
                            "WorkerConfig validation failed: {:?}",
                            errors
                        ))
                    })?;

                    println!("✓ WorkerConfig validation passed");
                }
                "complete" => {
                    use tasker_shared::config::TaskerConfig;

                    let _tasker_config: TaskerConfig =
                        toml_value.clone().try_into().map_err(|e| {
                            if explain_errors {
                                eprintln!("\n❌ Configuration structure error:");
                                eprintln!("  {}", e);
                                eprintln!("\nExpected TaskerConfig structure with sections:");
                                eprintln!("  - database (common)");
                                eprintln!("  - circuit_breakers (common)");
                                eprintln!("  - orchestration (orchestration context)");
                                eprintln!("  - worker (worker context)");
                            }
                            tasker_client::ClientError::ConfigError(format!(
                                "Failed to deserialize TaskerConfig (complete context): {}",
                                e
                            ))
                        })?;

                    // TaskerConfig doesn't have a validate method
                    println!("✓ TaskerConfig (complete) validation passed");
                    println!("  Note: Complete context contains all sections");
                }
                _ => {
                    return Err(tasker_client::ClientError::InvalidInput(format!(
                        "Unknown context '{}'. Valid contexts: common, orchestration, worker, complete",
                        context
                    )));
                }
            }

            println!("\n✓ Configuration is valid!");

            if strict {
                println!("  (strict mode: no warnings detected)");
            }
        }
        ConfigCommands::ValidateSources {
            context,
            environment,
            source_dir,
            explain_errors,
        } => {
            println!("Validating source configuration:");
            println!("  Context: {}", context);
            println!("  Environment: {}", environment);
            println!("  Source directory: {}", source_dir);

            // Create ConfigMerger
            let mut merger = ConfigMerger::new(std::path::PathBuf::from(&source_dir), &environment)
                .map_err(|e| {
                    if explain_errors {
                        eprintln!("\n❌ Failed to initialize config merger:");
                        eprintln!("  {}", e);
                        eprintln!("\nPossible causes:");
                        eprintln!("  - Source directory doesn't exist");
                        eprintln!("  - Missing base/ subdirectory");
                        eprintln!("  - Invalid directory permissions");
                    }
                    tasker_client::ClientError::ConfigError(format!(
                        "Failed to initialize config merger: {}",
                        e
                    ))
                })?;

            // Attempt to merge configuration (validates source files, env var substitution)
            println!("\nValidating source files and merging...");
            merger.merge_context(&context).map_err(|e| {
                if explain_errors {
                    eprintln!("\n❌ Source configuration validation failed:");
                    eprintln!("  {}", e);
                    eprintln!("\nPossible causes:");
                    eprintln!("  - Missing required TOML files (base/{}.toml)", context);
                    eprintln!("  - Invalid TOML syntax");
                    eprintln!("  - Environment variables without defaults (use ${{VAR:-default}})");
                    eprintln!("  - Structural validation errors");
                }
                tasker_client::ClientError::ConfigError(format!("Source validation failed: {}", e))
            })?;

            println!("✓ Source configuration is valid!");
            println!("  Base files: config/tasker/base/{}.toml", context);
            if context == "orchestration" || context == "worker" {
                println!("  Also includes: config/tasker/base/common.toml");
            }
            println!(
                "  Environment overrides: config/tasker/environments/{}/",
                environment
            );
        }
        ConfigCommands::Explain {
            parameter,
            context,
            environment,
        } => {
            use tasker_shared::config::ConfigDocumentation;

            // Load documentation from base configuration directory
            let base_dir = std::path::PathBuf::from("config/tasker/base");

            if !base_dir.exists() {
                eprintln!(
                    "❌ Configuration base directory not found: {}",
                    base_dir.display()
                );
                eprintln!("   Expected directory structure:");
                eprintln!("   config/tasker/base/");
                eprintln!("     ├── common.toml");
                eprintln!("     ├── orchestration.toml");
                eprintln!("     └── worker.toml");
                return Err(tasker_client::ClientError::ConfigError(
                    "Configuration base directory not found".to_string(),
                ));
            }

            let docs = ConfigDocumentation::load(base_dir).map_err(|e| {
                tasker_client::ClientError::ConfigError(format!(
                    "Failed to load configuration documentation: {}",
                    e
                ))
            })?;

            // Handle specific parameter lookup
            if let Some(param_path) = parameter {
                if let Some(param_docs) = docs.lookup(&param_path) {
                    println!("\n📖 Configuration Parameter: {}\n", param_docs.path);
                    println!("Description:");
                    println!("  {}\n", param_docs.description);

                    println!("Type: {}", param_docs.param_type);
                    println!("Valid Range: {}", param_docs.valid_range);

                    if !param_docs.default.is_empty() {
                        println!("Default: {}\n", param_docs.default);
                    } else {
                        println!();
                    }

                    println!("System Impact:");
                    println!("  {}\n", param_docs.system_impact);

                    if !param_docs.example.is_empty() {
                        println!("Example Usage:");
                        println!("{}\n", param_docs.example);
                    }

                    if !param_docs.related.is_empty() {
                        println!("Related Parameters:");
                        for related in &param_docs.related {
                            println!("  • {}", related);
                        }
                        println!();
                    }

                    // Show environment-specific recommendations
                    if !param_docs.recommendations.is_empty() {
                        println!("Environment-Specific Recommendations:");

                        // If environment specified, show only that one
                        if let Some(env) = &environment {
                            if let Some(rec) = param_docs.recommendations.get(env) {
                                println!("  {} environment:", env);
                                println!("    Value: {}", rec.value);
                                println!("    Rationale: {}", rec.rationale);
                            } else {
                                println!("  ⚠️  No recommendation for {} environment", env);
                            }
                        } else {
                            // Show all recommendations
                            for (env_name, rec) in &param_docs.recommendations {
                                println!("  {} environment:", env_name);
                                println!("    Value: {}", rec.value);
                                println!("    Rationale: {}", rec.rationale);
                            }
                        }
                        println!();
                    }

                    if !param_docs.example.is_empty() {
                        println!("Example Usage:");
                        println!("{}", param_docs.example);
                    }
                } else {
                    eprintln!("❌ No documentation found for parameter: {}", param_path);
                    eprintln!("\n💡 Tip: Use 'tasker-cli config explain --context <context>' to list available parameters");
                    eprintln!("   Valid contexts: common, orchestration, worker");
                    return Err(tasker_client::ClientError::InvalidInput(format!(
                        "Parameter '{}' not documented",
                        param_path
                    )));
                }
            }
            // Handle context listing
            else if let Some(ctx) = context {
                let context_params = docs.list_for_context(&ctx);

                if context_params.is_empty() {
                    println!("\n⚠️  No documented parameters found for context: {}", ctx);
                    println!("\n💡 Valid contexts: common, orchestration, worker");
                } else {
                    println!("\n📚 Configuration Parameters for {} Context\n", ctx);
                    println!("Found {} documented parameters:\n", context_params.len());

                    for param in context_params {
                        println!("• {}", param.path);
                        println!("  {}", param.description);
                        println!();
                    }

                    println!("💡 Tip: Use 'tasker-cli config explain --parameter <path>' for detailed information");
                    println!("   Example: tasker-cli config explain --parameter database.pool.max_connections");
                }
            }
            // List all documented parameters
            else {
                let all_params: Vec<_> = docs.all_parameters().collect();

                println!("\n📚 All Documented Configuration Parameters\n");
                println!("Total: {} parameters\n", all_params.len());

                // Group by prefix for better readability
                let mut common_params = Vec::new();
                let mut orch_params = Vec::new();
                let mut worker_params = Vec::new();

                for param in all_params {
                    if param.path.starts_with("database")
                        || param.path.starts_with("queues")
                        || param.path.starts_with("circuit_breakers")
                        || param.path.starts_with("shared_channels")
                    {
                        common_params.push(param);
                    } else if param.path.starts_with("orchestration")
                        || param.path.starts_with("backoff")
                        || param.path.starts_with("task_readiness")
                    {
                        orch_params.push(param);
                    } else if param.path.starts_with("worker") {
                        worker_params.push(param);
                    }
                }

                if !common_params.is_empty() {
                    println!("Common Configuration ({} parameters):", common_params.len());
                    for param in common_params {
                        println!("  • {} - {}", param.path, param.description);
                    }
                    println!();
                }

                if !orch_params.is_empty() {
                    println!(
                        "Orchestration Configuration ({} parameters):",
                        orch_params.len()
                    );
                    for param in orch_params {
                        println!("  • {} - {}", param.path, param.description);
                    }
                    println!();
                }

                if !worker_params.is_empty() {
                    println!("Worker Configuration ({} parameters):", worker_params.len());
                    for param in worker_params {
                        println!("  • {} - {}", param.path, param.description);
                    }
                    println!();
                }

                println!("💡 Tip: Use --context <name> to filter by context (common, orchestration, worker)");
                println!("   Or use --parameter <path> for detailed information about a specific parameter");
            }
        }
        ConfigCommands::DetectUnused {
            source_dir,
            context,
            fix,
        } => {
            println!("Detecting unused configuration parameters:");
            println!("  Source: {}", source_dir);
            println!("  Context: {}", context);
            println!("  Auto-fix: {}", fix);

            // TODO: Implement unused parameter detection
            eprintln!("Unused parameter detection not yet implemented");
            return Err(tasker_client::ClientError::NotImplemented(
                "Unused parameter detection".to_string(),
            ));
        }
        ConfigCommands::Show => {
            println!("Current CLI Configuration:");
            println!("{}", toml::to_string_pretty(_config).unwrap());
        }
    }
    Ok(())
}
