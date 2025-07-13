use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio;

use tasker_core::orchestration::{
    types::StepResult, FrameworkIntegration, OrchestrationError, TaskContext, ViableStep,
    WorkflowCoordinator,
};

/// Mock framework integration for testing orchestration without real Ruby FFI
pub struct MockFrameworkIntegration {
    step_delays: HashMap<String, Duration>,
    step_results: HashMap<String, StepResult>,
    execution_log: Arc<Mutex<Vec<StepExecution>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StepExecution {
    pub step_id: i64,
    pub step_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl MockFrameworkIntegration {
    pub fn new() -> Self {
        Self {
            step_delays: HashMap::new(),
            step_results: HashMap::new(),
            execution_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    pub fn with_delays(delays: Vec<(&str, u64)>) -> Self {
        let mut mock = Self::new();
        for (step_name, delay_ms) in delays {
            mock.step_delays
                .insert(step_name.to_string(), Duration::from_millis(delay_ms));
        }
        mock
    }

    #[allow(dead_code)]
    pub fn set_step_result(&mut self, step_name: &str, result: StepResult) {
        self.step_results.insert(step_name.to_string(), result);
    }

    #[allow(dead_code)]
    pub fn get_execution_log(&self) -> Vec<StepExecution> {
        self.execution_log.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl FrameworkIntegration for MockFrameworkIntegration {
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        _context: &TaskContext,
    ) -> Result<StepResult, OrchestrationError> {
        // Log execution for test validation
        self.execution_log.lock().unwrap().push(StepExecution {
            step_id: step.step_id,
            step_name: step.name.clone(),
            started_at: chrono::Utc::now(),
        });

        // Apply configured delay
        if let Some(delay) = self.step_delays.get(&step.name) {
            tokio::time::sleep(*delay).await;
        }

        // Return configured result or default success
        Ok(self
            .step_results
            .get(&step.name)
            .cloned()
            .unwrap_or_else(|| StepResult {
                step_id: step.step_id,
                status: tasker_core::orchestration::types::StepStatus::Completed,
                output: json!({"mock": true}),
                execution_duration: Duration::from_millis(50),
                error_message: None,
                retry_after: None,
                error_code: None,
                error_context: None,
            }))
    }

    fn framework_name(&self) -> &'static str {
        "MockFramework"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, OrchestrationError> {
        Ok(TaskContext {
            task_id,
            data: json!({"mock": true}),
            metadata: HashMap::new(),
        })
    }

    async fn enqueue_task(
        &self,
        _task_id: i64,
        _delay: Option<Duration>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn mark_task_failed(
        &self,
        _task_id: i64,
        _error: &str,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn update_step_state(
        &self,
        _step_id: i64,
        _state: &str,
        _result: Option<&JsonValue>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }
}

/// Test database setup helper
#[allow(dead_code)]
pub async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://localhost/tasker_test".to_string());

    let pool = PgPool::connect(&database_url).await.unwrap();

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    // Clean up any existing test data
    sqlx::query("TRUNCATE workflow_step_transitions, task_transitions, workflow_step_edges, workflow_steps, tasks CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    pool
}

#[sqlx::test]
async fn test_orchestration_with_real_task(pool: PgPool) -> sqlx::Result<()> {
    // Create a real task with workflow steps using the factory system to expose deeper placeholders

    // Use the core factory system to create a task AND workflow steps
    use crate::factories::{
        base::SqlxFactory,
        core::{TaskFactory, WorkflowStepFactory},
    };

    // Create a task first
    let task_factory = TaskFactory::new()
        .with_context(json!({"test": "orchestration"}))
        .complete();
    let task = task_factory.create(&pool).await.unwrap();

    // Create some workflow steps for this task WITH proper state transitions
    let step1 = WorkflowStepFactory::new()
        .for_task(task.task_id)
        .with_named_step("start_step")
        .pending() // This creates initial state transition to 'pending'
        .create(&pool)
        .await
        .unwrap();

    let step2 = WorkflowStepFactory::new()
        .for_task(task.task_id)
        .with_named_step("process_step")
        .pending() // This creates initial state transition to 'pending'
        .create(&pool)
        .await
        .unwrap();

    println!(
        "Created task {} with steps {} and {}",
        task.task_id, step1.workflow_step_id, step2.workflow_step_id
    );

    // Debug: Check what transitions exist for our steps
    let transitions = sqlx::query!(
        "SELECT workflow_step_id, from_state, to_state, most_recent FROM tasker_workflow_step_transitions WHERE workflow_step_id IN ($1, $2) ORDER BY created_at",
        step1.workflow_step_id,
        step2.workflow_step_id
    ).fetch_all(&pool).await.unwrap();

    println!("Step transitions found:");
    for t in &transitions {
        println!(
            "  Step {}: {} -> {} (most_recent: {})",
            t.workflow_step_id,
            t.from_state.as_deref().unwrap_or("NULL"),
            t.to_state,
            t.most_recent
        );
    }

    // Debug: Check what the SQL function actually returns
    let readiness_results = sqlx::query!(
        "SELECT * FROM get_step_readiness_status($1, NULL)",
        task.task_id
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    println!("Step readiness results:");
    for r in &readiness_results {
        println!(
            "  Step {:?}: current_state={:?}, ready_for_execution={:?}",
            r.workflow_step_id, r.current_state, r.ready_for_execution
        );
    }

    let task_id = task.task_id;

    // Try to create a WorkflowCoordinator
    let coordinator = WorkflowCoordinator::new(pool.clone());

    // Try to create our mock framework integration
    let mock_integration = MockFrameworkIntegration::new();

    // This call should expose the critical placeholders we need to fix
    let result = coordinator
        .execute_task_workflow(task_id, Arc::new(mock_integration))
        .await;

    // This should expose deeper placeholders than the task-not-found error
    match result {
        Ok(result) => {
            println!("Orchestration result: {result:?}");
        }
        Err(e) => {
            println!("Orchestration error (this exposes placeholders): {e:?}");
            // This will help us identify what placeholders need fixing
        }
    }

    Ok(())
}

#[sqlx::test]
async fn test_workflow_coordinator_compilation(pool: PgPool) -> sqlx::Result<()> {
    // Simplified test to check WorkflowCoordinator can be instantiated
    let _coordinator = WorkflowCoordinator::new(pool.clone());

    println!("WorkflowCoordinator instantiated successfully");
    Ok(())
}

#[sqlx::test]
async fn test_mock_framework_integration_compilation(_pool: PgPool) -> sqlx::Result<()> {
    // Test that MockFrameworkIntegration compiles with all required traits
    let mock_integration = MockFrameworkIntegration::new();

    // Test framework name
    assert_eq!(mock_integration.framework_name(), "MockFramework");

    // Test get_task_context
    let context = mock_integration.get_task_context(123).await.unwrap();
    assert_eq!(context.task_id, 123);

    println!("MockFrameworkIntegration works correctly");
    Ok(())
}
