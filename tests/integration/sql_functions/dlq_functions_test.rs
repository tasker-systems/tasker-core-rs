//! Integration tests for DLQ SQL functions (TAS-49)
//!
//! Tests the complete DLQ detection and transition workflow including:
//! - Helper functions (threshold calculation, DLQ entry creation, state transitions)
//! - Discovery function (get_stale_tasks_for_dlq)
//! - Main detection function (detect_and_transition_stale_tasks)
//! - Complete workflow with dry run and real execution

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::orchestration::StalenessAction;

/// Helper to create test namespace
async fn create_test_namespace(
    pool: &PgPool,
    namespace_uuid: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO tasker_task_namespaces (task_namespace_uuid, name, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        ON CONFLICT (task_namespace_uuid) DO NOTHING
        "#,
        namespace_uuid,
        name
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Helper to create test named task with lifecycle config
async fn create_test_named_task(
    pool: &PgPool,
    named_task_uuid: Uuid,
    namespace_uuid: Uuid,
    name: &str,
    lifecycle_config: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO tasker_named_tasks (
            named_task_uuid,
            task_namespace_uuid,
            name,
            version,
            configuration,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, 1, $4, NOW(), NOW())
        ON CONFLICT (named_task_uuid) DO NOTHING
        "#,
        named_task_uuid,
        namespace_uuid,
        name,
        lifecycle_config
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Helper to create stale task in specific state
async fn create_stale_task(
    pool: &PgPool,
    task_uuid: Uuid,
    named_task_uuid: Uuid,
    state: &str,
    minutes_in_state: i32,
) -> Result<(), sqlx::Error> {
    let correlation_id = Uuid::now_v7();
    let task_created = Utc::now() - chrono::Duration::minutes(minutes_in_state as i64 + 60);
    let state_created = Utc::now() - chrono::Duration::minutes(minutes_in_state as i64);

    // Create task
    sqlx::query!(
        r#"
        INSERT INTO tasker_tasks (
            task_uuid,
            named_task_uuid,
            correlation_id,
            priority,
            complete,
            identity_hash,
            requested_at,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, 0, false, $4, $5, $5, $5)
        "#,
        task_uuid,
        named_task_uuid,
        correlation_id,
        format!("{}-{}", named_task_uuid, task_uuid),  // Simple identity hash for tests
        task_created.naive_utc()
    )
    .execute(pool)
    .await?;

    // Create state transition
    sqlx::query!(
        r#"
        INSERT INTO tasker_task_transitions (
            task_uuid,
            from_state,
            to_state,
            sort_key,
            most_recent,
            created_at,
            updated_at
        )
        VALUES ($1, 'pending', $2, 1, true, $3, $3)
        "#,
        task_uuid,
        state,
        state_created.naive_utc()
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ============================================================================
// HELPER FUNCTION TESTS
// ============================================================================

#[sqlx::test]
async fn test_calculate_staleness_threshold_with_template_override(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Test with template override (120 minutes)
    let config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 120
        }
    });

    let threshold = executor
        .calculate_staleness_threshold("waiting_for_dependencies", config, 60, 30, 30)
        .await?;

    assert_eq!(threshold, 120, "Should use template override");
    Ok(())
}

#[sqlx::test]
async fn test_calculate_staleness_threshold_with_defaults(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Test with no template override (should use default)
    let config = json!({});

    let threshold = executor
        .calculate_staleness_threshold("waiting_for_dependencies", config, 60, 30, 30)
        .await?;

    assert_eq!(threshold, 60, "Should use default when no override");
    Ok(())
}

#[sqlx::test]
async fn test_calculate_staleness_threshold_unknown_state(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    let config = json!({});

    let threshold = executor
        .calculate_staleness_threshold("unknown_state", config, 60, 30, 30)
        .await?;

    assert_eq!(threshold, 1440, "Unknown states should return 24 hours");
    Ok(())
}

#[sqlx::test]
async fn test_create_dlq_entry(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Create DLQ entry
    let dlq_uuid = executor
        .create_dlq_entry(
            task_uuid,
            "test_namespace",
            "test_task",
            "waiting_for_dependencies",
            120,
            60,
            None, // defaults to staleness_timeout
        )
        .await?;

    assert!(dlq_uuid.is_some(), "DLQ entry should be created");

    // Verify entry exists in database
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM tasker_tasks_dlq WHERE dlq_entry_uuid = $1
        "#,
    )
    .bind(dlq_uuid.unwrap())
    .fetch_one(&pool)
    .await?;

    assert_eq!(count, 1, "DLQ entry should exist in database");
    Ok(())
}

#[sqlx::test]
async fn test_create_dlq_entry_duplicate_prevention(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Create first DLQ entry (should succeed)
    let first_uuid = executor
        .create_dlq_entry(
            task_uuid,
            "test_namespace",
            "test_task",
            "waiting_for_dependencies",
            120,
            60,
            None,
        )
        .await?;

    assert!(first_uuid.is_some(), "First DLQ entry should succeed");

    // Try to create duplicate (should return None due to unique constraint)
    let second_uuid = executor
        .create_dlq_entry(
            task_uuid,
            "test_namespace",
            "test_task",
            "waiting_for_dependencies",
            120,
            60,
            None,
        )
        .await?;

    assert!(
        second_uuid.is_none(),
        "Duplicate DLQ entry should be prevented"
    );

    Ok(())
}

#[sqlx::test]
async fn test_transition_stale_task_to_error(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Transition to error
    let success = executor
        .transition_stale_task_to_error(
            task_uuid,
            "waiting_for_dependencies",
            "test_namespace",
            "test_task",
        )
        .await?;

    assert!(success, "Transition should succeed");

    // Verify task is in error state
    let current_state = sqlx::query_scalar::<_, String>(
        r#"
        SELECT to_state FROM tasker_task_transitions
        WHERE task_uuid = $1 AND most_recent = true
        "#,
    )
    .bind(task_uuid)
    .fetch_one(&pool)
    .await?;

    assert_eq!(current_state, "error", "Task should be in error state");
    Ok(())
}

// ============================================================================
// DISCOVERY FUNCTION TESTS
// ============================================================================

#[sqlx::test]
async fn test_get_stale_tasks_for_dlq_discovery(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data - create 3 stale tasks
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;

    // Task 1: Stale in waiting_for_dependencies (120 min > 60 min threshold)
    let task1 = Uuid::now_v7();
    create_stale_task(&pool, task1, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Task 2: Stale in waiting_for_retry (60 min > 30 min threshold)
    let task2 = Uuid::now_v7();
    create_stale_task(&pool, task2, named_task_uuid, "waiting_for_retry", 60).await?;

    // Task 3: NOT stale (20 min < 60 min threshold)
    let task3 = Uuid::now_v7();
    create_stale_task(&pool, task3, named_task_uuid, "waiting_for_dependencies", 20).await?;

    // Discover stale tasks
    let stale_tasks = executor
        .get_stale_tasks_for_dlq(60, 30, 30, 72, 100)
        .await?;

    assert_eq!(
        stale_tasks.len(),
        2,
        "Should discover exactly 2 stale tasks"
    );

    // Verify stale tasks are correct ones
    let stale_uuids: Vec<Uuid> = stale_tasks.iter().map(|t| t.task_uuid).collect();
    assert!(stale_uuids.contains(&task1), "Task 1 should be stale");
    assert!(stale_uuids.contains(&task2), "Task 2 should be stale");
    assert!(!stale_uuids.contains(&task3), "Task 3 should not be stale");

    Ok(())
}

#[sqlx::test]
async fn test_get_stale_tasks_for_dlq_excludes_dlq_tasks(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Task should be discovered initially
    let stale_tasks_before = executor
        .get_stale_tasks_for_dlq(60, 30, 30, 72, 100)
        .await?;
    assert_eq!(stale_tasks_before.len(), 1, "Should find 1 stale task");

    // Create DLQ entry
    executor
        .create_dlq_entry(
            task_uuid,
            "test_namespace",
            "test_task",
            "waiting_for_dependencies",
            120,
            60,
            None,
        )
        .await?;

    // Task should NOT be discovered after DLQ entry (anti-join)
    let stale_tasks_after = executor
        .get_stale_tasks_for_dlq(60, 30, 30, 72, 100)
        .await?;
    assert_eq!(
        stale_tasks_after.len(),
        0,
        "Should not find tasks already in DLQ"
    );

    Ok(())
}

// ============================================================================
// MAIN DETECTION FUNCTION TESTS
// ============================================================================

#[sqlx::test]
async fn test_detect_and_transition_stale_tasks_dry_run(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Run detection in dry run mode
    let results = executor
        .detect_and_transition_stale_tasks(
            true, // dry_run
            10, 60, 30, 30, 24,
        )
        .await?;

    assert_eq!(results.len(), 1, "Should detect 1 stale task");

    let result = &results[0];
    assert_eq!(result.task_uuid, task_uuid);
    assert_eq!(
        result.action_taken, "would_transition_to_dlq_and_error",
        "Dry run should report would_transition"
    );
    assert!(!result.moved_to_dlq, "Dry run should not create DLQ entry");
    assert!(
        !result.transition_success,
        "Dry run should not transition state"
    );

    // Verify no DLQ entry was created
    let dlq_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE task_uuid = $1",
    )
    .bind(task_uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(dlq_count, 0, "Dry run should not create DLQ entries");

    // Verify task state unchanged
    let state = sqlx::query_scalar::<_, String>(
        "SELECT to_state FROM tasker_task_transitions WHERE task_uuid = $1 AND most_recent = true",
    )
    .bind(task_uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        state, "waiting_for_dependencies",
        "Dry run should not change state"
    );

    Ok(())
}

#[sqlx::test]
async fn test_detect_and_transition_stale_tasks_real_execution(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120).await?;

    // Run detection with real execution
    let results = executor
        .detect_and_transition_stale_tasks(
            false, // NOT dry_run
            10, 60, 30, 30, 24,
        )
        .await?;

    assert_eq!(results.len(), 1, "Should detect and process 1 stale task");

    let result = &results[0];
    assert_eq!(result.task_uuid, task_uuid);
    assert_eq!(
        result.action_taken, "transitioned_to_dlq_and_error",
        "Should successfully transition"
    );
    assert!(result.moved_to_dlq, "Should create DLQ entry");
    assert!(result.transition_success, "Should transition state");

    // Verify DLQ entry was created
    let dlq_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE task_uuid = $1 AND resolution_status = 'pending'",
    )
    .bind(task_uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(dlq_count, 1, "Should create DLQ entry");

    // Verify task transitioned to error
    let state = sqlx::query_scalar::<_, String>(
        "SELECT to_state FROM tasker_task_transitions WHERE task_uuid = $1 AND most_recent = true",
    )
    .bind(task_uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(state, "error", "Should transition to error state");

    Ok(())
}

#[sqlx::test]
async fn test_detect_and_transition_batch_size_limit(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data - create 5 stale tasks
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", json!({})).await?;

    for _ in 0..5 {
        let task_uuid = Uuid::now_v7();
        create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 120)
            .await?;
    }

    // Run detection with batch_size=2
    let results = executor
        .detect_and_transition_stale_tasks(
            true, // dry_run
            2,    // batch_size=2
            60, 30, 30, 24,
        )
        .await?;

    assert_eq!(
        results.len(),
        2,
        "Should respect batch_size limit of 2 tasks"
    );

    Ok(())
}

#[sqlx::test]
async fn test_detect_and_transition_with_template_thresholds(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data with custom template thresholds
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    let config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 30  // Lower than default 60
        }
    });

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(&pool, named_task_uuid, namespace_uuid, "test_task", config).await?;

    // Task is 45 minutes in state - stale with template threshold (30), even with higher default (60)
    // Template thresholds OVERRIDE defaults via COALESCE
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 45).await?;

    // Should be detected because template threshold (30) takes precedence over default (60)
    let results_with_template = executor
        .detect_and_transition_stale_tasks(
            true, // dry_run
            10, 60, 30, 30, 24,  // default=60, but template=30 wins
        )
        .await?;
    assert_eq!(
        results_with_template.len(),
        1,
        "Should detect using template threshold (30 min) not default (60 min)"
    );

    // Create another task at 25 minutes - NOT stale even with template threshold
    let task_uuid2 = Uuid::now_v7();
    create_stale_task(&pool, task_uuid2, named_task_uuid, "waiting_for_dependencies", 25).await?;

    // Should not detect task at 25 minutes (< 30 minute template threshold)
    let results_below_threshold = executor
        .detect_and_transition_stale_tasks(
            true, // dry_run
            10, 60, 30, 30, 24,
        )
        .await?;
    assert_eq!(
        results_below_threshold.len(),
        1,
        "Should only detect the 45-minute task, not the 25-minute task"
    );

    Ok(())
}

// ============================================================================
// STALENESS ACTION TYPE SAFETY TESTS
// ============================================================================

#[test]
fn test_staleness_action_from_str() {
    assert_eq!(
        StalenessAction::from_str("would_transition_to_dlq_and_error"),
        StalenessAction::WouldTransitionToDlqAndError
    );
    assert_eq!(
        StalenessAction::from_str("transitioned_to_dlq_and_error"),
        StalenessAction::TransitionedToDlqAndError
    );
    assert_eq!(
        StalenessAction::from_str("moved_to_dlq_only"),
        StalenessAction::MovedToDlqOnly
    );
    assert_eq!(
        StalenessAction::from_str("transitioned_to_error_only"),
        StalenessAction::TransitionedToErrorOnly
    );
    assert_eq!(
        StalenessAction::from_str("unknown_action"),
        StalenessAction::TransitionFailed,
        "Unknown strings should be treated as failures"
    );
}

#[test]
fn test_staleness_action_helpers() {
    let success = StalenessAction::TransitionedToDlqAndError;
    assert!(!success.is_failure());
    assert!(success.dlq_created());
    assert!(success.transition_succeeded());

    let partial_failure = StalenessAction::MovedToDlqOnly;
    assert!(partial_failure.is_failure());
    assert!(partial_failure.dlq_created());
    assert!(!partial_failure.transition_succeeded());

    let complete_failure = StalenessAction::TransitionFailed;
    assert!(complete_failure.is_failure());
    assert!(!complete_failure.dlq_created());
    assert!(!complete_failure.transition_succeeded());
}
