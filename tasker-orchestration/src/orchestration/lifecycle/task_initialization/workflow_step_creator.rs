//! Workflow Step Creation - Shared Logic
//!
//! Provides reusable step creation logic for both task initialization and
//! dynamic decision point step creation.

use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::models::core::task_template::StepDefinition;
use tasker_shared::models::core::workflow_step::NewWorkflowStep;
use tasker_shared::models::{NamedStep, WorkflowStep};
use tasker_shared::system_context::SystemContext;

use super::TaskInitializationError;

/// Reusable workflow step creator
///
/// This component provides shared logic for creating workflow steps and named steps.
/// It is used by:
/// - TaskInitializer: During initial task creation
/// - DecisionPointService: For dynamic step creation from decision points
///
/// ## Usage
///
/// ```rust,ignore
/// use tasker_orchestration::orchestration::lifecycle::task_initialization::WorkflowStepCreator;
///
/// let creator = WorkflowStepCreator::new(context.clone());
///
/// // Create a single step within a transaction
/// let (workflow_step, named_step) = creator.create_single_step(
///     &mut tx,
///     task_uuid,
///     &step_definition,
/// ).await?;
///
/// // Create multiple steps
/// let step_mapping = creator.create_steps_batch(
///     &mut tx,
///     task_uuid,
///     &step_definitions,
/// ).await?;
/// ```
#[derive(Debug)]
pub struct WorkflowStepCreator {
    context: Arc<SystemContext>,
}

impl WorkflowStepCreator {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Create a single workflow step and its named step
    ///
    /// This is the core step creation logic, used by both task initialization
    /// and dynamic decision point creation.
    ///
    /// ## Transaction Safety
    ///
    /// This method operates within the provided transaction. The caller is
    /// responsible for committing or rolling back the transaction.
    pub async fn create_single_step(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_definition: &StepDefinition,
    ) -> Result<(WorkflowStep, NamedStep), TaskInitializationError> {
        // Find or create named step
        let named_step = self.find_or_create_named_step(tx, step_definition).await?;

        // Serialize handler initialization
        let inputs = self.serialize_handler_initialization(step_definition)?;

        // Create workflow step
        let new_workflow_step = NewWorkflowStep {
            task_uuid,
            named_step_uuid: named_step.named_step_uuid,
            retryable: Some(step_definition.retry.retryable),
            max_attempts: Some(step_definition.retry.max_attempts as i32),
            inputs,
            skippable: None,
        };

        let workflow_step = WorkflowStep::create_with_transaction(tx, new_workflow_step)
            .await
            .map_err(|e| {
                TaskInitializationError::Database(format!(
                    "Failed to create WorkflowStep '{}': {}",
                    step_definition.name, e
                ))
            })?;

        Ok((workflow_step, named_step))
    }

    /// Create multiple workflow steps in a batch
    ///
    /// Returns a mapping of step names to workflow step UUIDs.
    pub async fn create_steps_batch(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        step_definitions: &[StepDefinition],
    ) -> Result<HashMap<String, Uuid>, TaskInitializationError> {
        let mut step_mapping = HashMap::new();

        for step_definition in step_definitions {
            let (workflow_step, _named_step) = self
                .create_single_step(tx, task_uuid, step_definition)
                .await?;

            step_mapping.insert(
                step_definition.name.clone(),
                workflow_step.workflow_step_uuid,
            );
        }

        Ok(step_mapping)
    }

    /// Find or create a named step for the given step definition
    async fn find_or_create_named_step(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        step_definition: &StepDefinition,
    ) -> Result<NamedStep, TaskInitializationError> {
        // Search for existing named step
        let named_steps =
            NamedStep::find_by_name(self.context.database_pool(), &step_definition.name)
                .await
                .map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to search for NamedStep '{}': {}",
                        step_definition.name, e
                    ))
                })?;

        if let Some(existing_step) = named_steps.first() {
            Ok(existing_step.clone())
        } else {
            // Create new named step using transaction
            let system_name = "tasker_core_rust";
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
            })
        }
    }

    /// Serialize handler initialization configuration
    fn serialize_handler_initialization(
        &self,
        step_definition: &StepDefinition,
    ) -> Result<Option<serde_json::Value>, TaskInitializationError> {
        if step_definition.handler.initialization.is_empty() {
            Ok(None)
        } else {
            Some(
                serde_json::to_value(&step_definition.handler.initialization).map_err(|e| {
                    TaskInitializationError::Database(format!(
                        "Failed to serialize handler initialization for step '{}': {}",
                        step_definition.name, e
                    ))
                }),
            )
            .transpose()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::models::core::task_template::{
        BackoffStrategy, HandlerDefinition, RetryConfiguration, StepType,
    };
    use tasker_shared::models::factories::{base::SqlxFactory, TaskFactory};

    fn create_test_step_definition(name: &str) -> StepDefinition {
        StepDefinition {
            name: name.to_string(),
            description: Some(format!("Test step: {}", name)),
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration {
                retryable: true,
                max_attempts: 3,
                backoff: BackoffStrategy::Exponential,
                backoff_base_ms: Some(1000),
                max_backoff_ms: Some(30000),
            },
            timeout_seconds: Some(30),
            publishes_events: vec![],
        }
    }

    fn create_test_step_with_init(name: &str, timeout_ms: i32) -> StepDefinition {
        let mut initialization = HashMap::new();
        initialization.insert("timeout_ms".to_string(), serde_json::json!(timeout_ms));

        StepDefinition {
            name: name.to_string(),
            description: Some(format!("Test step: {}", name)),
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
                initialization,
            },
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: RetryConfiguration::default(),
            timeout_seconds: Some(30),
            publishes_events: vec![],
        }
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_single_step(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        // Create a valid task first
        let task = TaskFactory::new()
            .with_namespace("test")
            .create(&pool)
            .await?;
        let task_uuid = task.task_uuid;

        let step_def = create_test_step_definition("test_step_1");

        let mut tx = context.database_pool().begin().await?;
        let (workflow_step, named_step) = creator
            .create_single_step(&mut tx, task_uuid, &step_def)
            .await?;
        tx.commit().await?;

        assert_eq!(workflow_step.task_uuid, task_uuid);
        assert_eq!(workflow_step.named_step_uuid, named_step.named_step_uuid);
        assert_eq!(named_step.name, "test_step_1");
        assert!(workflow_step.retryable);
        assert_eq!(workflow_step.max_attempts, Some(3));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_single_step_with_handler_initialization(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        // Create a valid task first
        let task = TaskFactory::new()
            .with_namespace("test")
            .create(&pool)
            .await?;
        let task_uuid = task.task_uuid;

        let step_def = create_test_step_with_init("test_step_with_init", 5000);

        let mut tx = context.database_pool().begin().await?;
        let (workflow_step, _named_step) = creator
            .create_single_step(&mut tx, task_uuid, &step_def)
            .await?;
        tx.commit().await?;

        assert!(workflow_step.inputs.is_some());
        let inputs = workflow_step.inputs.unwrap();
        assert_eq!(inputs["timeout_ms"], 5000);

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_steps_batch(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        // Create a valid task first
        let task = TaskFactory::new()
            .with_namespace("test")
            .create(&pool)
            .await?;
        let task_uuid = task.task_uuid;

        let step_defs = vec![
            create_test_step_definition("step_1"),
            create_test_step_definition("step_2"),
            create_test_step_definition("step_3"),
        ];

        let mut tx = context.database_pool().begin().await?;
        let step_mapping = creator
            .create_steps_batch(&mut tx, task_uuid, &step_defs)
            .await?;
        tx.commit().await?;

        assert_eq!(step_mapping.len(), 3);
        assert!(step_mapping.contains_key("step_1"));
        assert!(step_mapping.contains_key("step_2"));
        assert!(step_mapping.contains_key("step_3"));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_find_or_create_named_step_creates_new(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        let step_def = create_test_step_definition("unique_step_name");

        let mut tx = context.database_pool().begin().await?;
        let named_step = creator
            .find_or_create_named_step(&mut tx, &step_def)
            .await?;
        tx.commit().await?;

        assert_eq!(named_step.name, "unique_step_name");

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_find_or_create_named_step_finds_existing(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        let step_def = create_test_step_definition("reused_step_name");

        // Create the step the first time
        let mut tx = context.database_pool().begin().await?;
        let first_named_step = creator
            .find_or_create_named_step(&mut tx, &step_def)
            .await?;
        tx.commit().await?;

        // Create it again - should find existing
        let mut tx = context.database_pool().begin().await?;
        let second_named_step = creator
            .find_or_create_named_step(&mut tx, &step_def)
            .await?;
        tx.commit().await?;

        assert_eq!(
            first_named_step.named_step_uuid,
            second_named_step.named_step_uuid
        );
        assert_eq!(second_named_step.name, "reused_step_name");

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_create_decision_point_step(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let creator = WorkflowStepCreator::new(context.clone());

        // Create a valid task first
        let task = TaskFactory::new()
            .with_namespace("test")
            .create(&pool)
            .await?;
        let task_uuid = task.task_uuid;

        let mut step_def = create_test_step_definition("decision_step");
        step_def.step_type = StepType::Decision;

        let mut tx = context.database_pool().begin().await?;
        let (workflow_step, named_step) = creator
            .create_single_step(&mut tx, task_uuid, &step_def)
            .await?;
        tx.commit().await?;

        assert_eq!(workflow_step.task_uuid, task_uuid);
        assert_eq!(named_step.name, "decision_step");

        Ok(())
    }
}
