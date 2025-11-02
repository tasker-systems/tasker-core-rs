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
use tasker_shared::models::orchestration::{
    DlqEntry, DlqReason, StalenessAction, StalenessHealthStatus,
};

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
        format!("{}-{}", named_task_uuid, task_uuid), // Simple identity hash for tests
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
async fn test_calculate_staleness_threshold_with_defaults(pool: PgPool) -> Result<(), sqlx::Error> {
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
async fn test_calculate_staleness_threshold_unknown_state(pool: PgPool) -> Result<(), sqlx::Error> {
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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;

    // Task 1: Stale in waiting_for_dependencies (120 min > 60 min threshold)
    let task1 = Uuid::now_v7();
    create_stale_task(
        &pool,
        task1,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

    // Task 2: Stale in waiting_for_retry (60 min > 30 min threshold)
    let task2 = Uuid::now_v7();
    create_stale_task(&pool, task2, named_task_uuid, "waiting_for_retry", 60).await?;

    // Task 3: NOT stale (20 min < 60 min threshold)
    let task3 = Uuid::now_v7();
    create_stale_task(
        &pool,
        task3,
        named_task_uuid,
        "waiting_for_dependencies",
        20,
    )
    .await?;

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
async fn test_get_stale_tasks_for_dlq_excludes_dlq_tasks(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
async fn test_detect_and_transition_stale_tasks_dry_run(pool: PgPool) -> Result<(), sqlx::Error> {
    let executor = SqlFunctionExecutor::new(pool.clone());

    // Setup test data
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
    let dlq_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasker_tasks_dlq WHERE task_uuid = $1")
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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        120,
    )
    .await?;

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
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;

    for _ in 0..5 {
        let task_uuid = Uuid::now_v7();
        create_stale_task(
            &pool,
            task_uuid,
            named_task_uuid,
            "waiting_for_dependencies",
            120,
        )
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
    create_stale_task(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        45,
    )
    .await?;

    // Should be detected because template threshold (30) takes precedence over default (60)
    let results_with_template = executor
        .detect_and_transition_stale_tasks(
            true, // dry_run
            10, 60, 30, 30, 24, // default=60, but template=30 wins
        )
        .await?;
    assert_eq!(
        results_with_template.len(),
        1,
        "Should detect using template threshold (30 min) not default (60 min)"
    );

    // Create another task at 25 minutes - NOT stale even with template threshold
    let task_uuid2 = Uuid::now_v7();
    create_stale_task(
        &pool,
        task_uuid2,
        named_task_uuid,
        "waiting_for_dependencies",
        25,
    )
    .await?;

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
    // Test FromStr trait implementation
    assert_eq!(
        "would_transition_to_dlq_and_error"
            .parse::<StalenessAction>()
            .unwrap(),
        StalenessAction::WouldTransitionToDlqAndError
    );
    assert_eq!(
        "transitioned_to_dlq_and_error"
            .parse::<StalenessAction>()
            .unwrap(),
        StalenessAction::TransitionedToDlqAndError
    );
    assert_eq!(
        "moved_to_dlq_only".parse::<StalenessAction>().unwrap(),
        StalenessAction::MovedToDlqOnly
    );
    assert_eq!(
        "transitioned_to_error_only"
            .parse::<StalenessAction>()
            .unwrap(),
        StalenessAction::TransitionedToErrorOnly
    );
    assert_eq!(
        "unknown_action".parse::<StalenessAction>().unwrap(),
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

// ============================================================================
// VIEW INTEGRATION TESTS
// ============================================================================

#[sqlx::test]
async fn test_dlq_stats_view_integration(pool: PgPool) -> Result<(), sqlx::Error> {
    // Setup: Create test namespace and tasks
    let namespace_uuid = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
    let named_task_uuid = Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;

    // Create multiple DLQ entries with different statuses and reasons
    let entries = vec![
        // Staleness timeout entries (3 total: 2 pending, 1 resolved)
        (
            Uuid::new_v4(),
            "waiting_for_dependencies",
            "staleness_timeout",
            "pending",
        ),
        (
            Uuid::new_v4(),
            "waiting_for_retry",
            "staleness_timeout",
            "pending",
        ),
        (
            Uuid::new_v4(),
            "steps_in_process",
            "staleness_timeout",
            "manually_resolved",
        ),
        // Max retries exceeded entries (2 total: 1 pending, 1 cancelled)
        (
            Uuid::new_v4(),
            "waiting_for_retry",
            "max_retries_exceeded",
            "pending",
        ),
        (
            Uuid::new_v4(),
            "steps_in_process",
            "max_retries_exceeded",
            "cancelled",
        ),
        // Worker unavailable entry (1 total: 1 permanently failed)
        (
            Uuid::new_v4(),
            "steps_in_process",
            "worker_unavailable",
            "permanently_failed",
        ),
    ];

    for (task_uuid, state, reason, resolution_status) in &entries {
        // Create task
        create_stale_task(&pool, *task_uuid, named_task_uuid, state, 120).await?;

        // Create DLQ entry
        sqlx::query!(
            r#"
            INSERT INTO tasker_tasks_dlq (
                dlq_entry_uuid, task_uuid, original_state, dlq_reason,
                dlq_timestamp, resolution_status, task_snapshot, created_at, updated_at
            ) VALUES ($1, $2, $3, $4::text::dlq_reason, NOW() - INTERVAL '1 hour',
                     $5::text::dlq_resolution_status, '{}'::jsonb, NOW(), NOW())
            "#,
            Uuid::new_v4(),
            task_uuid,
            state,
            reason,
            resolution_status
        )
        .execute(&pool)
        .await?;
    }

    // Query using DlqEntry::get_stats() which uses v_dlq_dashboard view
    let stats = DlqEntry::get_stats(&pool).await?;

    // Verify stats are returned and properly aggregated
    assert_eq!(
        stats.len(),
        3,
        "Should have stats for 3 different DLQ reasons"
    );

    // Find staleness_timeout stats
    let staleness_stats = stats
        .iter()
        .find(|s| matches!(s.dlq_reason, DlqReason::StalenessTimeout))
        .expect("Should have staleness_timeout stats");

    assert_eq!(
        staleness_stats.total_entries, 3,
        "Should have 3 total staleness entries"
    );
    assert_eq!(staleness_stats.pending, 2, "Should have 2 pending");
    assert_eq!(
        staleness_stats.manually_resolved, 1,
        "Should have 1 manually resolved"
    );
    assert_eq!(
        staleness_stats.permanent_failures, 0,
        "Should have 0 permanent failures"
    );
    assert_eq!(staleness_stats.cancelled, 0, "Should have 0 cancelled");
    assert!(
        staleness_stats.oldest_entry.is_some(),
        "Should have oldest_entry"
    );
    assert!(
        staleness_stats.newest_entry.is_some(),
        "Should have newest_entry"
    );
    assert!(
        staleness_stats.avg_resolution_time_minutes.is_some(),
        "Should have avg_resolution_time_minutes"
    );

    // Find max_retries_exceeded stats
    let retries_stats = stats
        .iter()
        .find(|s| matches!(s.dlq_reason, DlqReason::MaxRetriesExceeded))
        .expect("Should have max_retries_exceeded stats");

    assert_eq!(
        retries_stats.total_entries, 2,
        "Should have 2 total retry entries"
    );
    assert_eq!(retries_stats.pending, 1, "Should have 1 pending");
    assert_eq!(retries_stats.cancelled, 1, "Should have 1 cancelled");

    // Find worker_unavailable stats
    let worker_stats = stats
        .iter()
        .find(|s| matches!(s.dlq_reason, DlqReason::WorkerUnavailable))
        .expect("Should have worker_unavailable stats");

    assert_eq!(
        worker_stats.total_entries, 1,
        "Should have 1 total worker entry"
    );
    assert_eq!(
        worker_stats.permanent_failures, 1,
        "Should have 1 permanent failure"
    );

    Ok(())
}

#[sqlx::test]
async fn test_investigation_queue_view_integration(pool: PgPool) -> Result<(), sqlx::Error> {
    // Setup: Create test namespace and tasks
    let namespace_uuid = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
    let named_task_uuid = Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;

    // Create DLQ entries with different reasons and ages
    let entries = vec![
        // Dependency cycle (highest priority reason)
        (
            Uuid::new_v4(),
            "waiting_for_dependencies",
            "dependency_cycle_detected",
            "pending",
            120,
        ),
        // Max retries (high priority reason, older)
        (
            Uuid::new_v4(),
            "waiting_for_retry",
            "max_retries_exceeded",
            "pending",
            180,
        ),
        // Max retries (high priority reason, newer)
        (
            Uuid::new_v4(),
            "steps_in_process",
            "max_retries_exceeded",
            "pending",
            30,
        ),
        // Staleness timeout (medium priority)
        (
            Uuid::new_v4(),
            "waiting_for_dependencies",
            "staleness_timeout",
            "pending",
            90,
        ),
        // Worker unavailable (low priority)
        (
            Uuid::new_v4(),
            "steps_in_process",
            "worker_unavailable",
            "pending",
            60,
        ),
        // Resolved entry (should be excluded from queue)
        (
            Uuid::new_v4(),
            "steps_in_process",
            "staleness_timeout",
            "manually_resolved",
            150,
        ),
    ];

    for (task_uuid, state, reason, resolution_status, minutes_ago) in &entries {
        // Create task
        create_stale_task(&pool, *task_uuid, named_task_uuid, state, 120).await?;

        // Create DLQ entry with specific timestamp
        sqlx::query!(
            r#"
            INSERT INTO tasker_tasks_dlq (
                dlq_entry_uuid, task_uuid, original_state, dlq_reason,
                dlq_timestamp, resolution_status,
                task_snapshot, created_at, updated_at
            ) VALUES ($1, $2, $3::text, $4::text::dlq_reason,
                     NOW() - make_interval(mins => $5::integer),
                     $6::text::dlq_resolution_status,
                     jsonb_build_object(
                         'namespace', 'test_namespace',
                         'task_name', 'test_task',
                         'current_state', $3::text
                     ),
                     NOW(), NOW())
            "#,
            Uuid::new_v4(),
            task_uuid,
            state,
            reason,
            *minutes_ago,
            resolution_status
        )
        .execute(&pool)
        .await?;
    }

    // Query investigation queue (should exclude resolved entries)
    let queue = DlqEntry::list_investigation_queue(&pool, None).await?;

    // Verify count excludes resolved entries
    assert_eq!(
        queue.len(),
        5,
        "Should have 5 pending entries (excludes resolved)"
    );

    // Verify priority ordering by reason (base scores)
    // Dependency cycle has base 1000 (highest)
    assert!(
        matches!(queue[0].dlq_reason, DlqReason::DependencyCycleDetected),
        "Highest priority should be dependency cycle, got: {:?}",
        queue[0].dlq_reason
    );

    // Max retries have base 500 - should be entries 1 and 2
    assert!(
        matches!(queue[1].dlq_reason, DlqReason::MaxRetriesExceeded),
        "Second priority should be max retries"
    );
    assert!(
        matches!(queue[2].dlq_reason, DlqReason::MaxRetriesExceeded),
        "Third priority should be max retries"
    );

    // Within max retries, older entry (180 min) should come before newer (30 min)
    assert!(
        queue[1].minutes_in_dlq > queue[2].minutes_in_dlq,
        "Older max retries entry should rank higher than newer, got: {} vs {}",
        queue[1].minutes_in_dlq,
        queue[2].minutes_in_dlq
    );

    // Staleness timeout has base 100
    assert!(
        matches!(queue[3].dlq_reason, DlqReason::StalenessTimeout),
        "Fourth priority should be staleness timeout"
    );

    // Worker unavailable has base 50 (lowest)
    assert!(
        matches!(queue[4].dlq_reason, DlqReason::WorkerUnavailable),
        "Lowest priority should be worker unavailable"
    );

    // Verify priority scores are in descending order
    for i in 0..queue.len() - 1 {
        assert!(
            queue[i].priority_score >= queue[i + 1].priority_score,
            "Priority scores should be in descending order: entry {} ({}) >= entry {} ({})",
            i,
            queue[i].priority_score,
            i + 1,
            queue[i + 1].priority_score
        );
    }

    // Verify all entries have required fields populated
    for entry in &queue {
        assert!(
            entry.dlq_entry_uuid != Uuid::nil(),
            "Should have DLQ entry UUID"
        );
        assert!(entry.task_uuid != Uuid::nil(), "Should have task UUID");
        assert!(
            !entry.original_state.is_empty(),
            "Should have original state"
        );
        assert!(entry.minutes_in_dlq >= 0.0, "Should have minutes in DLQ");
        assert_eq!(
            entry.namespace_name.as_deref(),
            Some("test_namespace"),
            "Should extract namespace from snapshot"
        );
        assert_eq!(
            entry.task_name.as_deref(),
            Some("test_task"),
            "Should extract task name from snapshot"
        );
        assert!(
            entry.current_state.is_some(),
            "Should extract current state from snapshot"
        );
    }

    // Test limit parameter
    let limited_queue = DlqEntry::list_investigation_queue(&pool, Some(3)).await?;
    assert_eq!(limited_queue.len(), 3, "Should respect limit parameter");

    // Top 3 should be in same priority order
    assert!(matches!(
        limited_queue[0].dlq_reason,
        DlqReason::DependencyCycleDetected
    ));
    assert!(matches!(
        limited_queue[1].dlq_reason,
        DlqReason::MaxRetriesExceeded
    ));
    assert!(matches!(
        limited_queue[2].dlq_reason,
        DlqReason::MaxRetriesExceeded
    ));

    Ok(())
}

#[sqlx::test]
async fn test_staleness_monitoring_view_integration(pool: PgPool) -> Result<(), sqlx::Error> {
    // Setup: Create test namespace and tasks
    let namespace_uuid = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
    let named_task_uuid = Uuid::parse_str("20000000-0000-0000-0000-000000000002").unwrap();

    create_test_namespace(&pool, namespace_uuid, "test_namespace").await?;
    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        json!({}),
    )
    .await?;

    // Create tasks with varying health statuses:
    // - Healthy tasks: < 80% of threshold (48 minutes for 60-min threshold)
    // - Warning tasks: 80-99% of threshold (48-59 minutes)
    // - Stale tasks: >= threshold (60+ minutes)

    let test_cases = vec![
        // (task_uuid, state, time_in_state_minutes, expected_health_status)
        // Healthy tasks
        (Uuid::new_v4(), "waiting_for_dependencies", 30, "healthy"), // 50% of 60-min threshold
        (Uuid::new_v4(), "waiting_for_retry", 15, "healthy"),        // 50% of 30-min threshold
        (Uuid::new_v4(), "steps_in_process", 10, "healthy"),         // 33% of 30-min threshold
        // Warning tasks
        (Uuid::new_v4(), "waiting_for_dependencies", 50, "warning"), // 83% of 60-min threshold
        (Uuid::new_v4(), "waiting_for_retry", 25, "warning"),        // 83% of 30-min threshold
        // Stale tasks
        (Uuid::new_v4(), "waiting_for_dependencies", 90, "stale"), // 150% of threshold
        (Uuid::new_v4(), "waiting_for_retry", 45, "stale"),        // 150% of threshold
        (Uuid::new_v4(), "steps_in_process", 60, "stale"),         // 200% of threshold
    ];

    for (task_uuid, state, minutes_in_state, _expected_health) in &test_cases {
        // Create task in specific state
        create_stale_task(&pool, *task_uuid, named_task_uuid, state, *minutes_in_state).await?;
    }

    // Query staleness monitoring view
    let monitoring = DlqEntry::get_staleness_monitoring(&pool, None).await?;

    // Verify we got all tasks
    assert_eq!(
        monitoring.len(),
        8,
        "Should have 8 tasks (3 healthy + 2 warning + 3 stale)"
    );

    // Verify tasks are ordered by health status (stale first, then warning, then healthy)
    // Within each health status group, ordered by time_in_state DESC
    let stale_tasks: Vec<_> = monitoring
        .iter()
        .filter(|m| m.health_status.is_stale())
        .collect();
    let warning_tasks: Vec<_> = monitoring
        .iter()
        .filter(|m| m.health_status == StalenessHealthStatus::Warning)
        .collect();
    let healthy_tasks: Vec<_> = monitoring
        .iter()
        .filter(|m| m.health_status.is_healthy())
        .collect();

    assert_eq!(stale_tasks.len(), 3, "Should have 3 stale tasks");
    assert_eq!(warning_tasks.len(), 2, "Should have 2 warning tasks");
    assert_eq!(healthy_tasks.len(), 3, "Should have 3 healthy tasks");

    // Verify stale tasks are first
    assert_eq!(
        monitoring[0].health_status,
        StalenessHealthStatus::Stale,
        "First task should be stale"
    );
    assert_eq!(
        monitoring[1].health_status,
        StalenessHealthStatus::Stale,
        "Second task should be stale"
    );
    assert_eq!(
        monitoring[2].health_status,
        StalenessHealthStatus::Stale,
        "Third task should be stale"
    );

    // Verify stale tasks are ordered by time_in_state DESC
    assert!(
        monitoring[0].time_in_state_minutes >= monitoring[1].time_in_state_minutes,
        "Stale tasks should be ordered by time_in_state DESC"
    );
    assert!(
        monitoring[1].time_in_state_minutes >= monitoring[2].time_in_state_minutes,
        "Stale tasks should be ordered by time_in_state DESC"
    );

    // Verify warning tasks come after stale
    assert_eq!(
        monitoring[3].health_status,
        StalenessHealthStatus::Warning,
        "Fourth task should be warning"
    );
    assert_eq!(
        monitoring[4].health_status,
        StalenessHealthStatus::Warning,
        "Fifth task should be warning"
    );

    // Verify all entries have required fields
    for entry in &monitoring {
        assert!(entry.task_uuid != Uuid::nil(), "Should have task UUID");
        assert_eq!(
            entry.namespace_name.as_deref(),
            Some("test_namespace"),
            "Should have namespace name"
        );
        assert_eq!(
            entry.task_name.as_deref(),
            Some("test_task"),
            "Should have task name"
        );
        assert!(!entry.current_state.is_empty(), "Should have current state");
        assert!(entry.time_in_state_minutes > 0, "Should have time in state");
        assert!(
            entry.staleness_threshold_minutes > 0,
            "Should have staleness threshold"
        );

        // Verify threshold calculation methods
        let percentage = entry.threshold_percentage();
        assert!(
            percentage > 0.0,
            "Should have positive threshold percentage"
        );

        if entry.health_status.is_stale() {
            assert!(
                percentage >= 100.0,
                "Stale tasks should have ≥ 100% threshold consumption"
            );
            assert_eq!(
                entry.minutes_until_stale(),
                0,
                "Stale tasks should have 0 minutes until stale"
            );
            assert!(
                entry.is_approaching_threshold(),
                "Stale tasks should be marked as approaching threshold"
            );
        } else if entry.health_status == StalenessHealthStatus::Warning {
            assert!(
                (80.0..100.0).contains(&percentage),
                "Warning tasks should have 80-99% threshold consumption, got {}%",
                percentage
            );
            assert!(
                entry.minutes_until_stale() > 0,
                "Warning tasks should have positive minutes until stale"
            );
            assert!(
                entry.is_approaching_threshold(),
                "Warning tasks should be marked as approaching threshold"
            );
        } else {
            // Healthy
            assert!(
                percentage < 80.0,
                "Healthy tasks should have < 80% threshold consumption, got {}%",
                percentage
            );
            assert!(
                entry.minutes_until_stale() > 0,
                "Healthy tasks should have positive minutes until stale"
            );
        }
    }

    // Test limit parameter
    let limited_monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(5)).await?;
    assert_eq!(
        limited_monitoring.len(),
        5,
        "Should respect limit parameter"
    );

    // Limited results should prioritize stale tasks
    assert_eq!(
        limited_monitoring[0].health_status,
        StalenessHealthStatus::Stale
    );
    assert_eq!(
        limited_monitoring[1].health_status,
        StalenessHealthStatus::Stale
    );
    assert_eq!(
        limited_monitoring[2].health_status,
        StalenessHealthStatus::Stale
    );

    // Test helper methods on StalenessHealthStatus
    assert!(StalenessHealthStatus::Healthy.is_healthy());
    assert!(!StalenessHealthStatus::Healthy.is_stale());
    assert!(!StalenessHealthStatus::Healthy.needs_attention());

    assert!(!StalenessHealthStatus::Warning.is_healthy());
    assert!(!StalenessHealthStatus::Warning.is_stale());
    assert!(StalenessHealthStatus::Warning.needs_attention());

    assert!(!StalenessHealthStatus::Stale.is_healthy());
    assert!(StalenessHealthStatus::Stale.is_stale());
    assert!(StalenessHealthStatus::Stale.needs_attention());

    Ok(())
}
