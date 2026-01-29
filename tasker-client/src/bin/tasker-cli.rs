//! # Tasker CLI Tool
//!
//! Command-line interface for interacting with Tasker orchestration and worker APIs.
//! Provides task management, worker monitoring, and system health checking capabilities.

mod cli;

use clap::{Parser, Subcommand};
use tasker_client::ClientConfig;
use tracing::info;

use cli::{
    handle_auth_command, handle_config_command, handle_dlq_command, handle_system_command,
    handle_task_command, handle_worker_command,
};

#[derive(Parser, Debug)]
#[command(name = "tasker-cli")]
#[command(about = "Command-line interface for Tasker orchestration system")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Configuration file path (default: ~/.tasker/config.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Configuration profile to use (TAS-177)
    ///
    /// Profiles are defined in .config/tasker-client.toml (like nextest).
    /// Use `tasker-cli config show` to see available profiles.
    ///
    /// Can also be set via TASKER_CLIENT_PROFILE environment variable.
    #[arg(short, long)]
    profile: Option<String>,

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

#[derive(Debug, Subcommand)]
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

    /// Dead Letter Queue (DLQ) investigation operations (TAS-49)
    #[command(subcommand)]
    Dlq(DlqCommands),

    /// Authentication and security operations (TAS-150)
    #[command(subcommand)]
    Auth(AuthCommands),
}

#[derive(Debug, Subcommand)]
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
    /// List workflow steps for a task
    Steps {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
    },
    /// Get workflow step details
    Step {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
        /// Step UUID
        #[arg(value_name = "STEP_UUID")]
        step_id: String,
    },
    /// Reset step attempt counter and return to pending for automatic retry
    ResetStep {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
        /// Step UUID
        #[arg(value_name = "STEP_UUID")]
        step_id: String,
        /// Reason for reset
        #[arg(short, long)]
        reason: String,
        /// Operator performing reset
        #[arg(short = 'b', long, default_value = "cli-operator")]
        reset_by: String,
    },
    /// Mark step as manually resolved without providing results
    ResolveStep {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
        /// Step UUID
        #[arg(value_name = "STEP_UUID")]
        step_id: String,
        /// Reason for resolution
        #[arg(short, long)]
        reason: String,
        /// Operator performing resolution
        #[arg(short = 'b', long, default_value = "cli-operator")]
        resolved_by: String,
    },
    /// Complete step manually with execution results for dependent steps
    CompleteStep {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
        /// Step UUID
        #[arg(value_name = "STEP_UUID")]
        step_id: String,
        /// Execution result as JSON string
        #[arg(long)]
        result: String,
        /// Optional metadata as JSON string
        #[arg(long)]
        metadata: Option<String>,
        /// Reason for manual completion
        #[arg(short, long)]
        reason: String,
        /// Operator performing completion
        #[arg(short = 'b', long, default_value = "cli-operator")]
        completed_by: String,
    },
    /// Get step audit history (TAS-62: SOC2 compliance)
    StepAudit {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_id: String,
        /// Step UUID
        #[arg(value_name = "STEP_UUID")]
        step_id: String,
    },
}

#[derive(Debug, Subcommand)]
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

#[derive(Debug, Subcommand)]
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

#[derive(Debug, Subcommand)]
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

    /// Analyze configuration usage patterns across the codebase (TAS-61)
    AnalyzeUsage {
        /// Source directory to analyze for code usage
        #[arg(long, default_value = ".")]
        source_dir: String,

        /// Filter by configuration context (orchestration, worker, common, all)
        #[arg(short, long, default_value = "all")]
        context: String,

        /// Report format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Only show unused configuration parameters
        #[arg(long)]
        show_unused: bool,

        /// Write report to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,

        /// Include usage locations in report
        #[arg(long)]
        show_locations: bool,
    },

    /// Dump configuration structure for analysis (TAS-61)
    Dump {
        /// Configuration context (orchestration, worker, complete)
        #[arg(short, long, required_unless_present = "path")]
        context: Option<String>,

        /// Target environment (test, development, production)
        #[arg(short, long, required_unless_present = "path")]
        environment: Option<String>,

        /// Source directory containing base and environment configs
        #[arg(short, long, default_value = "config/tasker")]
        source_dir: String,

        /// Output format (json, yaml, toml)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// Path to complete TOML configuration file (bypasses context/environment)
        #[arg(short, long, conflicts_with = "context")]
        path: Option<String>,
    },

    /// Show current CLI configuration
    Show,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Generate RSA key pair for JWT signing
    GenerateKeys {
        /// Output directory for key files
        #[arg(short, long, default_value = "./keys")]
        output_dir: String,
        /// RSA key size in bits
        #[arg(short, long, default_value = "2048")]
        key_size: usize,
    },
    /// Generate a JWT token with specified permissions
    GenerateToken {
        /// Comma-separated list of permissions (e.g., tasks:create,tasks:read)
        #[arg(short, long)]
        permissions: String,
        /// Token subject (service/user identifier)
        #[arg(short, long, default_value = "tasker-client")]
        subject: String,
        /// Path to private key PEM file
        #[arg(long, default_value = "./keys/jwt-private-key.pem")]
        private_key: String,
        /// Token expiry in hours
        #[arg(short, long, default_value = "24")]
        expiry_hours: u64,
        /// JWT issuer
        #[arg(long, default_value = "tasker-core")]
        issuer: String,
        /// JWT audience
        #[arg(long, default_value = "tasker-api")]
        audience: String,
    },
    /// Show all known permissions
    ShowPermissions,
    /// Validate a JWT token
    ValidateToken {
        /// The JWT token to validate
        #[arg(short, long)]
        token: String,
        /// Path to public key PEM file
        #[arg(long, default_value = "./keys/jwt-public-key.pem")]
        public_key: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DlqCommands {
    /// List DLQ entries
    List {
        /// Filter by resolution status (pending, manually_resolved, permanently_failed, cancelled)
        #[arg(short, long)]
        status: Option<String>,
        /// Limit number of results
        #[arg(short, long, default_value = "50")]
        limit: i64,
        /// Offset for pagination
        #[arg(short, long, default_value = "0")]
        offset: i64,
    },
    /// Get DLQ entry by task UUID
    Get {
        /// Task UUID
        #[arg(value_name = "TASK_UUID")]
        task_uuid: String,
    },
    /// Update DLQ investigation status
    Update {
        /// DLQ entry UUID
        #[arg(value_name = "DLQ_ENTRY_UUID")]
        dlq_entry_uuid: String,
        /// New resolution status (pending, manually_resolved, permanently_failed, cancelled)
        #[arg(short, long)]
        status: Option<String>,
        /// Resolution notes
        #[arg(short, long)]
        notes: Option<String>,
        /// Who resolved the investigation
        #[arg(short = 'b', long)]
        resolved_by: Option<String>,
    },
    /// Get DLQ statistics
    Stats,
}

#[tokio::main]
async fn main() -> tasker_client::ClientResult<()> {
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

    // Load configuration with precedence: --config > --profile > TASKER_CLIENT_PROFILE > default
    let config = if let Some(config_path) = cli.config {
        // Explicit config file takes highest priority
        ClientConfig::load_from_file(std::path::Path::new(&config_path))?
    } else if let Some(profile_name) = cli.profile {
        // CLI --profile flag
        ClientConfig::load_profile(&profile_name)?
    } else {
        // Default loading (checks TASKER_CLIENT_PROFILE env var)
        ClientConfig::load()?
    };

    info!(
        transport = %config.transport,
        orchestration_url = %config.orchestration.base_url,
        "Tasker CLI starting"
    );

    // Execute command
    match cli.command {
        Commands::Task(task_cmd) => handle_task_command(task_cmd, &config).await,
        Commands::Worker(worker_cmd) => handle_worker_command(worker_cmd, &config).await,
        Commands::System(system_cmd) => handle_system_command(system_cmd, &config).await,
        Commands::Config(config_cmd) => handle_config_command(config_cmd, &config).await,
        Commands::Dlq(dlq_cmd) => handle_dlq_command(dlq_cmd, &config).await,
        Commands::Auth(auth_cmd) => handle_auth_command(auth_cmd).await,
    }
}
