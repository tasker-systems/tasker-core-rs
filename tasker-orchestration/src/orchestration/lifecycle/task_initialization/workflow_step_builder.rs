//! Workflow Step and Dependency Creation
//!
//! Handles creating workflow steps and their dependencies from task templates.
//! This component builds the DAG (Directed Acyclic Graph) structure representing
//! the workflow execution plan.

use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::models::core::task_template::TaskTemplate;
use tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge;
use tasker_shared::system_context::SystemContext;

use super::{TaskInitializationError, WorkflowStepCreator};

/// Builds workflow steps and dependencies from task templates
pub struct WorkflowStepBuilder {
    context: Arc<SystemContext>,
    step_creator: WorkflowStepCreator,
}

impl WorkflowStepBuilder {
    pub fn new(context: Arc<SystemContext>) -> Self {
        let step_creator = WorkflowStepCreator::new(context.clone());
        Self {
            context,
            step_creator,
        }
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

    /// Create named steps and workflow steps using the shared WorkflowStepCreator
    ///
    /// For workflows with decision points, this only creates the initial step set
    /// (steps up to and including decision points, but not their descendants).
    /// For workflows without decision points, this creates all steps.
    async fn create_steps(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        task_template: &TaskTemplate,
    ) -> Result<HashMap<String, Uuid>, TaskInitializationError> {
        // Get the initial step set (partial graph for decision-point workflows)
        let initial_steps = task_template.initial_step_set();

        // Convert borrowed step definitions to owned for the batch creator
        let step_defs: Vec<_> = initial_steps.into_iter().cloned().collect();

        // Delegate to the shared step creator
        self.step_creator
            .create_steps_batch(tx, task_uuid, &step_defs)
            .await
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
                    // Check for self-referencing cycles (A -> A)
                    if from_step_uuid == to_step_uuid {
                        return Err(TaskInitializationError::CycleDetected {
                            from: dependency_name.clone(),
                            to: step_definition.name.clone(),
                        });
                    }

                    // Check for cycles before creating the edge
                    let would_cycle = tasker_shared::models::WorkflowStepEdge::would_create_cycle(
                        self.context.database_pool(),
                        from_step_uuid,
                        to_step_uuid,
                    )
                    .await
                    .map_err(|e| {
                        TaskInitializationError::Database(format!(
                            "Failed to check for cycles when adding edge '{}' -> '{}': {}",
                            dependency_name, step_definition.name, e
                        ))
                    })?;

                    if would_cycle {
                        return Err(TaskInitializationError::CycleDetected {
                            from: dependency_name.clone(),
                            to: step_definition.name.clone(),
                        });
                    }

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
    use tasker_shared::models::core::workflow_step::NewWorkflowStep;

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
