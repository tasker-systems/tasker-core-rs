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
use uuid::Uuid;

/// Statistics about the task template cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_cached: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_evictions: u64,
    pub oldest_entry_age_seconds: u64,
    pub average_access_count: f64,
    pub supported_namespaces: Vec<String>,
}

// ViableStep is now defined in events::types - re-export for backwards compatibility
pub use crate::events::types::ViableStep;

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
/// hydrates all necessary context from the database using StepMessage UUIDs.
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
    /// TAS-65 Phase 1.5b: Trace ID for distributed tracing (propagated to FFI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// TAS-65 Phase 1.5b: Span ID for distributed tracing (propagated to FFI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
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
    /// TAS-65 Phase 1.5b: Trace ID for distributed tracing (propagated from FFI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// TAS-65 Phase 1.5b: Span ID for distributed tracing (propagated from FFI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
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
            trace_id: None,
            span_id: None,
        }
    }

    /// Create a step execution event with specific event ID (for correlation)
    pub fn with_event_id(event_id: Uuid, payload: StepEventPayload) -> Self {
        Self {
            event_id,
            payload,
            trace_id: None,
            span_id: None,
        }
    }

    /// TAS-65 Phase 1.5b: Create a step execution event with trace context for distributed tracing
    pub fn with_trace_context(
        payload: StepEventPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            payload,
            trace_id,
            span_id,
        }
    }

    /// TAS-65 Phase 1.5b: Create a step execution event with event ID and trace context
    pub fn with_event_id_and_trace(
        event_id: Uuid,
        payload: StepEventPayload,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Self {
        Self {
            event_id,
            payload,
            trace_id,
            span_id,
        }
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
            trace_id: None,
            span_id: None,
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
            trace_id: None,
            span_id: None,
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
            trace_id: None,
            span_id: None,
        }
    }

    /// TAS-65 Phase 1.5b: Create a completion event with trace context for distributed tracing
    pub fn with_trace_context(
        event_id: Uuid,
        task_uuid: Uuid,
        step_uuid: Uuid,
        success: bool,
        result: serde_json::Value,
        metadata: Option<serde_json::Value>,
        error_message: Option<String>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Self {
        Self {
            event_id,
            task_uuid,
            step_uuid,
            success,
            result,
            metadata,
            error_message,
            trace_id,
            span_id,
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
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # use serde::{Deserialize, Serialize};
    /// # #[derive(Deserialize, Serialize)]
    /// # struct UserInfo { name: String }
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// // Get a string field
    /// let order_id: String = step_data.get_context_field("order_id")?;
    ///
    /// // Get a numeric field
    /// let quantity: i32 = step_data.get_context_field("quantity")?;
    ///
    /// // Get a complex object
    /// let user_info: UserInfo = step_data.get_context_field("user")?;
    /// # Ok::<(), anyhow::Error>(())
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
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # use serde::{Deserialize, Serialize};
    /// # #[derive(Deserialize, Serialize)]
    /// # struct ValidationResult { valid: bool }
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// // Get a simple value result
    /// let calculated_value: i64 = step_data.get_dependency_result_column_value("calculate_step")?;
    ///
    /// // Get a complex result object
    /// let validation_result: ValidationResult = step_data.get_dependency_result_column_value("validate_step")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_dependency_result_column_value<T>(&self, step_name: &str) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let step_result = self.dependency_results.get(step_name).ok_or_else(|| {
            anyhow::anyhow!("Dependency result for step '{}' not found", step_name)
        })?;

        let column_value: T = serde_json::from_value(step_result.result.clone()).map_err(|e| {
            anyhow::anyhow!(
                "Failed to deserialize result from step '{}': {}",
                step_name,
                e
            )
        })?;

        Ok(column_value)
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

    // =========================================================================
    // CROSS-LANGUAGE STANDARD ACCESSORS (TAS-137)
    // =========================================================================

    /// Get a field from the task context - cross-language standard alias
    ///
    /// This is an alias for `get_context_field` that matches the naming convention
    /// used in Python (`get_input`), Ruby (`get_input`), and TypeScript (`getInput`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// let order_id: String = step_data.get_input("order_id")?;
    /// let quantity: i32 = step_data.get_input("quantity")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_input<T>(&self, field_name: &str) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        self.get_context_field(field_name)
    }

    /// Get a field from the task context with a default value
    ///
    /// Returns the default value if the field doesn't exist or deserialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// let batch_size: i32 = step_data.get_input_or("batch_size", 100);
    /// ```
    pub fn get_input_or<T>(&self, field_name: &str, default: T) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        self.get_context_field(field_name).unwrap_or(default)
    }

    /// Get a configuration value from the handler initialization
    ///
    /// Accesses values from `step_definition.handler.initialization`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// let api_url: String = step_data.get_config("api_url")?;
    /// let timeout: u64 = step_data.get_config("timeout_seconds")?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_config<T>(&self, key: &str) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let init = &self.step_definition.handler.initialization;
        let value = init
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("Config key '{}' not found", key))?;
        serde_json::from_value(value.clone())
            .map_err(|e| anyhow::anyhow!("Failed to deserialize config key '{}': {}", key, e))
    }

    /// Extract a nested field from a dependency result
    ///
    /// Useful when dependency results are complex objects and you need
    /// to extract a specific nested value.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// // Extract a nested field
    /// let csv_path: String = step_data.get_dependency_field("analyze_csv", &["csv_file_path"])?;
    ///
    /// // Multiple levels deep
    /// let value: i32 = step_data.get_dependency_field("step_1", &["data", "count"])?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_dependency_field<T>(
        &self,
        step_name: &str,
        path: &[&str],
    ) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let step_result = self.dependency_results.get(step_name).ok_or_else(|| {
            anyhow::anyhow!("Dependency result for step '{}' not found", step_name)
        })?;

        let mut value = &step_result.result;
        for key in path {
            value = value
                .get(*key)
                .ok_or_else(|| anyhow::anyhow!("Path element '{}' not found in result", key))?;
        }

        serde_json::from_value(value.clone()).map_err(|e| {
            anyhow::anyhow!(
                "Failed to deserialize field at path {:?} from step '{}': {}",
                path,
                step_name,
                e
            )
        })
    }

    // =========================================================================
    // RETRY HELPERS (TAS-137)
    // =========================================================================

    /// Check if this execution is a retry attempt
    ///
    /// Returns true if the step has been attempted at least once before.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// if step_data.is_retry() {
    ///     println!("Retrying step, attempt {}", step_data.retry_count());
    /// }
    /// ```
    pub fn is_retry(&self) -> bool {
        self.workflow_step.attempts.unwrap_or(0) > 0
    }

    /// Check if this is the last retry attempt
    ///
    /// Returns true if the current attempt count equals or exceeds max_attempts - 1.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// if step_data.is_last_retry() {
    ///     // Send alert or take special action on final attempt
    /// }
    /// ```
    pub fn is_last_retry(&self) -> bool {
        let attempts = self.workflow_step.attempts.unwrap_or(0);
        let max = self.workflow_step.max_attempts.unwrap_or(3);
        attempts >= max.saturating_sub(1)
    }

    /// Get the current retry attempt count
    pub fn retry_count(&self) -> i32 {
        self.workflow_step.attempts.unwrap_or(0)
    }

    /// Get the maximum retry attempts allowed
    pub fn max_retries(&self) -> i32 {
        self.workflow_step.max_attempts.unwrap_or(3)
    }

    // =========================================================================
    // CHECKPOINT ACCESSORS (TAS-125/TAS-137 Batch Processing Support)
    // =========================================================================

    /// Get the raw checkpoint data from the workflow step
    ///
    /// Returns the checkpoint JSONB value if set.
    pub fn checkpoint(&self) -> Option<&serde_json::Value> {
        self.workflow_step.checkpoint.as_ref()
    }

    /// Get the checkpoint cursor position with type conversion
    ///
    /// The cursor represents the current position in batch processing,
    /// allowing handlers to resume from where they left off.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::types::base::TaskSequenceStep;
    /// # let step_data: TaskSequenceStep = panic!("Example only");
    /// let cursor: Option<i64> = step_data.checkpoint_cursor();
    /// let start_from = cursor.unwrap_or(0);
    /// ```
    pub fn checkpoint_cursor<T>(&self) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.checkpoint()
            .and_then(|cp| cp.get("cursor"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the number of items processed in the current batch run
    pub fn checkpoint_items_processed(&self) -> i64 {
        self.checkpoint()
            .and_then(|cp| cp.get("items_processed"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }

    /// Get the accumulated results from batch processing
    ///
    /// Accumulated results allow handlers to maintain running totals
    /// or aggregated state across checkpoint boundaries.
    pub fn accumulated_results<T>(&self) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.checkpoint()
            .and_then(|cp| cp.get("accumulated_results"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Check if a checkpoint exists for this step
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint().and_then(|cp| cp.get("cursor")).is_some()
    }
}
