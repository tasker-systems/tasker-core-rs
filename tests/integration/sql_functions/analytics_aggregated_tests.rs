//! Analytics Aggregated SQL Function Tests (TAS-168)
//!
//! Tests for the pre-aggregated analytics SQL functions that power the
//! bottleneck analysis endpoints. These tests verify:
//!
//! - Correct aggregation by template identity (namespace, task_name, version)
//! - Step disambiguation within template context
//! - Proper calculation of duration, error rates, and execution counts

use anyhow::Result;
use bigdecimal::BigDecimal;
use sqlx::PgPool;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test that get_slowest_steps_aggregated returns data with correct structure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_slowest_steps_aggregated_structure(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: get_slowest_steps_aggregated structure validation");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create and complete multiple tasks to generate execution data
    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_one",
            "test_sql",
            serde_json::json!({"iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        // Complete the step to create execution data
        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "success"}),
            )
            .await?;

        // Complete the task to finalize
        manager.complete_task(task_uuid).await?;
    }

    // Query the aggregated function
    let executor = SqlFunctionExecutor::new(pool);
    let results = executor
        .get_slowest_steps_aggregated(Some(10), Some(5))
        .await?;

    tracing::info!(
        result_count = results.len(),
        "📊 get_slowest_steps_aggregated returned results"
    );

    // Should have at least one result (our test step)
    assert!(
        !results.is_empty(),
        "Should have results after completing 6 tasks (min_executions=5)"
    );

    // Verify structure of first result
    let first = &results[0];
    assert!(
        !first.namespace_name.is_empty(),
        "namespace_name should be populated"
    );
    assert!(!first.task_name.is_empty(), "task_name should be populated");
    assert!(!first.version.is_empty(), "version should be populated");
    assert!(!first.step_name.is_empty(), "step_name should be populated");
    assert!(
        first.execution_count >= 5,
        "Should have at least 5 executions"
    );
    assert!(
        first.average_duration_seconds >= BigDecimal::from(0),
        "Duration should be non-negative"
    );

    tracing::info!(
        namespace = %first.namespace_name,
        task = %first.task_name,
        version = %first.version,
        step = %first.step_name,
        executions = first.execution_count,
        "✅ Structure validated for first result"
    );

    tracing::info!("✅ TEST PASSED: get_slowest_steps_aggregated structure");
    Ok(())
}

/// Test that get_slowest_tasks_aggregated returns data with correct structure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_slowest_tasks_aggregated_structure(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: get_slowest_tasks_aggregated structure validation");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create and complete multiple tasks to generate execution data
    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_one",
            "test_sql",
            serde_json::json!({"iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        // Complete the step
        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "success"}),
            )
            .await?;

        // Complete the task
        manager.complete_task(task_uuid).await?;
    }

    // Query the aggregated function
    let executor = SqlFunctionExecutor::new(pool);
    let results = executor
        .get_slowest_tasks_aggregated(Some(10), Some(5))
        .await?;

    tracing::info!(
        result_count = results.len(),
        "📊 get_slowest_tasks_aggregated returned results"
    );

    // Should have at least one result
    assert!(
        !results.is_empty(),
        "Should have results after completing 6 tasks (min_executions=5)"
    );

    // Verify structure of first result
    let first = &results[0];
    assert!(
        !first.namespace_name.is_empty(),
        "namespace_name should be populated"
    );
    assert!(!first.task_name.is_empty(), "task_name should be populated");
    assert!(!first.version.is_empty(), "version should be populated");
    assert!(
        first.execution_count >= 5,
        "Should have at least 5 executions"
    );
    assert!(
        first.average_step_count >= BigDecimal::from(1),
        "Should have at least 1 step per task"
    );

    tracing::info!(
        namespace = %first.namespace_name,
        task = %first.task_name,
        version = %first.version,
        executions = first.execution_count,
        avg_steps = %first.average_step_count,
        "✅ Structure validated for first result"
    );

    tracing::info!("✅ TEST PASSED: get_slowest_tasks_aggregated structure");
    Ok(())
}

/// Test that error counts are properly tracked in step aggregation
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_aggregation_error_tracking(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: Step aggregation error tracking");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create tasks - some succeed, some fail
    // Use max_attempts_three so we can have failures without exhausting retries
    for i in 0..8 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_three",
            "test_sql",
            serde_json::json!({"iteration": i, "fail": i % 3 == 0}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        if i % 3 == 0 {
            // Fail every third task's step
            manager
                .fail_step(task_uuid, "boundary_test_step", "Intentional test failure")
                .await?;
        } else {
            // Complete the rest
            manager
                .complete_step(
                    task_uuid,
                    "boundary_test_step",
                    serde_json::json!({"result": "success"}),
                )
                .await?;
        }

        manager.complete_task(task_uuid).await?;
    }

    // Query the aggregated function
    let executor = SqlFunctionExecutor::new(pool);
    let results = executor
        .get_slowest_steps_aggregated(Some(10), Some(5))
        .await?;

    // Find our test step
    let test_step = results.iter().find(|r| {
        r.step_name == "boundary_test_step" && r.task_name == "boundary_max_attempts_three"
    });

    assert!(
        test_step.is_some(),
        "Should find boundary_test_step for boundary_max_attempts_three"
    );

    let step = test_step.unwrap();
    tracing::info!(
        step = %step.step_name,
        executions = step.execution_count,
        errors = step.error_count,
        error_rate = %step.error_rate,
        "📊 Error tracking results"
    );

    // Should have some errors tracked (we failed every 3rd task = ~3 errors in 8 tasks)
    assert!(
        step.error_count >= 1,
        "Should have at least 1 error tracked"
    );

    // Error rate should be non-zero
    assert!(
        step.error_rate > BigDecimal::from(0),
        "Error rate should be non-zero"
    );

    tracing::info!("✅ TEST PASSED: Step aggregation error tracking");
    Ok(())
}

/// Test that min_executions filter works correctly
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_min_executions_filter(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: min_executions filter");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create only 3 tasks (below default min_executions of 5)
    for i in 0..3 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_one",
            "test_sql",
            serde_json::json!({"iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "success"}),
            )
            .await?;

        manager.complete_task(task_uuid).await?;
    }

    let executor = SqlFunctionExecutor::new(pool);

    // With min_executions=5, should get no results for our step
    let results_high_min = executor
        .get_slowest_steps_aggregated(Some(10), Some(5))
        .await?;

    let our_step_high = results_high_min.iter().find(|r| {
        r.step_name == "boundary_test_step" && r.task_name == "boundary_max_attempts_one"
    });

    assert!(
        our_step_high.is_none(),
        "Should NOT find step when below min_executions threshold"
    );

    // With min_executions=2, should get results
    let results_low_min = executor
        .get_slowest_steps_aggregated(Some(10), Some(2))
        .await?;

    let our_step_low = results_low_min.iter().find(|r| {
        r.step_name == "boundary_test_step" && r.task_name == "boundary_max_attempts_one"
    });

    assert!(
        our_step_low.is_some(),
        "Should find step when above min_executions threshold"
    );

    tracing::info!("✅ TEST PASSED: min_executions filter");
    Ok(())
}

/// Test that different templates with same step names are properly distinguished
///
/// This is the critical test for TAS-168: step names must be aggregated within
/// their template context (namespace, task_name, version), not globally.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_name_disambiguation_by_template(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: Step name disambiguation by template context");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create tasks from two different templates that both have "boundary_test_step"
    // boundary_max_attempts_one and boundary_max_attempts_three both have this step

    // 6 executions of boundary_max_attempts_one
    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_one",
            "test_sql",
            serde_json::json!({"template": "one", "iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "from_one"}),
            )
            .await?;

        manager.complete_task(task_uuid).await?;
    }

    // 6 executions of boundary_max_attempts_three
    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_three",
            "test_sql",
            serde_json::json!({"template": "three", "iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "from_three"}),
            )
            .await?;

        manager.complete_task(task_uuid).await?;
    }

    let executor = SqlFunctionExecutor::new(pool);
    let results = executor
        .get_slowest_steps_aggregated(Some(20), Some(5))
        .await?;

    // Find steps named "boundary_test_step"
    let boundary_steps: Vec<_> = results
        .iter()
        .filter(|r| r.step_name == "boundary_test_step")
        .collect();

    tracing::info!(
        boundary_step_count = boundary_steps.len(),
        "📊 Found distinct boundary_test_step entries"
    );

    // Should have TWO distinct entries - one for each template
    assert!(
        boundary_steps.len() >= 2,
        "Should have at least 2 distinct entries for same step name in different templates, got {}",
        boundary_steps.len()
    );

    // Verify they're from different templates
    let template_names: std::collections::HashSet<_> = boundary_steps
        .iter()
        .map(|s| s.task_name.as_str())
        .collect();

    assert!(
        template_names.contains("boundary_max_attempts_one"),
        "Should have entry from boundary_max_attempts_one"
    );
    assert!(
        template_names.contains("boundary_max_attempts_three"),
        "Should have entry from boundary_max_attempts_three"
    );

    // Each should have ~6 executions (not 12 combined!)
    for step in &boundary_steps {
        tracing::info!(
            task = %step.task_name,
            step = %step.step_name,
            executions = step.execution_count,
            "📊 Step entry"
        );

        assert!(
            step.execution_count >= 5 && step.execution_count <= 7,
            "Each template's step should have ~6 executions, not combined. Got {} for {}",
            step.execution_count,
            step.task_name
        );
    }

    tracing::info!("✅ TEST PASSED: Step names properly disambiguated by template context");
    Ok(())
}

/// Test that task aggregation groups correctly by template identity
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_aggregation_by_template_identity(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: Task aggregation by template identity");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    // Create tasks from two different templates
    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_one",
            "test_sql",
            serde_json::json!({"template": "one", "iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "success"}),
            )
            .await?;

        manager.complete_task(task_uuid).await?;
    }

    for i in 0..6 {
        let task_request = manager.create_task_request_for_template(
            "boundary_max_attempts_three",
            "test_sql",
            serde_json::json!({"template": "three", "iteration": i}),
        );

        let init_result = manager.initialize_task(task_request).await?;
        let task_uuid = init_result.task_uuid;

        manager
            .complete_step(
                task_uuid,
                "boundary_test_step",
                serde_json::json!({"result": "success"}),
            )
            .await?;

        manager.complete_task(task_uuid).await?;
    }

    let executor = SqlFunctionExecutor::new(pool);
    let results = executor
        .get_slowest_tasks_aggregated(Some(20), Some(5))
        .await?;

    // Find our test templates
    let one_result = results
        .iter()
        .find(|r| r.task_name == "boundary_max_attempts_one");
    let three_result = results
        .iter()
        .find(|r| r.task_name == "boundary_max_attempts_three");

    assert!(
        one_result.is_some(),
        "Should find boundary_max_attempts_one template"
    );
    assert!(
        three_result.is_some(),
        "Should find boundary_max_attempts_three template"
    );

    // Each should have ~6 executions (not 12 combined)
    let one = one_result.unwrap();
    let three = three_result.unwrap();

    tracing::info!(
        one_task = %one.task_name,
        one_executions = one.execution_count,
        three_task = %three.task_name,
        three_executions = three.execution_count,
        "📊 Task aggregation results"
    );

    assert!(
        one.execution_count >= 5 && one.execution_count <= 7,
        "boundary_max_attempts_one should have ~6 executions, got {}",
        one.execution_count
    );
    assert!(
        three.execution_count >= 5 && three.execution_count <= 7,
        "boundary_max_attempts_three should have ~6 executions, got {}",
        three.execution_count
    );

    tracing::info!("✅ TEST PASSED: Task aggregation by template identity");
    Ok(())
}
