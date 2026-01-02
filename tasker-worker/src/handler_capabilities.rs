//! # TAS-112: Ergonomic Handler Capability Traits
//!
//! This module provides composable traits for common step handler patterns,
//! following the Ruby mixin pattern adapted for Rust's trait system.
//!
//! ## Design Philosophy
//!
//! - **Composition over inheritance**: Traits compose independently
//! - **Cross-language consistency**: Matches Ruby/Python/TypeScript patterns
//! - **Type safety**: Leverages Rust's type system for compile-time guarantees
//! - **Ergonomic API**: Reduces boilerplate for common handler patterns
//!
//! ## Available Traits
//!
//! - [`APICapable`]: HTTP/API response handling with status classification
//! - [`DecisionCapable`]: Decision point handlers with branching logic
//! - [`BatchableCapable`]: Batch processing with cursor-based pagination
//!
//! ## Example Usage
//!
//! ```ignore
//! use tasker_worker::handler_capabilities::{APICapable, DecisionCapable};
//!
//! pub struct MyApiHandler {
//!     // handler fields
//! }
//!
//! impl APICapable for MyApiHandler {}
//! impl DecisionCapable for MyApiHandler {}
//!
//! // Now the handler can use api_success(), decision_success(), etc.
//! ```

use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use tasker_shared::messaging::{
    BatchProcessingOutcome, CursorConfig, DecisionPointOutcome, StepExecutionResult,
};

// ============================================================================
// Error Classification
// ============================================================================

/// HTTP error classification for retry and backoff decisions
///
/// Maps HTTP status codes to error categories that inform retry behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClassification {
    /// 2xx - Success, no error
    Success,
    /// 4xx (except 429) - Client error, not retryable
    PermanentError,
    /// 429 - Rate limited, retryable with backoff
    RateLimited,
    /// 5xx - Server error, may be retryable
    TransientError,
    /// Network/timeout - May be retryable
    NetworkError,
}

impl ErrorClassification {
    /// Check if this classification should trigger a retry
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::TransientError | Self::NetworkError
        )
    }

    /// Get the error type string for metadata
    #[must_use]
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::PermanentError => "permanent_error",
            Self::RateLimited => "rate_limited",
            Self::TransientError => "transient_error",
            Self::NetworkError => "network_error",
        }
    }
}

// ============================================================================
// APICapable Trait
// ============================================================================

/// Trait for handlers that interact with HTTP APIs
///
/// Provides ergonomic methods for creating API response results with proper
/// HTTP metadata and status code classification.
///
/// ## Example
///
/// ```ignore
/// use tasker_worker::handler_capabilities::APICapable;
///
/// impl APICapable for MyApiHandler {}
///
/// // In handler implementation:
/// async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
///     let response = self.call_external_api().await?;
///
///     if response.status().is_success() {
///         Ok(self.api_success(
///             step_data.workflow_step.workflow_step_uuid,
///             response.json().await?,
///             response.status().as_u16(),
///             None,
///             execution_time_ms,
///         ))
///     } else {
///         Ok(self.api_failure(
///             step_data.workflow_step.workflow_step_uuid,
///             &format!("API returned {}", response.status()),
///             response.status().as_u16(),
///             "api_error",
///             execution_time_ms,
///         ))
///     }
/// }
/// ```
pub trait APICapable {
    /// Create a successful API response result
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `data` - Response data to include in result
    /// * `status` - HTTP status code (for observability)
    /// * `headers` - Optional response headers to preserve
    /// * `execution_time_ms` - Execution time in milliseconds
    fn api_success(
        &self,
        step_uuid: Uuid,
        data: Value,
        status: u16,
        headers: Option<Value>,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let mut result_data = json!({
            "data": data,
            "status_code": status,
        });

        if let Some(hdrs) = headers {
            result_data["headers"] = hdrs;
        }

        let mut custom_metadata = HashMap::new();
        custom_metadata.insert("http_status".to_string(), json!(status));

        StepExecutionResult::success(
            step_uuid,
            result_data,
            execution_time_ms,
            Some(custom_metadata),
        )
    }

    /// Create a failed API response result
    ///
    /// Automatically classifies the status code for retry decisions.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `message` - Error message describing the failure
    /// * `status` - HTTP status code
    /// * `error_type` - Error type for classification (e.g., "api_error", "timeout")
    /// * `execution_time_ms` - Execution time in milliseconds
    fn api_failure(
        &self,
        step_uuid: Uuid,
        message: &str,
        status: u16,
        error_type: &str,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let classification = self.classify_status_code(status);

        let mut context = HashMap::new();
        context.insert("http_status".to_string(), json!(status));
        context.insert(
            "classification".to_string(),
            json!(classification.error_type()),
        );

        StepExecutionResult::failure(
            step_uuid,
            message.to_string(),
            Some(format!("HTTP_{status}")),
            Some(error_type.to_string()),
            classification.is_retryable(),
            execution_time_ms,
            Some(context),
        )
    }

    /// Classify an HTTP status code for retry decisions
    ///
    /// Standard classification:
    /// - 2xx: Success
    /// - 4xx (except 429): Permanent error, not retryable
    /// - 429: Rate limited, retryable with backoff
    /// - 5xx: Transient error, may be retryable
    fn classify_status_code(&self, status: u16) -> ErrorClassification {
        match status {
            200..=299 => ErrorClassification::Success,
            429 => ErrorClassification::RateLimited,
            400..=499 => ErrorClassification::PermanentError,
            500..=599 => ErrorClassification::TransientError,
            _ => ErrorClassification::NetworkError,
        }
    }
}

// ============================================================================
// DecisionCapable Trait
// ============================================================================

/// Trait for decision point handlers that route workflow execution
///
/// Provides ergonomic methods for creating decision outcomes that determine
/// which downstream steps should be created and executed.
///
/// ## Example
///
/// ```ignore
/// use tasker_worker::handler_capabilities::DecisionCapable;
///
/// impl DecisionCapable for MyDecisionHandler {}
///
/// // In handler implementation:
/// async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
///     let amount: f64 = step_data.get_context_field("amount")?;
///
///     let steps_to_create = if amount >= 5000.0 {
///         vec!["manager_approval".to_string(), "finance_review".to_string()]
///     } else if amount >= 1000.0 {
///         vec!["manager_approval".to_string()]
///     } else {
///         vec!["auto_approve".to_string()]
///     };
///
///     Ok(self.decision_success(
///         step_data.workflow_step.workflow_step_uuid,
///         steps_to_create,
///         Some(json!({ "routing_path": "threshold_based" })),
///         execution_time_ms,
///     ))
/// }
/// ```
pub trait DecisionCapable {
    /// Create a successful decision result that creates downstream steps
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `step_names` - Names of steps to create (must exist in template)
    /// * `routing_context` - Optional context explaining the routing decision
    /// * `execution_time_ms` - Execution time in milliseconds
    fn decision_success(
        &self,
        step_uuid: Uuid,
        step_names: Vec<String>,
        routing_context: Option<Value>,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let outcome = DecisionPointOutcome::create_steps(step_names.clone());

        let mut result_data = json!({
            "decision_point_outcome": outcome.to_value(),
            "steps_created": step_names,
        });

        if let Some(ctx) = routing_context {
            result_data["routing_context"] = ctx;
        }

        StepExecutionResult::success(step_uuid, result_data, execution_time_ms, None)
    }

    /// Create a decision result that skips all branches
    ///
    /// Used when the decision determines no downstream steps should execute.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `reason` - Explanation for why branches were skipped
    /// * `routing_context` - Optional additional context
    /// * `execution_time_ms` - Execution time in milliseconds
    fn skip_branches(
        &self,
        step_uuid: Uuid,
        reason: &str,
        routing_context: Option<Value>,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let outcome = DecisionPointOutcome::no_branches();

        let mut result_data = json!({
            "decision_point_outcome": outcome.to_value(),
            "skip_reason": reason,
        });

        if let Some(ctx) = routing_context {
            result_data["routing_context"] = ctx;
        }

        StepExecutionResult::success(step_uuid, result_data, execution_time_ms, None)
    }

    /// Create a decision failure result
    ///
    /// Used when the decision logic encounters an error.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `message` - Error message
    /// * `error_type` - Error type for classification
    /// * `execution_time_ms` - Execution time in milliseconds
    fn decision_failure(
        &self,
        step_uuid: Uuid,
        message: &str,
        error_type: &str,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        StepExecutionResult::failure(
            step_uuid,
            message.to_string(),
            Some("DECISION_ERROR".to_string()),
            Some(error_type.to_string()),
            false, // Decision errors typically aren't retryable
            execution_time_ms,
            None,
        )
    }
}

// ============================================================================
// BatchableCapable Trait
// ============================================================================

/// Trait for batch processing handlers with cursor-based pagination
///
/// Provides ergonomic methods for creating batch worker configurations
/// and reporting batch processing outcomes.
///
/// ## Batch Processing Flow
///
/// 1. **Analyzer step** examines data and creates cursor configs
/// 2. **Worker steps** process individual batches in parallel
/// 3. **Aggregator step** combines results (if needed)
///
/// ## Example
///
/// ```ignore
/// use tasker_worker::handler_capabilities::BatchableCapable;
///
/// impl BatchableCapable for MyBatchAnalyzer {}
///
/// // Analyzer creates batch configs:
/// async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
///     let total_items = self.count_items().await?;
///
///     if total_items == 0 {
///         return Ok(self.no_batches_outcome(
///             step_data.workflow_step.workflow_step_uuid,
///             "No items to process",
///             execution_time_ms,
///         ));
///     }
///
///     let configs = self.create_cursor_ranges(total_items, 1000, Some(10));
///
///     Ok(self.batch_analyzer_success(
///         step_data.workflow_step.workflow_step_uuid,
///         "batch_worker_template",
///         configs,
///         total_items,
///         execution_time_ms,
///     ))
/// }
/// ```
pub trait BatchableCapable {
    /// Create cursor configurations by evenly dividing items across workers
    ///
    /// # Arguments
    ///
    /// * `total_items` - Total number of items to process
    /// * `worker_count` - Number of parallel workers
    ///
    /// # Returns
    ///
    /// Vector of `CursorConfig` with evenly distributed ranges
    fn create_cursor_configs(&self, total_items: u64, worker_count: u32) -> Vec<CursorConfig> {
        if worker_count == 0 || total_items == 0 {
            return vec![];
        }

        let items_per_worker = total_items / u64::from(worker_count);
        let remainder = total_items % u64::from(worker_count);

        let mut configs = Vec::with_capacity(worker_count as usize);
        let mut current_start = 0u64;

        for i in 0..worker_count {
            // Distribute remainder across first N workers
            let extra = if u64::from(i) < remainder { 1 } else { 0 };
            let batch_size = items_per_worker + extra;
            let end_cursor = current_start + batch_size;

            configs.push(CursorConfig {
                batch_id: format!("batch_{:03}", i + 1),
                start_cursor: json!(current_start),
                end_cursor: json!(end_cursor),
                batch_size: batch_size as u32,
            });

            current_start = end_cursor;
        }

        configs
    }

    /// Create cursor configurations by batch size with optional max batches
    ///
    /// # Arguments
    ///
    /// * `total_items` - Total number of items to process
    /// * `batch_size` - Items per batch
    /// * `max_batches` - Optional limit on number of batches
    ///
    /// # Returns
    ///
    /// Vector of `CursorConfig` with fixed batch sizes
    fn create_cursor_ranges(
        &self,
        total_items: u64,
        batch_size: u64,
        max_batches: Option<u32>,
    ) -> Vec<CursorConfig> {
        if batch_size == 0 || total_items == 0 {
            return vec![];
        }

        let total_batches = total_items.div_ceil(batch_size);
        let batch_count = match max_batches {
            Some(max) => total_batches.min(u64::from(max)),
            None => total_batches,
        };

        let mut configs = Vec::with_capacity(batch_count as usize);

        for i in 0..batch_count {
            let start = i * batch_size;
            let end = ((i + 1) * batch_size).min(total_items);
            let actual_size = end - start;

            configs.push(CursorConfig {
                batch_id: format!("batch_{:03}", i + 1),
                start_cursor: json!(start),
                end_cursor: json!(end),
                batch_size: actual_size as u32,
            });
        }

        configs
    }

    /// Create a successful batch analyzer result
    ///
    /// This result triggers creation of batch worker steps.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `worker_template` - Name of the worker template step
    /// * `configs` - Cursor configurations for each worker
    /// * `total_items` - Total items being processed
    /// * `execution_time_ms` - Execution time in milliseconds
    fn batch_analyzer_success(
        &self,
        step_uuid: Uuid,
        worker_template: &str,
        configs: Vec<CursorConfig>,
        total_items: u64,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let worker_count = configs.len() as u32;
        let outcome = BatchProcessingOutcome::create_batches(
            worker_template.to_string(),
            worker_count,
            configs,
            total_items,
        );

        let result_data = json!({
            "batch_processing_outcome": outcome.to_value(),
            "worker_count": worker_count,
            "total_items": total_items,
            "worker_template": worker_template,
        });

        StepExecutionResult::success(step_uuid, result_data, execution_time_ms, None)
    }

    /// Create a successful batch worker result
    ///
    /// Reports processing statistics from a single batch worker.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `processed` - Total items processed
    /// * `succeeded` - Successfully processed items
    /// * `failed` - Failed items
    /// * `skipped` - Skipped items
    /// * `execution_time_ms` - Execution time in milliseconds
    fn batch_worker_success(
        &self,
        step_uuid: Uuid,
        processed: u64,
        succeeded: u64,
        failed: u64,
        skipped: u64,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let result_data = json!({
            "processed": processed,
            "succeeded": succeeded,
            "failed": failed,
            "skipped": skipped,
            "success_rate": if processed > 0 {
                (succeeded as f64 / processed as f64) * 100.0
            } else {
                0.0
            },
        });

        let mut custom_metadata = HashMap::new();
        custom_metadata.insert("batch_processed".to_string(), json!(processed));
        custom_metadata.insert("batch_succeeded".to_string(), json!(succeeded));
        custom_metadata.insert("batch_failed".to_string(), json!(failed));

        StepExecutionResult::success(
            step_uuid,
            result_data,
            execution_time_ms,
            Some(custom_metadata),
        )
    }

    /// Create a no-batches outcome result
    ///
    /// Used when analysis determines no batch processing is needed.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `reason` - Explanation for why no batches are needed
    /// * `execution_time_ms` - Execution time in milliseconds
    fn no_batches_outcome(
        &self,
        step_uuid: Uuid,
        reason: &str,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        let outcome = BatchProcessingOutcome::no_batches();

        let result_data = json!({
            "batch_processing_outcome": outcome.to_value(),
            "reason": reason,
        });

        StepExecutionResult::success(step_uuid, result_data, execution_time_ms, None)
    }

    /// Create a batch failure result
    ///
    /// Used when batch processing encounters an error.
    ///
    /// # Arguments
    ///
    /// * `step_uuid` - The workflow step UUID
    /// * `message` - Error message
    /// * `error_type` - Error type for classification
    /// * `retryable` - Whether the batch should be retried
    /// * `execution_time_ms` - Execution time in milliseconds
    fn batch_failure(
        &self,
        step_uuid: Uuid,
        message: &str,
        error_type: &str,
        retryable: bool,
        execution_time_ms: i64,
    ) -> StepExecutionResult {
        StepExecutionResult::failure(
            step_uuid,
            message.to_string(),
            Some("BATCH_ERROR".to_string()),
            Some(error_type.to_string()),
            retryable,
            execution_time_ms,
            None,
        )
    }
}

// ============================================================================
// Handler Capabilities Metadata
// ============================================================================

/// Capability flags for handlers
///
/// Used for runtime introspection and documentation.
#[derive(Debug, Clone, Default)]
pub struct HandlerCapabilities {
    /// Handler supports API interactions
    pub api_capable: bool,
    /// Handler is a decision point
    pub decision_capable: bool,
    /// Handler supports batch processing
    pub batchable: bool,
    /// Handler publishes domain events
    pub event_publishing: bool,
}

impl HandlerCapabilities {
    /// Create empty capabilities
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set API capable
    #[must_use]
    pub fn with_api(mut self) -> Self {
        self.api_capable = true;
        self
    }

    /// Builder: set decision capable
    #[must_use]
    pub fn with_decision(mut self) -> Self {
        self.decision_capable = true;
        self
    }

    /// Builder: set batchable
    #[must_use]
    pub fn with_batch(mut self) -> Self {
        self.batchable = true;
        self
    }

    /// Builder: set event publishing
    #[must_use]
    pub fn with_events(mut self) -> Self {
        self.event_publishing = true;
        self
    }

    /// Convert to JSON for serialization
    #[must_use]
    pub fn to_value(&self) -> Value {
        json!({
            "api_capable": self.api_capable,
            "decision_capable": self.decision_capable,
            "batchable": self.batchable,
            "event_publishing": self.event_publishing,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Test struct implementing all traits
    struct TestHandler;

    impl APICapable for TestHandler {}
    impl DecisionCapable for TestHandler {}
    impl BatchableCapable for TestHandler {}

    #[test]
    fn test_error_classification() {
        let handler = TestHandler;

        assert_eq!(
            handler.classify_status_code(200),
            ErrorClassification::Success
        );
        assert_eq!(
            handler.classify_status_code(201),
            ErrorClassification::Success
        );
        assert_eq!(
            handler.classify_status_code(400),
            ErrorClassification::PermanentError
        );
        assert_eq!(
            handler.classify_status_code(404),
            ErrorClassification::PermanentError
        );
        assert_eq!(
            handler.classify_status_code(429),
            ErrorClassification::RateLimited
        );
        assert_eq!(
            handler.classify_status_code(500),
            ErrorClassification::TransientError
        );
        assert_eq!(
            handler.classify_status_code(503),
            ErrorClassification::TransientError
        );
    }

    #[test]
    fn test_error_classification_retryable() {
        assert!(!ErrorClassification::Success.is_retryable());
        assert!(!ErrorClassification::PermanentError.is_retryable());
        assert!(ErrorClassification::RateLimited.is_retryable());
        assert!(ErrorClassification::TransientError.is_retryable());
        assert!(ErrorClassification::NetworkError.is_retryable());
    }

    #[test]
    fn test_api_success() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.api_success(
            step_uuid,
            json!({"user_id": 123}),
            200,
            Some(json!({"content-type": "application/json"})),
            150,
        );

        assert!(result.is_success());
        assert_eq!(result.result["status_code"], 200);
        assert_eq!(result.result["data"]["user_id"], 123);
        assert_eq!(result.result["headers"]["content-type"], "application/json");
    }

    #[test]
    fn test_api_failure() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.api_failure(step_uuid, "Not found", 404, "not_found", 50);

        assert!(!result.is_success());
        assert!(!result.is_retryable()); // 404 is permanent
        assert_eq!(result.error_code(), Some("HTTP_404"));
    }

    #[test]
    fn test_api_failure_retryable() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.api_failure(step_uuid, "Rate limited", 429, "rate_limit", 50);

        assert!(!result.is_success());
        assert!(result.is_retryable()); // 429 is retryable
    }

    #[test]
    fn test_decision_success() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.decision_success(
            step_uuid,
            vec!["step_a".to_string(), "step_b".to_string()],
            Some(json!({"reason": "threshold exceeded"})),
            100,
        );

        assert!(result.is_success());
        assert_eq!(result.result["steps_created"][0], "step_a");
        assert_eq!(result.result["steps_created"][1], "step_b");
        assert!(result.result["decision_point_outcome"].is_object());
    }

    #[test]
    fn test_skip_branches() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.skip_branches(step_uuid, "No action required", None, 50);

        assert!(result.is_success());
        assert_eq!(result.result["skip_reason"], "No action required");
    }

    #[test]
    fn test_decision_failure() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.decision_failure(step_uuid, "Missing data", "validation_error", 30);

        assert!(!result.is_success());
        assert!(!result.is_retryable());
        assert_eq!(result.error_code(), Some("DECISION_ERROR"));
    }

    #[test]
    fn test_create_cursor_configs() {
        let handler = TestHandler;

        let configs = handler.create_cursor_configs(1000, 4);

        assert_eq!(configs.len(), 4);
        assert_eq!(configs[0].batch_id, "batch_001");
        assert_eq!(configs[0].start_cursor, json!(0));
        assert_eq!(configs[0].end_cursor, json!(250));
        assert_eq!(configs[0].batch_size, 250);

        assert_eq!(configs[3].batch_id, "batch_004");
        assert_eq!(configs[3].start_cursor, json!(750));
        assert_eq!(configs[3].end_cursor, json!(1000));
    }

    #[test]
    fn test_create_cursor_configs_with_remainder() {
        let handler = TestHandler;

        // 10 items across 3 workers = 4, 3, 3
        let configs = handler.create_cursor_configs(10, 3);

        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].batch_size, 4); // Gets extra item
        assert_eq!(configs[1].batch_size, 3);
        assert_eq!(configs[2].batch_size, 3);
    }

    #[test]
    fn test_create_cursor_ranges() {
        let handler = TestHandler;

        let configs = handler.create_cursor_ranges(250, 100, None);

        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].batch_size, 100);
        assert_eq!(configs[1].batch_size, 100);
        assert_eq!(configs[2].batch_size, 50); // Remainder
    }

    #[test]
    fn test_create_cursor_ranges_with_max() {
        let handler = TestHandler;

        let configs = handler.create_cursor_ranges(1000, 100, Some(5));

        assert_eq!(configs.len(), 5); // Limited to 5 despite 10 possible
    }

    #[test]
    fn test_batch_analyzer_success() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let configs = handler.create_cursor_configs(1000, 4);
        let result = handler.batch_analyzer_success(step_uuid, "process_batch", configs, 1000, 200);

        assert!(result.is_success());
        assert_eq!(result.result["worker_count"], 4);
        assert_eq!(result.result["total_items"], 1000);
        assert_eq!(result.result["worker_template"], "process_batch");
    }

    #[test]
    fn test_batch_worker_success() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.batch_worker_success(step_uuid, 100, 95, 3, 2, 500);

        assert!(result.is_success());
        assert_eq!(result.result["processed"], 100);
        assert_eq!(result.result["succeeded"], 95);
        assert_eq!(result.result["failed"], 3);
        assert_eq!(result.result["skipped"], 2);
        assert_eq!(result.result["success_rate"], 95.0);
    }

    #[test]
    fn test_no_batches_outcome() {
        let handler = TestHandler;
        let step_uuid = Uuid::now_v7();

        let result = handler.no_batches_outcome(step_uuid, "Empty dataset", 10);

        assert!(result.is_success());
        assert_eq!(result.result["reason"], "Empty dataset");
    }

    #[test]
    fn test_handler_capabilities() {
        let caps = HandlerCapabilities::new()
            .with_api()
            .with_decision()
            .with_batch()
            .with_events();

        assert!(caps.api_capable);
        assert!(caps.decision_capable);
        assert!(caps.batchable);
        assert!(caps.event_publishing);

        let value = caps.to_value();
        assert_eq!(value["api_capable"], true);
        assert_eq!(value["decision_capable"], true);
    }

    #[test]
    fn test_empty_cursor_configs() {
        let handler = TestHandler;

        // Zero workers
        let configs = handler.create_cursor_configs(100, 0);
        assert!(configs.is_empty());

        // Zero items
        let configs = handler.create_cursor_configs(0, 4);
        assert!(configs.is_empty());
    }

    #[test]
    fn test_empty_cursor_ranges() {
        let handler = TestHandler;

        // Zero batch size
        let configs = handler.create_cursor_ranges(100, 0, None);
        assert!(configs.is_empty());

        // Zero items
        let configs = handler.create_cursor_ranges(0, 100, None);
        assert!(configs.is_empty());
    }
}
