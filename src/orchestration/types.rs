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

/// Configuration for YAML processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandlerConfig {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub steps: Vec<StepTemplate>,
    pub schema: Option<serde_json::Value>,
    pub retry_policy: Option<RetryPolicy>,
    pub environment_overrides: Option<HashMap<String, serde_json::Value>>,
}

/// Step template from YAML configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplate {
    pub name: String,
    pub handler_class: String,
    pub dependencies: Vec<String>,
    pub retry_limit: Option<i32>,
    pub timeout: Option<Duration>,
    pub config: Option<serde_json::Value>,
}

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

/// Framework integration trait for delegation
///
/// This trait defines the interface for framework-specific step execution.
/// The orchestration core handles concurrency, DAG traversal, and viable step
/// discovery, while frameworks only need to implement individual step execution.
#[async_trait::async_trait]
pub trait FrameworkIntegration: Send + Sync {
    /// Framework name for logging/metrics
    fn framework_name(&self) -> &'static str;

    /// Get task context for execution
    async fn get_task_context(
        &self,
        task_uuid: Uuid,
    ) -> Result<TaskContext, crate::orchestration::errors::OrchestrationError>;

    /// Enqueue task back to framework's queue
    async fn enqueue_task(
        &self,
        task_uuid: Uuid,
        delay: Option<Duration>,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError>;

    /// Check if this framework supports native batch execution
    ///
    /// All frameworks now support batch execution since individual step execution
    /// has been removed. This method can be used to distinguish between frameworks
    /// that implement true parallelism vs sequential batch processing.
    fn supports_batch_execution(&self) -> bool {
        true // All frameworks must support batch execution
    }
}

/// Task handler trait for registry
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync {
    /// Handle task execution
    async fn handle_task(
        &self,
        task_context: &TaskContext,
    ) -> Result<TaskResult, crate::orchestration::errors::OrchestrationError>;

    /// Get handler metadata
    fn metadata(&self) -> HandlerMetadata;
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
