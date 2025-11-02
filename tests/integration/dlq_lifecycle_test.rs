//! # DLQ Lifecycle Integration Tests (TAS-49)
//!
//! Comprehensive end-to-end tests for DLQ staleness detection and lifecycle management.
//!
//! ## Test Strategy
//!
//! 1. Create real tasks using dlq_staleness_test.yaml template
//! 2. Use state machine to transition steps to specific states
//! 3. Manipulate timestamps with direct SQL to simulate staleness
//! 4. Invoke staleness detector directly (bypass interval timing)
//! 5. Validate DLQ records created correctly
//! 6. Validate database views reflect correct state

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::models::orchestration::{
    DlqEntry, DlqInvestigationUpdate, DlqReason, DlqResolutionStatus,
};

// ============================================================================
// Helper Functions
// ============================================================================

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
        VALUES ($1, $2, $3, '1.0.0', $4, NOW(), NOW())
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

/// Helper to create task in specific state with timestamp
async fn create_task_in_state(
    pool: &PgPool,
    task_uuid: Uuid,
    named_task_uuid: Uuid,
    state: &str,
    minutes_in_state: i32,
) -> Result<(), sqlx::Error> {
    let correlation_id = Uuid::now_v7();
    let task_created = Utc::now() - chrono::Duration::minutes(minutes_in_state as i64 + 10);
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
        format!("{}-{}", named_task_uuid, task_uuid),
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

/// Helper to manipulate task state timestamp
/// Sets the task's state transition timestamp to a specific number of seconds in the past
async fn manipulate_task_state_timestamp(
    pool: &PgPool,
    task_uuid: Uuid,
    offset_seconds: i32,
) -> Result<(), sqlx::Error> {
    // Use raw query to avoid PgInterval type issues in test helpers
    let query = format!(
        r#"
        UPDATE tasker_task_transitions
        SET created_at = NOW() - INTERVAL '{} seconds',
            updated_at = NOW() - INTERVAL '{} seconds'
        WHERE task_uuid = $1 AND most_recent = true
        "#,
        offset_seconds, offset_seconds
    );

    sqlx::query(&query).bind(task_uuid).execute(pool).await?;

    Ok(())
}

// ============================================================================
// Test 1: Waiting for Dependencies Staleness
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_waiting_for_dependencies_staleness(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Phase 1: Setup - Create task with 2-minute threshold
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "dlq_test_waiting_deps").await?;

    // Create named task with 2-minute threshold for waiting_for_dependencies
    let lifecycle_config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 2,
            "max_waiting_for_retry_minutes": 2,
            "max_steps_in_process_minutes": 2,
            "max_duration_minutes": 30
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "dlq_staleness_test",
        lifecycle_config,
    )
    .await?;

    // Create task in waiting_for_dependencies state, 3 minutes in state (exceeds 2-minute threshold)
    create_task_in_state(
        &pool,
        task_uuid,
        named_task_uuid,
        "waiting_for_dependencies",
        3,
    )
    .await?;

    // Phase 2: Staleness Detection - Invoke detector directly
    let executor = SqlFunctionExecutor::new(pool.clone());
    let results = executor
        .detect_and_transition_stale_tasks(
            false, // dry_run = false
            100,   // batch_size
            2,     // waiting_deps_threshold (matches template config)
            2,     // waiting_retry_threshold
            2,     // steps_in_process_threshold
            24,    // max_lifetime_hours
        )
        .await?;

    // Phase 3: DLQ Validation - Verify DLQ entry created
    assert_eq!(results.len(), 1, "Should detect exactly 1 stale task");

    let stale_result = &results[0];
    assert_eq!(stale_result.task_uuid, task_uuid);
    assert_eq!(stale_result.current_state, "waiting_for_dependencies");
    assert!(stale_result.moved_to_dlq, "Task should be moved to DLQ");
    assert!(
        stale_result.transition_success,
        "Task should transition to Error"
    );

    // Query DLQ table directly
    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should exist");

    assert_eq!(dlq_entry.dlq_reason, DlqReason::StalenessTimeout);
    assert_eq!(dlq_entry.original_state, "waiting_for_dependencies");
    assert_eq!(dlq_entry.resolution_status, DlqResolutionStatus::Pending);

    // Phase 4: View Validation
    let stats = DlqEntry::get_stats(&pool).await?;
    let staleness_stats = stats
        .iter()
        .find(|s| s.dlq_reason == DlqReason::StalenessTimeout)
        .expect("Should have staleness_timeout stats");

    assert!(staleness_stats.total_entries >= 1);
    assert!(staleness_stats.pending >= 1);

    // Test investigation queue
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    assert!(
        queue.iter().any(|e| e.task_uuid == task_uuid),
        "Task should appear in investigation queue"
    );

    Ok(())
}

// ============================================================================
// Test 2: Steps in Process Staleness
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_steps_in_process_staleness(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create task in steps_in_process state
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "dlq_test_steps_in_process").await?;

    let lifecycle_config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 2,
            "max_waiting_for_retry_minutes": 2,
            "max_steps_in_process_minutes": 2,
            "max_duration_minutes": 30
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "dlq_staleness_test",
        lifecycle_config,
    )
    .await?;

    // Create task in steps_in_process state, 3 minutes in state
    create_task_in_state(&pool, task_uuid, named_task_uuid, "steps_in_process", 3).await?;

    // Run staleness detection
    let executor = SqlFunctionExecutor::new(pool.clone());
    let results = executor
        .detect_and_transition_stale_tasks(false, 100, 2, 2, 2, 24)
        .await?;

    // Validate DLQ entry
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].current_state, "steps_in_process");
    assert!(results[0].moved_to_dlq);

    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should exist");

    assert_eq!(dlq_entry.dlq_reason, DlqReason::StalenessTimeout);
    assert_eq!(dlq_entry.original_state, "steps_in_process");

    Ok(())
}

// ============================================================================
// Test 3: Proactive Staleness Monitoring
// ============================================================================

#[sqlx::test]
async fn test_dlq_lifecycle_proactive_monitoring(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create task in waiting_for_retry state with different time thresholds
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "dlq_test_proactive").await?;

    let lifecycle_config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 2,
            "max_waiting_for_retry_minutes": 2,
            "max_steps_in_process_minutes": 2,
            "max_duration_minutes": 30
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "dlq_staleness_test",
        lifecycle_config,
    )
    .await?;

    // Scenario 1: Healthy task (50% of threshold - 1 minute out of 2)
    let healthy_task_uuid = Uuid::now_v7();
    create_task_in_state(
        &pool,
        healthy_task_uuid,
        named_task_uuid,
        "waiting_for_retry",
        1,
    )
    .await?;

    // Scenario 2: Warning task (85% of threshold - 1.7 minutes out of 2)
    let warning_task_uuid = Uuid::now_v7();
    create_task_in_state(
        &pool,
        warning_task_uuid,
        named_task_uuid,
        "waiting_for_retry",
        2,
    )
    .await?;
    // Adjust to be exactly 1.7 minutes (102 seconds) = 85% of 2-minute threshold
    manipulate_task_state_timestamp(&pool, warning_task_uuid, 102).await?;

    // Scenario 3: Stale task (150% of threshold - 3 minutes out of 2)
    let stale_task_uuid = Uuid::now_v7();
    create_task_in_state(
        &pool,
        stale_task_uuid,
        named_task_uuid,
        "waiting_for_retry",
        3,
    )
    .await?;

    // Query staleness monitoring view
    let monitoring = DlqEntry::get_staleness_monitoring(&pool, Some(100)).await?;

    // Verify healthy task
    let healthy_monitoring = monitoring.iter().find(|m| m.task_uuid == healthy_task_uuid);
    if let Some(m) = healthy_monitoring {
        assert!(
            m.health_status.is_healthy(),
            "Task at 50% threshold should be healthy"
        );
    }

    // Verify warning task
    let warning_monitoring = monitoring.iter().find(|m| m.task_uuid == warning_task_uuid);
    if let Some(m) = warning_monitoring {
        assert!(
            m.health_status.needs_attention(),
            "Task at 85% threshold should need attention"
        );
    }

    // Verify stale task
    let stale_monitoring = monitoring
        .iter()
        .find(|m| m.task_uuid == stale_task_uuid)
        .expect("Stale task should appear in monitoring");

    assert!(
        stale_monitoring.health_status.is_stale(),
        "Task at 150% threshold should be stale"
    );
    assert!(stale_monitoring.threshold_percentage() > 100.0);

    Ok(())
}

// ============================================================================
// Test 4: DLQ Investigation Workflow
// ============================================================================

#[sqlx::test]
async fn test_dlq_investigation_workflow(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Setup: Create stale task and DLQ entry
    let namespace_uuid = Uuid::now_v7();
    let named_task_uuid = Uuid::now_v7();
    let task_uuid = Uuid::now_v7();

    create_test_namespace(&pool, namespace_uuid, "dlq_test_investigation").await?;

    let lifecycle_config = json!({
        "lifecycle": {
            "max_waiting_for_dependencies_minutes": 2,
            "max_waiting_for_retry_minutes": 2,
            "max_steps_in_process_minutes": 2,
            "max_duration_minutes": 30
        }
    });

    create_test_named_task(
        &pool,
        named_task_uuid,
        namespace_uuid,
        "dlq_staleness_test",
        lifecycle_config,
    )
    .await?;

    create_task_in_state(&pool, task_uuid, named_task_uuid, "steps_in_process", 3).await?;

    // Run staleness detection to create DLQ entry
    let executor = SqlFunctionExecutor::new(pool.clone());
    executor
        .detect_and_transition_stale_tasks(false, 100, 2, 2, 2, 24)
        .await?;

    // Test Investigation Queue Endpoint
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    let investigation = queue
        .iter()
        .find(|e| e.task_uuid == task_uuid)
        .expect("Task should appear in investigation queue");

    assert_eq!(investigation.dlq_reason, DlqReason::StalenessTimeout);
    assert!(investigation.priority_score > 0.0);

    // Test DLQ Entry Retrieval
    let dlq_entry = DlqEntry::find_by_task(&pool, task_uuid)
        .await?
        .expect("DLQ entry should exist");

    // Simulate operator investigation and resolution
    let update = DlqInvestigationUpdate {
        resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
        resolution_notes: Some("Fixed by manually completing stuck step".to_string()),
        resolved_by: Some("operator@example.com".to_string()),
        metadata: None,
    };

    let updated = DlqEntry::update_investigation(&pool, dlq_entry.dlq_entry_uuid, update).await?;

    assert!(updated, "Investigation should be updated");

    // Verify investigation no longer appears in queue
    let queue = DlqEntry::list_investigation_queue(&pool, Some(100)).await?;
    assert!(
        !queue.iter().any(|e| e.task_uuid == task_uuid),
        "Resolved entries should not appear in investigation queue"
    );

    // Verify stats updated
    let stats = DlqEntry::get_stats(&pool).await?;
    let staleness_stats = stats
        .iter()
        .find(|s| s.dlq_reason == DlqReason::StalenessTimeout)
        .expect("Should have staleness stats");

    assert!(staleness_stats.manually_resolved >= 1);

    Ok(())
}
