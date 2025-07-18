//! # Rust Client Traits
//!
//! Defines the core traits that Rust applications implement to handle
//! task and step execution within the Tasker ecosystem.

use crate::client::context::{StepContext, TaskContext};
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for Rust-native step handlers
///
/// This trait should be implemented by Rust applications that want to handle
/// specific steps within a task workflow. The BaseStepHandler coordinates
/// execution and calls into implementations of this trait.
#[async_trait]
pub trait RustStepHandler: Send + Sync {
    /// Process a step with the given context
    ///
    /// This is the main entry point where your business logic should be implemented.
    /// The context contains all necessary information about the step execution,
    /// including input data, configuration, and metadata.
    ///
    /// # Arguments
    ///
    /// * `context` - The step execution context
    ///
    /// # Returns
    ///
    /// * `Ok(Value)` - The step output data on success
    /// * `Err` - Any error that occurred during processing
    async fn process(&self, context: &StepContext) -> Result<Value>;

    /// Process the results from the main process() method
    ///
    /// This method is called after process() completes successfully and allows
    /// the handler to transform, validate, or augment the output before it's
    /// stored as the final step result. This aligns with Rails' process_results
    /// method pattern.
    ///
    /// # Arguments
    ///
    /// * `context` - The step execution context
    /// * `process_output` - The output returned from the process() method
    /// * `initial_results` - Any results already set (for advanced use cases)
    ///
    /// # Returns
    ///
    /// * `Ok(Value)` - The final processed result to store for this step
    /// * `Err` - Any error that occurred during result processing
    async fn process_results(
        &self,
        context: &StepContext,
        process_output: &Value,
        initial_results: Option<&Value>,
    ) -> Result<Value> {
        let _ = (context, initial_results);
        // Default implementation returns the process output unchanged
        Ok(process_output.clone())
    }

    /// Validate step configuration before execution
    ///
    /// This hook allows handlers to validate their configuration during
    /// initialization or before execution begins.
    /// Default implementation accepts all configurations.
    ///
    /// # Arguments
    ///
    /// * `config` - The step configuration from the handler configuration
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration is valid
    /// * `Err` - Configuration validation failed
    fn validate_config(&self, config: &HashMap<String, Value>) -> Result<()> {
        let _ = config;
        Ok(())
    }

    /// Get the step handler name for identification
    ///
    /// This is used for logging and debugging purposes.
    /// Default implementation returns the type name.
    fn handler_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Trait for Rust-native task handlers
///
/// Task handlers coordinate the execution of multiple steps and handle
/// task-level concerns like dependency management, error handling, and
/// result aggregation.
#[async_trait]
pub trait RustTaskHandler: Send + Sync {
    /// Get a step handler for the given step name
    ///
    /// This method is called by the orchestration system to resolve
    /// step handlers for execution.
    ///
    /// # Arguments
    ///
    /// * `step_name` - The name of the step to get a handler for
    ///
    /// # Returns
    ///
    /// * `Some(handler)` - Handler for the requested step
    /// * `None` - No handler available for this step
    async fn get_step_handler(&self, step_name: &str) -> Option<Box<dyn RustStepHandler>>;

    /// Initialize the task handler
    ///
    /// Called once when the task handler is first created or registered.
    /// Can be used for setup, resource allocation, or configuration validation.
    ///
    /// # Arguments
    ///
    /// * `context` - The task execution context
    async fn initialize(&self, context: &TaskContext) -> Result<()> {
        let _ = context;
        Ok(())
    }

    /// Finalize the task handler
    ///
    /// Called when task execution is complete (successful or failed).
    /// Can be used for cleanup, resource deallocation, or final processing.
    ///
    /// # Arguments
    ///
    /// * `context` - The task execution context
    /// * `success` - Whether the task completed successfully
    async fn finalize(&self, context: &TaskContext, success: bool) -> Result<()> {
        let _ = (context, success);
        Ok(())
    }

    /// Get the task handler name for identification
    ///
    /// This is used for logging and debugging purposes.
    /// Default implementation returns the type name.
    fn handler_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Validate task configuration
    ///
    /// Called during task handler registration to validate the
    /// task-level configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The task configuration from the handler configuration
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration is valid
    /// * `Err` - Configuration validation failed
    fn validate_task_config(&self, config: &HashMap<String, Value>) -> Result<()> {
        let _ = config;
        Ok(())
    }
}
