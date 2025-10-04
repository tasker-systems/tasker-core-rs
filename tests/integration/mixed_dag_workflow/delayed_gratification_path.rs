use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test complex DAG workflow with init step retry exhaustion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_init_retry_exhaustion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Init step retry exhaustion");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "mixed_dag_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail dag_init 3 times (retry limit)
    tracing::info!("📊 PHASE 1: Failing dag_init 3 times");
    for attempt in 1..=3 {
        manager
            .fail_step(task_uuid, "dag_init", "Init processing error")
            .await?;
        tracing::info!("Failed dag_init attempt {}/3", attempt);
    }

    // Validate: dag_init in terminal error state, no retry eligibility
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let init_step = step_readiness
        .iter()
        .find(|s| s.name == "dag_init")
        .unwrap();
    assert_eq!(init_step.current_state.as_deref(), Some("error"));
    assert_eq!(init_step.attempts, 3);
    assert!(!init_step.retry_eligible, "Should not be retry eligible");

    // All downstream steps remain pending
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

    tracing::info!("✅ Init retry exhaustion blocks entire DAG");
    tracing::info!("🎯 DELAYED GRATIFICATION: Init retry exhaustion test complete");

    Ok(())
}

/// Test complex DAG workflow with processing step retry and recovery
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_process_retry_recovery(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Processing step retry and recovery");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "mixed_dag_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init
    tracing::info!("📊 PHASE 1: Completing dag_init");
    manager
        .complete_step(task_uuid, "dag_init", serde_json::json!({"result": 4}))
        .await?;

    // **PHASE 2**: Fail dag_process_left twice, succeed on third attempt
    tracing::info!("📊 PHASE 2: Failing dag_process_left twice, then recovering");
    manager
        .fail_step(task_uuid, "dag_process_left", "Transient processing error")
        .await?;
    manager
        .fail_step(task_uuid, "dag_process_left", "Transient processing error")
        .await?;

    // Validate: process_left still retry eligible after 2 failures
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let process_left = step_readiness
        .iter()
        .find(|s| s.name == "dag_process_left")
        .unwrap();
    assert_eq!(process_left.current_state.as_deref(), Some("error"));
    assert_eq!(process_left.attempts, 2);
    assert!(process_left.retry_eligible);

    // **PHASE 3**: Complete dag_process_right (parallel path succeeds)
    tracing::info!("📊 PHASE 3: Completing dag_process_right");
    manager
        .complete_step(
            task_uuid,
            "dag_process_right",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // **PHASE 4**: Recover dag_process_left on third attempt
    tracing::info!("📊 PHASE 4: Recovering dag_process_left");
    manager
        .complete_step(
            task_uuid,
            "dag_process_left",
            serde_json::json!({"result": 16}),
        )
        .await?;

    // Validate: All 3 next steps now ready
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    for step_name in ["dag_validate", "dag_transform", "dag_analyze"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert!(step.dependencies_satisfied);
        assert!(step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 7, 3, 3)
        .await?;

    tracing::info!("✅ Process left recovery unblocks downstream steps");
    tracing::info!("🎯 DELAYED GRATIFICATION: Process retry recovery test complete");

    Ok(())
}

/// Test complex DAG workflow with convergence step retry and backoff
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_convergence_retry_backoff(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Convergence step retry with backoff");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "mixed_dag_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init and both process steps
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

    // **PHASE 2**: Fail dag_validate twice (convergence point failure)
    tracing::info!("📊 PHASE 2: Failing dag_validate twice");
    manager
        .fail_step(task_uuid, "dag_validate", "Validation convergence error")
        .await?;
    manager
        .fail_step(task_uuid, "dag_validate", "Validation convergence error")
        .await?;

    // Validate: SQL shows backoff timing and retry eligibility
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let validate = step_readiness
        .iter()
        .find(|s| s.name == "dag_validate")
        .unwrap();

    assert_eq!(validate.current_state.as_deref(), Some("error"));
    assert_eq!(validate.attempts, 2);
    assert!(validate.retry_eligible);
    assert!(validate.next_retry_at.is_some(), "Should have backoff time");

    // Transform and analyze should still be ready (independent paths)
    for step_name in ["dag_transform", "dag_analyze"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert!(step.dependencies_satisfied);
        assert!(step.ready_for_execution);
    }

    // Finalize blocked by validate failure
    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert!(!finalize.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 7, 3, 2)
        .await?;

    tracing::info!(
        "✅ Convergence failure applies backoff, blocks finalize, independent paths unaffected"
    );
    tracing::info!("🎯 DELAYED GRATIFICATION: Convergence retry backoff test complete");

    Ok(())
}

/// Test complex DAG workflow with mid-DAG permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_mid_dag_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Mid-DAG permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "mixed_dag_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init and both process steps
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

    // **PHASE 2**: Fail dag_transform permanently (3 attempts)
    tracing::info!("📊 PHASE 2: Failing dag_transform permanently");
    for _ in 0..3 {
        manager
            .fail_step(task_uuid, "dag_transform", "Transform processing error")
            .await?;
    }

    // **PHASE 3**: Complete dag_validate and dag_analyze
    tracing::info!("📊 PHASE 3: Completing dag_validate and dag_analyze");
    manager
        .complete_step(
            task_uuid,
            "dag_validate",
            serde_json::json!({"result": 65536}),
        )
        .await?;
    manager
        .complete_step(task_uuid, "dag_analyze", serde_json::json!({"result": 256}))
        .await?;

    // Validate: transform in terminal error, finalize blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let transform = step_readiness
        .iter()
        .find(|s| s.name == "dag_transform")
        .unwrap();
    assert_eq!(transform.current_state.as_deref(), Some("error"));
    assert_eq!(transform.attempts, 3);
    assert!(!transform.retry_eligible);

    // Finalize blocked by transform failure (needs all 3 inputs)
    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert_eq!(finalize.completed_parents, 2); // validate and analyze complete
    assert!(!finalize.dependencies_satisfied); // transform failed

    manager
        .validate_task_execution_context(task_uuid, 7, 5, 0)
        .await?;

    tracing::info!("✅ Transform permanent failure blocks 3-way convergence");
    tracing::info!("🎯 DELAYED GRATIFICATION: Mid-DAG permanent failure test complete");

    Ok(())
}

/// Test complex DAG workflow with transient failure and eventual convergence
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complex_dag_transient_failure_eventual_convergence(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Transient failure eventual convergence");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "complex_dag",
        "mixed_dag_workflow",
        serde_json::json!({"even_number": 2}),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete dag_init and process steps
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

    // **PHASE 2**: Complete transform and analyze, fail validate (transient)
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
    manager
        .fail_step(task_uuid, "dag_validate", "Transient validation error")
        .await?;

    // **VALIDATION 1**: SQL shows failure blocks convergence
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let validate = step_readiness
        .iter()
        .find(|s| s.name == "dag_validate")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("error"));
    assert!(validate.retry_eligible);

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

    tracing::info!("✅ Validate failure blocks finalize convergence");

    // **PHASE 3**: Recover dag_validate
    tracing::info!("📊 PHASE 3: Recovering dag_validate");
    manager
        .complete_step(
            task_uuid,
            "dag_validate",
            serde_json::json!({"result": 65536}),
        )
        .await?;

    // **VALIDATION 2**: SQL shows recovery unblocks convergence
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let finalize = step_readiness
        .iter()
        .find(|s| s.name == "dag_finalize")
        .unwrap();
    assert_eq!(finalize.completed_parents, 3); // all 3 inputs complete
    assert!(finalize.dependencies_satisfied);
    assert!(finalize.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 7, 6, 1)
        .await?;

    tracing::info!("✅ Validate recovery unblocks finalize convergence");

    // **PHASE 4**: Complete dag_finalize
    tracing::info!("📊 PHASE 4: Completing dag_finalize");
    manager
        .complete_step(
            task_uuid,
            "dag_finalize",
            serde_json::json!({"result": 4294967296u64}),
        )
        .await?;

    // **VALIDATION 3**: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 7, 7, 0)
        .await?;

    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;
    for step in &final_readiness {
        assert_eq!(step.current_state.as_deref(), Some("complete"));
    }

    tracing::info!("✅ Transient failure recovered, workflow completed successfully");
    tracing::info!(
        "🎯 DELAYED GRATIFICATION: Transient failure eventual convergence test complete"
    );

    Ok(())
}
