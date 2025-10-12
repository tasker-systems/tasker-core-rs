//! Workflow Step and Dependency Creation
//!
//! Handles creating workflow steps and their dependencies from task templates.
//! This component builds the DAG (Directed Acyclic Graph) structure representing
//! the workflow execution plan.

use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::models::core::workflow_step::NewWorkflowStep;
use tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge;
use tasker_shared::models::{NamedStep, WorkflowStep};
use tasker_shared::system_context::SystemContext;

use super::TaskInitializationError;

/// Builds workflow steps and dependencies from task templates
pub struct WorkflowStepBuilder {
    context: Arc<SystemContext>,
}

impl WorkflowStepBuilder {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Create workflow steps and their dependencies from task template
    pub async fn create_workflow_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<(usize, HashMap<String, Uuid>), TaskInitializationError> {
        // Step 1: Create all named steps and workflow steps
        let step_mapping = self.create_steps(tx, task_uuid, task_template).await?;

        // Step 2: Create dependencies between steps
        self.create_step_dependencies(tx, task_template, &step_mapping)
            .await?;

        Ok((step_mapping.len(), step_mapping))
    }

    /// Create named steps and workflow steps using consistent transaction methods
    async fn create_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<HashMap<String, Uuid>, TaskInitializationError> {
        let mut step_mapping = HashMap::new();

        for step_definition in &task_template.steps {
            // Create or find named step using transaction
            let named_steps =
                NamedStep::find_by_name(self.context.database_pool(), &step_definition.name)
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to search for NamedStep '{}': {}",
                            step_definition.name, e
                        ))
                    })?;

            let named_step = if let Some(existing_step) = named_steps.first() {
                existing_step.clone()
            } else {
                // Create new named step using transaction-aware method
                let system_name = "tasker_core_rust"; // Use a consistent system name for Rust core
                NamedStep::find_or_create_by_name_with_transaction(
                    tx,
                    self.context.database_pool(),
                    &step_definition.name,
                    system_name,
                )
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create NamedStep '{}': {}",
                        step_definition.name, e
                    ))
                })?
            };

            // Create workflow step using consistent transaction method
            // Convert handler initialization to JSON Value for inputs
            let inputs = if step_definition.handler.initialization.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_value(&step_definition.handler.initialization).map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to serialize handler initialization for step '{}': {}",
                            step_definition.name, e
                        ))
                    })?,
                )
            };

            let new_workflow_step = NewWorkflowStep {
                task_uuid,
                named_step_uuid: named_step.named_step_uuid,
                retryable: Some(step_definition.retry.retryable),
                max_attempts: Some(step_definition.retry.max_attempts as i32),
                inputs,
                skippable: None, // Not available in new TaskTemplate - could be added if needed
            };

            let workflow_step = WorkflowStep::create_with_transaction(tx, new_workflow_step)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to create WorkflowStep '{}': {}",
                        step_definition.name, e
                    ))
                })?;

            step_mapping.insert(
                step_definition.name.clone(),
                workflow_step.workflow_step_uuid,
            );
        }

        Ok(step_mapping)
    }

    /// Create dependencies between workflow steps using consistent transaction methods
    async fn create_step_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_template: &TaskTemplate,
        step_mapping: &HashMap<String, Uuid>,
    ) -> Result<(), TaskInitializationError> {
        for step_definition in &task_template.steps {
            let to_step_uuid = step_mapping[&step_definition.name];

            // Create edges for all dependencies using transaction method
            for dependency_name in &step_definition.dependencies {
                if let Some(&from_step_uuid) = step_mapping.get(dependency_name) {
                    let new_edge = NewWorkflowStepEdge {
                        from_step_uuid,
                        to_step_uuid,
                        name: "provides".to_string(),
                    };

                    tasker_shared::models::WorkflowStepEdge::create_with_transaction(tx, new_edge)
                        .await
                        .map_err(|e| {
                            TaskInitializationError::Database(format!(
                                "Failed to create edge '{}' -> '{}': {}",
                                dependency_name, step_definition.name, e
                            ))
                        })?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_mapping_structure() {
        // This test verifies the step mapping logic without database
        let mut step_mapping = HashMap::new();
        step_mapping.insert("step1".to_string(), Uuid::new_v4());
        step_mapping.insert("step2".to_string(), Uuid::new_v4());

        assert_eq!(step_mapping.len(), 2);
        assert!(step_mapping.contains_key("step1"));
        assert!(step_mapping.contains_key("step2"));
    }

    #[test]
    fn test_handler_initialization_serialization() {
        let empty_init = serde_json::json!({});
        let with_data = serde_json::json!({"key": "value"});

        // Verify we can serialize handler initialization
        assert!(serde_json::to_value(&empty_init).is_ok());
        assert!(serde_json::to_value(&with_data).is_ok());
    }

    #[test]
    fn test_new_workflow_step_structure() {
        let task_uuid = Uuid::new_v4();
        let named_step_uuid = Uuid::new_v4();

        let new_step = NewWorkflowStep {
            task_uuid,
            named_step_uuid,
            retryable: Some(true),
            max_attempts: Some(3),
            inputs: Some(serde_json::json!({"test": "value"})),
            skippable: None,
        };

        assert_eq!(new_step.task_uuid, task_uuid);
        assert_eq!(new_step.named_step_uuid, named_step_uuid);
        assert_eq!(new_step.retryable, Some(true));
        assert_eq!(new_step.max_attempts, Some(3));
        assert!(new_step.inputs.is_some());
    }
}
