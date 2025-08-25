//! # Orchestration Types
//!
//! Core types and data structures used throughout the orchestration system.
//!
//! This module provides the fundamental types that are shared across all orchestration
//! components, including task results, step results, handler metadata, and configuration
//! structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

// Import StepExecutionContext from step_handler module

/// Result of task orchestration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskResult {
    /// Task completed successfully
    Complete(TaskCompletionInfo),
    /// Task failed due to step failures
    Error(TaskErrorInfo),
    /// Task should be re-queued immediately
    ReenqueueImmediate,
    /// Task should be re-queued after delay
    ReenqueueDelayed(Duration),
}

/// Information about a completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompletionInfo {
    pub task_uuid: Uuid,
    pub steps_executed: usize,
    pub total_execution_time_ms: u64,
    pub completed_at: DateTime<Utc>,
    pub step_results: Vec<StepResult>,
}

/// Information about a failed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskErrorInfo {
    pub task_uuid: Uuid,
    pub error_message: String,
    pub error_code: Option<String>,
    pub failed_steps: Vec<i64>,
    pub failed_at: DateTime<Utc>,
}

/// Result of step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_uuid: Uuid,
    pub status: StepStatus,
    pub output: serde_json::Value,
    pub execution_duration: Duration,
    pub error_message: Option<String>,
    pub retry_after: Option<Duration>,
    pub error_code: Option<String>,
    pub error_context: Option<HashMap<String, serde_json::Value>>,
}

/// Status of step execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StepStatus {
    /// Step completed successfully
    Completed,
    /// Step failed with error
    Failed,
    /// Step is retrying
    Retrying,
    /// Step was skipped
    Skipped,
    /// Step is in progress (published but not yet completed)
    InProgress,
}

/// A step that is ready for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViableStep {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub name: String,
    pub named_step_uuid: Uuid,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub attempts: i32,
    pub retry_limit: i32,
    pub last_failure_at: Option<chrono::NaiveDateTime>,
    pub next_retry_at: Option<chrono::NaiveDateTime>,
}

/// Task execution context from SQL functions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskContext {
    pub task_uuid: Uuid,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Handler metadata for registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerMetadata {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub handler_class: String,
    pub config_schema: Option<serde_json::Value>,
    pub default_dependent_system: Option<String>,
    pub registered_at: DateTime<Utc>,
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_handlers: usize,
    pub total_ffi_handlers: usize,
    pub namespaces: Vec<String>,
    pub thread_safe: bool,
}

// Legacy TaskHandlerConfig and StepTemplate removed - use crate::models::core::task_template::TaskTemplate instead

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: i32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub jitter: bool,
}

/// Orchestration event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrchestrationEvent {
    /// Task orchestration started
    TaskOrchestrationStarted {
        task_uuid: Uuid,
        framework: String,
        started_at: DateTime<Utc>,
    },
    /// Viable steps discovered
    ViableStepsDiscovered {
        task_uuid: Uuid,
        step_count: usize,
        steps: Vec<ViableStep>,
    },
    /// Task orchestration completed
    TaskOrchestrationCompleted {
        task_uuid: Uuid,
        result: TaskResult,
        completed_at: DateTime<Utc>,
    },
    /// Step execution started
    StepExecutionStarted {
        step_uuid: Uuid,
        task_uuid: Uuid,
        step_name: String,
        started_at: DateTime<Utc>,
    },
    /// Step execution completed
    StepExecutionCompleted {
        step_uuid: Uuid,
        task_uuid: Uuid,
        result: StepResult,
        completed_at: DateTime<Utc>,
    },
    /// Handler registered
    HandlerRegistered {
        key: String,
        metadata: HandlerMetadata,
        registered_at: DateTime<Utc>,
    },
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl TaskResult {
    /// Check if task completed successfully
    pub fn is_success(&self) -> bool {
        matches!(self, TaskResult::Complete(_))
    }

    /// Check if task failed
    pub fn is_error(&self) -> bool {
        matches!(self, TaskResult::Error(_))
    }

    /// Check if task should be re-queued
    pub fn should_requeue(&self) -> bool {
        matches!(
            self,
            TaskResult::ReenqueueImmediate | TaskResult::ReenqueueDelayed(_)
        )
    }
}

impl StepResult {
    /// Check if step completed successfully
    pub fn is_success(&self) -> bool {
        self.status == StepStatus::Completed
    }

    /// Check if step failed
    pub fn is_failure(&self) -> bool {
        self.status == StepStatus::Failed
    }

    /// Check if step should be retried
    pub fn should_retry(&self) -> bool {
        self.status == StepStatus::Retrying
    }
}

/// Result of task orchestration - Updated for fire-and-forget ZeroMQ architecture
#[derive(Debug)]
pub enum TaskOrchestrationResult {
    /// Task completed successfully (from async result processing)
    Complete {
        task_uuid: Uuid,
        steps_completed: usize,
        total_execution_time_ms: u64,
    },
    /// Task failed due to step failures (from async result processing)
    Failed {
        task_uuid: Uuid,
        error: String,
        failed_steps: Vec<i64>,
    },
    /// Fire-and-forget: Steps published to ZeroMQ, execution continuing asynchronously
    Published {
        task_uuid: Uuid,
        viable_steps_discovered: usize,
        steps_published: usize,
        batch_id: Option<String>,
        publication_time_ms: u64,
        next_poll_delay_ms: u64,
    },
    /// Task is blocked waiting for dependencies
    Blocked {
        task_uuid: Uuid,
        blocking_reason: String,
        viable_steps_checked: usize,
    },
}

// ===== TAS-40 Worker-FFI Event System Types =====

/// Event payload for step execution events sent from Rust workers to FFI handlers
///
/// This payload contains all the information needed for an FFI handler (Ruby, Python, WASM)
/// to execute a step, following the database-as-API-layer pattern where the worker
/// hydrates all necessary context from the database using SimpleStepMessage UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEventPayload {
    /// Task UUID for the step being executed
    pub task_uuid: Uuid,
    /// Workflow step UUID for the step being executed
    pub step_uuid: Uuid,
    /// Human-readable step name from the task template
    pub step_name: String,
    /// Handler class to execute (e.g., "OrderFulfillment::StepHandler::ProcessPayment")
    pub handler_class: String,
    /// Step-specific payload/configuration from the task template
    pub step_payload: serde_json::Value,
    /// Results from dependency steps, keyed by step name for easy lookup
    pub dependency_results: HashMap<String, serde_json::Value>,
    /// Execution context matching Ruby expectations (task, sequence, step structure)
    pub execution_context: serde_json::Value,
}

/// Event for step execution requests (Rust → FFI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionEvent {
    /// Unique event identifier for correlation
    pub event_id: Uuid,
    /// Hydrated step payload with all execution context
    pub payload: StepEventPayload,
}

/// Event for step execution completion (FFI → Rust)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionCompletionEvent {
    /// Unique event identifier for correlation
    pub event_id: Uuid,
    /// Task UUID that this step belongs to
    pub task_uuid: Uuid,
    /// Workflow step UUID that was executed
    pub step_uuid: Uuid,
    /// Whether the step execution was successful
    pub success: bool,
    /// Step execution result data
    pub result: serde_json::Value,
    /// Optional metadata about the execution (timing, diagnostics, etc.)
    pub metadata: Option<serde_json::Value>,
    /// Error message if step execution failed
    pub error_message: Option<String>,
}

/// Worker event types for categorizing different events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerEventType {
    /// Step execution request sent to FFI handler
    StepExecutionRequest,
    /// Step execution completion notification from FFI handler
    StepExecutionCompletion,
    /// Worker status update
    WorkerStatusUpdate,
    /// Handler registration notification
    HandlerRegistration,
}

/// Generic event payload for worker events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    /// Step execution event payload
    StepExecution(StepEventPayload),
    /// Step completion event payload
    StepCompletion(StepExecutionCompletionEvent),
    /// Generic JSON payload for other event types
    Generic(serde_json::Value),
}

impl StepEventPayload {
    /// Create a new step event payload with all required context
    pub fn new(
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: String,
        handler_class: String,
        step_payload: serde_json::Value,
        dependency_results: HashMap<String, serde_json::Value>,
        execution_context: serde_json::Value,
    ) -> Self {
        Self {
            task_uuid,
            step_uuid,
            step_name,
            handler_class,
            step_payload,
            dependency_results,
            execution_context,
        }
    }
}

impl StepExecutionEvent {
    /// Create a new step execution event with generated event ID
    pub fn new(payload: StepEventPayload) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            payload,
        }
    }

    /// Create a step execution event with specific event ID (for correlation)
    pub fn with_event_id(event_id: Uuid, payload: StepEventPayload) -> Self {
        Self { event_id, payload }
    }
}

impl StepExecutionCompletionEvent {
    /// Create a successful completion event
    pub fn success(
        task_uuid: Uuid,
        step_uuid: Uuid,
        result: serde_json::Value,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            task_uuid,
            step_uuid,
            success: true,
            result,
            metadata,
            error_message: None,
        }
    }

    /// Create a failed completion event
    pub fn failure(
        task_uuid: Uuid,
        step_uuid: Uuid,
        error_message: String,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            task_uuid,
            step_uuid,
            success: false,
            result: serde_json::Value::Null,
            metadata,
            error_message: Some(error_message),
        }
    }

    /// Create a completion event with specific event ID (for correlation)
    pub fn with_event_id(
        event_id: Uuid,
        task_uuid: Uuid,
        step_uuid: Uuid,
        success: bool,
        result: serde_json::Value,
        metadata: Option<serde_json::Value>,
        error_message: Option<String>,
    ) -> Self {
        Self {
            event_id,
            task_uuid,
            step_uuid,
            success,
            result,
            metadata,
            error_message,
        }
    }
}
