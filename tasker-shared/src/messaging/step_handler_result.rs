//! # Step Handler Result Types
//!
//! Rust equivalents of Ruby StepHandlerCallResult types to ensure consistent
//! data structures across language boundaries. These types are specifically
//! designed to match Ruby expectations for step execution results.
//!
//! ## Design Principles
//!
//! 1. All results have `success: bool`, `result: Any`, `metadata: Hash` structure
//! 2. Even failures include result field (usually empty object)
//! 3. Comprehensive metadata for observability and backoff evaluation
//! 4. Direct mapping to Ruby StepHandlerCallResult::Success and StepHandlerCallResult::Error

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Success result structure matching Ruby StepHandlerCallResult::Success
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepHandlerSuccessResult {
    /// Always true for success results
    pub success: bool,
    /// The actual result data from the step handler (any JSON-serializable value)
    pub result: Value,
    /// Optional metadata for observability
    pub metadata: HashMap<String, Value>,
}

/// Error result structure matching Ruby StepHandlerCallResult::Error
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepHandlerErrorResult {
    /// Always false for error results
    pub success: bool,
    /// Empty result object for failures (maintains consistent structure)
    pub result: Value,
    /// Error type matching Ruby error class names
    pub error_type: String,
    /// Human-readable error message
    pub message: String,
    /// Optional error code for categorization
    pub error_code: Option<String>,
    /// Whether this error should trigger a retry
    pub retryable: bool,
    /// Additional error context and metadata
    pub metadata: HashMap<String, Value>,
}

/// Unified result type that can represent either success or failure
///
/// This enum provides a type-safe way to handle step results while maintaining
/// compatibility with Ruby StepHandlerCallResult structure.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "success")]
pub enum StepHandlerCallResult {
    #[serde(rename = "true")]
    Success(StepHandlerSuccessResult),
    #[serde(rename = "false")]
    Error(StepHandlerErrorResult),
}

impl StepHandlerCallResult {
    /// Create a success result
    pub fn success(result: Value, metadata: Option<HashMap<String, Value>>) -> Self {
        Self::Success(StepHandlerSuccessResult {
            success: true,
            result,
            metadata: metadata.unwrap_or_default(),
        })
    }

    /// Create an error result
    pub fn error(
        error_type: String,
        message: String,
        error_code: Option<String>,
        retryable: bool,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self::Error(StepHandlerErrorResult {
            success: false,
            result: serde_json::json!({}), // Empty object for failures
            error_type,
            message,
            error_code,
            retryable,
            metadata: metadata.unwrap_or_default(),
        })
    }

    /// Create an error result from an exception-like structure
    pub fn from_exception(
        exception_type: &str,
        message: String,
        context: Option<HashMap<String, Value>>,
    ) -> Self {
        let (error_type, retryable) = match exception_type {
            "PermanentError" => ("PermanentError".to_string(), false),
            "RetryableError" => ("RetryableError".to_string(), true),
            "ValidationError" => ("ValidationError".to_string(), false),
            "StepCompletionError" => ("StepCompletionError".to_string(), true),
            _ => ("UnexpectedError".to_string(), true),
        };

        let mut metadata = HashMap::new();
        if let Some(ctx) = context {
            metadata.extend(ctx);
        }
        metadata.insert(
            "exception_class".to_string(),
            serde_json::json!(exception_type),
        );

        Self::Error(StepHandlerErrorResult {
            success: false,
            result: serde_json::json!({}),
            error_type,
            message,
            error_code: None,
            retryable,
            metadata,
        })
    }

    /// Convert arbitrary handler output to StepHandlerCallResult
    ///
    /// This mirrors Ruby's `from_handler_output` method for handling various
    /// return types from step handlers.
    pub fn from_handler_output(output: Value) -> Self {
        match output {
            Value::Object(map) => {
                // Check if it looks like a result structure
                if let Some(success) = map.get("success") {
                    if success == &serde_json::json!(true) {
                        // Looks like a success result
                        let result = map
                            .get("result")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!(map));
                        let metadata = map
                            .get("metadata")
                            .and_then(|m| m.as_object())
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default();

                        Self::success(result, Some(metadata))
                    } else if success == &serde_json::json!(false) {
                        // Looks like an error result
                        let error_type = map
                            .get("error_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("UnexpectedError")
                            .to_string();
                        let message = map
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown error")
                            .to_string();
                        let error_code = map
                            .get("error_code")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let retryable = map
                            .get("retryable")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let metadata = map
                            .get("metadata")
                            .and_then(|m| m.as_object())
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default();

                        Self::error(error_type, message, error_code, retryable, Some(metadata))
                    } else {
                        // Just a hash of results - wrap as success
                        let mut metadata = HashMap::new();
                        metadata.insert("wrapped".to_string(), serde_json::json!(true));
                        metadata.insert("original_type".to_string(), serde_json::json!("Object"));
                        Self::success(Value::Object(map), Some(metadata))
                    }
                } else {
                    // Just a hash of results - wrap as success
                    let mut metadata = HashMap::new();
                    metadata.insert("wrapped".to_string(), serde_json::json!(true));
                    metadata.insert("original_type".to_string(), serde_json::json!("Object"));
                    Self::success(Value::Object(map), Some(metadata))
                }
            }
            _ => {
                // Any other type - wrap as success
                let mut metadata = HashMap::new();
                metadata.insert("wrapped".to_string(), serde_json::json!(true));
                metadata.insert(
                    "original_type".to_string(),
                    serde_json::json!(match output {
                        Value::Null => "Null",
                        Value::Bool(_) => "Bool",
                        Value::Number(_) => "Number",
                        Value::String(_) => "String",
                        Value::Array(_) => "Array",
                        Value::Object(_) => "Object",
                    }),
                );
                Self::success(output, Some(metadata))
            }
        }
    }

    /// Check if this result represents a successful execution
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Check if this result should be retried (only applicable for errors)
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Success(_) => false,
            Self::Error(err) => err.retryable,
        }
    }

    /// Get the result value regardless of success/failure
    pub fn result(&self) -> &Value {
        match self {
            Self::Success(success) => &success.result,
            Self::Error(error) => &error.result,
        }
    }

    /// Get the metadata regardless of success/failure
    pub fn metadata(&self) -> &HashMap<String, Value> {
        match self {
            Self::Success(success) => &success.metadata,
            Self::Error(error) => &error.metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_result_creation() {
        let result = StepHandlerCallResult::success(
            serde_json::json!({"order_id": 123, "status": "processed"}),
            Some(HashMap::from([(
                "processing_time_ms".to_string(),
                serde_json::json!(250),
            )])),
        );

        assert!(result.is_success());
        assert!(!result.is_retryable());
        assert_eq!(result.result()["order_id"], 123);
        assert_eq!(result.metadata()["processing_time_ms"], 250);
    }

    #[test]
    fn test_error_result_creation() {
        let result = StepHandlerCallResult::error(
            "ValidationError".to_string(),
            "Invalid order total".to_string(),
            Some("INVALID_TOTAL".to_string()),
            false,
            Some(HashMap::from([(
                "attempted_total".to_string(),
                serde_json::json!(-50),
            )])),
        );

        assert!(!result.is_success());
        assert!(!result.is_retryable());
        assert_eq!(result.result(), &serde_json::json!({}));
        assert_eq!(result.metadata()["attempted_total"], -50);
    }

    #[test]
    fn test_from_handler_output_object_success() {
        let output = serde_json::json!({
            "success": true,
            "result": {"data": "test"},
            "metadata": {"timing": 100}
        });

        let result = StepHandlerCallResult::from_handler_output(output);
        assert!(result.is_success());
        assert_eq!(result.result()["data"], "test");
        assert_eq!(result.metadata()["timing"], 100);
    }

    #[test]
    fn test_from_handler_output_object_error() {
        let output = serde_json::json!({
            "success": false,
            "error_type": "RetryableError",
            "message": "Temporary failure",
            "retryable": true,
            "metadata": {"attempt": 1}
        });

        let result = StepHandlerCallResult::from_handler_output(output);
        assert!(!result.is_success());
        assert!(result.is_retryable());
        assert_eq!(result.result(), &serde_json::json!({}));
        assert_eq!(result.metadata()["attempt"], 1);
    }

    #[test]
    fn test_from_handler_output_plain_object() {
        let output = serde_json::json!({"data": "value", "count": 42});

        let result = StepHandlerCallResult::from_handler_output(output.clone());
        assert!(result.is_success());
        assert_eq!(result.result(), &output);
        assert_eq!(result.metadata()["wrapped"], true);
        assert_eq!(result.metadata()["original_type"], "Object");
    }

    #[test]
    fn test_from_handler_output_primitive() {
        let output = serde_json::json!("simple string result");

        let result = StepHandlerCallResult::from_handler_output(output.clone());
        assert!(result.is_success());
        assert_eq!(result.result(), &output);
        assert_eq!(result.metadata()["wrapped"], true);
        assert_eq!(result.metadata()["original_type"], "String");
    }

    #[test]
    fn test_from_exception() {
        let result = StepHandlerCallResult::from_exception(
            "RetryableError",
            "Database timeout".to_string(),
            Some(HashMap::from([(
                "timeout_seconds".to_string(),
                serde_json::json!(30),
            )])),
        );

        assert!(!result.is_success());
        assert!(result.is_retryable());
        assert_eq!(result.metadata()["exception_class"], "RetryableError");
        assert_eq!(result.metadata()["timeout_seconds"], 30);
    }
}
