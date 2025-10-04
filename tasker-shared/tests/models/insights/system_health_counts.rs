use sqlx::PgPool;
use tasker_shared::models::insights::SystemHealthCounts;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_system_health_counts(pool: PgPool) -> sqlx::Result<()> {
    // Test getting current system health counts
    let health = SystemHealthCounts::get_current(&pool).await?;

    // Should return some counts (might be zero for empty system)
    if let Some(h) = health {
        // All counts should be non-negative by definition

        // Test computed metrics
        let _task_completion = h.task_completion_rate();
        let _task_error = h.task_error_rate();
        let _step_completion = h.step_completion_rate();
        let _step_error = h.step_error_rate();
        let _connection_util = h.connection_utilization();
        let _health_score = h.overall_health_score();
        let _health_status = h.health_status();
        let _is_healthy = h.is_healthy();
        let _high_errors = h.has_high_error_rate();
        let _pool_stressed = h.connection_pool_stressed();
        let _active_work = h.active_work_count();
        let _blocked_work = h.blocked_work_count();
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_get_health_summary(pool: PgPool) -> sqlx::Result<()> {
    // Test getting health summary
    let summary = SystemHealthCounts::get_health_summary(&pool).await?;

    // Should return without error (might be None for empty system)
    if let Some(s) = summary {
        assert!(s.overall_health_score >= 0.0);
        assert!(s.overall_health_score <= 100.0);
        assert!(!s.health_status.is_empty());
    }

    Ok(())
}

#[test]
fn test_health_calculations() {
    let health = SystemHealthCounts {
        total_tasks: 100,
        pending_tasks: 10,
        in_progress_tasks: 20,
        complete_tasks: 60,
        error_tasks: 10,
        cancelled_tasks: 0,
        total_steps: 500,
        pending_steps: 50,
        in_progress_steps: 100,
        complete_steps: 300,
        error_steps: 40,
        retryable_error_steps: 30,
        exhausted_retry_steps: 10,
        in_backoff_steps: 10,
        active_connections: 8,
        max_connections: 10,
        enqueued_steps: 2,
    };

    assert_eq!(health.task_completion_rate(), 0.6);
    assert_eq!(health.task_error_rate(), 0.1);
    assert_eq!(health.step_completion_rate(), 0.6);
    assert_eq!(health.step_error_rate(), 0.08);
    assert_eq!(health.connection_utilization(), 0.8);

    let health_score = health.overall_health_score();
    assert!(health_score > 0.0 && health_score <= 100.0);

    assert!(!health.health_status().is_empty());
    assert_eq!(health.active_work_count(), 120);
    assert_eq!(health.blocked_work_count(), 70);
}
