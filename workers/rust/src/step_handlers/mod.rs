//! # Native Rust Step Handlers
//!
//! This module defines the core trait and utilities for implementing native Rust step handlers
//! that integrate seamlessly with the tasker-worker foundation and match YAML configuration expectations.
//!
//! ## Architecture Integration
//!
//! - **Corrected Types**: Uses production types `TaskSequenceStep` and `StepExecutionResult`
//! - **Ruby Compatibility**: Matches Ruby `StepHandlerCallResult` structure for data persistence
//! - **Type Safety**: Compile-time guarantees with Rust's type system
//! - **Performance**: Native Rust performance with zero-overhead abstractions
//! - **YAML Integration**: Compatible with `TaskTemplate` initialization configurations
//!
//! ## Usage
//!
//! ```ignore
//! use anyhow::Result;
//! use async_trait::async_trait;
//! use tasker_shared::messaging::StepExecutionResult;
//! use tasker_shared::types::TaskSequenceStep;
//! use tasker_worker_rust::step_handlers::{RustStepHandler, StepHandlerConfig, success_result};
//! use serde_json::Value;
//! use std::collections::HashMap;
//!
//! pub struct MyStepHandler {
//!     config: StepHandlerConfig,
//! }
//!
//! // Dummy function to simulate data processing
//! fn process_data(_value: &Value) -> Value {
//!     serde_json::json!({ "processed": true })
//! }
//!
//! #[async_trait]
//! impl RustStepHandler for MyStepHandler {
//!     async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
//!         let start_time = std::time::Instant::now();
//!         let step_uuid = step_data.workflow_step.workflow_step_uuid;
//!
//!         // Access initialization parameters
//!         let debug_mode = self.config.get_bool("debug_mode").unwrap_or(false);
//!         let _timeout = self.config.get_u64("timeout_ms").unwrap_or(30000);
//!
//!         // Extract data from task context
//!         let value = step_data.task.context
//!             .get("my_field")
//!             .ok_or_else(|| anyhow::anyhow!("Missing required field"))?;
//!
//!         // Perform step logic...
//!         let result = process_data(value);
//!
//!         Ok(success_result(
//!             step_uuid,
//!             result,
//!             start_time.elapsed().as_millis() as i64,
//!             Some(HashMap::new()),
//!         ))
//!     }
//!
//!     fn name(&self) -> &str { "my_step" }
//!
//!     fn new(config: StepHandlerConfig) -> Self {
//!         Self { config }
//!     }
//! }
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

// CORRECTED: Use actual production types from the codebase
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

/// Configuration structure for step handlers from YAML initialization blocks
///
/// Provides type-safe access to initialization parameters from `TaskTemplate` YAML configurations.
/// Each step handler receives a `StepHandlerConfig` built from the handler.initialization section.
///
/// ## YAML Integration
///
/// This structure corresponds to the `initialization` field in `TaskTemplate` YAML:
/// ```yaml
/// handler:
///   callable: "MyHandler"
///   initialization:
///     operation: "square"
///     step_number: 1
///     debug_mode: true
///     timeout_ms: 30000
/// ```
///
/// ## Type-Safe Access
///
/// Provides convenience methods for common types with proper error handling:
/// - `get_string()` - String values
/// - `get_bool()` - Boolean values
/// - `get_i64()` - Integer values
/// - `get_u64()` - Unsigned integer values
/// - `get_f64()` - Floating point values
#[derive(Debug, Clone, Default)]
pub struct StepHandlerConfig {
    /// Raw configuration data from YAML initialization block
    pub data: HashMap<String, Value>,
}

impl StepHandlerConfig {
    /// Create new config from initialization data
    #[must_use]
    pub fn new(data: HashMap<String, Value>) -> Self {
        Self { data }
    }

    /// Create empty config (for handlers that don't require initialization)
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Get string value with optional default
    #[must_use]
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.data
            .get(key)
            .and_then(|v| v.as_str().map(std::string::ToString::to_string))
    }

    /// Get boolean value with optional default
    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.get(key).and_then(serde_json::Value::as_bool)
    }

    /// Get i64 value with optional default
    #[must_use]
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.data.get(key).and_then(serde_json::Value::as_i64)
    }

    /// Get u64 value with optional default
    #[must_use]
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.data.get(key).and_then(serde_json::Value::as_u64)
    }

    /// Get f64 value with optional default
    #[must_use]
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.data.get(key).and_then(serde_json::Value::as_f64)
    }

    /// Get raw JSON value
    #[must_use]
    pub fn get_value(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Check if key exists
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
}

/// Core trait for native Rust step handlers with YAML configuration support
///
/// This trait mirrors the Ruby `TaskerCore::StepHandler::Base` pattern but uses Rust's
/// type system for compile-time safety and native performance. All step handlers must
/// implement this trait to be compatible with the tasker-worker execution system.
///
/// ## Architectural Corrections Applied
///
/// This implementation uses the **actual production types** from the codebase:
/// - `TaskSequenceStep` contains all step execution data (task, `workflow_step`, `dependency_results`, `step_definition`)
/// - `StepExecutionResult` matches Ruby `StepHandlerCallResult` for seamless data persistence
/// - Method signature uses single parameter with all needed data, not separate parameters
/// - `new()` method matches YAML `TaskTemplate` initialization expectations
///
/// ## YAML Integration
///
/// Handlers receive configuration from `TaskTemplate` YAML initialization blocks:
/// ```yaml
/// handler:
///   callable: "MyHandler"
///   initialization:
///     operation: "square"
///     debug_mode: true
/// ```
///
/// ## Data Available in `TaskSequenceStep`
///
/// - `step_data.task`: `TaskForOrchestration` with context and metadata
/// - `step_data.workflow_step`: `WorkflowStepWithName` with step UUID and details
/// - `step_data.dependency_results`: Previous step results for dependency resolution
/// - `step_data.step_definition`: Step configuration from `TaskTemplate` YAML
#[async_trait]
pub trait RustStepHandler: Send + Sync {
    /// Execute the step - equivalent to Ruby's `call(task, sequence, step)` method
    ///
    /// ## Corrected Method Signature
    ///
    /// Uses a single `TaskSequenceStep` parameter that contains all needed data:
    /// - Task context via `step_data.task.context`
    /// - Step UUID via `step_data.workflow_step.workflow_step_uuid`
    /// - Previous results via `step_data.dependency_results`
    /// - Step configuration via `step_data.step_definition`
    ///
    /// ## Performance Considerations
    ///
    /// - Measure execution time for observability
    /// - Use structured error handling with proper error codes
    /// - Include comprehensive metadata for debugging
    /// - Return results that match Ruby persistence expectations
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult>;

    /// Step handler identifier for registration and debugging
    fn name(&self) -> &str;

    /// Create new handler instance with YAML initialization parameters
    ///
    /// This method is called during handler instantiation with configuration data from
    /// the `TaskTemplate` YAML initialization block. All handlers must implement this
    /// method to be compatible with YAML-based configuration.
    ///
    /// ## Implementation Pattern
    ///
    /// Most handlers will store the config and use it during execution:
    /// ```ignore
    /// fn new(config: StepHandlerConfig) -> Self {
    ///     Self { config }
    /// }
    /// ```
    fn new(config: StepHandlerConfig) -> Self
    where
        Self: Sized;
}

/// Error types for Rust step handler failures
#[derive(Debug, thiserror::Error)]
pub enum RustStepHandlerError {
    #[error("Handler execution error: {message}")]
    ExecutionError { message: String, retryable: bool },

    #[error("Data validation error: {message}")]
    ValidationError { message: String },

    #[error("System error: {message}")]
    SystemError { message: String },

    #[error("Missing dependency result: {step_name}")]
    MissingDependency { step_name: String },

    #[error("Invalid task context: {field}")]
    InvalidContext { field: String },
}

/// Helper function for creating successful `StepExecutionResult`
///
/// **CORRECTED**: Uses actual `StepExecutionResult::success` factory method from `execution_types.rs`
///
/// This function provides a convenient way to create success results that match
/// the Ruby `StepHandlerCallResult.success` structure for seamless data persistence.
///
/// ## Parameters
///
/// - `step_uuid`: UUID of the workflow step being executed
/// - `result_data`: The actual result data (any JSON-serializable value)
/// - `execution_time_ms`: Execution time in milliseconds for performance monitoring
/// - `custom_metadata`: Optional additional metadata for observability
#[must_use]
pub fn success_result(
    step_uuid: Uuid,
    result_data: Value,
    execution_time_ms: i64,
    custom_metadata: Option<HashMap<String, Value>>,
) -> StepExecutionResult {
    StepExecutionResult::success(step_uuid, result_data, execution_time_ms, custom_metadata)
}

/// Helper function for creating failed `StepExecutionResult`
///
/// **CORRECTED**: Uses actual `StepExecutionResult::failure` factory method from `execution_types.rs`
///
/// This function provides a convenient way to create failure results that match
/// the Ruby `StepHandlerCallResult.error` structure for consistent error handling.
///
/// ## Parameters
///
/// - `step_uuid`: UUID of the workflow step that failed
/// - `error_message`: Human-readable error message
/// - `error_code`: Optional error code for categorization (e.g., "`VALIDATION_ERROR`")
/// - `error_type`: Optional error type for classification (e.g., "`ValidationError`")
/// - `retryable`: Whether this error should trigger a retry
/// - `execution_time_ms`: Execution time before failure occurred
/// - `context`: Optional additional error context for debugging
#[must_use]
pub fn error_result(
    step_uuid: Uuid,
    error_message: String,
    error_code: Option<String>,
    error_type: Option<String>,
    retryable: bool,
    execution_time_ms: i64,
    context: Option<HashMap<String, Value>>,
) -> StepExecutionResult {
    StepExecutionResult::failure(
        step_uuid,
        error_message,
        error_code,
        error_type,
        retryable,
        execution_time_ms,
        context,
    )
}

// Workflow handler modules
pub mod conditional_approval_rust;
pub mod diamond_workflow;
pub mod linear_workflow;
pub mod mixed_dag_workflow;
pub mod order_fulfillment;
pub mod tree_workflow;

// Handler registry
pub mod registry;

// Re-export core types for convenience
pub use registry::{GlobalRustStepHandlerRegistry, RustStepHandlerRegistry};

// StepHandlerConfig is defined in this module, no need to re-export
