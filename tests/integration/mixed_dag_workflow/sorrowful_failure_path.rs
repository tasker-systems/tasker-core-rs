use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test complex DAG workflow with init permanent failure blocking entire DAG
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_init_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Init permanent failure blocks entire DAG");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail dag_init permanently
    tracing::info!("📊 PHASE 1: Failing dag_init permanently");
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "dag_init", "Init validation error")
            .await?;
    }

    // Validate: init in terminal error, entire DAG blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let init = step_readiness
        .iter()
        .find(|s| s.name == "dag_init")
        .unwrap();
    assert_eq!(init.current_state.as_deref(), Some("error"));
    assert_eq!(init.attempts, 3);
    assert!(!init.retry_eligible);

    // All descendant steps remain pending, never executable
    for step_name in [
        "dag_process_left",
        "dag_process_right",
        "dag_validate",
        "dag_transform",
        "dag_analyze",
        "dag_finalize",
    ] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(step.current_state.as_deref(), Some("pending"));
        assert!(!step.dependencies_satisfied);
        assert!(!step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 7, 0, 0)
        .await?;

    tracing::info!("✅ Init permanent failure permanently blocks entire DAG");
    tracing::info!("🎯 SORROWFUL FAILURE: Init permanent failure test complete");

    Ok(())
}

/// Test complex DAG workflow with processing step permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_process_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Processing step permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init
    tracing::info!("📊 PHASE 1: Completing dag_init");
    manager
        .complete_step(task_uuid, "dag_init", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Complete process_right, fail process_left permanently
    tracing::info!("📊 PHASE 2: Completing process_right, failing process_left");
    manager
        .complete_step(
            task_uuid,
            "dag_process_right",
            serde_json::json!({"result": 16}),
        )
        .await?;
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "dag_process_left", "Processing error")
            .await?;
    }

    // Validate: process_left failed, its downstream steps blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let process_left = step_readiness
        .iter()
        .find(|s| s.name == "dag_process_left")
        .unwrap();
    assert_eq!(process_left.current_state.as_deref(), Some("error"));
    assert!(!process_left.retry_eligible);

    // Validate blocked (needs both process steps)
    let validate = step_readiness
        .iter()
        .find(|s| s.name == "dag_validate")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("pending"));
    assert!(!validate.dependencies_satisfied);

    // Transform blocked (needs process_left)
    let transform = step_readiness
        .iter()
        .find(|s| s.name == "dag_transform")
        .unwrap();
    assert_eq!(transform.current_state.as_deref(), Some("pending"));
    assert!(!transform.dependencies_satisfied);

    // Analyze ready (only needs process_right)
    let analyze = step_readiness
        .iter()
        .find(|s| s.name == "dag_analyze")
        .unwrap();
    assert!(analyze.dependencies_satisfied);
    assert!(analyze.ready_for_execution);

    // Finalize blocked (needs all 3 convergence inputs)
    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert!(!finalize.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 7, 2, 1)
        .await?;

    tracing::info!(
        "✅ Process left failure blocks validate/transform/finalize, analyze unaffected"
    );
    tracing::info!("🎯 SORROWFUL FAILURE: Processing permanent failure test complete");

    Ok(())
}

/// Test complex DAG workflow with convergence step permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_convergence_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Convergence step permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete init and process steps
    tracing::info!("📊 PHASE 1: Completing init and process steps");
    manager
        .complete_step(task_uuid, "dag_init", serde_json::json!({"result": 4}))
        .await?;
    manager
        .complete_step(
            task_uuid,
            "dag_process_left",
            serde_json::json!({"result": 16}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "dag_process_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // **PHASE 2**: Complete transform and analyze, fail validate permanently
    tracing::info!("📊 PHASE 2: Completing transform/analyze, failing validate");
    manager
        .complete_step(
            task_uuid,
            "dag_transform",
            serde_json::json!({"result": 256}),
        )
        .await?;
    manager
        .complete_step(task_uuid, "dag_analyze", serde_json::json!({"result": 256}))
        .await?;
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "dag_validate", "Validation convergence error")
            .await?;
    }

    // Validate: validate in terminal error, finalize permanently blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let validate = step_readiness
        .iter()
        .find(|s| s.name == "dag_validate")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("error"));
    assert!(!validate.retry_eligible);

    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert_eq!(finalize.completed_parents, 2); // transform and analyze
    assert!(!finalize.dependencies_satisfied); // validate failed
    assert!(!finalize.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 7, 5, 0)
        .await?;

    tracing::info!("✅ Validate permanent failure blocks 3-way convergence");
    tracing::info!("🎯 SORROWFUL FAILURE: Convergence permanent failure test complete");

    Ok(())
}

/// Test complex DAG workflow with mixed states and SQL accuracy
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_mixed_states_sql_accuracy(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mixed states SQL accuracy");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "rust_e2e_mixed_dag",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete init
    manager
        .complete_step(task_uuid, "dag_init", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Fail process_left (transient), complete process_right
    manager
        .fail_step(task_uuid, "dag_process_left", "Transient error")
        .await?;
    manager
        .complete_step(
            task_uuid,
            "dag_process_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // **PHASE 3**: Complete analyze (only depends on process_right)
    manager
        .complete_step(task_uuid, "dag_analyze", serde_json::json!({"result": 256}))
        .await?;

    // **VALIDATION**: Complex mixed state verification
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    // Init: complete
    let init = step_readiness
        .iter()
        .find(|s| s.name == "dag_init")
        .unwrap();
    assert_eq!(init.current_state.as_deref(), Some("complete"));

    // Process left: error (with retry eligibility)
    let process_left = step_readiness
        .iter()
        .find(|s| s.name == "dag_process_left")
        .unwrap();
    assert_eq!(process_left.current_state.as_deref(), Some("error"));
    assert!(process_left.retry_eligible);

    // Process right: complete
    let process_right = step_readiness
        .iter()
        .find(|s| s.name == "dag_process_right")
        .unwrap();
    assert_eq!(process_right.current_state.as_deref(), Some("complete"));

    // Validate: pending (blocked by process_left error)
    let validate = step_readiness
        .iter()
        .find(|s| s.name == "dag_validate")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("pending"));
    assert_eq!(validate.completed_parents, 1); // only process_right
    assert!(!validate.dependencies_satisfied);

    // Transform: pending (blocked by process_left error)
    let transform = step_readiness
        .iter()
        .find(|s| s.name == "dag_transform")
        .unwrap();
    assert_eq!(transform.current_state.as_deref(), Some("pending"));
    assert!(!transform.dependencies_satisfied);

    // Analyze: complete
    let analyze = step_readiness
        .iter()
        .find(|s| s.name == "dag_analyze")
        .unwrap();
    assert_eq!(analyze.current_state.as_deref(), Some("complete"));

    // Finalize: pending (blocked by incomplete validate and transform)
    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert_eq!(finalize.current_state.as_deref(), Some("pending"));
    assert_eq!(finalize.completed_parents, 1); // only analyze
    assert!(!finalize.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 7, 3, 0)
        .await?;

    tracing::info!("✅ SQL functions accurately reflect complex mixed state DAG");
    tracing::info!("🎯 SORROWFUL FAILURE: Mixed states SQL accuracy test complete");

    Ok(())
}
