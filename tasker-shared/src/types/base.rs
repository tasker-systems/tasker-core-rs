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
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
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
    /// Construct a new TaskSequenceStep (used primarily for testing and deserialization)
    pub fn new(
        task: TaskForOrchestration,
        workflow_step: WorkflowStepWithName,
        dependency_results: StepDependencyResultMap,
        step_definition: StepDefinition,
    ) -> Self {
        Self {
            task,
            workflow_step,
            dependency_results,
            step_definition,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::StepExecutionResult;
    use crate::models::core::task::Task;
    use crate::models::core::task_template::{
        BackoffStrategy, HandlerDefinition, RetryConfiguration, StepType,
    };
    use serde_json::json;
    use std::collections::HashMap;

    // =========================================================================
    // Helper: build a TaskSequenceStep with customizable fields
    // =========================================================================

    fn make_task(context: Option<serde_json::Value>) -> TaskForOrchestration {
        let now = Utc::now().naive_utc();
        TaskForOrchestration {
            task: Task {
                task_uuid: Uuid::now_v7(),
                named_task_uuid: Uuid::now_v7(),
                complete: false,
                requested_at: now,
                initiator: Some("test".to_string()),
                source_system: Some("unit-tests".to_string()),
                reason: None,
                tags: None,
                context,
                identity_hash: "test-hash".to_string(),
                priority: 0,
                created_at: now,
                updated_at: now,
                correlation_id: Uuid::now_v7(),
                parent_correlation_id: None,
            },
            task_name: "test_task".to_string(),
            task_version: "1.0.0".to_string(),
            namespace_name: "default".to_string(),
        }
    }

    fn make_workflow_step(
        attempts: Option<i32>,
        max_attempts: Option<i32>,
        checkpoint: Option<serde_json::Value>,
    ) -> WorkflowStepWithName {
        let now = Utc::now().naive_utc();
        WorkflowStepWithName {
            workflow_step_uuid: Uuid::now_v7(),
            task_uuid: Uuid::now_v7(),
            named_step_uuid: Uuid::now_v7(),
            name: "test_step".to_string(),
            template_step_name: "test_step".to_string(),
            retryable: true,
            max_attempts,
            in_process: false,
            processed: false,
            processed_at: None,
            attempts,
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,
            checkpoint,
            created_at: now,
            updated_at: now,
        }
    }

    fn make_step_definition(initialization: HashMap<String, serde_json::Value>) -> StepDefinition {
        StepDefinition {
            name: "test_step".to_string(),
            description: Some("A test step".to_string()),
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
                method: None,
                resolver: None,
                initialization,
            },
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration {
                retryable: true,
                max_attempts: 3,
                backoff: BackoffStrategy::Exponential,
                backoff_base_ms: Some(1000),
                max_backoff_ms: Some(30000),
            },
            timeout_seconds: None,
            publishes_events: vec![],
            batch_config: None,
        }
    }

    fn make_task_sequence_step(
        context: Option<serde_json::Value>,
        dependency_results: StepDependencyResultMap,
        initialization: HashMap<String, serde_json::Value>,
        attempts: Option<i32>,
        max_attempts: Option<i32>,
        checkpoint: Option<serde_json::Value>,
    ) -> TaskSequenceStep {
        TaskSequenceStep {
            task: make_task(context),
            workflow_step: make_workflow_step(attempts, max_attempts, checkpoint),
            dependency_results,
            step_definition: make_step_definition(initialization),
        }
    }

    // =========================================================================
    // StepEventPayload tests
    // =========================================================================

    #[test]
    fn test_step_event_payload_new() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let payload = StepEventPayload::new(task_uuid, step_uuid, tss.clone());

        assert_eq!(payload.task_uuid, task_uuid);
        assert_eq!(payload.step_uuid, step_uuid);
        assert_eq!(
            payload.task_sequence_step.task.task_name,
            tss.task.task_name
        );
    }

    // =========================================================================
    // StepExecutionEvent tests
    // =========================================================================

    #[test]
    fn test_step_execution_event_new() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        let payload = StepEventPayload::new(task_uuid, step_uuid, tss);

        let event = StepExecutionEvent::new(payload.clone());

        // event_id should be generated (non-nil)
        assert!(!event.event_id.is_nil());
        assert_eq!(event.payload.task_uuid, task_uuid);
        assert_eq!(event.payload.step_uuid, step_uuid);
        assert!(event.trace_id.is_none());
        assert!(event.span_id.is_none());
    }

    #[test]
    fn test_step_execution_event_with_event_id() {
        let event_id = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        let payload = StepEventPayload::new(task_uuid, step_uuid, tss);

        let event = StepExecutionEvent::with_event_id(event_id, payload);

        assert_eq!(event.event_id, event_id);
        assert!(event.trace_id.is_none());
        assert!(event.span_id.is_none());
    }

    #[test]
    fn test_step_execution_event_with_trace_context() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        let payload = StepEventPayload::new(task_uuid, step_uuid, tss);

        let trace_id = Some("abc123trace".to_string());
        let span_id = Some("span456".to_string());
        let event =
            StepExecutionEvent::with_trace_context(payload, trace_id.clone(), span_id.clone());

        assert!(!event.event_id.is_nil());
        assert_eq!(event.trace_id, trace_id);
        assert_eq!(event.span_id, span_id);
    }

    #[test]
    fn test_step_execution_event_with_event_id_and_trace() {
        let event_id = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        let payload = StepEventPayload::new(task_uuid, step_uuid, tss);

        let trace_id = Some("trace-xyz".to_string());
        let span_id = Some("span-789".to_string());
        let event = StepExecutionEvent::with_event_id_and_trace(
            event_id,
            payload,
            trace_id.clone(),
            span_id.clone(),
        );

        assert_eq!(event.event_id, event_id);
        assert_eq!(event.trace_id, trace_id);
        assert_eq!(event.span_id, span_id);
    }

    // =========================================================================
    // StepExecutionCompletionEvent tests
    // =========================================================================

    #[test]
    fn test_step_execution_completion_event_success() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let result = json!({"status": "ok", "data": 42});
        let metadata = Some(json!({"timing_ms": 150}));

        let event = StepExecutionCompletionEvent::success(
            task_uuid,
            step_uuid,
            result.clone(),
            metadata.clone(),
        );

        assert!(!event.event_id.is_nil());
        assert_eq!(event.task_uuid, task_uuid);
        assert_eq!(event.step_uuid, step_uuid);
        assert!(event.success);
        assert_eq!(event.result, result);
        assert_eq!(event.metadata, metadata);
        assert!(event.error_message.is_none());
        assert!(event.trace_id.is_none());
        assert!(event.span_id.is_none());
    }

    #[test]
    fn test_step_execution_completion_event_failure() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let error_msg = "Something went wrong".to_string();
        let metadata = Some(json!({"retry_count": 2}));

        let event = StepExecutionCompletionEvent::failure(
            task_uuid,
            step_uuid,
            error_msg.clone(),
            metadata.clone(),
        );

        assert!(!event.event_id.is_nil());
        assert_eq!(event.task_uuid, task_uuid);
        assert_eq!(event.step_uuid, step_uuid);
        assert!(!event.success);
        assert_eq!(event.result, serde_json::Value::Null);
        assert_eq!(event.metadata, metadata);
        assert_eq!(event.error_message, Some(error_msg));
        assert!(event.trace_id.is_none());
        assert!(event.span_id.is_none());
    }

    #[test]
    fn test_step_execution_completion_event_with_event_id() {
        let event_id = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();

        let event = StepExecutionCompletionEvent::with_event_id(
            event_id,
            task_uuid,
            step_uuid,
            true,
            json!({"done": true}),
            None,
            None,
        );

        assert_eq!(event.event_id, event_id);
        assert_eq!(event.task_uuid, task_uuid);
        assert_eq!(event.step_uuid, step_uuid);
        assert!(event.success);
        assert!(event.trace_id.is_none());
        assert!(event.span_id.is_none());
    }

    #[test]
    fn test_step_execution_completion_event_with_trace_context() {
        let event_id = Uuid::now_v7();
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let trace_id = Some("trace-completion".to_string());
        let span_id = Some("span-completion".to_string());

        let event = StepExecutionCompletionEvent::with_trace_context(
            event_id,
            task_uuid,
            step_uuid,
            false,
            json!(null),
            None,
            Some("timeout".to_string()),
            trace_id.clone(),
            span_id.clone(),
        );

        assert_eq!(event.event_id, event_id);
        assert!(!event.success);
        assert_eq!(event.error_message, Some("timeout".to_string()));
        assert_eq!(event.trace_id, trace_id);
        assert_eq!(event.span_id, span_id);
    }

    // =========================================================================
    // TaskSequenceStep::get_context_field tests
    // =========================================================================

    #[test]
    fn test_get_context_field_happy_path() {
        let context = json!({"order_id": "ORD-123", "quantity": 5});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let order_id: String = tss.get_context_field("order_id").unwrap();
        assert_eq!(order_id, "ORD-123");

        let quantity: i32 = tss.get_context_field("quantity").unwrap();
        assert_eq!(quantity, 5);
    }

    #[test]
    fn test_get_context_field_none_context() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let result: Result<String, _> = tss.get_context_field("order_id");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Task context is None"));
    }

    #[test]
    fn test_get_context_field_missing_field() {
        let context = json!({"name": "test"});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let result: Result<String, _> = tss.get_context_field("nonexistent");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    // =========================================================================
    // TaskSequenceStep::get_context_field_raw tests
    // =========================================================================

    #[test]
    fn test_get_context_field_raw_happy_path() {
        let context = json!({"raw_val": [1, 2, 3]});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let raw = tss.get_context_field_raw("raw_val").unwrap();
        assert_eq!(*raw, json!([1, 2, 3]));
    }

    // =========================================================================
    // TaskSequenceStep::has_context_field tests
    // =========================================================================

    #[test]
    fn test_has_context_field_true() {
        let context = json!({"exists": "yes"});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        assert!(tss.has_context_field("exists"));
    }

    #[test]
    fn test_has_context_field_false() {
        let context = json!({"exists": "yes"});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        assert!(!tss.has_context_field("missing"));
    }

    #[test]
    fn test_has_context_field_none_context() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(!tss.has_context_field("anything"));
    }

    // =========================================================================
    // TaskSequenceStep::has_dependency_result tests
    // =========================================================================

    #[test]
    fn test_has_dependency_result_true() {
        let step_result =
            StepExecutionResult::success(Uuid::now_v7(), json!({"ok": true}), 100, None);
        let mut deps: StepDependencyResultMap = HashMap::new();
        deps.insert("validate_step".to_string(), step_result);

        let tss = make_task_sequence_step(None, deps, HashMap::new(), None, None, None);

        assert!(tss.has_dependency_result("validate_step"));
    }

    #[test]
    fn test_has_dependency_result_false() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(!tss.has_dependency_result("nonexistent_step"));
    }

    // =========================================================================
    // TaskSequenceStep::get_context_field_names tests
    // =========================================================================

    #[test]
    fn test_get_context_field_names_with_fields() {
        let context = json!({"alpha": 1, "beta": 2, "gamma": 3});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let mut names = tss.get_context_field_names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn test_get_context_field_names_none_context() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(tss.get_context_field_names().is_empty());
    }

    // =========================================================================
    // TaskSequenceStep::get_dependency_step_names tests
    // =========================================================================

    #[test]
    fn test_get_dependency_step_names_with_deps() {
        let mut deps: StepDependencyResultMap = HashMap::new();
        deps.insert(
            "step_a".to_string(),
            StepExecutionResult::success(Uuid::now_v7(), json!({}), 50, None),
        );
        deps.insert(
            "step_b".to_string(),
            StepExecutionResult::success(Uuid::now_v7(), json!({}), 50, None),
        );

        let tss = make_task_sequence_step(None, deps, HashMap::new(), None, None, None);

        let mut names = tss.get_dependency_step_names();
        names.sort();
        assert_eq!(names, vec!["step_a", "step_b"]);
    }

    #[test]
    fn test_get_dependency_step_names_empty() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(tss.get_dependency_step_names().is_empty());
    }

    // =========================================================================
    // TaskSequenceStep::get_input (alias) tests
    // =========================================================================

    #[test]
    fn test_get_input_alias_for_get_context_field() {
        let context = json!({"order_id": "ORD-456"});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let order_id: String = tss.get_input("order_id").unwrap();
        assert_eq!(order_id, "ORD-456");
    }

    // =========================================================================
    // TaskSequenceStep::get_input_or tests
    // =========================================================================

    #[test]
    fn test_get_input_or_returns_value_when_present() {
        let context = json!({"batch_size": 500});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let batch_size: i32 = tss.get_input_or("batch_size", 100);
        assert_eq!(batch_size, 500);
    }

    #[test]
    fn test_get_input_or_returns_default_when_absent() {
        let context = json!({"other_field": "value"});
        let tss = make_task_sequence_step(
            Some(context),
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            None,
        );

        let batch_size: i32 = tss.get_input_or("batch_size", 100);
        assert_eq!(batch_size, 100);
    }

    // =========================================================================
    // TaskSequenceStep::get_config tests
    // =========================================================================

    #[test]
    fn test_get_config_happy_path() {
        let mut init = HashMap::new();
        init.insert("api_url".to_string(), json!("https://api.example.com"));
        init.insert("timeout_seconds".to_string(), json!(30));

        let tss = make_task_sequence_step(None, HashMap::new(), init, None, None, None);

        let api_url: String = tss.get_config("api_url").unwrap();
        assert_eq!(api_url, "https://api.example.com");

        let timeout: u64 = tss.get_config("timeout_seconds").unwrap();
        assert_eq!(timeout, 30);
    }

    #[test]
    fn test_get_config_missing_key() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let result: Result<String, _> = tss.get_config("nonexistent_key");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Config key"));
        assert!(err_msg.contains("not found"));
    }

    // =========================================================================
    // TaskSequenceStep::get_dependency_field tests
    // =========================================================================

    #[test]
    fn test_get_dependency_field_nested_path() {
        let step_result = StepExecutionResult::success(
            Uuid::now_v7(),
            json!({"data": {"count": 42, "nested": {"deep": "value"}}}),
            100,
            None,
        );
        let mut deps: StepDependencyResultMap = HashMap::new();
        deps.insert("step_1".to_string(), step_result);

        let tss = make_task_sequence_step(None, deps, HashMap::new(), None, None, None);

        let count: i32 = tss
            .get_dependency_field("step_1", &["data", "count"])
            .unwrap();
        assert_eq!(count, 42);

        let deep: String = tss
            .get_dependency_field("step_1", &["data", "nested", "deep"])
            .unwrap();
        assert_eq!(deep, "value");
    }

    // =========================================================================
    // TaskSequenceStep::get_dependency_result_column_value tests
    // =========================================================================

    #[test]
    fn test_get_dependency_result_column_value_happy() {
        let step_result =
            StepExecutionResult::success(Uuid::now_v7(), json!({"total": 99.99}), 50, None);
        let mut deps: StepDependencyResultMap = HashMap::new();
        deps.insert("calc_step".to_string(), step_result);

        let tss = make_task_sequence_step(None, deps, HashMap::new(), None, None, None);

        let result: serde_json::Value =
            tss.get_dependency_result_column_value("calc_step").unwrap();
        assert_eq!(result["total"], json!(99.99));
    }

    #[test]
    fn test_get_dependency_result_column_value_missing() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let result: Result<serde_json::Value, _> =
            tss.get_dependency_result_column_value("nonexistent");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    // =========================================================================
    // TaskSequenceStep::is_retry tests
    // =========================================================================

    #[test]
    fn test_is_retry_zero_attempts() {
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(0), Some(3), None);
        assert!(!tss.is_retry());
    }

    #[test]
    fn test_is_retry_positive_attempts() {
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(1), Some(3), None);
        assert!(tss.is_retry());
    }

    #[test]
    fn test_is_retry_none_attempts() {
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, Some(3), None);
        assert!(!tss.is_retry());
    }

    // =========================================================================
    // TaskSequenceStep::is_last_retry tests
    // =========================================================================

    #[test]
    fn test_is_last_retry_at_max_minus_one() {
        // attempts=2, max_attempts=3 => 2 >= 3-1=2 => true
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(2), Some(3), None);
        assert!(tss.is_last_retry());
    }

    #[test]
    fn test_is_last_retry_below_threshold() {
        // attempts=0, max_attempts=3 => 0 >= 2 => false
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(0), Some(3), None);
        assert!(!tss.is_last_retry());
    }

    #[test]
    fn test_is_last_retry_exceeds_max() {
        // attempts=5, max_attempts=3 => 5 >= 2 => true
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(5), Some(3), None);
        assert!(tss.is_last_retry());
    }

    #[test]
    fn test_is_last_retry_none_defaults() {
        // attempts=None (0), max_attempts=None (3) => 0 >= 2 => false
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        assert!(!tss.is_last_retry());
    }

    // =========================================================================
    // TaskSequenceStep::retry_count and max_retries tests
    // =========================================================================

    #[test]
    fn test_retry_count_with_some() {
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), Some(4), Some(5), None);
        assert_eq!(tss.retry_count(), 4);
    }

    #[test]
    fn test_retry_count_with_none() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        assert_eq!(tss.retry_count(), 0);
    }

    #[test]
    fn test_max_retries_with_some() {
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, Some(10), None);
        assert_eq!(tss.max_retries(), 10);
    }

    #[test]
    fn test_max_retries_with_none() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);
        assert_eq!(tss.max_retries(), 3);
    }

    // =========================================================================
    // TaskSequenceStep::checkpoint tests
    // =========================================================================

    #[test]
    fn test_checkpoint_some() {
        let cp = json!({"cursor": 42, "items_processed": 100});
        let tss = make_task_sequence_step(
            None,
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            Some(cp.clone()),
        );

        assert_eq!(tss.checkpoint(), Some(&cp));
    }

    #[test]
    fn test_checkpoint_none() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(tss.checkpoint().is_none());
    }

    // =========================================================================
    // TaskSequenceStep::checkpoint_cursor tests
    // =========================================================================

    #[test]
    fn test_checkpoint_cursor_with_cursor() {
        let cp = json!({"cursor": 42});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        let cursor: Option<i64> = tss.checkpoint_cursor();
        assert_eq!(cursor, Some(42));
    }

    #[test]
    fn test_checkpoint_cursor_without_cursor() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let cursor: Option<i64> = tss.checkpoint_cursor();
        assert_eq!(cursor, None);
    }

    #[test]
    fn test_checkpoint_cursor_no_cursor_field() {
        let cp = json!({"other": "data"});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        let cursor: Option<i64> = tss.checkpoint_cursor();
        assert_eq!(cursor, None);
    }

    // =========================================================================
    // TaskSequenceStep::checkpoint_items_processed tests
    // =========================================================================

    #[test]
    fn test_checkpoint_items_processed_with_field() {
        let cp = json!({"items_processed": 250});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        assert_eq!(tss.checkpoint_items_processed(), 250);
    }

    #[test]
    fn test_checkpoint_items_processed_without_field() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert_eq!(tss.checkpoint_items_processed(), 0);
    }

    // =========================================================================
    // TaskSequenceStep::accumulated_results tests
    // =========================================================================

    #[test]
    fn test_accumulated_results_with_field() {
        let cp = json!({"accumulated_results": {"total": 1000, "errors": 5}});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        let acc: Option<serde_json::Value> = tss.accumulated_results();
        assert!(acc.is_some());
        let acc = acc.unwrap();
        assert_eq!(acc["total"], 1000);
        assert_eq!(acc["errors"], 5);
    }

    #[test]
    fn test_accumulated_results_without_field() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        let acc: Option<serde_json::Value> = tss.accumulated_results();
        assert!(acc.is_none());
    }

    // =========================================================================
    // TaskSequenceStep::has_checkpoint tests
    // =========================================================================

    #[test]
    fn test_has_checkpoint_with_cursor() {
        let cp = json!({"cursor": 10});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        assert!(tss.has_checkpoint());
    }

    #[test]
    fn test_has_checkpoint_without_cursor() {
        let cp = json!({"items_processed": 10});
        let tss =
            make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, Some(cp));

        assert!(!tss.has_checkpoint());
    }

    #[test]
    fn test_has_checkpoint_none() {
        let tss = make_task_sequence_step(None, HashMap::new(), HashMap::new(), None, None, None);

        assert!(!tss.has_checkpoint());
    }

    // =========================================================================
    // TaskSequenceStep::new constructor test
    // =========================================================================

    #[test]
    fn test_task_sequence_step_new_constructor() {
        let task = make_task(Some(json!({"key": "val"})));
        let ws = make_workflow_step(Some(1), Some(5), None);
        let deps = HashMap::new();
        let sd = make_step_definition(HashMap::new());

        let tss = TaskSequenceStep::new(task.clone(), ws.clone(), deps, sd.clone());

        assert_eq!(tss.task.task_name, task.task_name);
        assert_eq!(tss.workflow_step.name, ws.name);
        assert_eq!(tss.step_definition.name, sd.name);
        assert!(tss.dependency_results.is_empty());
    }
}
