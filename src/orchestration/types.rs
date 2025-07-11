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
    pub task_id: i64,
    pub steps_executed: usize,
    pub total_execution_time_ms: u64,
    pub completed_at: DateTime<Utc>,
    pub step_results: Vec<StepResult>,
}

/// Information about a failed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskErrorInfo {
    pub task_id: i64,
    pub error_message: String,
    pub error_code: Option<String>,
    pub failed_steps: Vec<i64>,
    pub failed_at: DateTime<Utc>,
}

/// Result of step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: i64,
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
}

/// A step that is ready for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViableStep {
    pub step_id: i64,
    pub task_id: i64,
    pub name: String,
    pub named_step_id: i64,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub attempts: i32,
    pub retry_limit: i32,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
}

/// Task execution context from SQL functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: i64,
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
        task_id: i64,
        framework: String,
        started_at: DateTime<Utc>,
    },
    /// Viable steps discovered
    ViableStepsDiscovered {
        task_id: i64,
        step_count: usize,
        steps: Vec<ViableStep>,
    },
    /// Task orchestration completed
    TaskOrchestrationCompleted {
        task_id: i64,
        result: TaskResult,
        completed_at: DateTime<Utc>,
    },
    /// Step execution started
    StepExecutionStarted {
        step_id: i64,
        task_id: i64,
        step_name: String,
        started_at: DateTime<Utc>,
    },
    /// Step execution completed
    StepExecutionCompleted {
        step_id: i64,
        task_id: i64,
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
    /// Execute a single step
    ///
    /// This is the core method that frameworks must implement. The orchestration
    /// layer will call this method for each individual step that needs execution.
    /// Concurrency and batch operations are handled by the orchestration core.
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        task_context: &TaskContext,
    ) -> Result<StepResult, crate::orchestration::errors::OrchestrationError>;

    /// Framework name for logging/metrics
    fn framework_name(&self) -> &'static str;

    /// Get task context for execution
    async fn get_task_context(
        &self,
        task_id: i64,
    ) -> Result<TaskContext, crate::orchestration::errors::OrchestrationError>;

    /// Notify framework of task orchestration starting
    async fn on_task_start(
        &self,
        task_id: i64,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError> {
        let _ = task_id;
        Ok(())
    }

    /// Notify framework of task orchestration completion
    async fn on_task_complete(
        &self,
        task_id: i64,
        result: &TaskResult,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError> {
        let _ = (task_id, result);
        Ok(())
    }

    /// Enqueue task back to framework's queue
    async fn enqueue_task(
        &self,
        task_id: i64,
        delay: Option<Duration>,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError>;

    /// Mark task as failed in framework
    async fn mark_task_failed(
        &self,
        task_id: i64,
        error: &str,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError>;

    /// Update step state in framework's storage
    async fn update_step_state(
        &self,
        step_id: i64,
        state: &str,
        result: Option<&serde_json::Value>,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError>;

    /// Publish event through framework's event system
    async fn publish_event(
        &self,
        event: &OrchestrationEvent,
    ) -> Result<(), crate::orchestration::errors::OrchestrationError> {
        let _ = event;
        Ok(())
    }

    /// Check if framework is healthy and ready
    async fn health_check(&self) -> Result<bool, crate::orchestration::errors::OrchestrationError> {
        Ok(true)
    }

    /// Get framework-specific configuration
    async fn get_config(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, crate::orchestration::errors::OrchestrationError> {
        let _ = key;
        Ok(None)
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

/// Result of task orchestration (moved from coordinator.rs)
#[derive(Debug)]
pub enum TaskOrchestrationResult {
    /// Task completed successfully
    Complete {
        task_id: i64,
        steps_executed: usize,
        total_execution_time_ms: u64,
    },
    /// Task failed due to step failures
    Failed {
        task_id: i64,
        error: String,
        failed_steps: Vec<i64>,
    },
    /// Task is still in progress, should be re-queued
    InProgress {
        task_id: i64,
        steps_executed: usize,
        next_poll_delay_ms: u64,
    },
    /// Task is blocked waiting for dependencies
    Blocked {
        task_id: i64,
        blocking_reason: String,
    },
}
