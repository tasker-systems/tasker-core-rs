//! # Ruby Step Wrapper
//!
//! Magnus-wrapped Ruby class for WorkflowStep model access from Ruby code.

use magnus::{prelude::*, Error, Ruby};
use tasker_core::models::core::workflow_step::WorkflowStep;

/// Ruby wrapper for WorkflowStep model data
///
/// This provides type-safe access to WorkflowStep fields from Ruby code.
/// Uses `free_immediately` for memory safety since these are read-only snapshots.
#[derive(Clone)]
#[magnus::wrap(class = "TaskerCore::Models::Step", free_immediately, size)]
pub struct RubyStep {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
    pub retryable: bool,
    pub retry_limit: Option<i32>,
    pub in_process: bool,
    pub processed: bool,
    pub processed_at: Option<String>,
    pub attempts: Option<i32>,
    pub last_attempted_at: Option<String>,
    pub backoff_request_seconds: Option<i32>,
    pub inputs: Option<serde_json::Value>,
    pub results: Option<serde_json::Value>,
    pub skippable: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl RubyStep {
    /// Create a new RubyStep from a core WorkflowStep model
    pub fn from_workflow_step(step: &WorkflowStep) -> Self {
        Self {
            workflow_step_id: step.workflow_step_id,
            task_id: step.task_id,
            named_step_id: step.named_step_id,
            retryable: step.retryable,
            retry_limit: step.retry_limit,
            in_process: step.in_process,
            processed: step.processed,
            processed_at: step.processed_at.map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            attempts: step.attempts,
            last_attempted_at: step.last_attempted_at.map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            backoff_request_seconds: step.backoff_request_seconds,
            inputs: step.inputs.clone(),
            results: step.results.clone(),
            skippable: step.skippable,
            created_at: step.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
            updated_at: step.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
        }
    }

    /// Get workflow step ID
    pub fn workflow_step_id(&self) -> i64 {
        self.workflow_step_id
    }

    /// Get task ID
    pub fn task_id(&self) -> i64 {
        self.task_id
    }

    /// Get named step ID
    pub fn named_step_id(&self) -> i32 {
        self.named_step_id
    }

    /// Check if step is retryable
    pub fn retryable(&self) -> bool {
        self.retryable
    }

    /// Get retry limit
    pub fn retry_limit(&self) -> Option<i32> {
        self.retry_limit
    }

    /// Check if step is in process
    pub fn in_process(&self) -> bool {
        self.in_process
    }

    /// Check if step is processed
    pub fn processed(&self) -> bool {
        self.processed
    }

    /// Get processed at timestamp
    pub fn processed_at(&self) -> Option<String> {
        self.processed_at.clone()
    }

    /// Get attempts count
    pub fn attempts(&self) -> Option<i32> {
        self.attempts
    }

    /// Get last attempted at timestamp
    pub fn last_attempted_at(&self) -> Option<String> {
        self.last_attempted_at.clone()
    }

    /// Get backoff request seconds
    pub fn backoff_request_seconds(&self) -> Option<i32> {
        self.backoff_request_seconds
    }

    /// Get inputs as Ruby value
    pub fn inputs(&self) -> Result<magnus::Value, Error> {
        match &self.inputs {
            Some(value) => crate::context::json_to_ruby_value(value.clone()),
            None => {
                let ruby = Ruby::get().map_err(|e| Error::new(
                    magnus::exception::runtime_error(),
                    format!("Ruby unavailable: {}", e)
                ))?;
                Ok(ruby.qnil().as_value())
            }
        }
    }

    /// Get results as Ruby value
    pub fn results(&self) -> Result<magnus::Value, Error> {
        match &self.results {
            Some(value) => crate::context::json_to_ruby_value(value.clone()),
            None => {
                let ruby = Ruby::get().map_err(|e| Error::new(
                    magnus::exception::runtime_error(),
                    format!("Ruby unavailable: {}", e)
                ))?;
                Ok(ruby.qnil().as_value())
            }
        }
    }

    /// Check if step is skippable
    pub fn skippable(&self) -> bool {
        self.skippable
    }

    /// Get created at timestamp
    pub fn created_at(&self) -> String {
        self.created_at.clone()
    }

    /// Get updated at timestamp
    pub fn updated_at(&self) -> String {
        self.updated_at.clone()
    }
}

// Note: Methods are automatically available on Magnus wrapped classes
// The #[magnus::wrap] attribute makes all public methods available to Ruby
