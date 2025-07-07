use crate::factories::{
    Factory, FactoryContext, FactoryResult,
    task::{TaskFactory, TaskFactoryConfig},
    workflow_step::{WorkflowStepFactory, WorkflowStepFactoryConfig},
    workflow_step_edge::{WorkflowStepEdgeFactory, WorkflowStepEdgeFactoryConfig},
    named_task::{NamedTaskFactory, NamedTaskFactoryConfig},
    named_step::{NamedStepFactory, NamedStepFactoryConfig},
    dependent_system::{DependentSystemFactory, DependentSystemFactoryConfig},
    task_transition::{TaskTransitionFactory, TaskTransitionFactoryConfig},
    workflow_step_transition::{WorkflowStepTransitionFactory, WorkflowStepTransitionFactoryConfig},
};
use serde_json::json;
use std::collections::HashMap;

/// Configuration for dummy task workflow factory
#[derive(Debug, Clone)]
pub struct DummyTaskWorkflowConfig {
    /// Base context data for the task
    pub context: HashMap<String, serde_json::Value>,
    /// Whether to create with dependencies
    pub with_dependencies: bool,
    /// Initial state for steps (None means no state transitions)
    pub step_states: Option<String>,
    /// Initiator for the task
    pub initiator: String,
    /// Source system for the task
    pub source_system: String,
    /// Reason for the task
    pub reason: String,
    /// Tags to apply to the task
    pub tags: Vec<String>,
    /// Use dummy_task_two variant
    pub use_variant_two: bool,
}

impl Default for DummyTaskWorkflowConfig {
    fn default() -> Self {
        let mut context = HashMap::new();
        context.insert("dummy".to_string(), json!(true));
        context.insert("test".to_string(), json!(true));
        
        Self {
            context,
            with_dependencies: true,
            step_states: Some("pending".to_string()),
            initiator: "pete@test".to_string(),
            source_system: "test-system".to_string(),
            reason: "testing!".to_string(),
            tags: vec!["dummy".to_string(), "testing".to_string()],
            use_variant_two: false,
        }
    }
}

/// Factory for creating dummy task workflows for testing
pub struct DummyTaskWorkflowFactory;

impl DummyTaskWorkflowFactory {
    /// Create a dummy task workflow
    pub async fn create(
        ctx: &mut FactoryContext,
        config: DummyTaskWorkflowConfig,
    ) -> FactoryResult<i64> {
        // Create the named task (dummy_task or dummy_task_two)
        let task_name = if config.use_variant_two {
            "dummy_task_two"
        } else {
            "dummy_task"
        };
        
        let description = if config.use_variant_two {
            "Second dummy task variant for testing"
        } else {
            "Dummy task for testing workflow step logic"
        };

        let named_task_id = NamedTaskFactory::create(
            ctx,
            NamedTaskFactoryConfig {
                name: task_name.to_string(),
                namespace_name: Some("default".to_string()),
                version: Some("0.1.0".to_string()),
                description: Some(description.to_string()),
                ..Default::default()
            },
        ).await?;

        // Create the task
        let task_id = TaskFactory::create(
            ctx,
            TaskFactoryConfig {
                named_task_id: Some(named_task_id),
                initiator: config.initiator,
                source_system: config.source_system,
                reason: config.reason,
                context: config.context,
                tags: config.tags,
                ..Default::default()
            },
        ).await?;

        // Create initial task transition to pending state
        Self::create_initial_task_transition(ctx, task_id).await?;

        // Create workflow steps if dependencies are requested
        if config.with_dependencies {
            Self::create_dependencies(ctx, task_id, config.step_states).await?;
        }

        Ok(task_id)
    }

    /// Create initial task transition to pending state
    async fn create_initial_task_transition(
        ctx: &mut FactoryContext,
        task_id: i64,
    ) -> FactoryResult<()> {
        TaskTransitionFactory::create(
            ctx,
            TaskTransitionFactoryConfig {
                task_id,
                to_state: "pending".to_string(),
                sort_key: 0,
                most_recent: true,
                metadata: Some(json!({
                    "created_by": "dummy_task_factory"
                })),
                ..Default::default()
            },
        ).await?;
        
        Ok(())
    }

    /// Create the workflow dependencies (systems, steps, and edges)
    async fn create_dependencies(
        ctx: &mut FactoryContext,
        task_id: i64,
        step_states: Option<String>,
    ) -> FactoryResult<()> {
        // Create dummy system
        let dummy_system_id = Self::get_or_create_system(
            ctx,
            "dummy-system",
            "Dummy system for testing workflow step logic",
        ).await?;

        // Create named steps matching DummyTask structure
        let step_one_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "step-one".to_string(),
                dependent_system_id: Some(dummy_system_id),
                description: Some("Independent Step One".to_string()),
                ..Default::default()
            },
        ).await?;

        let step_two_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "step-two".to_string(),
                dependent_system_id: Some(dummy_system_id),
                description: Some("Independent Step Two".to_string()),
                ..Default::default()
            },
        ).await?;

        let step_three_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "step-three".to_string(),
                dependent_system_id: Some(dummy_system_id),
                description: Some("Step Three Dependent on Step Two".to_string()),
                ..Default::default()
            },
        ).await?;

        let step_four_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "step-four".to_string(),
                dependent_system_id: Some(dummy_system_id),
                description: Some("Step Four Dependent on Step Three".to_string()),
                ..Default::default()
            },
        ).await?;

        // Create workflow steps with dummy inputs
        let mut dummy_inputs = HashMap::new();
        dummy_inputs.insert("dummy".to_string(), json!(true));

        let ws1_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: step_one_id,
                inputs: dummy_inputs.clone(),
                ..Default::default()
            },
        ).await?;

        let ws2_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: step_two_id,
                inputs: dummy_inputs.clone(),
                ..Default::default()
            },
        ).await?;

        let ws3_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: step_three_id,
                inputs: dummy_inputs.clone(),
                ..Default::default()
            },
        ).await?;

        let ws4_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: step_four_id,
                inputs: dummy_inputs.clone(),
                ..Default::default()
            },
        ).await?;

        // Create step dependencies to match DummyTask structure
        // step_three depends on step_two, step_four depends on step_three
        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws2_id,
                to_step_id: ws3_id,
                name: "provides".to_string(),
            },
        ).await?;

        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws3_id,
                to_step_id: ws4_id,
                name: "provides".to_string(),
            },
        ).await?;

        // Create initial step transitions if step_states is provided
        if let Some(state) = step_states {
            if state != "none" {
                Self::create_initial_step_transitions(
                    ctx, 
                    vec![ws1_id, ws2_id, ws3_id, ws4_id]
                ).await?;
            }
        }

        Ok(())
    }

    /// Create initial step transitions for all workflow steps
    async fn create_initial_step_transitions(
        ctx: &mut FactoryContext,
        step_ids: Vec<i64>,
    ) -> FactoryResult<()> {
        for step_id in step_ids {
            WorkflowStepTransitionFactory::create(
                ctx,
                WorkflowStepTransitionFactoryConfig {
                    workflow_step_id: step_id,
                    to_state: "pending".to_string(),
                    sort_key: 0,
                    most_recent: true,
                    metadata: Some(json!({
                        "created_by": "dummy_task_factory"
                    })),
                    ..Default::default()
                },
            ).await?;
        }
        
        Ok(())
    }

    /// Get or create dummy system
    async fn get_or_create_system(
        ctx: &mut FactoryContext,
        name: &str,
        description: &str,
    ) -> FactoryResult<i64> {
        // Check if system already exists
        let existing = sqlx::query!(
            "SELECT dependent_system_id FROM tasker_dependent_systems WHERE name = $1",
            name
        )
        .fetch_optional(&ctx.pool)
        .await?;
        
        if let Some(row) = existing {
            return Ok(row.dependent_system_id);
        }
        
        // Create new system
        DependentSystemFactory::create(
            ctx,
            DependentSystemFactoryConfig {
                name: name.to_string(),
                description: Some(description.to_string()),
            },
        ).await
    }
}

/// Factory for creating dummy task workflows in specific states for orchestration testing
pub struct DummyTaskOrchestrationFactory;

impl DummyTaskOrchestrationFactory {
    /// Create a dummy task workflow specifically for orchestration testing
    pub async fn create_for_orchestration(
        ctx: &mut FactoryContext,
    ) -> FactoryResult<i64> {
        let config = DummyTaskWorkflowConfig {
            step_states: None, // Don't apply any state trait initially
            ..Default::default()
        };
        
        let task_id = DummyTaskWorkflowFactory::create(ctx, config).await?;
        
        // Ensure task starts in pending state for orchestration
        Self::ensure_pending_state(ctx, task_id).await?;
        
        Ok(task_id)
    }

    /// Create a dummy task with partial completion
    pub async fn create_with_partial_completion(
        ctx: &mut FactoryContext,
    ) -> FactoryResult<i64> {
        let task_id = Self::create_for_orchestration(ctx).await?;
        
        // Complete first two steps (step-one and step-two)
        Self::complete_steps_by_name(ctx, task_id, vec!["step-one", "step-two"]).await?;
        
        Ok(task_id)
    }

    /// Create a dummy task with a step in progress
    pub async fn create_with_step_in_progress(
        ctx: &mut FactoryContext,
    ) -> FactoryResult<i64> {
        let task_id = Self::create_for_orchestration(ctx).await?;
        
        // Set step three to in_progress
        Self::set_step_in_progress(ctx, task_id, "step-three").await?;
        
        Ok(task_id)
    }

    /// Create a dummy task with a step in error state
    pub async fn create_with_step_error(
        ctx: &mut FactoryContext,
    ) -> FactoryResult<i64> {
        let task_id = Self::create_for_orchestration(ctx).await?;
        
        // Set step one to error state with max retries
        Self::set_step_error(ctx, task_id, "step-one").await?;
        
        Ok(task_id)
    }

    /// Ensure task is in pending state
    async fn ensure_pending_state(
        ctx: &mut FactoryContext,
        task_id: i64,
    ) -> FactoryResult<()> {
        // Update task columns to pending state
        sqlx::query!(
            "UPDATE tasker_tasks SET status = 'pending' WHERE task_id = $1",
            task_id
        )
        .execute(&ctx.pool)
        .await?;

        // Ensure all steps are in pending state with proper initial transitions
        let step_ids: Vec<i64> = sqlx::query!(
            "SELECT workflow_step_id FROM tasker_workflow_steps WHERE task_id = $1",
            task_id
        )
        .fetch_all(&ctx.pool)
        .await?
        .into_iter()
        .map(|row| row.workflow_step_id)
        .collect();

        for step_id in step_ids {
            // Clear any existing transitions
            sqlx::query!(
                "DELETE FROM tasker_workflow_step_transitions WHERE workflow_step = $1",
                step_id
            )
            .execute(&ctx.pool)
            .await?;

            // Create initial transition to pending state
            WorkflowStepTransitionFactory::create(
                ctx,
                WorkflowStepTransitionFactoryConfig {
                    workflow_step_id: step_id,
                    to_state: "pending".to_string(),
                    sort_key: 0,
                    most_recent: true,
                    metadata: Some(json!({
                        "created_by": "factory_for_orchestration"
                    })),
                    ..Default::default()
                },
            ).await?;

            // Set step attributes to pending state
            sqlx::query!(
                "UPDATE tasker_workflow_steps 
                 SET processed = false, in_process = false, processed_at = NULL, attempts = 0
                 WHERE workflow_step_id = $1",
                step_id
            )
            .execute(&ctx.pool)
            .await?;
        }

        Ok(())
    }

    /// Complete specific steps by name
    async fn complete_steps_by_name(
        ctx: &mut FactoryContext,
        task_id: i64,
        step_names: Vec<&str>,
    ) -> FactoryResult<()> {
        for step_name in step_names {
            let step_id = sqlx::query!(
                "SELECT ws.workflow_step_id 
                 FROM tasker_workflow_steps ws
                 JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
                 WHERE ws.task_id = $1 AND ns.name = $2",
                task_id,
                step_name
            )
            .fetch_optional(&ctx.pool)
            .await?;

            if let Some(row) = step_id {
                // Create state machine transitions: pending -> in_progress -> complete
                Self::transition_step_to_in_progress(ctx, row.workflow_step_id).await?;
                Self::transition_step_to_complete(ctx, row.workflow_step_id).await?;
            }
        }

        Ok(())
    }

    /// Set a step to in_progress state
    async fn set_step_in_progress(
        ctx: &mut FactoryContext,
        task_id: i64,
        step_name: &str,
    ) -> FactoryResult<()> {
        let step_id = sqlx::query!(
            "SELECT ws.workflow_step_id 
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             WHERE ws.task_id = $1 AND ns.name = $2",
            task_id,
            step_name
        )
        .fetch_optional(&ctx.pool)
        .await?;

        if let Some(row) = step_id {
            Self::transition_step_to_in_progress(ctx, row.workflow_step_id).await?;
        }

        Ok(())
    }

    /// Set a step to error state
    async fn set_step_error(
        ctx: &mut FactoryContext,
        task_id: i64,
        step_name: &str,
    ) -> FactoryResult<()> {
        let step_id = sqlx::query!(
            "SELECT ws.workflow_step_id 
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             WHERE ws.task_id = $1 AND ns.name = $2",
            task_id,
            step_name
        )
        .fetch_optional(&ctx.pool)
        .await?;

        if let Some(row) = step_id {
            // Transition: pending -> in_progress -> error
            Self::transition_step_to_in_progress(ctx, row.workflow_step_id).await?;
            Self::transition_step_to_error(ctx, row.workflow_step_id).await?;
        }

        Ok(())
    }

    /// Transition step to in_progress state
    async fn transition_step_to_in_progress(
        ctx: &mut FactoryContext,
        step_id: i64,
    ) -> FactoryResult<()> {
        // Mark previous transition as not most recent
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions 
             SET most_recent = false 
             WHERE workflow_step = $1 AND most_recent = true",
            step_id
        )
        .execute(&ctx.pool)
        .await?;

        // Create new transition to in_progress
        WorkflowStepTransitionFactory::create(
            ctx,
            WorkflowStepTransitionFactoryConfig {
                workflow_step_id: step_id,
                to_state: "in_progress".to_string(),
                sort_key: 1,
                most_recent: true,
                metadata: Some(json!({
                    "transitioned_by": "orchestration_factory"
                })),
                ..Default::default()
            },
        ).await?;

        // Update step attributes
        sqlx::query!(
            "UPDATE tasker_workflow_steps 
             SET in_process = true
             WHERE workflow_step_id = $1",
            step_id
        )
        .execute(&ctx.pool)
        .await?;

        Ok(())
    }

    /// Transition step to complete state
    async fn transition_step_to_complete(
        ctx: &mut FactoryContext,
        step_id: i64,
    ) -> FactoryResult<()> {
        // Mark previous transition as not most recent
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions 
             SET most_recent = false 
             WHERE workflow_step = $1 AND most_recent = true",
            step_id
        )
        .execute(&ctx.pool)
        .await?;

        // Create new transition to complete
        WorkflowStepTransitionFactory::create(
            ctx,
            WorkflowStepTransitionFactoryConfig {
                workflow_step_id: step_id,
                to_state: "complete".to_string(),
                sort_key: 2,
                most_recent: true,
                metadata: Some(json!({
                    "transitioned_by": "orchestration_factory"
                })),
                ..Default::default()
            },
        ).await?;

        // Update step attributes
        sqlx::query!(
            "UPDATE tasker_workflow_steps 
             SET processed = true, 
                 in_process = false,
                 processed_at = NOW(),
                 results = $2
             WHERE workflow_step_id = $1",
            step_id,
            json!({"dummy": true, "completed": true})
        )
        .execute(&ctx.pool)
        .await?;

        Ok(())
    }

    /// Transition step to error state
    async fn transition_step_to_error(
        ctx: &mut FactoryContext,
        step_id: i64,
    ) -> FactoryResult<()> {
        // Mark previous transition as not most recent
        sqlx::query!(
            "UPDATE tasker_workflow_step_transitions 
             SET most_recent = false 
             WHERE workflow_step = $1 AND most_recent = true",
            step_id
        )
        .execute(&ctx.pool)
        .await?;

        // Create new transition to error
        WorkflowStepTransitionFactory::create(
            ctx,
            WorkflowStepTransitionFactoryConfig {
                workflow_step_id: step_id,
                to_state: "error".to_string(),
                sort_key: 2,
                most_recent: true,
                metadata: Some(json!({
                    "transitioned_by": "orchestration_factory",
                    "error": "Test error"
                })),
                ..Default::default()
            },
        ).await?;

        // Update step attributes with max retries
        sqlx::query!(
            "UPDATE tasker_workflow_steps 
             SET processed = false, 
                 in_process = false,
                 attempts = 4,  -- Max retries + 1
                 results = $2
             WHERE workflow_step_id = $1",
            step_id,
            json!({"error": "Test error"})
        )
        .execute(&ctx.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factories::test_helpers::TestHelper;

    #[sqlx::test]
    async fn test_create_dummy_task_workflow(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let config = DummyTaskWorkflowConfig::default();
        let task_id = DummyTaskWorkflowFactory::create(&mut ctx, config).await?;
        
        // Verify task was created
        let task_exists = sqlx::query!(
            "SELECT 1 FROM tasker_tasks WHERE task_id = $1",
            task_id
        )
        .fetch_optional(&ctx.pool)
        .await?
        .is_some();
        
        assert!(task_exists);
        
        // Verify workflow steps were created (4 steps for dummy task)
        let step_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_workflow_steps WHERE task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(step_count, 4);
        
        // Verify edges were created (2 edges: step2->step3, step3->step4)
        let edge_count = sqlx::query!(
            "SELECT COUNT(*) as count 
             FROM tasker_workflow_step_edges e
             JOIN tasker_workflow_steps s1 ON e.from_step = s1.workflow_step_id
             JOIN tasker_workflow_steps s2 ON e.to_step = s2.workflow_step_id
             WHERE s1.task_id = $1 AND s2.task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(edge_count, 2);
        
        // Verify task transition was created
        let transition_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_task_transitions WHERE task = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(transition_count, 1);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_dummy_task_variant_two(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let config = DummyTaskWorkflowConfig {
            use_variant_two: true,
            ..Default::default()
        };
        let task_id = DummyTaskWorkflowFactory::create(&mut ctx, config).await?;
        
        // Verify task was created with variant two named task
        let named_task_name = sqlx::query!(
            "SELECT nt.name 
             FROM tasker_tasks t
             JOIN tasker_named_tasks nt ON t.named_task = nt.named_task_id
             WHERE t.task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .name;
        
        assert_eq!(named_task_name, "dummy_task_two");
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_for_orchestration(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let task_id = DummyTaskOrchestrationFactory::create_for_orchestration(&mut ctx).await?;
        
        // Verify task is in pending state
        let task_status = sqlx::query!(
            "SELECT status FROM tasker_tasks WHERE task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .status;
        
        assert_eq!(task_status, "pending");
        
        // Verify all steps have initial pending transitions
        let pending_transitions = sqlx::query!(
            "SELECT COUNT(*) as count
             FROM tasker_workflow_step_transitions wst
             JOIN tasker_workflow_steps ws ON wst.workflow_step = ws.workflow_step_id
             WHERE ws.task_id = $1 AND wst.to_state = 'pending' AND wst.most_recent = true",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(pending_transitions, 4);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_with_partial_completion(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let task_id = DummyTaskOrchestrationFactory::create_with_partial_completion(&mut ctx).await?;
        
        // Verify that steps step-one and step-two are completed
        let completed_steps = sqlx::query!(
            "SELECT COUNT(*) as count
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             JOIN tasker_workflow_step_transitions wst ON ws.workflow_step_id = wst.workflow_step
             WHERE ws.task_id = $1 
               AND ns.name IN ('step-one', 'step-two')
               AND wst.to_state = 'complete' 
               AND wst.most_recent = true",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(completed_steps, 2);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_with_step_in_progress(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let task_id = DummyTaskOrchestrationFactory::create_with_step_in_progress(&mut ctx).await?;
        
        // Verify that step-three is in progress
        let in_progress_step = sqlx::query!(
            "SELECT COUNT(*) as count
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             JOIN tasker_workflow_step_transitions wst ON ws.workflow_step_id = wst.workflow_step
             WHERE ws.task_id = $1 
               AND ns.name = 'step-three'
               AND wst.to_state = 'in_progress' 
               AND wst.most_recent = true",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(in_progress_step, 1);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_with_step_error(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let task_id = DummyTaskOrchestrationFactory::create_with_step_error(&mut ctx).await?;
        
        // Verify that step-one is in error state
        let error_step = sqlx::query!(
            "SELECT COUNT(*) as count
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             JOIN tasker_workflow_step_transitions wst ON ws.workflow_step_id = wst.workflow_step
             WHERE ws.task_id = $1 
               AND ns.name = 'step-one'
               AND wst.to_state = 'error' 
               AND wst.most_recent = true",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(error_step, 1);
        
        // Verify that the step has max attempts
        let attempts = sqlx::query!(
            "SELECT ws.attempts
             FROM tasker_workflow_steps ws
             JOIN tasker_named_steps ns ON ws.named_step = ns.named_step_id
             WHERE ws.task_id = $1 AND ns.name = 'step-one'",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .attempts;
        
        assert_eq!(attempts, 4); // Max retries + 1
        
        Ok(())
    }
}