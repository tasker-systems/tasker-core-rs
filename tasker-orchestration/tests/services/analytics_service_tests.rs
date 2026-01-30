//! # AnalyticsQueryService and AnalyticsService Tests (TAS-63)
//!
//! Integration tests for analytics services validating:
//! - Performance metrics from database SQL functions
//! - Bottleneck analysis with slow step/task aggregation
//! - Cache-aside pattern with noop cache provider
//! - Analytics service delegates to query service

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;

use tasker_orchestration::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
use tasker_orchestration::orchestration::lifecycle::task_initialization::TaskInitializer;
use tasker_orchestration::services::{AnalyticsQueryService, AnalyticsService};
use tasker_shared::cache::CacheProvider;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::system_context::SystemContext;
use uuid::Uuid;

/// Get the path to task template fixtures in the workspace root
fn fixture_path() -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("tests/fixtures/task_templates/rust")
        .to_string_lossy()
        .to_string()
}

/// Helper to register templates and create some tasks for analytics
async fn seed_analytics_data(pool: &PgPool) -> Result<()> {
    let registry = TaskHandlerRegistry::new(pool.clone());
    registry
        .discover_and_register_templates(&fixture_path())
        .await?;

    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
    let step_enqueuer = Arc::new(StepEnqueuerService::new(system_context.clone()).await?);
    let task_initializer = TaskInitializer::new(system_context, step_enqueuer);

    // Create a few tasks to have data for analytics queries
    for _ in 0..3 {
        let request = TaskRequest {
            name: "boundary_max_attempts_one".to_string(),
            namespace: "test_sql".to_string(),
            version: "1.0.0".to_string(),
            context: serde_json::json!({
                "_test_run_id": Uuid::now_v7().to_string(),
                "test_value": 42
            }),
            correlation_id: Uuid::now_v7(),
            parent_correlation_id: None,
            initiator: "analytics_test".to_string(),
            source_system: "integration_test".to_string(),
            reason: "Seeding analytics data".to_string(),
            tags: vec!["test".to_string()],
            requested_at: chrono::Utc::now().naive_utc(),
            options: None,
            priority: Some(5),
            idempotency_key: None,
        };

        task_initializer.create_task_from_request(request).await?;
    }

    Ok(())
}

// =============================================================================
// AnalyticsQueryService integration tests
// =============================================================================

/// Test: get_performance_metrics returns valid metrics from database
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_performance_metrics_from_db(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    let metrics = query_service.get_performance_metrics(24).await?;

    // Verify metrics structure
    assert!(metrics.total_tasks >= 3, "Should have at least 3 tasks");
    assert!(
        metrics.completion_rate >= 0.0 && metrics.completion_rate <= 1.0,
        "Completion rate should be 0..1"
    );
    assert!(
        metrics.error_rate >= 0.0 && metrics.error_rate <= 1.0,
        "Error rate should be 0..1"
    );
    assert!(
        metrics.system_health_score >= 0.0,
        "Health score should be non-negative"
    );
    assert!(
        !metrics.analysis_period_start.is_empty(),
        "Should have analysis period"
    );
    assert!(
        !metrics.calculated_at.is_empty(),
        "Should have calculation timestamp"
    );

    Ok(())
}

/// Test: get_performance_metrics with zero hours returns all-time metrics
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_performance_metrics_all_time(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    let metrics = query_service.get_performance_metrics(0).await?;

    assert!(metrics.total_tasks >= 3);
    assert!(metrics.average_task_duration_seconds >= 0.0);
    assert!(metrics.average_step_duration_seconds >= 0.0);

    Ok(())
}

/// Test: get_bottleneck_analysis returns analysis from database
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_bottleneck_analysis_from_db(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    let analysis = query_service.get_bottleneck_analysis(10, 1).await?;

    // The analysis structure should be valid even with minimal data
    assert!(
        analysis.resource_utilization.database_pool_utilization >= 0.0,
        "Pool utilization should be non-negative"
    );
    assert!(
        analysis.resource_utilization.database_pool_utilization <= 1.0,
        "Pool utilization should be <= 1.0"
    );

    // System health counts should be populated
    assert!(
        analysis.resource_utilization.system_health.total_tasks >= 3,
        "Should have at least 3 tasks"
    );

    Ok(())
}

/// Test: get_bottleneck_analysis slow_steps structure validation
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_bottleneck_analysis_step_structure(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    let analysis = query_service.get_bottleneck_analysis(10, 1).await?;

    // Validate slow step entries if present
    for step in &analysis.slow_steps {
        assert!(
            !step.namespace_name.is_empty(),
            "Step should have namespace"
        );
        assert!(!step.task_name.is_empty(), "Step should have task name");
        assert!(!step.version.is_empty(), "Step should have version");
        assert!(!step.step_name.is_empty(), "Step should have step name");
        assert!(
            step.average_duration_seconds >= 0.0,
            "Duration should be non-negative"
        );
        assert!(
            step.execution_count >= 1,
            "Should have at least 1 execution"
        );
        assert!(
            step.error_rate >= 0.0 && step.error_rate <= 1.0,
            "Error rate should be 0..1"
        );
    }

    Ok(())
}

/// Test: get_bottleneck_analysis slow_tasks structure validation
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_bottleneck_analysis_task_structure(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    let analysis = query_service.get_bottleneck_analysis(10, 1).await?;

    for task in &analysis.slow_tasks {
        assert!(
            !task.namespace_name.is_empty(),
            "Task should have namespace"
        );
        assert!(!task.task_name.is_empty(), "Task should have task name");
        assert!(!task.version.is_empty(), "Task should have version");
        assert!(
            task.average_duration_seconds >= 0.0,
            "Duration should be non-negative"
        );
        assert!(
            task.error_rate >= 0.0 && task.error_rate <= 1.0,
            "Error rate should be 0..1"
        );
    }

    Ok(())
}

/// Test: get_bottleneck_analysis with high min_executions returns fewer results
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_bottleneck_analysis_min_executions_filter(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let query_service = AnalyticsQueryService::new(pool);

    // With high min_executions threshold, we should get fewer or no results
    let analysis = query_service.get_bottleneck_analysis(5, 10000).await?;

    // With min_executions=10000, no steps/tasks should qualify
    assert!(
        analysis.slow_steps.is_empty(),
        "No steps should meet high execution threshold"
    );
    assert!(
        analysis.slow_tasks.is_empty(),
        "No tasks should meet high execution threshold"
    );

    Ok(())
}

// =============================================================================
// AnalyticsService integration tests (with cache)
// =============================================================================

/// Test: AnalyticsService delegates to query service (noop cache)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_analytics_service_performance_metrics(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let cache_provider = Arc::new(CacheProvider::noop());
    let service = AnalyticsService::new(pool, cache_provider, None);

    let metrics = service.get_performance_metrics(24).await?;

    assert!(metrics.total_tasks >= 3);
    assert!(!service.cache_enabled(), "Noop cache should not be enabled");

    Ok(())
}

/// Test: AnalyticsService bottleneck analysis through cache layer
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_analytics_service_bottleneck_analysis(pool: PgPool) -> Result<()> {
    seed_analytics_data(&pool).await?;

    let cache_provider = Arc::new(CacheProvider::noop());
    let service = AnalyticsService::new(pool, cache_provider, None);

    let analysis = service.get_bottleneck_analysis(10, 1).await?;

    assert!(analysis.resource_utilization.system_health.total_tasks >= 3);

    Ok(())
}
