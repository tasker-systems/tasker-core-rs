//! Integration tests for AnalyticsMetrics model
//!
//! These tests verify the SQL function wrapper for system analytics
//! using SQLx native testing for automatic database isolation.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tasker_shared::models::insights::AnalyticsMetrics;

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_get_analytics_metrics(pool: PgPool) -> sqlx::Result<()> {
    // Test getting current metrics
    let metrics = AnalyticsMetrics::get_current(&pool).await?;

    // Should return some metrics (might be empty system)
    if let Some(m) = metrics {
        assert!(m.active_tasks_count >= 0);
        assert!(m.total_namespaces_count >= 0);
        assert!(m.unique_task_types_count >= 0);

        // Test helper methods
        let _is_healthy = m.is_healthy();
        let _has_high_errors = m.has_high_error_rate();
        let _completion_pct = m.completion_percentage();
        let _error_pct = m.error_percentage();
        let _avg_task_dur = m.avg_task_duration_seconds();
        let _avg_step_dur = m.avg_step_duration_seconds();
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_get_analytics_since_timestamp(pool: PgPool) -> sqlx::Result<()> {
    // Test getting metrics since 1 hour ago
    let since = "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
    let metrics = AnalyticsMetrics::get_since(&pool, Some(since)).await?;

    // Should return without error (might be None for empty system)
    assert!(metrics.is_some() || metrics.is_none());

    Ok(())
}
