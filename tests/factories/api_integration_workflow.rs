use crate::factories::{
    Factory, FactoryContext, FactoryResult,
    task::{TaskFactory, TaskFactoryConfig},
    workflow_step::{WorkflowStepFactory, WorkflowStepFactoryConfig},
    workflow_step_edge::{WorkflowStepEdgeFactory, WorkflowStepEdgeFactoryConfig},
    named_task::{NamedTaskFactory, NamedTaskFactoryConfig},
    named_step::{NamedStepFactory, NamedStepFactoryConfig},
    dependent_system::{DependentSystemFactory, DependentSystemFactoryConfig},
};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

/// Step states for workflow steps
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepState {
    Pending,
    InProgress,
    Complete,
    Error,
}

/// Configuration for API integration workflow factory
#[derive(Debug, Clone)]
pub struct ApiIntegrationWorkflowConfig {
    /// Base context data for the task
    pub context: HashMap<String, serde_json::Value>,
    /// Whether to create with dependencies
    pub with_dependencies: bool,
    /// Initial state for all steps
    pub step_states: StepState,
    /// Initiator for the task
    pub initiator: String,
    /// Source system for the task
    pub source_system: String,
    /// Reason for the task
    pub reason: String,
    /// Tags to apply to the task
    pub tags: Vec<String>,
}

impl Default for ApiIntegrationWorkflowConfig {
    fn default() -> Self {
        let mut context = HashMap::new();
        context.insert("cart_id".to_string(), json!(123));
        context.insert("user_id".to_string(), json!(456));
        
        Self {
            context,
            with_dependencies: true,
            step_states: StepState::Pending,
            initiator: "pete@test".to_string(),
            source_system: "ecommerce-api".to_string(),
            reason: "automated_integration_test".to_string(),
            tags: vec!["api".to_string(), "integration".to_string(), "testing".to_string()],
        }
    }
}

/// Factory for creating API integration workflows
pub struct ApiIntegrationWorkflowFactory;

impl ApiIntegrationWorkflowFactory {
    /// Create an API integration workflow
    pub async fn create(
        ctx: &mut FactoryContext,
        config: ApiIntegrationWorkflowConfig,
    ) -> FactoryResult<i64> {
        // Create the named task
        let named_task_id = NamedTaskFactory::create(
            ctx,
            NamedTaskFactoryConfig {
                name: "api_integration_task".to_string(),
                namespace_name: Some("default".to_string()),
                version: Some("0.1.0".to_string()),
                description: Some("API integration workflow task".to_string()),
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

        // Create workflow steps if dependencies are requested
        if config.with_dependencies {
            Self::create_dependencies(ctx, task_id, config.step_states).await?;
        }

        Ok(task_id)
    }

    /// Create the workflow dependencies (systems, steps, and edges)
    async fn create_dependencies(
        ctx: &mut FactoryContext,
        task_id: i64,
        step_state: StepState,
    ) -> FactoryResult<()> {
        // Create dependent systems
        let api_system_id = Self::get_or_create_system(
            ctx,
            "api-system",
            "API system for integration testing",
        ).await?;

        let database_system_id = Self::get_or_create_system(
            ctx,
            "database-system", 
            "Database system for integration testing",
        ).await?;

        let notification_system_id = Self::get_or_create_system(
            ctx,
            "notification-system",
            "Notification system for integration testing",
        ).await?;

        // Create named steps
        let fetch_cart_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "fetch_cart".to_string(),
                dependent_system_id: Some(api_system_id),
                description: Some("Fetch cart data from API".to_string()),
                ..Default::default()
            },
        ).await?;

        let fetch_products_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "fetch_products".to_string(),
                dependent_system_id: Some(api_system_id),
                description: Some("Fetch product data from API".to_string()),
                ..Default::default()
            },
        ).await?;

        let validate_products_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "validate_products".to_string(),
                dependent_system_id: Some(database_system_id),
                description: Some("Validate product data".to_string()),
                ..Default::default()
            },
        ).await?;

        let create_order_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "create_order".to_string(),
                dependent_system_id: Some(database_system_id),
                description: Some("Create order in database".to_string()),
                ..Default::default()
            },
        ).await?;

        let publish_event_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "publish_event".to_string(),
                dependent_system_id: Some(notification_system_id),
                description: Some("Publish order created event".to_string()),
                ..Default::default()
            },
        ).await?;

        // Create workflow steps
        let ws1_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: fetch_cart_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        let ws2_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: fetch_products_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        let ws3_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: validate_products_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        let ws4_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: create_order_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        let ws5_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: publish_event_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        // Create workflow step edges (dependencies)
        // validate depends on fetch_cart and fetch_products
        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws1_id,
                to_step_id: ws3_id,
                name: "provides".to_string(),
            },
        ).await?;

        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws2_id,
                to_step_id: ws3_id,
                name: "provides".to_string(),
            },
        ).await?;

        // create_order depends on validate_products
        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws3_id,
                to_step_id: ws4_id,
                name: "provides".to_string(),
            },
        ).await?;

        // publish_event depends on create_order
        WorkflowStepEdgeFactory::create(
            ctx,
            WorkflowStepEdgeFactoryConfig {
                from_step_id: ws4_id,
                to_step_id: ws5_id,
                name: "provides".to_string(),
            },
        ).await?;

        // TODO: Apply step states based on config.step_states
        // This would involve creating appropriate workflow_step_transitions

        Ok(())
    }

    /// Get or create a dependent system
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

/// Configuration for simple linear workflow factory
#[derive(Debug, Clone)]
pub struct SimpleLinearWorkflowConfig {
    /// Number of steps in the workflow
    pub step_count: usize,
    /// Base context data for the task
    pub context: HashMap<String, serde_json::Value>,
    /// Initial state for all steps
    pub step_states: StepState,
    /// Initiator for the task
    pub initiator: String,
    /// Source system for the task
    pub source_system: String,
    /// Reason for the task
    pub reason: String,
}

impl Default for SimpleLinearWorkflowConfig {
    fn default() -> Self {
        let mut context = HashMap::new();
        context.insert("workflow_type".to_string(), json!("linear"));
        context.insert("test".to_string(), json!(true));
        
        Self {
            step_count: 3,
            context,
            step_states: StepState::Pending,
            initiator: "test_system".to_string(),
            source_system: "test".to_string(),
            reason: "testing".to_string(),
        }
    }
}

/// Factory for creating simple linear workflows
pub struct SimpleLinearWorkflowFactory;

impl SimpleLinearWorkflowFactory {
    /// Create a simple linear workflow
    pub async fn create(
        ctx: &mut FactoryContext,
        config: SimpleLinearWorkflowConfig,
    ) -> FactoryResult<i64> {
        // Create the named task
        let named_task_id = NamedTaskFactory::create(
            ctx,
            NamedTaskFactoryConfig {
                name: "simple_workflow".to_string(),
                namespace_name: Some("default".to_string()),
                version: Some("0.1.0".to_string()),
                description: Some("Simple linear workflow".to_string()),
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
                tags: vec!["linear".to_string(), "simple".to_string()],
                ..Default::default()
            },
        ).await?;

        // Create test system
        let system_id = Self::get_or_create_system(ctx, "test_system").await?;

        // Create linear workflow steps
        let mut previous_step_id = None;
        for i in 1..=config.step_count {
            let step_name = format!("step_{}", i);
            
            let named_step_id = NamedStepFactory::create(
                ctx,
                NamedStepFactoryConfig {
                    name: step_name.clone(),
                    dependent_system_id: Some(system_id),
                    description: Some(format!("Linear step {}", i)),
                    ..Default::default()
                },
            ).await?;

            let step_id = WorkflowStepFactory::create(
                ctx,
                WorkflowStepFactoryConfig {
                    task_id,
                    named_step_id,
                    inputs: HashMap::new(),
                    ..Default::default()
                },
            ).await?;

            // Create edge from previous step if not the first
            if let Some(prev_id) = previous_step_id {
                WorkflowStepEdgeFactory::create(
                    ctx,
                    WorkflowStepEdgeFactoryConfig {
                        from_step_id: prev_id,
                        to_step_id: step_id,
                        name: "provides".to_string(),
                    },
                ).await?;
            }

            previous_step_id = Some(step_id);
        }

        Ok(task_id)
    }

    /// Get or create test system
    async fn get_or_create_system(
        ctx: &mut FactoryContext,
        name: &str,
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
                description: Some(format!("{} for testing", name)),
            },
        ).await
    }
}

/// Configuration for parallel processing workflow factory
#[derive(Debug, Clone)]
pub struct ParallelWorkflowConfig {
    /// Number of parallel processing steps
    pub parallel_count: usize,
    /// Base context data for the task
    pub context: HashMap<String, serde_json::Value>,
    /// Initial state for all steps
    pub step_states: StepState,
    /// Initiator for the task
    pub initiator: String,
    /// Source system for the task
    pub source_system: String,
    /// Reason for the task
    pub reason: String,
}

impl Default for ParallelWorkflowConfig {
    fn default() -> Self {
        let mut context = HashMap::new();
        context.insert("batch_id".to_string(), json!(789));
        context.insert("chunk_size".to_string(), json!(100));
        
        Self {
            parallel_count: 3,
            context,
            step_states: StepState::Pending,
            initiator: "batch_processor".to_string(),
            source_system: "data_pipeline".to_string(),
            reason: "parallel_processing".to_string(),
        }
    }
}

/// Factory for creating parallel processing workflows
pub struct ParallelWorkflowFactory;

impl ParallelWorkflowFactory {
    /// Create a parallel processing workflow
    pub async fn create(
        ctx: &mut FactoryContext,
        config: ParallelWorkflowConfig,
    ) -> FactoryResult<i64> {
        // Create the named task
        let named_task_id = NamedTaskFactory::create(
            ctx,
            NamedTaskFactoryConfig {
                name: "data_processing".to_string(),
                namespace_name: Some("default".to_string()),
                version: Some("0.1.0".to_string()),
                description: Some("Parallel data processing workflow".to_string()),
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
                tags: vec!["parallel".to_string(), "processing".to_string()],
                ..Default::default()
            },
        ).await?;

        // Create database system
        let system_id = Self::get_or_create_system(ctx, "database_system").await?;

        // Create initial step
        let init_named_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "initialize_batch".to_string(),
                dependent_system_id: Some(system_id),
                description: Some("Initialize batch processing".to_string()),
                ..Default::default()
            },
        ).await?;

        let init_step_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: init_named_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        // Create parallel processing steps
        let mut parallel_step_ids = Vec::new();
        for i in 1..=config.parallel_count {
            let step_name = format!("process_chunk_{}", i);
            
            let named_step_id = NamedStepFactory::create(
                ctx,
                NamedStepFactoryConfig {
                    name: step_name.clone(),
                    dependent_system_id: Some(system_id),
                    description: Some(format!("Process chunk {}", i)),
                    ..Default::default()
                },
            ).await?;

            let step_id = WorkflowStepFactory::create(
                ctx,
                WorkflowStepFactoryConfig {
                    task_id,
                    named_step_id,
                    inputs: HashMap::new(),
                    ..Default::default()
                },
            ).await?;

            // Each parallel step depends on the init step
            WorkflowStepEdgeFactory::create(
                ctx,
                WorkflowStepEdgeFactoryConfig {
                    from_step_id: init_step_id,
                    to_step_id: step_id,
                    name: "provides".to_string(),
                },
            ).await?;

            parallel_step_ids.push(step_id);
        }

        // Create final aggregation step
        let final_named_step_id = NamedStepFactory::create(
            ctx,
            NamedStepFactoryConfig {
                name: "aggregate_results".to_string(),
                dependent_system_id: Some(system_id),
                description: Some("Aggregate processing results".to_string()),
                ..Default::default()
            },
        ).await?;

        let final_step_id = WorkflowStepFactory::create(
            ctx,
            WorkflowStepFactoryConfig {
                task_id,
                named_step_id: final_named_step_id,
                inputs: HashMap::new(),
                ..Default::default()
            },
        ).await?;

        // Final step depends on all parallel steps
        for parallel_step_id in parallel_step_ids {
            WorkflowStepEdgeFactory::create(
                ctx,
                WorkflowStepEdgeFactoryConfig {
                    from_step_id: parallel_step_id,
                    to_step_id: final_step_id,
                    name: "provides".to_string(),
                },
            ).await?;
        }

        Ok(task_id)
    }

    /// Get or create database system
    async fn get_or_create_system(
        ctx: &mut FactoryContext,
        name: &str,
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
                description: Some("Database system for data processing".to_string()),
            },
        ).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factories::test_helpers::TestHelper;

    #[sqlx::test]
    async fn test_create_api_integration_workflow(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let config = ApiIntegrationWorkflowConfig::default();
        let task_id = ApiIntegrationWorkflowFactory::create(&mut ctx, config).await?;
        
        // Verify task was created
        let task_exists = sqlx::query!(
            "SELECT 1 FROM tasker_tasks WHERE task_id = $1",
            task_id
        )
        .fetch_optional(&ctx.pool)
        .await?
        .is_some();
        
        assert!(task_exists);
        
        // Verify workflow steps were created (5 steps for API integration)
        let step_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_workflow_steps WHERE task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(step_count, 5);
        
        // Verify edges were created (4 edges for the dependency structure)
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
        
        assert_eq!(edge_count, 4);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_simple_linear_workflow(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let config = SimpleLinearWorkflowConfig {
            step_count: 5,
            ..Default::default()
        };
        let task_id = SimpleLinearWorkflowFactory::create(&mut ctx, config).await?;
        
        // Verify workflow steps were created
        let step_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_workflow_steps WHERE task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(step_count, 5);
        
        // Verify edges were created (4 edges for 5 steps in linear pattern)
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
        
        assert_eq!(edge_count, 4);
        
        Ok(())
    }

    #[sqlx::test]
    async fn test_create_parallel_workflow(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = TestHelper::setup_context(pool).await?;
        
        let config = ParallelWorkflowConfig {
            parallel_count: 4,
            ..Default::default()
        };
        let task_id = ParallelWorkflowFactory::create(&mut ctx, config).await?;
        
        // Verify workflow steps were created (init + 4 parallel + final = 6 steps)
        let step_count = sqlx::query!(
            "SELECT COUNT(*) as count FROM tasker_workflow_steps WHERE task_id = $1",
            task_id
        )
        .fetch_one(&ctx.pool)
        .await?
        .count
        .unwrap_or(0);
        
        assert_eq!(step_count, 6);
        
        // Verify edges were created (4 init->parallel + 4 parallel->final = 8 edges)
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
        
        assert_eq!(edge_count, 8);
        
        Ok(())
    }
}