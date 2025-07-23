//! # Ruby Step Sequence Wrapper
//!
//! Magnus-wrapped Ruby class for step sequence data access from Ruby code.

use magnus::{prelude::*, Error, Ruby};
use tasker_core::models::core::workflow_step::WorkflowStep;
use sqlx::PgPool;
use crate::models::ruby_step::RubyStep;

/// Ruby wrapper for step sequence data
///
/// This provides type-safe access to step sequence information from Ruby code.
/// Uses `free_immediately` for memory safety since these are read-only snapshots.
#[magnus::wrap(class = "TaskerCore::Models::StepSequence", free_immediately, size)]
#[derive(Clone)]
pub struct RubyStepSequence {
    pub total_steps: usize,
    pub current_position: usize,
    pub dependencies: Vec<RubyStep>,
    pub current_step_id: i64,
    pub all_steps: Vec<RubyStep>, // All steps for Ruby handler compatibility (includes dependencies + current)
}

impl RubyStepSequence {
    /// Create a new RubyStepSequence from a WorkflowStep and its dependencies
    /// DEPRECATED: This method uses meaningless fallback names and should not be used.
    /// Use from_workflow_step_with_step_names() instead to ensure proper step name resolution.
    pub async fn from_workflow_step(step: &WorkflowStep, pool: &PgPool) -> Result<Self, sqlx::Error> {
        // Try to get the real step name from the database
        let named_step = tasker_core::models::NamedStep::find_by_id(pool, step.named_step_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        
        Self::from_workflow_step_with_name(step, &named_step.name, pool).await
    }

    /// Create a new RubyStepSequence from a WorkflowStep with an explicit name
    pub async fn from_workflow_step_with_name(step: &WorkflowStep, step_name: &str, pool: &PgPool) -> Result<Self, sqlx::Error> {
        Self::from_workflow_step_with_step_names(step, step_name, None, pool).await
    }

    /// Create a new RubyStepSequence from a WorkflowStep with step name mapping for all steps
    pub async fn from_workflow_step_with_step_names(
        step: &WorkflowStep, 
        step_name: &str, 
        step_name_mapping: Option<std::collections::HashMap<i32, String>>, 
        pool: &PgPool
    ) -> Result<Self, sqlx::Error> {
        let dependencies = step.get_dependencies(pool).await?;
        let ruby_dependencies: Vec<RubyStep> = dependencies.iter()
            .map(|dep| RubyStep::from_workflow_step(dep))
            .collect();

        // Get ALL workflow steps for this task (not just dependencies)
        // This is needed so step handlers can access results from any previous step
        let all_workflow_steps = WorkflowStep::for_task(pool, step.task_id).await?;

        // Convert all workflow steps to RubyStep objects with proper step name resolution
        let mut all_ruby_steps = Vec::new();
        for ws in &all_workflow_steps {
            let ruby_step = if ws.workflow_step_id == step.workflow_step_id {
                // Use the provided step name for the current step
                RubyStep::from_workflow_step_with_name(ws, step_name)
            } else {
                // Resolve step name from mapping - fallback patterns are NOT ALLOWED
                // Every named_step_id MUST have a meaningful name from the task template
                let resolved_name = if let Some(ref mapping) = step_name_mapping {
                    mapping.get(&ws.named_step_id)
                        .cloned()
                        .ok_or_else(|| {
                            sqlx::Error::RowNotFound
                        })?
                } else {
                    return Err(sqlx::Error::RowNotFound);
                };
                RubyStep::from_workflow_step_with_name(ws, &resolved_name)
            };
            all_ruby_steps.push(ruby_step);
        }

        // Find current step position in the sequence
        let current_position = all_workflow_steps.iter()
            .position(|ws| ws.workflow_step_id == step.workflow_step_id)
            .unwrap_or(0);

        Ok(Self {
            total_steps: all_workflow_steps.len(),
            current_position,
            dependencies: ruby_dependencies,
            current_step_id: step.workflow_step_id,
            all_steps: all_ruby_steps,
        })
    }

    /// Create a new RubyStepSequence with explicit values
    pub fn new(total_steps: usize, current_position: usize, dependencies: Vec<RubyStep>, current_step_id: i64) -> Self {
        // For compatibility, create all_steps from dependencies for now
        // TODO: This should be updated to receive all steps properly
        let all_steps = dependencies.clone();
        
        Self {
            total_steps,
            current_position,
            dependencies,
            current_step_id,
            all_steps,
        }
    }

    /// Create a new RubyStepSequence with all steps included
    pub fn new_with_all_steps(total_steps: usize, current_position: usize, dependencies: Vec<RubyStep>, current_step_id: i64, all_steps: Vec<RubyStep>) -> Self {
        Self {
            total_steps,
            current_position,
            dependencies,
            current_step_id,
            all_steps,
        }
    }

    /// Get total steps count
    pub fn total_steps(&self) -> usize {
        self.total_steps
    }

    /// Get current position in sequence
    pub fn current_position(&self) -> usize {
        self.current_position
    }

    /// Get dependencies as Ruby array of RubyStep objects
    pub fn dependencies(&self) -> Result<magnus::Value, Error> {
        let ruby = Ruby::get().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Ruby unavailable: {}", e)
        ))?;
        let array = ruby.ary_new_capa(self.dependencies.len());

        for ruby_step in &self.dependencies {
            let step_clone = ruby_step.clone();
            let wrapped_step = ruby.obj_wrap(step_clone);
            array.push(wrapped_step)?;
        }

        Ok(array.as_value())
    }

    /// Get all steps as Ruby array (for Ruby handler compatibility)
    /// This includes dependencies + current step, which is what Ruby handlers expect
    pub fn steps(&self) -> Result<magnus::Value, Error> {
        let ruby = Ruby::get().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Ruby unavailable: {}", e)
        ))?;
        let array = ruby.ary_new_capa(self.all_steps.len());

        for ruby_step in &self.all_steps {
            let step_clone = ruby_step.clone();
            let wrapped_step = ruby.obj_wrap(step_clone);
            array.push(wrapped_step)?;
        }

        Ok(array.as_value())
    }

    /// Get current step ID
    pub fn current_step_id(&self) -> i64 {
        self.current_step_id
    }

    /// Check if this is the first step
    pub fn first_step(&self) -> bool {
        self.current_position == 0
    }

    /// Check if this is the last step
    pub fn last_step(&self) -> bool {
        self.current_position == self.total_steps.saturating_sub(1)
    }

    /// Get progress percentage (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            self.current_position as f64 / self.total_steps as f64
        }
    }

    /// Get remaining steps count
    pub fn remaining_steps(&self) -> usize {
        self.total_steps.saturating_sub(self.current_position + 1)
    }

    /// Get dependencies count
    pub fn dependencies_count(&self) -> usize {
        self.dependencies.len()
    }

    /// Get step sequence as Ruby hash
    pub fn to_h(&self) -> Result<magnus::RHash, Error> {
        let ruby = Ruby::get().map_err(|e| Error::new(
            magnus::exception::runtime_error(),
            format!("Ruby unavailable: {}", e)
        ))?;

        let hash = ruby.hash_new();
        hash.aset("total_steps", self.total_steps)?;
        hash.aset("current_position", self.current_position)?;
        hash.aset("current_step_id", self.current_step_id)?;

        // Convert dependencies to array of hashes
        let deps_array = ruby.ary_new_capa(self.dependencies.len());
        for ruby_step in &self.dependencies {
            let step_hash = ruby_step.to_h()?;
            deps_array.push(step_hash)?;
        }
        hash.aset("dependencies", deps_array)?;

        // Convert all_steps to array of hashes
        let all_steps_array = ruby.ary_new_capa(self.all_steps.len());
        for ruby_step in &self.all_steps {
            let step_hash = ruby_step.to_h()?;
            all_steps_array.push(step_hash)?;
        }
        hash.aset("all_steps", all_steps_array)?;

        Ok(hash)
    }
}

// Note: Methods are automatically available on Magnus wrapped classes
// The #[magnus::wrap] attribute makes all public methods available to Ruby
