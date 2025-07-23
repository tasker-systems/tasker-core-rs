//! # Ruby Step Wrapper
//!
//! Magnus-wrapped Ruby class for WorkflowStep model access from Ruby code.

use magnus::{prelude::*, Error, Ruby, RHash, TryConvert};
use tasker_core::models::core::workflow_step::WorkflowStep;
use std::sync::{Arc, RwLock};

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
    pub name: String, // Step name for Ruby handler compatibility
    pub retryable: bool,
    pub retry_limit: Option<i32>,
    pub in_process: bool,
    pub processed: bool,
    pub processed_at: Option<String>,
    pub attempts: Option<i32>,
    pub last_attempted_at: Option<String>,
    pub backoff_request_seconds: Option<i32>,
    pub inputs: Option<serde_json::Value>,
    pub results: Arc<RwLock<Option<serde_json::Value>>>, // Mutable results for Ruby handler compatibility (thread-safe)
    pub skippable: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl RubyStep {
    /// Create a new RubyStep from a core WorkflowStep model
    /// DEPRECATED: This method creates meaningless fallback names and should not be used.
    /// Use from_workflow_step_with_name() with a proper step name instead.
    /// This method is kept for backward compatibility but may be removed in the future.
    pub fn from_workflow_step(step: &WorkflowStep) -> Self {
        // Generate a fallback name from named_step_id
        // WARNING: This creates meaningless names that break step handler functionality
        let fallback_name = format!("step_{}", step.named_step_id);
        Self::from_workflow_step_with_name(step, &fallback_name)
    }

    /// Create a new RubyStep from a core WorkflowStep model with an explicit name
    pub fn from_workflow_step_with_name(step: &WorkflowStep, step_name: &str) -> Self {
        Self {
            workflow_step_id: step.workflow_step_id,
            task_id: step.task_id,
            named_step_id: step.named_step_id,
            name: step_name.to_string(),
            retryable: step.retryable,
            retry_limit: step.retry_limit,
            in_process: step.in_process,
            processed: step.processed,
            processed_at: step.processed_at.map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            attempts: step.attempts,
            last_attempted_at: step.last_attempted_at.map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
            backoff_request_seconds: step.backoff_request_seconds,
            inputs: step.inputs.clone(),
            results: Arc::new(RwLock::new(step.results.clone())),
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

    /// Get step name (for Ruby handler compatibility)
    pub fn name(&self) -> String {
        self.name.clone()
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

    /// Get results as Ruby hash (for Ruby handler compatibility)
    pub fn results(&self) -> Result<RHash, Error> {
        let ruby = Ruby::get().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Ruby unavailable: {}", e)
        ))?;

        let results_ref = self.results.read().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Failed to read results lock: {}", e)
        ))?;
        match results_ref.as_ref() {
            Some(value) => {
                // Convert JSON to Ruby hash
                let ruby_value = crate::context::json_to_ruby_value(value.clone())?;
                TryConvert::try_convert(ruby_value).map_err(|e| Error::new(
                    magnus::exception::type_error(),
                    format!("Failed to convert results to hash: {}", e)
                ))
            }
            None => {
                // Return empty hash if no results
                Ok(ruby.hash_new())
            }
        }
    }

    /// Set results from Ruby hash (for Ruby handler compatibility)
    pub fn set_results(&self, new_results: RHash) -> Result<(), Error> {
        // Convert Ruby hash to JSON Value
        let json_value = crate::context::ruby_value_to_json(new_results.as_value())?;
        
        // Update the RwLock
        *self.results.write().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Failed to write results lock: {}", e)
        ))? = Some(json_value);
        
        Ok(())
    }

    /// Ruby-style results assignment (step.results = {...})
    /// This delegates to set_results for hash assignment
    pub fn results_assign(&self, new_results: magnus::Value) -> Result<(), Error> {
        // Try to convert to hash first
        if let Ok(hash) = TryConvert::try_convert(new_results) {
            self.set_results(hash)
        } else {
            // If not a hash, convert to JSON and store
            let json_value = crate::context::ruby_value_to_json(new_results)?;
            *self.results.write().map_err(|e| Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to write results lock: {}", e)
            ))? = Some(json_value);
            Ok(())
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

    /// Get step as Ruby hash
    pub fn to_h(&self) -> Result<RHash, Error> {
        let ruby = Ruby::get().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Ruby unavailable: {}", e)
        ))?;

        let hash = ruby.hash_new();
        hash.aset("workflow_step_id", self.workflow_step_id)?;
        hash.aset("task_id", self.task_id)?;
        hash.aset("named_step_id", self.named_step_id)?;
        hash.aset("name", self.name.clone())?;
        hash.aset("retryable", self.retryable)?;
        hash.aset("retry_limit", self.retry_limit)?;
        hash.aset("in_process", self.in_process)?;
        hash.aset("processed", self.processed)?;
        hash.aset("processed_at", self.processed_at.clone())?;
        hash.aset("attempts", self.attempts)?;
        hash.aset("last_attempted_at", self.last_attempted_at.clone())?;
        hash.aset("backoff_request_seconds", self.backoff_request_seconds)?;
        hash.aset("skippable", self.skippable)?;
        hash.aset("created_at", self.created_at.clone())?;
        hash.aset("updated_at", self.updated_at.clone())?;

        // Handle inputs JSON
        match &self.inputs {
            Some(value) => {
                let ruby_value = crate::context::json_to_ruby_value(value.clone())?;
                hash.aset("inputs", ruby_value)?;
            }
            None => {
                hash.aset("inputs", ruby.qnil())?;
            }
        }

        // Handle results JSON - read from RwLock
        let results_ref = self.results.read().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Failed to read results lock: {}", e)
        ))?;
        match results_ref.as_ref() {
            Some(value) => {
                let ruby_value = crate::context::json_to_ruby_value(value.clone())?;
                hash.aset("results", ruby_value)?;
            }
            None => {
                hash.aset("results", ruby.qnil())?;
            }
        }

        Ok(hash)
    }
}

// Note: Methods are automatically available on Magnus wrapped classes
// The #[magnus::wrap] attribute makes all public methods available to Ruby
