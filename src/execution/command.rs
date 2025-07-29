//! Command Infrastructure for Tokio-Based TCP Architecture

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::WorkflowStep;
use crate::models::core::task_request::TaskRequest;


/// Unified command structure for all cross-language communication
///
/// Replaces ZeroMQ topic-based messaging with structured command protocol
/// that supports request/response correlation, timeout handling, and
/// intelligent routing based on worker capabilities.
///
/// # Examples
///
/// ```rust
/// use tasker_core::execution::command::*;
/// use chrono::Utc;
///
/// let command = Command {
///     command_type: CommandType::ExecuteBatch,
///     command_id: "cmd_12345".to_string(),
///     correlation_id: None,
///     metadata: CommandMetadata {
///         timestamp: Utc::now(),
///         source: CommandSource::RustOrchestrator { id: "coord_1".to_string() },
///         target: Some(CommandTarget::RubyWorker { id: "worker_123".to_string() }),
///         timeout_ms: Some(30000),
///         retry_policy: None,
///         namespace: Some("order_fulfillment".to_string()),
///         priority: Some(CommandPriority::Normal),
///     },
///     payload: CommandPayload::ExecuteBatch {
///         batch_id: "batch_789".to_string(),
///         steps: vec![],
///     },
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Type of command being executed
    pub command_type: CommandType,

    /// Unique identifier for this command
    pub command_id: String,

    /// Optional correlation ID for request/response tracking
    pub correlation_id: Option<String>,

    /// Command metadata for routing and execution
    pub metadata: CommandMetadata,

    /// The actual command payload
    pub payload: CommandPayload,
}

impl Command {
    /// Create a new command with generated ID and current timestamp
    pub fn new(command_type: CommandType, payload: CommandPayload, source: CommandSource) -> Self {
        Self {
            command_type,
            command_id: Uuid::new_v4().to_string(),
            correlation_id: None,
            metadata: CommandMetadata::new(source),
            payload,
        }
    }

    /// Create a response command correlated to this command
    pub fn create_response(
        &self,
        response_type: CommandType,
        payload: CommandPayload,
        source: CommandSource,
    ) -> Self {
        Self {
            command_type: response_type,
            command_id: Uuid::new_v4().to_string(),
            correlation_id: Some(self.command_id.clone()),
            metadata: CommandMetadata::new(source),
            payload,
        }
    }

    /// Check if this command is a response to another command
    pub fn is_response(&self) -> bool {
        self.correlation_id.is_some()
    }

    /// Get the original command ID this responds to
    pub fn original_command_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
}

/// Command types for all supported operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CommandType {
    // Task management operations
    InitializeTask,

    // Step execution operations
    ExecuteBatch,

    // Task readiness operations
    TryTaskIfReady,

    // Result reporting
    ReportPartialResult,
    ReportBatchCompletion,

    // Worker lifecycle management
    RegisterWorker,
    UnregisterWorker,
    WorkerHeartbeat,

    // Task handler registration
    RegisterTaskHandler,
    UnregisterTaskHandler,

    // System operations
    HealthCheck,

    // Generic FFI operation (future migration path)
    FfiOperation,

    // Response types
    Success,
    Error,
    WorkerRegistered,
    WorkerUnregistered,
    HeartbeatAcknowledged,
    BatchExecuted,
    TaskInitialized,
    TaskReadinessResult,
    HealthCheckResult,
    TaskHandlerRegistered,
    TaskHandlerUnregistered,
}

/// Command metadata for routing and execution control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    /// When this command was created
    pub timestamp: DateTime<Utc>,

    /// Source of the command
    pub source: CommandSource,

    /// Optional target for routing
    pub target: Option<CommandTarget>,

    /// Command timeout in milliseconds
    pub timeout_ms: Option<u64>,

    /// Retry policy for failed commands
    pub retry_policy: Option<RetryPolicy>,

    /// Namespace for capability-based routing
    pub namespace: Option<String>,

    /// Command priority for queue management
    pub priority: Option<CommandPriority>,
}

impl CommandMetadata {
    /// Create new metadata with current timestamp
    pub fn new(source: CommandSource) -> Self {
        Self {
            timestamp: Utc::now(),
            source,
            target: None,
            timeout_ms: None,
            retry_policy: None,
            namespace: None,
            priority: None,
        }
    }

    /// Set target for this command
    pub fn with_target(mut self, target: CommandTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Set timeout for this command
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Set namespace for capability routing
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Set command priority
    pub fn with_priority(mut self, priority: CommandPriority) -> Self {
        self.priority = Some(priority);
        self
    }
}

/// Command source identification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandSource {
    RustOrchestrator { id: String },
    RubyWorker { id: String },
    PythonWorker { id: String },
    NodeWorker { id: String },
    RustServer { id: String },
}

/// Command target identification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandTarget {
    RubyWorker { id: String },
    PythonWorker { id: String },
    NodeWorker { id: String },
    WorkerPool { namespace: String },
    AnyWorker,
}

/// Command priority levels for queue management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandPriority {
    Critical,
    High,
    Normal,
    Low,
}

/// Retry policy for failed commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,

    /// Initial delay between retries in milliseconds
    pub initial_delay_ms: u64,

    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,

    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30000,
        }
    }
}

/// Command payload variants for all supported operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandPayload {
    /// Initialize a new task with workflow steps
    InitializeTask {
        task_request: TaskRequest,
    },

    /// Execute a batch of workflow steps
    ExecuteBatch {
        batch_id: String,
        steps: Vec<StepExecutionRequest>,
        task_template: crate::models::core::task_template::TaskTemplate,
    },

    /// Try to process a task if it has ready steps
    TryTaskIfReady {
        task_id: i64,
    },

    /// Report partial result from step execution
    ReportPartialResult {
        batch_id: String,
        step_id: i64,
        result: StepResult,
        execution_time_ms: u64,
        worker_id: String,
    },

    /// Report batch completion with all step summaries
    ReportBatchCompletion {
        batch_id: String,
        step_summaries: Vec<StepSummary>,
        total_execution_time_ms: u64,
    },

    /// Register worker with capabilities
    RegisterWorker {
        worker_capabilities: WorkerCapabilities,
    },

    /// Unregister worker
    UnregisterWorker { worker_id: String, reason: String },

    /// Worker heartbeat with current status
    WorkerHeartbeat {
        worker_id: String,
        current_load: usize,
        system_stats: Option<SystemStats>,
    },

    /// Health check request
    HealthCheck { diagnostic_level: HealthCheckLevel },

    /// Register task handler with namespace and capabilities
    RegisterTaskHandler {
        worker_id: String,
        task_handler_info: TaskHandlerInfo,
    },

    /// Unregister task handler
    UnregisterTaskHandler {
        worker_id: String,
        namespace: String,
        handler_name: String,
    },

    /// Generic FFI operation for future migration
    FfiOperation {
        method: String,
        args: serde_json::Value,
    },

    /// Success response
    Success {
        message: String,
        data: Option<serde_json::Value>,
    },

    /// Error response
    Error {
        error_type: String,
        message: String,
        details: Option<HashMap<String, serde_json::Value>>,
        retryable: bool,
    },

    /// Worker registration acknowledgment
    WorkerRegistered {
        worker_id: String,
        assigned_pool: String,
        queue_position: usize,
    },

    /// Worker unregistration acknowledgment
    WorkerUnregistered {
        worker_id: String,
        unregistered_at: String,
        reason: String,
    },

    /// Worker heartbeat acknowledgment
    HeartbeatAcknowledged {
        worker_id: String,
        acknowledged_at: String,
        status: String,
        next_heartbeat_in: Option<u32>,
    },

    /// Batch execution acknowledgment
    BatchExecuted {
        batch_id: String,
        assigned_worker: String,
        estimated_completion_ms: u64,
    },

    /// Task initialization result
    TaskInitialized {
        task_id: i64,
        success: bool,
        step_count: usize,
        workflow_steps: serde_json::Value,
        error_message: Option<String>,
    },

    /// Task readiness result
    TaskReadinessResult {
        task_id: i64,
        ready: bool,
        batch_info: Option<TaskBatchInfo>,
        ready_steps_count: usize,
        error_message: Option<String>,
    },

    /// Health check result
    HealthCheckResult {
        status: String,
        uptime_seconds: u64,
        total_workers: usize,
        active_commands: usize,
        diagnostics: Option<serde_json::Value>,
    },

    /// Task handler registration acknowledgment
    TaskHandlerRegistered {
        worker_id: String,
        namespace: String,
        handler_name: String,
        version: String,
        registered_at: String,
    },

    /// Task handler unregistration acknowledgment
    TaskHandlerUnregistered {
        worker_id: String,
        namespace: String,
        handler_name: String,
        unregistered_at: String,
    },
}

/// Step execution request for batch processing
///
/// Contains all context needed for Ruby workers to execute workflow steps,
/// including task context, handler configuration, and dependency results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionRequest {
    pub step_id: i64,
    pub task_id: i64,
    pub step_name: String,
    pub handler_class: String,
    pub handler_config: HashMap<String, serde_json::Value>,
    pub task_context: serde_json::Value,
    pub previous_results: HashMap<String, serde_json::Value>,
    pub metadata: StepRequestMetadata,
}

/// Metadata for step execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRequestMetadata {
    pub attempt: i32,
    pub retry_limit: i32,
    pub timeout_ms: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Step execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub status: StepStatus,
    pub output: Option<serde_json::Value>,
    pub error: Option<StepError>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Step execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

/// Step execution error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepError {
    pub error_type: String,
    pub message: String,
    pub backtrace: Option<Vec<String>>,
    pub retryable: bool,
}

/// Step summary for batch completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSummary {
    pub step_id: i64,
    pub final_status: StepStatus,
    pub execution_time_ms: u64,
    pub worker_id: String,
}

/// Worker capabilities for registration and routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub worker_id: String,
    pub max_concurrent_steps: usize,
    pub supported_namespaces: Vec<String>,
    pub step_timeout_ms: u64,
    pub supports_retries: bool,
    pub language_runtime: String,
    pub version: String,
    pub custom_capabilities: HashMap<String, serde_json::Value>,
    // Connection information
    pub connection_info: Option<WorkerConnectionInfo>,
    // Runtime information
    pub runtime_info: Option<WorkerRuntimeInfo>,
}

/// Worker connection information for transport availability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConnectionInfo {
    pub host: String,
    pub port: u16,
    pub listener_port: Option<u16>,
    pub transport_type: String, // "tcp", "unix", etc.
    pub protocol_version: Option<String>,
}

/// Worker runtime information for metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRuntimeInfo {
    pub language_runtime: Option<String>,
    pub language_version: Option<String>,
    pub hostname: Option<String>,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
}

/// Task handler information for registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandlerInfo {
    pub namespace: String,
    pub handler_name: String,
    pub version: String,
    pub handler_class: String,
    pub supported_step_types: Vec<String>,
    pub handler_config: HashMap<String, serde_json::Value>,
    pub timeout_ms: u64,
    pub supports_retries: bool,
}

/// System statistics for worker health monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub active_connections: usize,
    pub uptime_seconds: u64,
}

/// Health check diagnostic levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckLevel {
    Basic,
    Detailed,
    Full,
}

/// Task batch information for readiness results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBatchInfo {
    pub batch_id: String,
    pub estimated_steps: usize,
    pub publication_time_ms: u64,
    pub next_poll_delay_ms: u64,
}

/// Command result for tracking execution outcomes
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command_id: String,
    pub success: bool,
    pub response: Option<Command>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

impl CommandResult {
    /// Create successful result
    pub fn success(command_id: String, response: Option<Command>, execution_time_ms: u64) -> Self {
        Self {
            command_id,
            success: true,
            response,
            error: None,
            execution_time_ms,
        }
    }

    /// Create error result
    pub fn error(command_id: String, error: String, execution_time_ms: u64) -> Self {
        Self {
            command_id,
            success: false,
            response: None,
            error: Some(error),
            execution_time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let source = CommandSource::RustOrchestrator {
            id: "coord_1".to_string(),
        };

        let payload = CommandPayload::RegisterWorker {
            worker_capabilities: WorkerCapabilities {
                worker_id: "worker_123".to_string(),
                max_concurrent_steps: 10,
                supported_namespaces: vec!["order_fulfillment".to_string()],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "ruby".to_string(),
                version: "3.1.0".to_string(),
                custom_capabilities: HashMap::new(),
                connection_info: None,
                runtime_info: None,
            },
        };

        let command = Command::new(CommandType::RegisterWorker, payload, source);

        assert_eq!(command.command_type, CommandType::RegisterWorker);
        assert!(!command.command_id.is_empty());
        assert!(command.correlation_id.is_none());
        assert!(!command.is_response());
    }

    #[test]
    fn test_response_creation() {
        let original_source = CommandSource::RubyWorker {
            id: "worker_123".to_string(),
        };

        let original_payload = CommandPayload::RegisterWorker {
            worker_capabilities: WorkerCapabilities {
                worker_id: "worker_123".to_string(),
                max_concurrent_steps: 10,
                supported_namespaces: vec!["order_fulfillment".to_string()],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "ruby".to_string(),
                version: "3.1.0".to_string(),
                custom_capabilities: HashMap::new(),
                connection_info: None,
                runtime_info: None,
            },
        };

        let original_command = Command::new(
            CommandType::RegisterWorker,
            original_payload,
            original_source,
        );

        let response_source = CommandSource::RustServer {
            id: "server_main".to_string(),
        };

        let response_payload = CommandPayload::WorkerRegistered {
            worker_id: "worker_123".to_string(),
            assigned_pool: "default".to_string(),
            queue_position: 1,
        };

        let response = original_command.create_response(
            CommandType::WorkerRegistered,
            response_payload,
            response_source,
        );

        assert_eq!(response.command_type, CommandType::WorkerRegistered);
        assert!(response.is_response());
        assert_eq!(
            response.original_command_id(),
            Some(original_command.command_id.as_str())
        );
    }

    #[test]
    fn test_command_serialization() {
        let source = CommandSource::RustOrchestrator {
            id: "coord_1".to_string(),
        };

        let payload = CommandPayload::ExecuteBatch {
            batch_id: "batch_789".to_string(),
            steps: vec![],
            task_template: crate::models::core::task_template::TaskTemplate {
                name: "test_task".to_string(),
                module_namespace: Some("TestModule".to_string()),
                task_handler_class: "TestHandler".to_string(),
                namespace_name: "test".to_string(),
                version: "1.0.0".to_string(),
                default_dependent_system: None,
                named_steps: vec![],
                step_templates: std::collections::HashMap::new(),
                schema: None,
                environments: None,
                default_context: None,
                default_options: None,
            },
        };

        let command = Command::new(CommandType::ExecuteBatch, payload, source);

        // Test JSON serialization/deserialization
        let json = serde_json::to_string(&command).expect("Failed to serialize command");
        let deserialized: Command =
            serde_json::from_str(&json).expect("Failed to deserialize command");

        assert_eq!(command.command_type, deserialized.command_type);
        assert_eq!(command.command_id, deserialized.command_id);
    }

    #[test]
    fn test_metadata_builder_pattern() {
        let source = CommandSource::RustOrchestrator {
            id: "coord_1".to_string(),
        };

        let target = CommandTarget::RubyWorker {
            id: "worker_123".to_string(),
        };

        let metadata = CommandMetadata::new(source)
            .with_target(target)
            .with_timeout(30000)
            .with_namespace("order_fulfillment".to_string())
            .with_priority(CommandPriority::High);

        assert!(metadata.target.is_some());
        assert_eq!(metadata.timeout_ms, Some(30000));
        assert_eq!(metadata.namespace.as_deref(), Some("order_fulfillment"));
        assert_eq!(metadata.priority, Some(CommandPriority::High));
    }
}
