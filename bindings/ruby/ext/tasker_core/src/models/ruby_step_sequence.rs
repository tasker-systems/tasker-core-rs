//! # Ruby Step Sequence Wrapper
//!
//! Magnus-wrapped Ruby class for step sequence data access from Ruby code.

use magnus::{prelude::*, Error, Ruby};
use tasker_core::models::core::workflow_step::WorkflowStep;
use sqlx::PgPool;
use crate::models::RubyStep;

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
}

impl RubyStepSequence {
    /// Create a new RubyStepSequence from a WorkflowStep and its dependencies
    pub async fn from_workflow_step(step: &WorkflowStep, pool: &PgPool) -> Result<Self, sqlx::Error> {
        let dependencies = step.get_dependencies(pool).await?;
        let ruby_dependencies: Vec<RubyStep> = dependencies.iter()
            .map(|dep| RubyStep::from_workflow_step(dep))
            .collect();

        Ok(Self {
            total_steps: dependencies.len() + 1, // Include current step
            current_position: dependencies.len(), // Current step is last
            dependencies: ruby_dependencies,
            current_step_id: step.workflow_step_id,
        })
    }

    /// Create a new RubyStepSequence with explicit values
    pub fn new(total_steps: usize, current_position: usize, dependencies: Vec<RubyStep>, current_step_id: i64) -> Self {
        Self {
            total_steps,
            current_position,
            dependencies,
            current_step_id,
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
}

// Note: Methods are automatically available on Magnus wrapped classes
// The #[magnus::wrap] attribute makes all public methods available to Ruby
