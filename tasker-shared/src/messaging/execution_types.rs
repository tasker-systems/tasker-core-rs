//! # Execution Types for pgmq Architecture
//!
//! Message types for step execution requests and results in the pgmq-based orchestration system.
//! These types are used for communication between Rust orchestration and language workers.
//!
//! ## Key Design Principle
//!
//! These types are aligned with Ruby StepHandlerCallResult structure to ensure consistent
//! data persistence to `tasker_workflow_steps.results`. All results follow the pattern:
//! `{ success: bool, result: Any, metadata: OrchestrationMetadata }`
//!
//! Even failures include result and metadata attributes for API-level backoff evaluation.

use crate::messaging::message::OrchestrationMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Request message sent from Rust orchestrator to language handlers
#[derive(Serialize, Debug, Clone)]
pub struct StepBatchRequest {
    pub batch_id: String,
    pub protocol_version: String,
    pub steps: Vec<StepExecutionRequest>,
}

/// Individual step execution request within a batch
#[derive(Serialize, Debug, Clone)]
pub struct StepExecutionRequest {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub handler_class: String,
    pub handler_config: HashMap<String, serde_json::Value>,
    pub task_context: serde_json::Value,
    pub previous_results: HashMap<String, StepExecutionResult>,
    pub metadata: StepRequestMetadata,
}

/// Metadata for step execution request
#[derive(Serialize, Debug, Clone)]
pub struct StepRequestMetadata {
    pub attempt: i32,
    pub max_attempts: i32,
    pub timeout_ms: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Legacy response message for backward compatibility
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepBatchResponse {
    pub batch_id: String,
    pub protocol_version: String,
    pub results: Vec<StepExecutionResult>,
}

/// Individual step execution result within a batch response
///
/// This structure aligns with Ruby StepHandlerCallResult to ensure consistent
/// data serialization to `tasker_workflow_steps.results`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionResult {
    pub step_uuid: Uuid,
    /// Always present - indicates overall success or failure of step execution
    pub success: bool,
    /// Always present - the actual result data (may be empty object for failures)
    /// This matches Ruby's expectation that even failures have a result field
    pub result: Value,
    /// Always present - comprehensive metadata for observability and backoff evaluation
    pub metadata: StepExecutionMetadata,
    /// Legacy status field for backward compatibility - derived from success field
    pub status: String, // "completed", "failed", "error"
    /// Error details when success=false - provides detailed error information
    pub error: Option<StepExecutionError>,
    /// Orchestration metadata for workflow coordination
    pub orchestration_metadata: Option<OrchestrationMetadata>,
}

impl From<serde_json::Value> for StepExecutionResult {
    fn from(value: serde_json::Value) -> Self {
        let mut step_uuid = Uuid::nil();
        let mut success = false;
        let mut result: Value = Value::Null;
        let mut metadata = StepExecutionMetadata::default();
        let mut status: String = String::new();
        let mut error: Option<StepExecutionError> = None;
        let mut orchestration_metadata: Option<OrchestrationMetadata> = None;

        match value {
            serde_json::Value::Object(obj) => {
                for (key, val) in obj {
                    match key.as_str() {
                        "step_uuid" => {
                            step_uuid = if let Some(uuid_str) = val.as_str() {
                                Uuid::parse_str(uuid_str).unwrap_or(Uuid::nil())
                            } else {
                                Uuid::nil()
                            };
                        }
                        "success" => success = val.as_bool().unwrap_or(false),
                        "result" => result = val,
                        "metadata" => metadata = StepExecutionMetadata::from(val),
                        "status" => {
                            status = val.as_str().map(|s| s.to_string()).unwrap_or_default()
                        }
                        "error" => error = StepExecutionError::from_json(&val),
                        "orchestration_metadata" => {
                            orchestration_metadata = OrchestrationMetadata::from_json(&val)
                        }
                        _ => {}
                    }
                }
                StepExecutionResult {
                    step_uuid,
                    success,
                    result,
                    metadata,
                    status,
                    error,
                    orchestration_metadata,
                }
            }
            _ => StepExecutionResult::error(
                step_uuid,
                String::from("Invalid step execution result"),
                Some(String::from("INVALD_STEP_EXECUTION_RESULT")),
                None,
                0,
                None,
            ),
        }
    }
}

/// Error information for failed step execution
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionError {
    pub message: String,
    pub error_type: Option<String>,
    pub backtrace: Option<Vec<String>>,
    pub retryable: bool,
}

impl StepExecutionError {
    /// Try to create StepExecutionError from JSON value
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::Object(obj) => {
                let message = obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_default();

                let error_type = obj
                    .get("error_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let backtrace = obj.get("backtrace").and_then(|v| v.as_array()).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                });

                let retryable = obj
                    .get("retryable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                Some(StepExecutionError {
                    message,
                    error_type,
                    backtrace,
                    retryable,
                })
            }
            _ => None,
        }
    }
}

/// Comprehensive metadata for step execution results
///
/// This structure provides rich metadata for observability, backoff evaluation,
/// and operational insights. Aligns with Ruby StepHandlerCallResult metadata expectations.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecutionMetadata {
    /// Execution time in milliseconds for performance monitoring
    pub execution_time_ms: i64,
    /// Handler version for compatibility tracking
    pub handler_version: Option<String>,
    /// Whether this operation should be retried on failure
    pub retryable: bool,
    /// When the step execution completed
    pub completed_at: chrono::DateTime<chrono::Utc>,
    /// Worker identification for distributed system observability
    pub worker_id: Option<String>,
    /// Worker hostname for operational debugging
    pub worker_hostname: Option<String>,
    /// When step execution began
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Additional custom metadata for specific step types
    pub custom: HashMap<String, Value>,
    /// Error code for categorization (when success=false)
    pub error_code: Option<String>,
    /// Error type classification (when success=false)
    pub error_type: Option<String>,
    /// Context data for API-level backoff evaluation
    pub context: HashMap<String, Value>,
}

impl From<serde_json::Value> for StepExecutionMetadata {
    fn from(value: serde_json::Value) -> Self {
        let mut metadata = StepExecutionMetadata::default();
        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                match key.as_str() {
                    "execution_time_ms" => metadata.execution_time_ms = val.as_i64().unwrap_or(0),
                    "handler_version" => metadata.handler_version = val.as_str().map(String::from),
                    "retryable" => metadata.retryable = val.as_bool().unwrap_or(false),
                    "completed_at" => {
                        if let Some(s) = val.as_str() {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                metadata.completed_at = dt.with_timezone(&chrono::Utc);
                            }
                        }
                    }
                    "worker_id" => metadata.worker_id = val.as_str().map(String::from),
                    "worker_hostname" => metadata.worker_hostname = val.as_str().map(String::from),
                    "started_at" => {
                        if let Some(s) = val.as_str() {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                                metadata.started_at = Some(dt.with_timezone(&chrono::Utc));
                            }
                        }
                    }
                    "custom" => {
                        if let Some(obj) = val.as_object() {
                            metadata.custom =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                        }
                    }
                    "error_code" => metadata.error_code = val.as_str().map(String::from),
                    "error_type" => metadata.error_type = val.as_str().map(String::from),
                    "context" => {
                        if let Some(obj) = val.as_object() {
                            metadata.context =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                        }
                    }
                    _ => {}
                }
            }
        }
        metadata
    }
}

impl Default for StepExecutionMetadata {
    fn default() -> Self {
        Self {
            execution_time_ms: 0,
            handler_version: None,
            retryable: false,
            completed_at: chrono::Utc::now(),
            worker_id: None,
            worker_hostname: None,
            started_at: None,
            custom: HashMap::new(),
            error_code: None,
            error_type: None,
            context: HashMap::new(),
        }
    }
}

impl StepBatchRequest {
    pub fn new(batch_id: String, steps: Vec<StepExecutionRequest>) -> Self {
        Self {
            batch_id,
            protocol_version: "1.0".to_string(),
            steps,
        }
    }
}

impl StepBatchResponse {
    pub fn new(batch_id: String, results: Vec<StepExecutionResult>) -> Self {
        Self {
            batch_id,
            protocol_version: "1.0".to_string(),
            results,
        }
    }
}

impl StepExecutionRequest {
    /// Constructor with all required fields - clippy allows many args for data constructors
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step_uuid: Uuid,
        task_uuid: Uuid,
        step_name: String,
        handler_class: String,
        handler_config: HashMap<String, serde_json::Value>,
        task_context: serde_json::Value,
        previous_results: HashMap<String, StepExecutionResult>,
        max_attempts: i32,
        timeout_ms: i64,
    ) -> Self {
        Self {
            step_uuid,
            task_uuid,
            step_name,
            handler_class,
            handler_config,
            task_context,
            previous_results,
            metadata: StepRequestMetadata {
                attempt: 0,
                max_attempts,
                timeout_ms,
                created_at: chrono::Utc::now(),
            },
        }
    }
}

impl StepExecutionResult {
    /// Create a successful step execution result
    ///
    /// Follows Ruby StepHandlerCallResult.success pattern with comprehensive metadata
    pub fn success(
        step_uuid: Uuid,
        result: Value,
        execution_time_ms: i64,
        custom_metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: true,
            result,
            status: "completed".to_string(),
            error: None,
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false, // Success doesn't need retry
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: custom_metadata.unwrap_or_default(),
                error_code: None,
                error_type: None,
                context: HashMap::new(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Create a failed step execution result
    ///
    /// Follows Ruby StepHandlerCallResult.error pattern - includes result field for consistency
    pub fn failure(
        step_uuid: Uuid,
        error_message: String,
        error_code: Option<String>,
        error_type: Option<String>,
        retryable: bool,
        execution_time_ms: i64,
        context: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: false,
            result: serde_json::json!({}), // Empty object for failures, as per Ruby pattern
            status: "failed".to_string(),
            error: Some(StepExecutionError {
                message: error_message.clone(),
                error_type: error_type.clone(),
                backtrace: None,
                retryable,
            }),
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable,
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: HashMap::new(),
                error_code,
                error_type,
                context: context.unwrap_or_default(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Create an error step execution result (system-level errors)
    ///
    /// Distinguishes between business logic failures and system errors
    pub fn error(
        step_uuid: Uuid,
        error_message: String,
        error_type: Option<String>,
        backtrace: Option<Vec<String>>,
        execution_time_ms: i64,
        context: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            step_uuid,
            success: false,
            result: serde_json::json!({}), // Empty object for errors
            status: "error".to_string(),
            error: Some(StepExecutionError {
                message: error_message.clone(),
                error_type: error_type.clone(),
                backtrace,
                retryable: false, // System errors typically aren't retryable
            }),
            metadata: StepExecutionMetadata {
                execution_time_ms,
                handler_version: None,
                retryable: false,
                completed_at: chrono::Utc::now(),
                started_at: Some(
                    chrono::Utc::now() - chrono::Duration::milliseconds(execution_time_ms),
                ),
                worker_id: None,
                worker_hostname: None,
                custom: HashMap::new(),
                error_code: None,
                error_type,
                context: context.unwrap_or_default(),
            },
            orchestration_metadata: Some(OrchestrationMetadata::default()),
        }
    }

    /// Check if this result represents a successful execution
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Check if this result should be retried
    pub fn is_retryable(&self) -> bool {
        self.metadata.retryable
    }

    /// Get the result as the expected structure for persistence
    ///
    /// Returns the structure that will be persisted to tasker_workflow_steps.results:
    /// { success: bool, result: Any, metadata: OrchestrationMetadata }
    pub fn to_persistence_format(&self) -> Value {
        serde_json::json!({
            "success": self.success,
            "result": self.result,
            "metadata": self.metadata
        })
    }
}

/// Decision Point Outcome
///
/// Returned by decision point step handlers to indicate which downstream steps
/// should be dynamically created and executed.
///
/// ## Usage
///
/// Decision point handlers evaluate parent step results and return a typed outcome:
///
/// ```rust
/// use tasker_shared::messaging::DecisionPointOutcome;
///
/// // No branches - workflow ends here
/// let no_action = DecisionPointOutcome::NoBranches;
///
/// // Create specific steps by name
/// let create_steps = DecisionPointOutcome::CreateSteps {
///     step_names: vec!["step_a".to_string(), "step_b".to_string()],
/// };
/// ```
///
/// ## Serialization
///
/// This type serializes to JSON in a tagged format for clear communication:
///
/// ```json
/// // NoBranches variant
/// { "type": "no_branches" }
///
/// // CreateSteps variant
/// { "type": "create_steps", "step_names": ["step_a", "step_b"] }
/// ```
///
/// The decision outcome is embedded in the `StepExecutionResult.result` field
/// and processed by the DecisionPointActor for dynamic step creation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionPointOutcome {
    /// No branches should be created - workflow completes or waits
    NoBranches,

    /// Create and execute specific steps by name
    ///
    /// Step names must be declared in the task template as potential children
    /// of this decision point. Only explicitly declared steps can be created.
    CreateSteps {
        /// Names of steps to create (must exist in template)
        step_names: Vec<String>,
    },
}

impl DecisionPointOutcome {
    /// Create a NoBranches outcome
    pub fn no_branches() -> Self {
        Self::NoBranches
    }

    /// Create a CreateSteps outcome with specified step names
    pub fn create_steps(step_names: Vec<String>) -> Self {
        Self::CreateSteps { step_names }
    }

    /// Check if this outcome requires step creation
    pub fn requires_step_creation(&self) -> bool {
        matches!(self, Self::CreateSteps { .. })
    }

    /// Get the step names to create (if any)
    pub fn step_names(&self) -> Vec<String> {
        match self {
            Self::NoBranches => vec![],
            Self::CreateSteps { step_names } => step_names.clone(),
        }
    }

    /// Convert to JSON Value for embedding in StepExecutionResult
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("DecisionPointOutcome should always serialize")
    }

    /// Try to extract DecisionPointOutcome from StepExecutionResult
    ///
    /// Returns None if the result does not contain a valid decision outcome
    pub fn from_step_result(result: &StepExecutionResult) -> Option<Self> {
        serde_json::from_value(result.result.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_execution_request_creation() {
        let task_uuid = Uuid::now_v7();
        let step_uuid = Uuid::now_v7();
        let request = StepExecutionRequest::new(
            step_uuid,
            task_uuid,
            "validate_order".to_string(),
            "OrderValidator".to_string(),
            HashMap::new(),
            serde_json::json!({}),
            HashMap::new(),
            3,
            30000,
        );

        assert_eq!(request.step_uuid, step_uuid);
        assert_eq!(request.task_uuid, task_uuid);
        assert_eq!(request.step_name, "validate_order");
        assert_eq!(request.metadata.max_attempts, 3);
        assert_eq!(request.metadata.timeout_ms, 30000);
    }

    #[test]
    fn test_step_execution_result_success() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::success(
            step_uuid,
            serde_json::json!({"status": "valid"}),
            1500,
            None,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "completed");
        assert!(result.success);
        assert_eq!(result.result, serde_json::json!({"status": "valid"}));
        assert!(result.error.is_none());
        assert_eq!(result.metadata.execution_time_ms, 1500);
        assert!(!result.metadata.retryable); // Success shouldn't need retry
    }

    #[test]
    fn test_step_execution_result_failure() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::failure(
            step_uuid,
            "Validation failed".to_string(),
            Some("VALIDATION_ERROR".to_string()),
            Some("ValidationError".to_string()),
            true,
            800,
            None,
        );

        assert_eq!(result.step_uuid, step_uuid);
        assert_eq!(result.status, "failed");
        assert!(!result.success);
        assert_eq!(result.result, serde_json::json!({})); // Empty object for failures
        assert!(result.error.is_some());
        assert!(result.is_retryable());

        let error = result.error.unwrap();
        assert_eq!(error.message, "Validation failed");
        assert!(error.retryable);
    }

    #[test]
    fn test_step_execution_result_persistence_format() {
        let step_uuid = Uuid::now_v7();
        let result = StepExecutionResult::success(
            step_uuid,
            serde_json::json!({"order_id": 12345, "total": 99.99}),
            750,
            Some(HashMap::from([(
                "custom_field".to_string(),
                serde_json::json!("custom_value"),
            )])),
        );

        let persistence_format = result.to_persistence_format();

        // Verify the expected structure for tasker_workflow_steps.results
        assert_eq!(persistence_format["success"], true);
        assert_eq!(persistence_format["result"]["order_id"], 12345);
        assert_eq!(persistence_format["result"]["total"], 99.99);
        assert!(persistence_format["metadata"].is_object());
        assert_eq!(persistence_format["metadata"]["execution_time_ms"], 750);
        assert_eq!(
            persistence_format["metadata"]["custom"]["custom_field"],
            "custom_value"
        );
    }

    #[test]
    fn test_step_execution_result_from_json() {
        // Test successful result
        let json_value = serde_json::json!({
            "step_uuid": "550e8400-e29b-41d4-a716-446655440000",
            "success": true,
            "result": {"data": "test"},
            "metadata": {
                "execution_time_ms": 100,
                "retryable": false
            },
            "status": "completed"
        });

        let result = StepExecutionResult::from(json_value);
        assert!(result.success);
        assert_eq!(result.status, "completed");
        assert_eq!(result.metadata.execution_time_ms, 100);

        // Test error result
        let error_json = serde_json::json!({
            "step_uuid": "550e8400-e29b-41d4-a716-446655440001",
            "success": false,
            "result": {},
            "metadata": {
                "execution_time_ms": 50,
                "retryable": true,
                "error_code": "TIMEOUT"
            },
            "status": "failed",
            "error": {
                "message": "Operation timed out",
                "error_type": "TimeoutError",
                "retryable": true
            }
        });

        let error_result = StepExecutionResult::from(error_json);
        assert!(!error_result.success);
        assert_eq!(error_result.status, "failed");
        assert!(error_result.error.is_some());
        let error = error_result.error.unwrap();
        assert_eq!(error.message, "Operation timed out");
        assert!(error.retryable);
    }

    #[test]
    fn test_step_execution_result_failure_persistence_format() {
        let step_uuid = Uuid::now_v7();
        let context = HashMap::from([("retry_count".to_string(), serde_json::json!(2))]);
        let result = StepExecutionResult::failure(
            step_uuid,
            "Database connection failed".to_string(),
            Some("DB_CONNECTION_ERROR".to_string()),
            Some("RetryableError".to_string()),
            true,
            500,
            Some(context),
        );

        let persistence_format = result.to_persistence_format();

        // Verify failure structure still includes result field (empty object)
        assert_eq!(persistence_format["success"], false);
        assert_eq!(persistence_format["result"], serde_json::json!({}));
        assert!(persistence_format["metadata"].is_object());
        assert_eq!(persistence_format["metadata"]["retryable"], true);
        assert_eq!(
            persistence_format["metadata"]["error_code"],
            "DB_CONNECTION_ERROR"
        );
        assert_eq!(persistence_format["metadata"]["context"]["retry_count"], 2);
    }

    #[test]
    fn test_decision_point_outcome_no_branches() {
        let outcome = DecisionPointOutcome::no_branches();
        assert_eq!(outcome, DecisionPointOutcome::NoBranches);
        assert!(!outcome.requires_step_creation());
        assert_eq!(outcome.step_names(), Vec::<String>::new());
    }

    #[test]
    fn test_decision_point_outcome_create_steps() {
        let steps = vec!["step_a".to_string(), "step_b".to_string()];
        let outcome = DecisionPointOutcome::create_steps(steps.clone());

        assert!(outcome.requires_step_creation());
        assert_eq!(outcome.step_names(), steps);

        if let DecisionPointOutcome::CreateSteps { step_names } = outcome {
            assert_eq!(step_names, steps);
        } else {
            panic!("Expected CreateSteps variant");
        }
    }

    #[test]
    fn test_decision_point_outcome_serialization() {
        // Test NoBranches serialization
        let no_branches = DecisionPointOutcome::NoBranches;
        let json = serde_json::to_value(&no_branches).unwrap();
        assert_eq!(json["type"], "no_branches");

        // Test CreateSteps serialization
        let create_steps = DecisionPointOutcome::CreateSteps {
            step_names: vec!["step_1".to_string(), "step_2".to_string()],
        };
        let json = serde_json::to_value(&create_steps).unwrap();
        assert_eq!(json["type"], "create_steps");
        assert_eq!(json["step_names"][0], "step_1");
        assert_eq!(json["step_names"][1], "step_2");
    }

    #[test]
    fn test_decision_point_outcome_deserialization() {
        // Test NoBranches deserialization
        let json = serde_json::json!({"type": "no_branches"});
        let outcome: DecisionPointOutcome = serde_json::from_value(json).unwrap();
        assert_eq!(outcome, DecisionPointOutcome::NoBranches);

        // Test CreateSteps deserialization
        let json = serde_json::json!({
            "type": "create_steps",
            "step_names": ["step_a", "step_b"]
        });
        let outcome: DecisionPointOutcome = serde_json::from_value(json).unwrap();
        assert!(outcome.requires_step_creation());
        assert_eq!(outcome.step_names(), vec!["step_a", "step_b"]);
    }

    #[test]
    fn test_decision_point_outcome_to_value() {
        let outcome = DecisionPointOutcome::create_steps(vec!["step_1".to_string()]);
        let value = outcome.to_value();

        assert_eq!(value["type"], "create_steps");
        assert_eq!(value["step_names"][0], "step_1");
    }

    #[test]
    fn test_decision_point_outcome_from_step_result() {
        let step_uuid = Uuid::now_v7();

        // Test extraction from successful result with decision outcome
        let decision_outcome = DecisionPointOutcome::create_steps(vec![
            "branch_a".to_string(),
            "branch_b".to_string(),
        ]);
        let result =
            StepExecutionResult::success(step_uuid, decision_outcome.to_value(), 100, None);

        let extracted = DecisionPointOutcome::from_step_result(&result);
        assert!(extracted.is_some());
        let extracted = extracted.unwrap();
        assert!(extracted.requires_step_creation());
        assert_eq!(extracted.step_names(), vec!["branch_a", "branch_b"]);
    }

    #[test]
    fn test_decision_point_outcome_from_step_result_no_branches() {
        let step_uuid = Uuid::now_v7();

        // Test extraction of NoBranches outcome
        let decision_outcome = DecisionPointOutcome::NoBranches;
        let result =
            StepExecutionResult::success(step_uuid, decision_outcome.to_value(), 100, None);

        let extracted = DecisionPointOutcome::from_step_result(&result);
        assert!(extracted.is_some());
        let extracted = extracted.unwrap();
        assert!(!extracted.requires_step_creation());
        assert_eq!(extracted.step_names(), Vec::<String>::new());
    }

    #[test]
    fn test_decision_point_outcome_from_non_decision_result() {
        let step_uuid = Uuid::now_v7();

        // Test with regular result that's not a decision outcome
        let result = StepExecutionResult::success(
            step_uuid,
            serde_json::json!({"regular": "data"}),
            100,
            None,
        );

        let extracted = DecisionPointOutcome::from_step_result(&result);
        assert!(extracted.is_none());
    }

    #[test]
    fn test_decision_point_outcome_empty_step_names() {
        // Test CreateSteps with empty step names
        let outcome = DecisionPointOutcome::create_steps(vec![]);
        assert!(outcome.requires_step_creation());
        assert_eq!(outcome.step_names(), Vec::<String>::new());
    }

    #[test]
    fn test_decision_point_outcome_roundtrip() {
        // Test full serialization/deserialization roundtrip
        let original = DecisionPointOutcome::create_steps(vec![
            "step_1".to_string(),
            "step_2".to_string(),
            "step_3".to_string(),
        ]);

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: DecisionPointOutcome = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
        assert_eq!(original.step_names(), deserialized.step_names());
    }
}
