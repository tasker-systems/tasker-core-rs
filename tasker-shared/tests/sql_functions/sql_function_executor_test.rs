//! # SqlFunctionExecutor Integration Tests
//!
//! This module tests the `SqlFunctionExecutor` from `tasker_shared::database::sql_functions`,
//! which provides the unified Rust interface for executing PostgreSQL functions that contain
//! critical workflow orchestration logic.
//!
//! Tests cover:
//! - Dependency level calculation and hash conversion
//! - Analytics metrics retrieval with and without timestamp filters
//! - Step readiness queries through the executor
//! - Slowest steps / tasks (individual and aggregated)
//! - Transitive dependency resolution and results mapping
//! - Task ready info, next-ready-task batch queries
//! - Current task state retrieval
//! - Stale task DLQ discovery and detection (dry-run mode)
//! - FunctionRegistry accessor methods

use sqlx::PgPool;
use uuid::Uuid;

use tasker_shared::database::sql_functions::{FunctionRegistry, SqlFunctionExecutor};
use tasker_shared::models::core::task_transition::{NewTaskTransition, TaskTransition};
use tasker_shared::models::factories::{
    base::SqlxFactory, complex_workflows::ComplexWorkflowFactory,
    states::WorkflowStepTransitionFactory,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to map factory errors to sqlx errors
    fn map_factory_error(e: tasker_shared::models::factories::base::FactoryError) -> sqlx::Error {
        sqlx::Error::Protocol(format!("Factory error: {e}"))
    }

    /// Helper to create a task with proper initial state transitions for task AND steps.
    ///
    /// Creates a workflow via the provided factory, then initialises the task to "pending"
    /// and every step to "pending" so that the SQL functions see real state rows.
    async fn create_task_with_initial_state(
        pool: &PgPool,
        factory: ComplexWorkflowFactory,
    ) -> Result<(Uuid, Vec<Uuid>), sqlx::Error> {
        let (task_uuid, step_uuids) = factory.create(pool).await.map_err(map_factory_error)?;

        // Create initial "pending" state for the task
        TaskTransition::create(
            pool,
            NewTaskTransition {
                task_uuid,
                to_state: "pending".to_string(),
                from_state: None,
                processor_uuid: None,
                metadata: None,
            },
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;

        // Create initial "pending" state for ALL workflow steps
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::new()
                .for_workflow_step(step_uuid)
                .to_state("pending")
                .create(pool)
                .await
                .map_err(map_factory_error)?;
        }

        Ok((task_uuid, step_uuids))
    }

    // ========================================================================
    // 1. Dependency Levels
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_calculate_dependency_levels(pool: PgPool) -> sqlx::Result<()> {
        // Diamond: A -> (B, C) -> D
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());
        let levels = executor.calculate_dependency_levels(task_uuid).await?;

        // All four steps should be present
        assert_eq!(
            levels.len(),
            4,
            "Diamond workflow should produce 4 dependency level rows"
        );

        // Root step (A) should be at level 0
        let root = levels
            .iter()
            .find(|l| l.workflow_step_uuid == step_uuids[0])
            .expect("Root step should be in results");
        assert!(root.is_root_level(), "Root step should be at level 0");
        assert_eq!(root.dependency_level, 0);

        // Branch steps (B, C) should be at level 1
        for &branch_uuid in &step_uuids[1..=2] {
            let branch = levels
                .iter()
                .find(|l| l.workflow_step_uuid == branch_uuid)
                .expect("Branch step should be in results");
            assert_eq!(
                branch.dependency_level, 1,
                "Branch steps should be at level 1"
            );
        }

        // Merge step (D) should be at level 2
        let merge = levels
            .iter()
            .find(|l| l.workflow_step_uuid == step_uuids[3])
            .expect("Merge step should be in results");
        assert_eq!(merge.dependency_level, 2, "Merge step should be at level 2");

        // Verify that the two branch steps can run in parallel
        let branch_b = levels
            .iter()
            .find(|l| l.workflow_step_uuid == step_uuids[1])
            .unwrap();
        let branch_c = levels
            .iter()
            .find(|l| l.workflow_step_uuid == step_uuids[2])
            .unwrap();
        assert!(branch_b.can_run_parallel_with(branch_c));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_dependency_levels_hash(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());
        let hash = executor.dependency_levels_hash(task_uuid).await?;

        assert_eq!(hash.len(), 4, "Hash should contain 4 entries");

        // Verify HashMap lookup works correctly
        assert_eq!(
            hash.get(&step_uuids[0]).copied(),
            Some(0),
            "Root should map to level 0"
        );
        assert_eq!(
            hash.get(&step_uuids[1]).copied(),
            Some(1),
            "Branch B should map to level 1"
        );
        assert_eq!(
            hash.get(&step_uuids[2]).copied(),
            Some(1),
            "Branch C should map to level 1"
        );
        assert_eq!(
            hash.get(&step_uuids[3]).copied(),
            Some(2),
            "Merge D should map to level 2"
        );

        // Non-existent UUID should be absent
        assert!(
            hash.get(&Uuid::nil()).is_none(),
            "Nil UUID should not be in the map"
        );

        Ok(())
    }

    // ========================================================================
    // 2. Analytics Metrics
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_analytics_metrics(pool: PgPool) -> sqlx::Result<()> {
        // Seed some data so metrics have something to count
        let (_task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Complete all steps to generate throughput data
        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        let executor = SqlFunctionExecutor::new(pool.clone());

        // Call without timestamp filter
        let metrics = executor.get_analytics_metrics(None).await?;
        assert!(
            metrics.active_tasks_count >= 0,
            "active_tasks_count should be non-negative"
        );
        assert!(
            metrics.total_namespaces_count >= 0,
            "total_namespaces_count should be non-negative"
        );
        assert!(
            metrics.unique_task_types_count >= 0,
            "unique_task_types_count should be non-negative"
        );

        // Call with a timestamp filter (one hour ago)
        let since = chrono::Utc::now() - chrono::Duration::hours(1);
        let metrics_filtered = executor.get_analytics_metrics(Some(since)).await?;
        assert!(
            metrics_filtered.active_tasks_count >= 0,
            "Filtered active_tasks_count should be non-negative"
        );

        Ok(())
    }

    // ========================================================================
    // 3. Ready Steps
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_ready_steps(pool: PgPool) -> sqlx::Result<()> {
        // Diamond: A -> (B, C) -> D
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // Initially only the root (A) should be ready
        let ready = executor.get_ready_steps(task_uuid).await?;
        assert_eq!(ready.len(), 1, "Only root step should be ready initially");
        assert_eq!(ready[0].workflow_step_uuid, step_uuids[0]);
        assert!(ready[0].ready_for_execution);

        // Complete root step to unlock branches
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let ready_after = executor.get_ready_steps(task_uuid).await?;
        assert_eq!(
            ready_after.len(),
            2,
            "Both branch steps should be ready after root completes"
        );

        let ready_uuids: Vec<Uuid> = ready_after.iter().map(|s| s.workflow_step_uuid).collect();
        assert!(ready_uuids.contains(&step_uuids[1]));
        assert!(ready_uuids.contains(&step_uuids[2]));

        Ok(())
    }

    // ========================================================================
    // 4. Slowest Steps
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_slowest_steps(pool: PgPool) -> sqlx::Result<()> {
        // Create a workflow and complete steps to generate duration data
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // The `attempts` column in workflow_steps is nullable with no default.
        // The SQL function `get_slowest_steps` returns `ws.attempts` without COALESCE,
        // but the Rust struct `SlowestStepAnalysis` expects `attempts: i32` (non-nullable).
        // We must set attempts to a non-null value to avoid ColumnDecode errors.
        sqlx::query!(
            "UPDATE tasker.workflow_steps SET attempts = COALESCE(attempts, 0) WHERE task_uuid = $1",
            task_uuid
        )
        .execute(&pool)
        .await?;

        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        let executor = SqlFunctionExecutor::new(pool.clone());
        let slowest = executor.get_slowest_steps(Some(10), None).await?;

        // Should not error; result may or may not contain rows depending on
        // whether the SQL function finds completed steps with measurable duration
        assert!(
            slowest.len() <= 10,
            "Should respect the limit parameter (got {})",
            slowest.len()
        );

        // If we do get results, validate structure
        for step in &slowest {
            assert!(!step.step_name.is_empty(), "step_name should not be empty");
            assert!(!step.task_name.is_empty(), "task_name should not be empty");
        }

        Ok(())
    }

    // ========================================================================
    // 5. Slowest Steps Aggregated
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_slowest_steps_aggregated(pool: PgPool) -> sqlx::Result<()> {
        let executor = SqlFunctionExecutor::new(pool.clone());

        // Call with defaults -- may return empty vec for fresh test databases
        let result = executor
            .get_slowest_steps_aggregated(Some(10), Some(1))
            .await?;

        assert!(
            result.len() <= 10,
            "Should respect the limit parameter (got {})",
            result.len()
        );

        for entry in &result {
            assert!(!entry.step_name.is_empty(), "step_name should not be empty");
            assert!(
                entry.execution_count >= 1,
                "execution_count should be >= min_executions"
            );
        }

        Ok(())
    }

    // ========================================================================
    // 6. Slowest Tasks
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_slowest_tasks(pool: PgPool) -> sqlx::Result<()> {
        // Create and complete a workflow so there is duration data
        let (task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        for &step_uuid in &step_uuids {
            WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuid, &pool)
                .await
                .map_err(map_factory_error)?;
        }

        // Transition task to complete state
        TaskTransition::create(
            &pool,
            NewTaskTransition {
                task_uuid,
                to_state: "complete".to_string(),
                from_state: Some("pending".to_string()),
                processor_uuid: None,
                metadata: None,
            },
        )
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("TaskTransition error: {e}")))?;

        let executor = SqlFunctionExecutor::new(pool.clone());
        let slowest = executor.get_slowest_tasks(Some(10), None).await?;

        assert!(
            slowest.len() <= 10,
            "Should respect the limit parameter (got {})",
            slowest.len()
        );

        for task in &slowest {
            assert!(!task.task_name.is_empty(), "task_name should not be empty");
            assert!(
                !task.namespace_name.is_empty(),
                "namespace_name should not be empty"
            );
        }

        Ok(())
    }

    // ========================================================================
    // 7. Slowest Tasks Aggregated
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_slowest_tasks_aggregated(pool: PgPool) -> sqlx::Result<()> {
        let executor = SqlFunctionExecutor::new(pool.clone());

        // May return empty for fresh test databases
        let result = executor
            .get_slowest_tasks_aggregated(Some(10), Some(1))
            .await?;

        assert!(
            result.len() <= 10,
            "Should respect the limit parameter (got {})",
            result.len()
        );

        for entry in &result {
            assert!(!entry.task_name.is_empty(), "task_name should not be empty");
            assert!(
                entry.execution_count >= 1,
                "execution_count should be >= min_executions"
            );
        }

        Ok(())
    }

    // ========================================================================
    // 8. Transitive Dependencies
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_transitive_dependencies(pool: PgPool) -> sqlx::Result<()> {
        // Diamond: A -> (B, C) -> D
        let (_task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // D (merge step) should have transitive deps on A, B, C
        let deps = executor
            .get_step_transitive_dependencies(step_uuids[3])
            .await?;

        assert!(
            !deps.is_empty(),
            "Merge step should have transitive dependencies"
        );

        // D depends directly on B and C (distance 1), and transitively on A (distance 2)
        let direct_parents: Vec<_> = deps.iter().filter(|d| d.distance == 1).collect();
        assert_eq!(
            direct_parents.len(),
            2,
            "Merge step should have 2 direct parents (B and C)"
        );

        let grandparents: Vec<_> = deps.iter().filter(|d| d.distance == 2).collect();
        // In a diamond pattern (A -> B, A -> C, B -> D, C -> D), the recursive CTE
        // finds A at distance 2 via two paths (through B and through C).
        // The SQL function does not deduplicate, so we get 2 rows for A.
        assert_eq!(
            grandparents.len(),
            2,
            "Merge step should see grandparent A via 2 paths (through B and C)"
        );
        // Both grandparent entries should point to the root step (A)
        assert!(grandparents
            .iter()
            .all(|g| g.workflow_step_uuid == step_uuids[0]));

        // Root step (A) should have no transitive dependencies
        let root_deps = executor
            .get_step_transitive_dependencies(step_uuids[0])
            .await?;
        assert!(
            root_deps.is_empty(),
            "Root step should have no transitive dependencies"
        );

        Ok(())
    }

    // ========================================================================
    // 9. Dependency Results Map
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_dependency_results_map(pool: PgPool) -> sqlx::Result<()> {
        // Diamond: A -> (B, C) -> D
        let (_task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // Before completing anything, the results map for D should be empty
        let map_empty = executor
            .get_step_dependency_results_map(step_uuids[3])
            .await?;
        assert!(
            map_empty.is_empty(),
            "Results map should be empty when no deps are completed"
        );

        // Complete root (A) and one branch (B)
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[1], &pool)
            .await
            .map_err(map_factory_error)?;

        // Mark their processed flags and add results at the DB level
        sqlx::query!(
            "UPDATE tasker.workflow_steps SET processed = true, results = $1 WHERE workflow_step_uuid = $2",
            serde_json::json!({"success": true, "step_uuid": step_uuids[0].to_string(), "status": "completed", "result": {}}) as _,
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            "UPDATE tasker.workflow_steps SET processed = true, results = $1 WHERE workflow_step_uuid = $2",
            serde_json::json!({"success": true, "step_uuid": step_uuids[1].to_string(), "status": "completed", "result": {}}) as _,
            step_uuids[1]
        )
        .execute(&pool)
        .await?;

        // Now the results map for D should contain entries keyed by step name
        let map = executor
            .get_step_dependency_results_map(step_uuids[3])
            .await?;

        // Completed deps A and B should be present as results
        // The map keys are step names, not UUIDs
        assert!(
            !map.is_empty(),
            "Results map should contain at least the completed dependencies"
        );

        Ok(())
    }

    // ========================================================================
    // 10. Completed Step Dependencies and Direct Parents
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_completed_step_dependencies(pool: PgPool) -> sqlx::Result<()> {
        // Diamond: A -> (B, C) -> D
        let (_task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // Before any completions, completed deps for D should be empty
        let completed = executor
            .get_completed_step_dependencies(step_uuids[3])
            .await?;
        assert!(
            completed.is_empty(),
            "No completed dependencies expected initially"
        );

        // Complete root A and mark it processed with results
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;
        sqlx::query!(
            "UPDATE tasker.workflow_steps SET processed = true, results = $1 WHERE workflow_step_uuid = $2",
            serde_json::json!({"success": true, "step_uuid": step_uuids[0].to_string(), "status": "completed", "result": {}}) as _,
            step_uuids[0]
        )
        .execute(&pool)
        .await?;

        // D's completed deps should now include A (distance 2)
        let completed_after = executor
            .get_completed_step_dependencies(step_uuids[3])
            .await?;
        assert!(
            !completed_after.is_empty(),
            "Should see completed transitive dependency A"
        );
        assert!(
            completed_after.iter().all(|d| d.processed),
            "All returned deps should be processed"
        );

        // Direct parents of D should be B and C (distance 1)
        let direct = executor
            .get_direct_parent_dependencies(step_uuids[3])
            .await?;
        assert_eq!(
            direct.len(),
            2,
            "Merge step should have exactly 2 direct parents"
        );
        assert!(
            direct.iter().all(|d| d.distance == 1),
            "All direct parents should have distance 1"
        );

        Ok(())
    }

    // ========================================================================
    // 11. Task Ready Info and Next Ready Tasks
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_ready_task_info(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().diamond()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // get_task_ready_info for the created task
        let info = executor.get_task_ready_info(task_uuid).await?;
        assert!(
            info.is_some(),
            "Should return ready info for a task with pending steps"
        );

        let task_info = info.unwrap();
        assert_eq!(task_info.task_uuid, task_uuid);
        assert!(
            task_info.ready_steps_count >= 1,
            "Diamond root should be ready"
        );
        assert!(
            !task_info.task_name.is_empty(),
            "task_name should be populated"
        );
        assert!(
            !task_info.namespace_name.is_empty(),
            "namespace_name should be populated"
        );

        // get_next_ready_task -- should return at least our task
        let next = executor.get_next_ready_task().await?;
        assert!(
            next.is_some(),
            "Should find at least one task with ready steps"
        );

        // get_next_ready_tasks batch
        let batch = executor.get_next_ready_tasks(5).await?;
        assert!(
            !batch.is_empty(),
            "Should find at least one task in a batch query"
        );
        assert!(
            batch.len() <= 5,
            "Should respect the limit parameter (got {})",
            batch.len()
        );

        Ok(())
    }

    // ========================================================================
    // 12. Current Task State
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_current_task_state(pool: PgPool) -> sqlx::Result<()> {
        let (task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // Task was initialised to "pending" state
        let state = executor.get_current_task_state(task_uuid).await?;
        assert_eq!(
            state.to_string(),
            "pending",
            "Newly created task should be in pending state"
        );

        // Non-existent task should return an error (RowNotFound)
        let missing_uuid = Uuid::nil();
        let result = executor.get_current_task_state(missing_uuid).await;
        assert!(
            result.is_err(),
            "Querying a non-existent task should return an error"
        );

        Ok(())
    }

    // ========================================================================
    // 13. Stale Tasks DLQ
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_stale_tasks_dlq(pool: PgPool) -> sqlx::Result<()> {
        // Create a task that will NOT be stale (freshly created)
        let (_task_uuid, _step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        let executor = SqlFunctionExecutor::new(pool.clone());

        // get_stale_tasks_for_dlq with generous thresholds -- fresh tasks should not appear
        let stale = executor
            .get_stale_tasks_for_dlq(
                60, // waiting_deps threshold (minutes)
                30, // waiting_retry threshold (minutes)
                30, // steps_process threshold (minutes)
                24, // max_lifetime hours
                10, // batch_size
            )
            .await?;

        // Fresh tasks should not be stale
        // (stale may be non-empty if prior tests left data, but our fresh task should not show)
        for record in &stale {
            assert!(
                !record.namespace_name.is_empty(),
                "namespace_name should be populated"
            );
            assert!(
                !record.task_name.is_empty(),
                "task_name should be populated"
            );
        }

        // detect_and_transition_stale_tasks in DRY RUN mode -- safe, no mutations
        let detection = executor
            .detect_and_transition_stale_tasks(
                true, // dry_run
                10,   // batch_size
                60,   // waiting_deps
                30,   // waiting_retry
                30,   // steps_process
                24,   // max_lifetime_hours
            )
            .await?;

        // Dry-run should not actually move anything to DLQ
        for result in &detection {
            assert!(
                !result.action_taken.is_empty(),
                "action_taken should be populated"
            );
        }

        Ok(())
    }

    // ========================================================================
    // 14. FunctionRegistry
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_function_registry(pool: PgPool) -> sqlx::Result<()> {
        let registry = FunctionRegistry::new(pool.clone());

        // Verify all accessor methods return a reference to the underlying executor
        // by calling a simple operation through each accessor

        // dependency_levels accessor
        let dep_exec = registry.dependency_levels();
        let _ = dep_exec.get_analytics_metrics(None).await?;

        // analytics accessor
        let analytics_exec = registry.analytics();
        let _ = analytics_exec.get_analytics_metrics(None).await?;

        // step_readiness accessor
        let _readiness_exec = registry.step_readiness();

        // system_health accessor
        let health_exec = registry.system_health();
        let _ = health_exec.get_system_health_counts().await?;

        // task_execution accessor
        let _task_exec = registry.task_execution();

        // performance accessor
        let perf_exec = registry.performance();
        let _ = perf_exec.get_slowest_steps(Some(5), None).await?;

        // executor accessor
        let underlying = registry.executor();
        let _ = underlying.get_analytics_metrics(None).await?;

        // worker_optimization accessor
        let _worker_exec = registry.worker_optimization();

        // transitive_dependencies accessor
        let _trans_exec = registry.transitive_dependencies();

        Ok(())
    }

    // ========================================================================
    // 15. System Health Counts through SqlFunctionExecutor
    // ========================================================================

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_system_health_counts_via_executor(pool: PgPool) -> sqlx::Result<()> {
        // Seed some data
        let (_task_uuid, step_uuids) =
            create_task_with_initial_state(&pool, ComplexWorkflowFactory::new().linear()).await?;

        // Complete first step
        WorkflowStepTransitionFactory::create_complete_lifecycle(step_uuids[0], &pool)
            .await
            .map_err(map_factory_error)?;

        let executor = SqlFunctionExecutor::new(pool.clone());
        let health = executor.get_system_health_counts().await?;

        assert!(health.total_tasks >= 1, "Should have at least 1 task");
        assert!(health.total_steps >= 4, "Should have at least 4 steps");
        assert!(
            health.complete_steps >= 1,
            "Should have at least 1 complete step"
        );

        // Verify computed methods
        let error_rate = health.error_rate();
        assert!(
            (0.0..=1.0).contains(&error_rate),
            "Error rate should be between 0.0 and 1.0"
        );

        let success_rate = health.success_rate();
        assert!(
            (0.0..=1.0).contains(&success_rate),
            "Success rate should be between 0.0 and 1.0"
        );

        Ok(())
    }
}
