//! # Core Model Factories
//!
//! Factories for creating the main domain objects (Task, WorkflowStep, etc.)
//! with proper state management and relationship handling.

#![allow(dead_code)]

use super::base::*;
use super::foundation::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::PgPool;
use tasker_core::models::core::{
    task::NewTask, task_transition::NewTaskTransition, workflow_step::NewWorkflowStep,
    workflow_step_transition::NewWorkflowStepTransition,
};
use tasker_core::models::{Task, TaskTransition, WorkflowStep, WorkflowStepTransition};

/// Factory for creating Task instances
#[derive(Debug, Clone)]
pub struct TaskFactory {
    base: BaseFactory,
    named_task_name: Option<String>,
    named_task_namespace: String,
    named_task_version: String,
    context: Option<Value>,
    tags: Option<Value>,
    initiator: Option<String>,
    source_system: Option<String>,
    reason: Option<String>,
    complete: bool,
    with_transitions: bool,
    initial_state: Option<String>,
}

impl Default for TaskFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            named_task_name: None, // Will use dummy_task by default
            named_task_namespace: "default".to_string(),
            named_task_version: "0.1.0".to_string(),
            context: Some(utils::generate_test_context()),
            tags: Some(json!({"test": true, "auto_generated": true})),
            initiator: Some("test_suite".to_string()),
            source_system: Some("factory".to_string()),
            reason: Some("Testing workflow orchestration".to_string()),
            complete: false,
            with_transitions: false,
            initial_state: Some("pending".to_string()),
        }
    }
}

impl TaskFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_named_task(mut self, name: &str) -> Self {
        self.named_task_name = Some(name.to_string());
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.named_task_namespace = namespace.to_string();
        self
    }

    pub fn with_context(mut self, context: Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_tags(mut self, tags: Value) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_initiator(mut self, initiator: &str) -> Self {
        self.initiator = Some(initiator.to_string());
        self
    }

    pub fn completed(mut self) -> Self {
        self.complete = true;
        self.initial_state = Some("complete".to_string());
        self
    }

    pub fn with_state_transitions(mut self) -> Self {
        self.with_transitions = true;
        self
    }

    /// Create a complex workflow task with rich context
    pub fn complex_workflow(mut self) -> Self {
        self.context = Some(json!({
            "workflow_type": "complex",
            "order_id": 12345,
            "user_id": 67890,
            "priority": "high",
            "shipping_address": {
                "street": "123 Main St",
                "city": "Anytown",
                "state": "CA",
                "zip": "12345"
            },
            "items": [
                {"id": 1, "name": "Widget A", "quantity": 2, "price": 19.99},
                {"id": 2, "name": "Widget B", "quantity": 1, "price": 29.99}
            ],
            "payment_method": {
                "type": "credit_card",
                "last_four": "1234"
            }
        }));
        self.tags = Some(json!({
            "complexity": "high",
            "category": "e_commerce",
            "requires_payment": true,
            "requires_shipping": true
        }));
        self
    }

    /// Create an API integration task
    pub fn api_integration(mut self) -> Self {
        self.named_task_name = Some("api_integration_task".to_string());
        self.context = Some(json!({
            "api_endpoint": "https://api.partner.com/v1/orders",
            "method": "POST",
            "timeout": 30,
            "retry_policy": {
                "max_attempts": 3,
                "backoff": "exponential"
            }
        }));
        self.tags = Some(json!({
            "integration": "partner_api",
            "external": true
        }));
        self
    }
}

#[async_trait]
impl SqlxFactory<Task> for TaskFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<Task> {
        // Ensure named task exists
        let named_task_name = self.named_task_name.as_deref().unwrap_or("dummy_task");
        let named_task = NamedTaskFactory::new()
            .with_name(named_task_name)
            .with_namespace(&self.named_task_namespace)
            .with_version(&self.named_task_version)
            .find_or_create(pool)
            .await?;

        // Prepare and validate context
        let context = self.context.clone().unwrap_or_else(|| json!({}));
        utils::validate_jsonb(&context)?;

        let tags = self.tags.clone().unwrap_or_else(|| json!({}));
        utils::validate_jsonb(&tags)?;

        // Generate identity hash
        let identity_hash = utils::generate_test_identity_hash();

        let new_task = NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: None, // Will default to NOW()
            initiator: self.initiator.clone(),
            source_system: self.source_system.clone(),
            reason: self.reason.clone(),
            bypass_steps: None,
            tags: Some(tags),
            context: Some(context),
            identity_hash,
        };

        let task = Task::create(pool, new_task).await?;

        // Apply state transitions if requested
        if self.with_transitions {
            self.apply_state_transitions(&task, pool).await?;
        }

        Ok(task)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<Task> {
        // For tasks, we typically create new instances rather than find existing ones
        // since tasks represent unique execution instances
        self.create(pool).await
    }
}

// Implement StateFactory for TaskFactory
#[async_trait]
impl StateFactory<Task> for TaskFactory {
    async fn apply_state_transitions(&self, task: &Task, pool: &PgPool) -> FactoryResult<()> {
        if let Some(state) = &self.initial_state {
            let new_transition = NewTaskTransition {
                task_id: task.task_id,
                to_state: state.clone(),
                from_state: None,
                metadata: Some(json!({
                    "factory_created": true,
                    "initial_state": true
                })),
            };

            TaskTransition::create(pool, new_transition)
                .await
                .map_err(FactoryError::Database)?;
        }
        Ok(())
    }

    fn with_initial_state(mut self, state: &str) -> Self {
        self.initial_state = Some(state.to_string());
        self.with_transitions = true;
        self
    }

    fn with_state_sequence(mut self, states: Vec<String>) -> Self {
        if let Some(last_state) = states.last() {
            self.initial_state = Some(last_state.clone());
            self.with_transitions = true;
        }
        self
    }
}

// Convenience methods for TaskFactory states
impl TaskFactory {
    pub fn pending(self) -> Self {
        self.with_initial_state("pending")
    }

    pub fn in_progress(self) -> Self {
        self.with_initial_state("in_progress")
    }

    pub fn complete(self) -> Self {
        self.with_initial_state("complete")
    }

    pub fn error(self) -> Self {
        self.with_initial_state("error")
    }

    pub fn cancelled(self) -> Self {
        self.with_initial_state("cancelled")
    }

    pub fn resolved_manually(self) -> Self {
        self.with_initial_state("resolved_manually")
    }
}

/// Factory for creating WorkflowStep instances
#[derive(Debug, Clone)]
pub struct WorkflowStepFactory {
    base: BaseFactory,
    task_id: Option<i64>,
    named_step_name: Option<String>,
    inputs: Option<Value>,
    results: Option<Value>,
    retryable: bool,
    retry_limit: Option<i32>,
    in_process: bool,
    processed: bool,
    skippable: bool,
    with_transitions: bool,
    initial_state: Option<String>,
}

impl Default for WorkflowStepFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            task_id: None,
            named_step_name: None, // Will use dummy_step by default
            inputs: Some(json!({"test_input": true})),
            results: None,
            retryable: true,
            retry_limit: Some(3),
            in_process: false,
            processed: false,
            skippable: false,
            with_transitions: false,
            initial_state: Some("pending".to_string()),
        }
    }
}

impl WorkflowStepFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_task(mut self, task_id: i64) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_named_step(mut self, name: &str) -> Self {
        self.named_step_name = Some(name.to_string());
        self
    }

    pub fn with_inputs(mut self, inputs: Value) -> Self {
        self.inputs = Some(inputs);
        self
    }

    pub fn with_results(mut self, results: Value) -> Self {
        self.results = Some(results);
        self.processed = true;
        self
    }

    pub fn in_process(mut self) -> Self {
        self.in_process = true;
        self.initial_state = Some("in_progress".to_string());
        self.with_transitions = true;
        self
    }

    pub fn api_call_step(mut self) -> Self {
        self.named_step_name = Some("api_call_step".to_string());
        self.inputs = Some(json!({
            "url": "https://api.example.com/process",
            "method": "POST",
            "headers": {"Content-Type": "application/json"},
            "body": {"data": "test_payload"}
        }));
        self
    }

    pub fn database_step(mut self) -> Self {
        self.named_step_name = Some("db_operation_step".to_string());
        self.inputs = Some(json!({
            "query": "UPDATE test_table SET status = $1 WHERE id = $2",
            "params": ["active", 123]
        }));
        self
    }
}

#[async_trait]
impl SqlxFactory<WorkflowStep> for WorkflowStepFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<WorkflowStep> {
        // Ensure task exists
        let task_id = if let Some(id) = self.task_id {
            id
        } else {
            let task = TaskFactory::new().create(pool).await?;
            task.task_id
        };

        // Ensure named step exists
        let named_step_name = self.named_step_name.as_deref().unwrap_or("dummy_step");
        let named_step = NamedStepFactory::new()
            .with_name(named_step_name)
            .find_or_create(pool)
            .await?;

        // Prepare inputs
        let inputs = self.inputs.clone();
        if let Some(ref inp) = inputs {
            utils::validate_jsonb(inp)?;
        }

        let new_step = NewWorkflowStep {
            task_id,
            named_step_id: named_step.named_step_id,
            retryable: Some(self.retryable),
            retry_limit: self.retry_limit,
            inputs,
            skippable: Some(self.skippable),
        };

        let mut step = WorkflowStep::create(pool, new_step).await?;

        // Apply state changes if needed
        if self.in_process {
            step.mark_in_process(pool).await?;
        }

        if self.processed {
            if let Some(ref results) = self.results {
                utils::validate_jsonb(results)?;
            }
            step.mark_processed(pool, self.results.clone()).await?;
        }

        // Apply state transitions if requested
        if self.with_transitions {
            self.apply_state_transitions(&step, pool).await?;
        }

        Ok(step)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<WorkflowStep> {
        // Workflow steps are typically unique instances, so create new
        self.create(pool).await
    }
}

// Implement StateFactory for WorkflowStepFactory
#[async_trait]
impl StateFactory<WorkflowStep> for WorkflowStepFactory {
    async fn apply_state_transitions(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
    ) -> FactoryResult<()> {
        if let Some(state) = &self.initial_state {
            let new_transition = NewWorkflowStepTransition {
                workflow_step_id: step.workflow_step_id,
                to_state: state.clone(),
                from_state: None,
                metadata: Some(json!({
                    "factory_created": true,
                    "initial_state": true
                })),
            };

            WorkflowStepTransition::create(pool, new_transition)
                .await
                .map_err(FactoryError::Database)?;
        }
        Ok(())
    }

    fn with_initial_state(mut self, state: &str) -> Self {
        self.initial_state = Some(state.to_string());
        self.with_transitions = true;
        // Update internal flags based on state
        match state {
            "in_progress" => {
                self.in_process = true;
            }
            "complete" => {
                self.processed = true;
                if self.results.is_none() {
                    self.results = Some(json!({"success": true}));
                }
            }
            _ => {}
        }
        self
    }

    fn with_state_sequence(mut self, states: Vec<String>) -> Self {
        if let Some(last_state) = states.last() {
            self = self.with_initial_state(last_state);
        }
        self
    }
}

// Convenience methods for WorkflowStepFactory states
impl WorkflowStepFactory {
    pub fn pending(self) -> Self {
        self.with_initial_state("pending")
    }

    pub fn in_progress(self) -> Self {
        self.with_initial_state("in_progress")
    }

    pub fn completed(self) -> Self {
        self.with_initial_state("complete")
    }

    pub fn error(self) -> Self {
        self.with_initial_state("error")
    }

    pub fn cancelled(self) -> Self {
        self.with_initial_state("cancelled")
    }

    pub fn resolved_manually(self) -> Self {
        self.with_initial_state("resolved_manually")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test]
    async fn test_task_factory_basic(pool: PgPool) -> FactoryResult<()> {
        let task = TaskFactory::new()
            .with_initiator("test_user")
            .create(&pool)
            .await?;

        assert_eq!(task.initiator, Some("test_user".to_string()));
        assert!(!task.complete);
        assert!(task.context.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn test_task_factory_with_transitions(pool: PgPool) -> FactoryResult<()> {
        let task = TaskFactory::new().in_progress().create(&pool).await?;

        // Check that task was created successfully with transitions
        let transitions = TaskTransition::list_by_task(&pool, task.task_id).await?;
        // Should have one transition to in_progress state
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].to_state, "in_progress");

        Ok(())
    }

    #[sqlx::test]
    async fn test_workflow_step_factory(pool: PgPool) -> FactoryResult<()> {
        let task = TaskFactory::new().create(&pool).await?;

        let step = WorkflowStepFactory::new()
            .for_task(task.task_id)
            .api_call_step()
            .completed()
            .create(&pool)
            .await?;

        assert_eq!(step.task_id, task.task_id);
        assert!(step.processed);
        assert!(step.results.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn test_complex_workflow_creation(pool: PgPool) -> FactoryResult<()> {
        let task = TaskFactory::new()
            .complex_workflow()
            .with_state_transitions()
            .create(&pool)
            .await?;

        let context = task.context.unwrap();
        assert_eq!(context["order_id"], 12345);
        assert_eq!(context["user_id"], 67890);
        assert!(context["items"].is_array());

        Ok(())
    }

    #[sqlx::test]
    async fn test_workflow_step_factory_error_cases(pool: PgPool) -> FactoryResult<()> {
        // Test creating a step with invalid JSONB
        let result = WorkflowStepFactory::new()
            .with_inputs(serde_json::Value::Null) // This should be valid
            .create(&pool)
            .await;
        assert!(result.is_ok()); // Null is valid JSON

        // Test creating a step with invalid context - we can't easily test invalid JSON
        // since serde_json::Value enforces valid JSON structure, but we can test edge cases

        // Test with extremely large input data
        let large_input = json!({
            "data": "x".repeat(10000), // 10KB of data
            "nested": {
                "level1": {
                    "level2": {
                        "level3": "deep nesting test"
                    }
                }
            }
        });

        let result = WorkflowStepFactory::new()
            .with_inputs(large_input)
            .create(&pool)
            .await;
        assert!(result.is_ok()); // Should handle large valid JSON

        Ok(())
    }

    #[sqlx::test]
    async fn test_task_factory_edge_cases(pool: PgPool) -> FactoryResult<()> {
        // Test task with max length initiator name (varchar(128) limit)
        let max_length_name = "x".repeat(128);
        let task = TaskFactory::new()
            .with_initiator(&max_length_name)
            .create(&pool)
            .await?;

        assert_eq!(task.initiator.as_deref(), Some(max_length_name.as_str()));

        // Test task with name that would exceed limit (should fail)
        let too_long_name = "x".repeat(200);
        let result = TaskFactory::new()
            .with_initiator(&too_long_name)
            .create(&pool)
            .await;

        assert!(result.is_err()); // Should fail due to database constraint

        // Test task with empty context vs None context
        let task_empty_context = TaskFactory::new()
            .with_context(json!({}))
            .create(&pool)
            .await?;

        assert!(task_empty_context.context.is_some());

        // Test task with complex nested context
        let complex_context = json!({
            "workflow": {
                "steps": [
                    {"name": "step1", "config": {"timeout": 30}},
                    {"name": "step2", "config": {"retries": 3}}
                ],
                "metadata": {
                    "version": "1.0",
                    "environment": "test"
                }
            },
            "user_data": {
                "preferences": {},
                "history": []
            }
        });

        let task_complex = TaskFactory::new()
            .with_context(complex_context.clone())
            .create(&pool)
            .await?;

        assert_eq!(task_complex.context, Some(complex_context));

        Ok(())
    }

    #[sqlx::test]
    async fn test_state_transition_error_scenarios(pool: PgPool) -> FactoryResult<()> {
        // Test multiple rapid state transitions
        let task = TaskFactory::new().pending().create(&pool).await?;

        // Verify the transition was created
        let transitions = TaskTransition::list_by_task(&pool, task.task_id).await?;
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].to_state, "pending");

        // Test workflow step state transitions
        let step = WorkflowStepFactory::new()
            .for_task(task.task_id)
            .in_progress()
            .create(&pool)
            .await?;

        // Verify step state and transitions
        assert!(step.in_process);
        let step_transitions =
            WorkflowStepTransition::list_by_workflow_step(&pool, step.workflow_step_id).await?;
        assert_eq!(step_transitions.len(), 1);
        assert_eq!(step_transitions[0].to_state, "in_progress");

        Ok(())
    }

    #[sqlx::test]
    async fn test_factory_validation_edge_cases(pool: PgPool) -> FactoryResult<()> {
        // Test find_or_create behavior
        let task1 = TaskFactory::new()
            .with_initiator("test_user")
            .find_or_create(&pool)
            .await?;

        let task2 = TaskFactory::new()
            .with_initiator("test_user")
            .find_or_create(&pool)
            .await?;

        // Should create separate tasks (current implementation)
        assert_ne!(task1.task_id, task2.task_id);

        // Test step factory with missing task (should create one)
        let step_auto_task = WorkflowStepFactory::new()
            .api_call_step()
            .create(&pool)
            .await?;

        assert!(step_auto_task.task_id > 0);

        // Test step factory with explicit task
        let explicit_task = TaskFactory::new().create(&pool).await?;
        let step_explicit_task = WorkflowStepFactory::new()
            .for_task(explicit_task.task_id)
            .database_step()
            .create(&pool)
            .await?;

        assert_eq!(step_explicit_task.task_id, explicit_task.task_id);

        Ok(())
    }
}
