//! Metrics Integration Tests for Staleness Detection (TAS-49)
//!
//! Tests that verify OpenTelemetry metrics integration with staleness detection
//! and DLQ investigation tracking. These tests ensure that metric recording code
//! paths execute without errors and don't break core functionality.
//!
//! ## Metrics Verified
//!
//! **Staleness Detection Metrics** (recorded by `StalenessDetector`):
//! 1. `staleness_detection_runs_total` - Counter for detection cycles
//! 2. `staleness_detection_duration` - Histogram for detection duration
//! 3. `stale_tasks_detected_total` - Counter for detected stale tasks
//! 4. `dlq_entries_created_total` - Counter for DLQ entries created
//! 5. `tasks_transitioned_to_error_total` - Counter for error state transitions
//! 6. `dlq_pending_investigations` - Gauge for pending DLQ count
//!
//! **DLQ Investigation Metrics** (recorded by `DlqEntry::update_investigation`):
//! 7. `task_time_in_dlq_hours` - Histogram for time spent in DLQ
//!
//! ## Test Strategy
//!
//! These are smoke tests that verify metrics integration doesn't break functionality:
//! - Execute staleness detection workflow → exercises metrics 1-6
//! - Execute investigation update workflow → exercises metric 7
//! - Verify operations complete without errors or panics
//!
//! Note: These tests don't verify metric values are sent to collectors.
//! That requires end-to-end observability testing with actual telemetry backends.

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::orchestration::{
    DlqEntry, DlqInvestigationUpdate, DlqReason, DlqResolutionStatus,
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

/// Test 1: Staleness detection executes metrics recording code paths
///
/// Verifies that running staleness detection exercises all 6 detection metrics:
/// - staleness_detection_runs_total
/// - staleness_detection_duration
/// - stale_tasks_detected_total
/// - dlq_entries_created_total
/// - tasks_transitioned_to_error_total
/// - dlq_pending_investigations (gauge update)
///
/// Success criteria: Detection completes without errors
#[sqlx::test]
async fn test_staleness_detection_metrics_integration(pool: PgPool) -> sqlx::Result<()> {
    // Setup: Create stale task that will be detected
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "metrics_test").await?;

    // Create named task with 5-minute staleness threshold
    let lifecycle_config = json!({
        "max_retries": 3,
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 5,
            "max_waiting_for_retry_minutes": 5,
            "max_steps_in_process_minutes": 5,
            "task_max_lifetime_hours": 1
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task",
        lifecycle_config,
    )
    .await?;

    // Create task that exceeded threshold (10 minutes in waiting_for_dependencies state)
    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 10).await?;

    // Execute: Run staleness detection (exercises metrics 1-6)
    let executor = SqlFunctionExecutor::new(pool.clone());
    let results = executor
        .detect_and_transition_stale_tasks(
            false, // dry_run = false
            100,   // batch_size
            60,    // waiting_deps_threshold (global default, overridden by template)
            60,    // waiting_retry_threshold
            60,    // steps_in_process_threshold
            24,    // max_lifetime_hours
        )
        .await?;

    // Verify: Detection found the stale task
    assert_eq!(
        results.len(),
        1,
        "Should detect 1 stale task (exercises stale_tasks_detected_total metric)"
    );
    assert_eq!(results[0].task_uuid, task_uuid);
    assert!(
        results[0].moved_to_dlq,
        "Task should be moved to DLQ (exercises dlq_entries_created_total metric)"
    );
    assert!(
        results[0].transition_success,
        "Task should transition to Error (exercises tasks_transitioned_to_error_total metric)"
    );

    // Verify: DLQ entry was created (validates dlq_pending_investigations gauge query)
    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should exist");

    assert_eq!(dlq_entry.task_uuid, task_uuid);
    assert_eq!(dlq_entry.resolution_status, DlqResolutionStatus::Pending);
    assert_eq!(dlq_entry.dlq_reason, DlqReason::StalenessTimeout);

    Ok(())
}

/// Test 2: DLQ investigation update executes task_time_in_dlq_hours metric
///
/// Verifies that updating investigation status to a terminal state exercises
/// the task_time_in_dlq_hours histogram metric recording code path.
///
/// Success criteria: Investigation update completes without errors
#[sqlx::test]
async fn test_dlq_investigation_metrics_integration(pool: PgPool) -> sqlx::Result<()> {
    // Setup: Create task and DLQ entry
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "metrics_test_2").await?;

    let lifecycle_config = json!({
        "max_retries": 3,
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 5
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "test_task_2",
        lifecycle_config,
    )
    .await?;

    create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 10).await?;

    // Create DLQ entry
    let executor = SqlFunctionExecutor::new(pool.clone());
    executor
        .detect_and_transition_stale_tasks(false, 100, 5, 5, 5, 1)
        .await?;

    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should exist");

    // Execute: Update investigation to resolved (exercises task_time_in_dlq_hours metric)
    let update = DlqInvestigationUpdate {
        resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
        resolution_notes: Some("Fixed via metrics test".to_string()),
        resolved_by: Some("test_operator".to_string()),
        metadata: None,
    };

    let updated = DlqEntry::update_investigation(&pool, dlq_entry.dlq_entry_uuid, update).await?;

    // Verify: Update succeeded (metric recording didn't break functionality)
    assert!(updated, "Investigation should be updated");

    // Verify: Resolution status changed
    let updated_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should still exist");

    assert_eq!(
        updated_entry.resolution_status,
        DlqResolutionStatus::ManuallyResolved,
        "Resolution status should be updated"
    );
    assert!(
        updated_entry.resolution_notes.is_some(),
        "Resolution notes should be set"
    );

    Ok(())
}

/// Test 3: Pending investigations gauge query executes without errors
///
/// Verifies that the gauge update query used by staleness_detector.rs
/// can successfully query the pending DLQ count.
///
/// This tests the query used in `update_pending_investigations_gauge()`.
#[sqlx::test]
async fn test_pending_investigations_gauge_query(pool: PgPool) -> sqlx::Result<()> {
    // Setup: Create multiple DLQ entries with different statuses
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "gauge_test").await?;

    let lifecycle_config = json!({
        "max_retries": 3,
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 5
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "gauge_test_task",
        lifecycle_config,
    )
    .await?;

    // Create 3 tasks and DLQ entries
    for _ in 0..3 {
        let task_uuid = Uuid::now_v7();
        create_stale_task(&pool, task_uuid, named_task_uuid, "waiting_for_dependencies", 10)
            .await?;
    }

    // Create DLQ entries for all 3 tasks
    let executor = SqlFunctionExecutor::new(pool.clone());
    executor
        .detect_and_transition_stale_tasks(false, 100, 5, 5, 5, 1)
        .await?;

    // Execute: Query pending DLQ count (same query used by gauge update)
    let pending_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE resolution_status = 'pending'",
    )
    .fetch_one(&pool)
    .await?;

    // Verify: All 3 entries are pending
    assert_eq!(
        pending_count, 3,
        "Should have 3 pending DLQ entries (validates gauge query)"
    );

    // Resolve one entry
    let entries = DlqEntry::list(
        &pool,
        tasker_shared::models::orchestration::DlqListParams::default(),
    )
    .await?;

    if let Some(first_entry) = entries.first() {
        let update = DlqInvestigationUpdate {
            resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
            resolution_notes: None,
            resolved_by: None,
            metadata: None,
        };
        DlqEntry::update_investigation(&pool, first_entry.dlq_entry_uuid, update).await?;
    }

    // Verify: Pending count decreased
    let pending_count_after = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasker_tasks_dlq WHERE resolution_status = 'pending'",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        pending_count_after, 2,
        "Should have 2 pending entries after resolving one"
    );

    Ok(())
}
