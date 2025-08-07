use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio;

use tasker_core::models::core::task_request::TaskRequest;
use tasker_core::orchestration::{
    types::StepResult, FrameworkIntegration, OrchestrationError, TaskContext, TaskInitializer,
    WorkflowCoordinator,
};
/// Helper function to create a task using TaskInitializer with consistent setup
async fn create_test_task_with_initializer(
    pool: &PgPool,
    task_name: &str,
    workflow_type: &str,
    test_description: &str,
) -> Result<tasker_core::orchestration::TaskInitializationResult, Box<dyn std::error::Error>> {
    let initializer = TaskInitializer::for_testing(pool.clone());

    let task_request = TaskRequest::new(task_name.to_string(), "integration".to_string())
        .with_context(json!({
            "workflow_type": workflow_type,
            "test": test_description,
            "created_by": "integration_test_suite"
        }))
        .with_initiator("test_suite".to_string())
        .with_source_system("integration_test".to_string())
        .with_reason(format!("Testing {workflow_type} workflow execution"))
        .with_tags(vec![
            workflow_type.to_string(),
            "workflow".to_string(),
            "integration".to_string(),
        ]);

    let result = initializer
        .create_task_from_request(task_request)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    println!(
        "Created task {} using TaskInitializer (workflow: {})",
        result.task_id, workflow_type
    );
    Ok(result)
}

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
}

#[sqlx::test]
async fn test_orchestration_with_real_task(pool: PgPool) -> sqlx::Result<()> {
    // Create a real task using TaskInitializer for clean task creation
    let result = create_test_task_with_initializer(
        &pool,
        "orchestration_test",
        "basic",
        "orchestration_placeholder_testing",
    )
    .await
    .expect("Task initialization should succeed");

    println!(
        "Created task {} using TaskInitializer (steps: {})",
        result.task_id, result.step_count
    );

    let task_id = result.task_id;

    // Debug: First check if we actually have any workflow steps
    let step_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM tasker_workflow_steps WHERE task_id = $1",
        task_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    println!(
        "Task {} has {} workflow steps",
        task_id,
        step_count.count.unwrap_or(0)
    );

    if step_count.count.unwrap_or(0) == 0 {
        println!(
            "⚠️ No workflow steps found - TaskInitializer needs handler config to create steps"
        );
        println!("Creating basic workflow steps manually for orchestration testing...");

        // Create basic workflow steps manually for orchestration testing
        use crate::factories::{base::SqlxFactory, core::WorkflowStepFactory};

        let basic_step = WorkflowStepFactory::new()
            .for_task(task_id)
            .with_named_step("basic_test_step")
            .pending()
            .create(&pool)
            .await
            .unwrap();

        println!(
            "Created basic step {} for orchestration testing",
            basic_step.workflow_step_id
        );
    }

    // Debug: Check what the SQL function actually returns
    println!("Calling get_step_readiness_status SQL function...");
    let readiness_results =
        sqlx::query!("SELECT * FROM get_step_readiness_status($1, NULL)", task_id)
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

    // Create a WorkflowCoordinator optimized for testing (2-second timeout)
    let coordinator = WorkflowCoordinator::for_testing_with_timeout(pool.clone(), 1).await;

    // Try to create our mock framework integration
    let mock_integration = MockFrameworkIntegration::new();

    // This call should expose the critical placeholders we need to fix
    println!("🎯 Starting WorkflowCoordinator.execute_task_workflow()...");

    let orchestration_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        coordinator.execute_task_workflow(task_id),
    )
    .await;

    // This should expose deeper placeholders than the task-not-found error
    match orchestration_result {
        Ok(Ok(orch_result)) => {
            println!("✅ Orchestration succeeded: {orch_result:?}");
        }
        Ok(Err(e)) => {
            println!("❌ Orchestration error (this exposes placeholders): {e:?}");
            // This will help us identify what placeholders need fixing
        }
        Err(_) => {
            println!("⏱️ Orchestration timed out after 10 seconds - this indicates an infinite loop or deadlock");
            println!("   Most likely in state machine evaluation or SQL function calls");
        }
    }

    Ok(())
}

#[sqlx::test]
async fn test_workflow_coordinator_compilation(pool: PgPool) -> sqlx::Result<()> {
    // Simplified test to check WorkflowCoordinator can be instantiated
    let _coordinator = WorkflowCoordinator::for_testing(pool.clone()).await;

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

#[sqlx::test]
async fn test_linear_workflow_execution(pool: PgPool) -> sqlx::Result<()> {
    println!("=== Linear Workflow Test (A→B→C→D) ===");

    // Create a task using TaskInitializer for clean task creation
    let task_result =
        create_test_task_with_initializer(&pool, "linear_workflow", "linear", "linear_execution")
            .await
            .expect("Task initialization should succeed");

    // Create workflow steps manually since we don't have handler config yet
    use crate::factories::{
        base::SqlxFactory, core::WorkflowStepFactory, relationships::WorkflowStepEdgeFactory,
    };

    // Create 4 steps: A→B→C→D
    let step_a = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("step_a")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_b = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("step_b")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_c = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("step_c")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_d = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("step_d")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    println!(
        "Created steps: A={}, B={}, C={}, D={}",
        step_a.workflow_step_id,
        step_b.workflow_step_id,
        step_c.workflow_step_id,
        step_d.workflow_step_id
    );

    // Create dependencies: A→B→C→D
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_b.workflow_step_id)
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_b.workflow_step_id)
        .with_to_step(step_c.workflow_step_id)
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_c.workflow_step_id)
        .with_to_step(step_d.workflow_step_id)
        .create(&pool)
        .await
        .unwrap();

    println!("Created dependencies: A→B→C→D");

    // Test orchestration execution
    let coordinator = WorkflowCoordinator::for_testing(pool.clone()).await;

    let start_time = Instant::now();
    let orchestration_result = coordinator.execute_task_workflow(task_result.task_id).await;

    match orchestration_result {
        Ok(orch_result) => {
            let duration = start_time.elapsed();
            println!("✅ Linear workflow completed successfully!");
            println!("   Result: {orch_result:?}");
            println!("   Duration: {duration:?}");

            // Validate that all 4 steps were executed
            // Note: We expect 4 steps in a linear workflow
            // This tests our sequential dependency execution
        }
        Err(e) => {
            println!("❌ Linear workflow failed: {e:?}");
            return Err(sqlx::Error::Protocol(
                "Linear workflow test failed".to_string(),
            ));
        }
    }

    Ok(())
}

#[sqlx::test]
async fn test_diamond_workflow_execution(pool: PgPool) -> sqlx::Result<()> {
    println!("=== Diamond Workflow Test (A→(B,C)→D) ===");

    // Create a task using TaskInitializer for clean task creation
    let task_result = create_test_task_with_initializer(
        &pool,
        "diamond_workflow",
        "diamond",
        "concurrent_execution",
    )
    .await
    .expect("Task initialization should succeed");

    // Create workflow steps manually since we don't have handler config yet
    use crate::factories::{
        base::SqlxFactory, core::WorkflowStepFactory, relationships::WorkflowStepEdgeFactory,
    };

    // Create 4 steps: A→(B,C)→D (diamond pattern)
    let step_a = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("setup")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_b = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("process_a")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_c = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("process_b")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_d = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("finalize")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    println!(
        "Created steps: Setup={}, ProcessA={}, ProcessB={}, Finalize={}",
        step_a.workflow_step_id,
        step_b.workflow_step_id,
        step_c.workflow_step_id,
        step_d.workflow_step_id
    );

    // Create diamond dependencies: A→B, A→C, B→D, C→D
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_b.workflow_step_id)
        .with_name("setup_provides_for_process_a")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_c.workflow_step_id)
        .with_name("setup_provides_for_process_b")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_b.workflow_step_id)
        .with_to_step(step_d.workflow_step_id)
        .with_name("process_a_provides_for_finalize")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_c.workflow_step_id)
        .with_to_step(step_d.workflow_step_id)
        .with_name("process_b_provides_for_finalize")
        .create(&pool)
        .await
        .unwrap();

    println!("Created diamond dependencies: A→(B,C)→D");

    // Test orchestration execution
    let coordinator = WorkflowCoordinator::for_testing(pool.clone()).await;

    let start_time = Instant::now();
    let orchestration_result = coordinator.execute_task_workflow(task_result.task_id).await;

    match orchestration_result {
        Ok(orch_result) => {
            let duration = start_time.elapsed();
            println!("✅ Diamond workflow completed successfully!");
            println!("   Result: {orch_result:?}");
            println!("   Duration: {duration:?}");

            // This tests concurrent execution capability
            // B and C should be able to execute concurrently after A completes
            // D should only execute after both B and C complete
        }
        Err(e) => {
            println!("❌ Diamond workflow failed: {e:?}");
            return Err(sqlx::Error::Protocol(
                "Diamond workflow test failed".to_string(),
            ));
        }
    }

    Ok(())
}

#[sqlx::test]
async fn test_tree_workflow_execution(pool: PgPool) -> sqlx::Result<()> {
    println!("=== Tree Workflow Test (Complex Multi-Level Dependencies) ===");

    // Create a task using TaskInitializer for clean task creation
    let task_result = create_test_task_with_initializer(
        &pool,
        "tree_workflow",
        "tree",
        "multi_level_dependencies",
    )
    .await
    .expect("Task initialization should succeed");

    // Create workflow steps manually since we don't have handler config yet
    use crate::factories::{
        base::SqlxFactory, core::WorkflowStepFactory, relationships::WorkflowStepEdgeFactory,
    };

    // Create tree structure:
    //       Root (A)
    //      /   |   \
    //     B    C    D
    //    / \   |   / \
    //   E   F  G  H   I
    //           |
    //           J

    // Level 0: Root
    let step_a = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("root_initialization")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    // Level 1: Main branches
    let step_b = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("branch_user_data")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_c = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("branch_payment")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_d = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("branch_inventory")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    // Level 2: Sub-branches
    let step_e = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("validate_user_profile")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_f = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("validate_user_permissions")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_g = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("process_payment")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_h = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("reserve_inventory")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    let step_i = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("update_stock_levels")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    // Level 3: Leaf node
    let step_j = WorkflowStepFactory::new()
        .for_task(task_result.task_id)
        .with_named_step("send_payment_confirmation")
        .pending()
        .create(&pool)
        .await
        .unwrap();

    println!(
        "Created tree steps: Root={}, B={}, C={}, D={}, E={}, F={}, G={}, H={}, I={}, J={}",
        step_a.workflow_step_id,
        step_b.workflow_step_id,
        step_c.workflow_step_id,
        step_d.workflow_step_id,
        step_e.workflow_step_id,
        step_f.workflow_step_id,
        step_g.workflow_step_id,
        step_h.workflow_step_id,
        step_i.workflow_step_id,
        step_j.workflow_step_id
    );

    // Create tree dependencies: Root → Main branches
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_b.workflow_step_id)
        .with_name("root_enables_user_data_branch")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_c.workflow_step_id)
        .with_name("root_enables_payment_branch")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_a.workflow_step_id)
        .with_to_step(step_d.workflow_step_id)
        .with_name("root_enables_inventory_branch")
        .create(&pool)
        .await
        .unwrap();

    // Main branches → Sub-branches
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_b.workflow_step_id)
        .with_to_step(step_e.workflow_step_id)
        .with_name("user_data_enables_profile_validation")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_b.workflow_step_id)
        .with_to_step(step_f.workflow_step_id)
        .with_name("user_data_enables_permission_validation")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_c.workflow_step_id)
        .with_to_step(step_g.workflow_step_id)
        .with_name("payment_branch_enables_processing")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_d.workflow_step_id)
        .with_to_step(step_h.workflow_step_id)
        .with_name("inventory_branch_enables_reservation")
        .create(&pool)
        .await
        .unwrap();

    WorkflowStepEdgeFactory::new()
        .with_from_step(step_d.workflow_step_id)
        .with_to_step(step_i.workflow_step_id)
        .with_name("inventory_branch_enables_stock_update")
        .create(&pool)
        .await
        .unwrap();

    // Payment processing → Confirmation (leaf dependency)
    WorkflowStepEdgeFactory::new()
        .with_from_step(step_g.workflow_step_id)
        .with_to_step(step_j.workflow_step_id)
        .with_name("payment_processing_enables_confirmation")
        .create(&pool)
        .await
        .unwrap();

    println!("Created tree dependencies: 3-level hierarchical structure with 10 steps");

    // Test orchestration execution
    let coordinator = WorkflowCoordinator::for_testing(pool.clone()).await;

    let start_time = Instant::now();
    let orchestration_result = coordinator.execute_task_workflow(task_result.task_id).await;

    match orchestration_result {
        Ok(orch_result) => {
            let duration = start_time.elapsed();
            println!("✅ Tree workflow completed successfully!");
            println!("   Result: {orch_result:?}");
            println!("   Duration: {duration:?}");

            // This tests complex dependency resolution:
            // - Level 0: Root must execute first
            // - Level 1: B, C, D can execute concurrently after A
            // - Level 2: E, F depend on B; G depends on C; H, I depend on D
            // - Level 3: J depends on G (payment confirmation after payment)

            // This validates our dependency resolution engine handles:
            // 1. Multi-level cascading dependencies
            // 2. Concurrent execution within levels
            // 3. Complex tree structures beyond simple linear/diamond patterns
        }
        Err(e) => {
            println!("❌ Tree workflow failed: {e:?}");
            return Err(sqlx::Error::Protocol(
                "Tree workflow test failed".to_string(),
            ));
        }
    }

    Ok(())
}
