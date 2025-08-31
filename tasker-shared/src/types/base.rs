//! # Base Shared Types
//!
//! Core types and data structures used throughout the Tasker system.
//!
//! This module provides the fundamental types that are shared across all orchestration
//! components, including task results, step results, handler metadata, and configuration
//! structures.

use crate::models::core::task_template::StepDefinition;
use crate::models::core::{task::TaskForOrchestration, workflow_step::WorkflowStepWithName};
use crate::models::orchestration::StepDependencyResultMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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

// ===== TAS-40 Worker-FFI Event System Types =====

/// Event payload for step execution events sent from Rust workers to FFI handlers
///
/// This payload contains all the information needed for an FFI handler (Ruby, Python, WASM)
/// to execute a step, following the database-as-API-layer pattern where the worker
/// hydrates all necessary context from the database using SimpleStepMessage UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEventPayload {
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub task_sequence_step: TaskSequenceStep,
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
    pub fn new(task_uuid: Uuid, step_uuid: Uuid, task_sequence_step: TaskSequenceStep) -> Self {
        Self {
            task_uuid,
            step_uuid,
            task_sequence_step,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSequenceStep {
    pub task: TaskForOrchestration,
    pub workflow_step: WorkflowStepWithName,
    pub dependency_results: StepDependencyResultMap,
    pub step_definition: StepDefinition,
}

impl TaskSequenceStep {
    /// Get a field from the task context with automatic type conversion
    ///
    /// This method provides ergonomic access to task context fields, automatically
    /// deserializing the JSON value to the requested type T.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Get a string field
    /// let order_id: String = step_data.get_context_field("order_id")?;
    ///
    /// // Get a numeric field
    /// let quantity: i32 = step_data.get_context_field("quantity")?;
    ///
    /// // Get a complex object
    /// let user_info: UserInfo = step_data.get_context_field("user")?;
    /// ```
    pub fn get_context_field<T>(&self, field_name: &str) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let context = self
            .task
            .task
            .context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task context is None"))?;

        let field_value = context
            .get(field_name)
            .ok_or_else(|| anyhow::anyhow!("Context field '{}' not found", field_name))?;

        serde_json::from_value(field_value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialize field '{}': {}", field_name, e))
    }

    /// Get a field from the task context as a raw JSON value
    ///
    /// This method provides direct access to context fields as serde_json::Value
    /// for cases where you need to inspect the raw JSON or handle dynamic types.
    pub fn get_context_field_raw(
        &self,
        field_name: &str,
    ) -> Result<&serde_json::Value, anyhow::Error> {
        let context = self
            .task
            .task
            .context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Task context is None"))?;

        context
            .get(field_name)
            .ok_or_else(|| anyhow::anyhow!("Context field '{}' not found", field_name))
    }

    /// Get the result of a dependency step with automatic type conversion
    ///
    /// This method provides ergonomic access to previous step results, automatically
    /// deserializing the JSON result to the requested type T.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Get a simple value result
    /// let calculated_value: i64 = step_data.get_dependency_result("calculate_step")?;
    ///
    /// // Get a complex result object
    /// let validation_result: ValidationResult = step_data.get_dependency_result("validate_step")?;
    /// ```
    pub fn get_dependency_result<T>(&self, step_name: &str) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let step_result = self.dependency_results.get(step_name).ok_or_else(|| {
            anyhow::anyhow!("Dependency result for step '{}' not found", step_name)
        })?;

        serde_json::from_value(step_result.clone()).map_err(|e| {
            anyhow::anyhow!(
                "Failed to deserialize result from step '{}': {}",
                step_name,
                e
            )
        })
    }

    /// Get the result of a dependency step as a raw JSON value
    ///
    /// This method provides direct access to step results as serde_json::Value
    /// for cases where you need to inspect the raw JSON or handle dynamic result types.
    pub fn get_dependency_result_raw(
        &self,
        step_name: &str,
    ) -> Result<&serde_json::Value, anyhow::Error> {
        self.dependency_results
            .get(step_name)
            .ok_or_else(|| anyhow::anyhow!("Dependency result for step '{}' not found", step_name))
    }

    /// Check if a context field exists
    pub fn has_context_field(&self, field_name: &str) -> bool {
        self.task
            .task
            .context
            .as_ref()
            .map(|context| {
                if let serde_json::Value::Object(map) = context {
                    map.contains_key(field_name)
                } else {
                    false
                }
            })
            .unwrap_or(false)
    }

    /// Check if a dependency result exists
    pub fn has_dependency_result(&self, step_name: &str) -> bool {
        self.dependency_results.contains_key(step_name)
    }

    /// Get all context field names
    pub fn get_context_field_names(&self) -> Vec<String> {
        self.task
            .task
            .context
            .as_ref()
            .map(|context| {
                if let serde_json::Value::Object(map) = context {
                    map.keys().cloned().collect()
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default()
    }

    /// Get all dependency result step names
    pub fn get_dependency_step_names(&self) -> Vec<String> {
        self.dependency_results.keys().cloned().collect()
    }
}
