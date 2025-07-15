//! # Ruby Task Wrapper
//!
//! Magnus-wrapped Ruby class for Task model access from Ruby code.

use magnus::{prelude::*, Error, Ruby};
use tasker_core::models::core::task::Task;

/// Ruby wrapper for Task model data
///
/// This provides type-safe access to Task fields from Ruby code.
/// Uses `free_immediately` for memory safety since these are read-only snapshots.
#[magnus::wrap(class = "TaskerCore::Models::Task", free_immediately, size)]
pub struct RubyTask {
    pub task_id: i64,
    pub named_task_id: i32,
    pub complete: bool,
    pub requested_at: String, // ISO 8601 formatted
    pub initiator: Option<String>,
    pub source_system: Option<String>,
    pub reason: Option<String>,
    pub bypass_steps: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub context: Option<serde_json::Value>,
    pub identity_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

impl RubyTask {
    /// Create a new RubyTask from a core Task model
    pub fn from_task(task: &Task) -> Self {
        Self {
            task_id: task.task_id,
            named_task_id: task.named_task_id,
            complete: task.complete,
            requested_at: task.requested_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
            initiator: task.initiator.clone(),
            source_system: task.source_system.clone(),
            reason: task.reason.clone(),
            bypass_steps: task.bypass_steps.clone(),
            tags: task.tags.clone(),
            context: task.context.clone(),
            identity_hash: task.identity_hash.clone(),
            created_at: task.created_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
            updated_at: task.updated_at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
        }
    }

    /// Get task ID
    pub fn task_id(&self) -> i64 {
        self.task_id
    }

    /// Get named task ID
    pub fn named_task_id(&self) -> i32 {
        self.named_task_id
    }

    /// Check if task is complete
    pub fn complete(&self) -> bool {
        self.complete
    }

    /// Get requested at timestamp
    pub fn requested_at(&self) -> String {
        self.requested_at.clone()
    }

    /// Get initiator
    pub fn initiator(&self) -> Option<String> {
        self.initiator.clone()
    }

    /// Get source system
    pub fn source_system(&self) -> Option<String> {
        self.source_system.clone()
    }

    /// Get reason
    pub fn reason(&self) -> Option<String> {
        self.reason.clone()
    }

    /// Get bypass steps as Ruby value
    pub fn bypass_steps(&self) -> Result<magnus::Value, Error> {
        match &self.bypass_steps {
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

    /// Get tags as Ruby value
    pub fn tags(&self) -> Result<magnus::Value, Error> {
        match &self.tags {
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

    /// Get context as Ruby value
    pub fn context(&self) -> Result<magnus::Value, Error> {
        match &self.context {
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

    /// Get identity hash
    pub fn identity_hash(&self) -> String {
        self.identity_hash.clone()
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
