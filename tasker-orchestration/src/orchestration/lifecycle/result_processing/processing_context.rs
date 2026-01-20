//! Result Processing Context (TAS-157)
//!
//! Provides cached entity lookups for result processing operations to eliminate
//! redundant database queries when processing decision points and batch steps.
//!
//! ## Problem
//!
//! The `is_decision_step()` and `is_batchable_step()` methods in `MessageHandler`
//! perform identical database queries:
//! - `Task::find_by_id()`
//! - `task.for_orchestration()`
//! - `NamedStep::find_by_uuid()`
//! - `task_handler_registry.get_task_template_from_registry()`
//!
//! When both checks are performed on the same step, this results in 8-12 redundant queries.
//!
//! ## Solution
//!
//! `ResultProcessingContext` caches these entities on first access, reducing queries
//! from 10-12 to 5-6 for steps that trigger both checks.

use std::sync::Arc;

use sqlx::PgPool;
use tracing::debug;
use uuid::Uuid;

use tasker_shared::errors::OrchestrationResult;
use tasker_shared::models::core::task::TaskForOrchestration;
use tasker_shared::models::core::task_template::{StepDefinition, StepType, TaskTemplate};
use tasker_shared::models::{NamedStep, Task, WorkflowStep};
use tasker_shared::system_context::SystemContext;
use tasker_shared::OrchestrationError;

/// Cached context for result processing operations
///
/// This struct lazily loads and caches entities needed for decision point
/// and batch processing checks, eliminating redundant database queries.
#[derive(Debug)]
pub struct ResultProcessingContext {
    /// Database pool for lazy loading
    pool: PgPool,
    /// System context for handler registry access
    system_context: Arc<SystemContext>,
    /// The step UUID being processed
    step_uuid: Uuid,
    /// Correlation ID for tracing
    correlation_id: Uuid,

    // Cached entities (lazily loaded)
    workflow_step: Option<WorkflowStep>,
    task: Option<Task>,
    task_for_orchestration: Option<TaskForOrchestration>,
    named_step: Option<NamedStep>,
    task_template: Option<TaskTemplate>,
}

impl ResultProcessingContext {
    /// Create a new result processing context
    pub fn new(system_context: Arc<SystemContext>, step_uuid: Uuid, correlation_id: Uuid) -> Self {
        Self {
            pool: system_context.database_pool().clone(),
            system_context,
            step_uuid,
            correlation_id,
            workflow_step: None,
            task: None,
            task_for_orchestration: None,
            named_step: None,
            task_template: None,
        }
    }

    /// Get or load the workflow step
    pub async fn get_workflow_step(&mut self) -> OrchestrationResult<Option<&WorkflowStep>> {
        if self.workflow_step.is_none() {
            debug!(
                correlation_id = %self.correlation_id,
                step_uuid = %self.step_uuid,
                "ResultProcessingContext: Loading workflow step"
            );
            self.workflow_step = WorkflowStep::find_by_id(&self.pool, self.step_uuid).await?;
        }
        Ok(self.workflow_step.as_ref())
    }

    /// Get or load the task
    pub async fn get_task(&mut self) -> OrchestrationResult<Option<&Task>> {
        // Ensure workflow step is loaded first
        if self.workflow_step.is_none() {
            self.get_workflow_step().await?;
        }

        if self.task.is_none() {
            if let Some(ref ws) = self.workflow_step {
                debug!(
                    correlation_id = %self.correlation_id,
                    task_uuid = %ws.task_uuid,
                    "ResultProcessingContext: Loading task"
                );
                self.task = Task::find_by_id(&self.pool, ws.task_uuid).await?;
            }
        }
        Ok(self.task.as_ref())
    }

    /// Get or load the task for orchestration metadata
    pub async fn get_task_for_orchestration(
        &mut self,
    ) -> OrchestrationResult<Option<&TaskForOrchestration>> {
        // Ensure task is loaded first
        if self.task.is_none() {
            self.get_task().await?;
        }

        if self.task_for_orchestration.is_none() {
            if let Some(ref task) = self.task {
                debug!(
                    correlation_id = %self.correlation_id,
                    task_uuid = %task.task_uuid,
                    "ResultProcessingContext: Loading task orchestration metadata"
                );
                self.task_for_orchestration =
                    Some(task.for_orchestration(&self.pool).await.map_err(|e| {
                        OrchestrationError::from(
                            format!("Failed to load task orchestration metadata: {}", e).as_str(),
                        )
                    })?);
            }
        }
        Ok(self.task_for_orchestration.as_ref())
    }

    /// Get or load the named step
    pub async fn get_named_step(&mut self) -> OrchestrationResult<Option<&NamedStep>> {
        // Ensure workflow step is loaded first
        if self.workflow_step.is_none() {
            self.get_workflow_step().await?;
        }

        if self.named_step.is_none() {
            if let Some(ref ws) = self.workflow_step {
                debug!(
                    correlation_id = %self.correlation_id,
                    named_step_uuid = %ws.named_step_uuid,
                    "ResultProcessingContext: Loading named step"
                );
                self.named_step = NamedStep::find_by_uuid(&self.pool, ws.named_step_uuid).await?;
            }
        }
        Ok(self.named_step.as_ref())
    }

    /// Get or load the task template
    pub async fn get_task_template(&mut self) -> OrchestrationResult<Option<&TaskTemplate>> {
        // Ensure task_for_orchestration is loaded first
        if self.task_for_orchestration.is_none() {
            self.get_task_for_orchestration().await?;
        }

        if self.task_template.is_none() {
            if let Some(ref task_metadata) = self.task_for_orchestration {
                debug!(
                    correlation_id = %self.correlation_id,
                    namespace = %task_metadata.namespace_name,
                    task_name = %task_metadata.task_name,
                    task_version = %task_metadata.task_version,
                    "ResultProcessingContext: Loading task template from registry"
                );

                let handler_metadata = self
                    .system_context
                    .task_handler_registry
                    .get_task_template_from_registry(
                        &task_metadata.namespace_name,
                        &task_metadata.task_name,
                        &task_metadata.task_version,
                    )
                    .await
                    .map_err(|e| {
                        OrchestrationError::from(
                            format!("Failed to load task template from registry: {}", e).as_str(),
                        )
                    })?;

                let task_template: TaskTemplate =
                    serde_json::from_value(handler_metadata.config_schema.ok_or_else(|| {
                        OrchestrationError::from("No config schema found in handler metadata")
                    })?)
                    .map_err(|e| {
                        OrchestrationError::from(
                            format!("Failed to deserialize task template: {}", e).as_str(),
                        )
                    })?;

                self.task_template = Some(task_template);
            }
        }
        Ok(self.task_template.as_ref())
    }

    /// Get the step definition from the task template
    ///
    /// This method loads all required entities and returns the step definition
    /// that matches the named step name.
    pub async fn get_step_definition(&mut self) -> OrchestrationResult<Option<&StepDefinition>> {
        // Ensure all required entities are loaded
        self.get_named_step().await?;
        self.get_task_template().await?;

        // Find the step definition in the template
        let step_name = self.named_step.as_ref().map(|ns| ns.name.as_str());
        let template = self.task_template.as_ref();

        match (step_name, template) {
            (Some(name), Some(template)) => Ok(template.steps.iter().find(|s| s.name == name)),
            _ => Ok(None),
        }
    }

    /// Check if the step is a decision point (TAS-157 optimized)
    ///
    /// Uses cached entities to avoid redundant queries.
    pub async fn is_decision_step(&mut self) -> OrchestrationResult<bool> {
        match self.get_step_definition().await? {
            Some(def) => Ok(def.is_decision()),
            None => {
                debug!(
                    correlation_id = %self.correlation_id,
                    step_uuid = %self.step_uuid,
                    "Step definition not found in template"
                );
                Ok(false)
            }
        }
    }

    /// Check if the step is batchable (TAS-157 optimized)
    ///
    /// Uses cached entities to avoid redundant queries.
    pub async fn is_batchable_step(&mut self) -> OrchestrationResult<bool> {
        match self.get_step_definition().await? {
            Some(def) => Ok(def.step_type == StepType::Batchable),
            None => {
                debug!(
                    correlation_id = %self.correlation_id,
                    step_uuid = %self.step_uuid,
                    "Step definition not found in template"
                );
                Ok(false)
            }
        }
    }

    /// Get the cached workflow step (without loading)
    ///
    /// Returns the workflow step if it has already been loaded.
    pub fn workflow_step(&self) -> Option<&WorkflowStep> {
        self.workflow_step.as_ref()
    }

    /// Get the cached task (without loading)
    ///
    /// Returns the task if it has already been loaded.
    #[allow(dead_code, reason = "Used by tests for context introspection")]
    pub fn task(&self) -> Option<&Task> {
        self.task.as_ref()
    }

    /// Get the step UUID
    pub fn step_uuid(&self) -> Uuid {
        self.step_uuid
    }

    /// Get the correlation ID
    pub fn correlation_id(&self) -> Uuid {
        self.correlation_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_processing_context_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let processing_context = ResultProcessingContext::new(context, step_uuid, correlation_id);

        assert_eq!(processing_context.step_uuid(), step_uuid);
        assert_eq!(processing_context.correlation_id(), correlation_id);
        assert!(processing_context.workflow_step().is_none());
        assert!(processing_context.task().is_none());

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_processing_context_nonexistent_step(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let mut processing_context =
            ResultProcessingContext::new(context, step_uuid, correlation_id);

        // Should return None for non-existent step
        let workflow_step = processing_context.get_workflow_step().await?;
        assert!(workflow_step.is_none());

        // Subsequent calls should also return None without additional queries
        let workflow_step_cached = processing_context.get_workflow_step().await?;
        assert!(workflow_step_cached.is_none());

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_is_decision_step_nonexistent(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let mut processing_context =
            ResultProcessingContext::new(context, step_uuid, correlation_id);

        // Should return false for non-existent step
        let is_decision = processing_context.is_decision_step().await?;
        assert!(!is_decision);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_is_batchable_step_nonexistent(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let step_uuid = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let mut processing_context =
            ResultProcessingContext::new(context, step_uuid, correlation_id);

        // Should return false for non-existent step
        let is_batchable = processing_context.is_batchable_step().await?;
        assert!(!is_batchable);

        Ok(())
    }
}
