//! # Execution Context for Rust Clients
//!
//! Provides context structures that contain all the information needed
//! for task and step execution in Rust client applications.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Context for step execution
///
/// Contains all information needed to execute a single step within a task workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepContext {
    /// Unique identifier for this step instance
    pub step_id: i64,

    /// Unique identifier for the parent task
    pub task_id: i64,

    /// Name of the step from the handler configuration
    pub step_name: String,

    /// Input data for step processing
    pub input_data: serde_json::Value,

    /// Configuration specific to this step from the handler configuration
    pub step_config: HashMap<String, serde_json::Value>,

    /// Results from previous steps in the task
    pub previous_results: HashMap<String, serde_json::Value>,

    /// Current execution attempt number (1-based)
    pub attempt_number: u32,

    /// Maximum number of retry attempts allowed
    pub max_attempts: u32,

    /// Whether this step can be retried on failure
    pub is_retryable: bool,

    /// Execution timeout in seconds
    pub timeout_seconds: u64,

    /// Environment this step is executing in
    pub environment: String,

    /// Additional metadata about the execution context
    pub metadata: ExecutionMetadata,

    /// Step-level tags for categorization and filtering
    pub tags: Vec<String>,

    /// Steps that this step depends on
    pub dependencies: Vec<String>,
}

/// Context for task execution
///
/// Contains information about the overall task that coordinates multiple steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// Unique identifier for this task instance
    pub task_id: i64,

    /// Name of the task from the handler configuration
    pub task_name: String,

    /// Namespace the task belongs to
    pub namespace: String,

    /// Version of the task handler
    pub version: String,

    /// Input data for the entire task
    pub input_data: serde_json::Value,

    /// Task-level configuration from the handler configuration
    pub task_config: HashMap<String, serde_json::Value>,

    /// Environment this task is executing in
    pub environment: String,

    /// Additional metadata about the task execution
    pub metadata: ExecutionMetadata,

    /// Task-level tags for categorization and filtering
    pub tags: Vec<String>,

    /// Current task status
    pub status: String,

    /// Whether the task can be retried on failure
    pub is_retryable: bool,

    /// Maximum number of retry attempts for the task
    pub max_attempts: u32,

    /// Current execution attempt number (1-based)
    pub attempt_number: u32,

    /// All step names that are part of this task
    pub step_names: Vec<String>,
}

/// Execution metadata shared between task and step contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// When this execution started
    pub started_at: DateTime<Utc>,

    /// Unique identifier for tracking this execution
    pub execution_id: String,

    /// Source system that initiated this execution
    pub source_system: String,

    /// User or system that requested this execution
    pub initiator: String,

    /// Reason or context for this execution
    pub reason: String,

    /// Custom metadata fields
    pub custom: HashMap<String, serde_json::Value>,

    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,

    /// Parent task ID (for sub-tasks)
    pub parent_task_id: Option<i64>,

    /// Priority level for execution scheduling
    pub priority: ExecutionPriority,
}

/// Execution priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionPriority {
    /// Low priority - execute when resources are available
    Low,
    /// Normal priority - default execution priority
    Normal,
    /// High priority - execute before normal priority items
    High,
    /// Critical priority - execute immediately
    Critical,
}

impl Default for ExecutionPriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl StepContext {
    /// Create a new step context with minimal required fields
    pub fn new(
        step_id: i64,
        task_id: i64,
        step_name: String,
        input_data: serde_json::Value,
    ) -> Self {
        Self {
            step_id,
            task_id,
            step_name,
            input_data,
            step_config: HashMap::new(),
            previous_results: HashMap::new(),
            attempt_number: 1,
            max_attempts: 3,
            is_retryable: true,
            timeout_seconds: 300, // 5 minutes default
            environment: "development".to_string(),
            metadata: ExecutionMetadata::default(),
            tags: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    /// Check if this is a retry attempt
    pub fn is_retry(&self) -> bool {
        self.attempt_number > 1
    }

    /// Check if more retries are possible
    pub fn can_retry(&self) -> bool {
        self.is_retryable && self.attempt_number < self.max_attempts
    }

    /// Get the result from a specific previous step
    pub fn get_previous_result(&self, step_name: &str) -> Option<&serde_json::Value> {
        self.previous_results.get(step_name)
    }

    /// Get a configuration value for this step
    pub fn get_config_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.step_config.get(key)
    }
}

impl TaskContext {
    /// Create a new task context with minimal required fields
    pub fn new(
        task_id: i64,
        task_name: String,
        namespace: String,
        input_data: serde_json::Value,
    ) -> Self {
        Self {
            task_id,
            task_name,
            namespace,
            version: "0.1.0".to_string(),
            input_data,
            task_config: HashMap::new(),
            environment: "development".to_string(),
            metadata: ExecutionMetadata::default(),
            tags: Vec::new(),
            status: "pending".to_string(),
            is_retryable: true,
            max_attempts: 3,
            attempt_number: 1,
            step_names: Vec::new(),
        }
    }

    /// Check if this is a retry attempt
    pub fn is_retry(&self) -> bool {
        self.attempt_number > 1
    }

    /// Check if more retries are possible
    pub fn can_retry(&self) -> bool {
        self.is_retryable && self.attempt_number < self.max_attempts
    }

    /// Get a configuration value for this task
    pub fn get_config_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.task_config.get(key)
    }
}

impl Default for ExecutionMetadata {
    fn default() -> Self {
        Self {
            started_at: Utc::now(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            source_system: "tasker-core-rs".to_string(),
            initiator: "system".to_string(),
            reason: "manual".to_string(),
            custom: HashMap::new(),
            correlation_id: None,
            parent_task_id: None,
            priority: ExecutionPriority::default(),
        }
    }
}
