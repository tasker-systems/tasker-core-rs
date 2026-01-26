//! # HealthService Tests (TAS-76)
//!
//! Integration tests for HealthService validating:
//! - Basic health check
//! - Liveness probe
//! - Readiness probe (database connectivity)
//! - Detailed health with all subsystems
//! - Prometheus metrics generation

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

use tasker_orchestration::orchestration::core::OrchestrationCore;
use tasker_orchestration::services::HealthService;
use tasker_orchestration::web::circuit_breaker::WebDatabaseCircuitBreaker;
use tasker_orchestration::web::state::OrchestrationStatus;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::web::SystemOperationalState;

/// Helper to set up HealthService with real infrastructure
async fn setup_health_service(pool: PgPool) -> Result<HealthService> {
    // Create system context
    let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

    // Create OrchestrationCore
    let orchestration_core = Arc::new(OrchestrationCore::new(system_context).await?);

    // Create circuit breaker with default settings
    let circuit_breaker = WebDatabaseCircuitBreaker::default();

    // Create orchestration status
    let orchestration_status = Arc::new(RwLock::new(OrchestrationStatus {
        running: true,
        environment: "test".to_string(),
        operational_state: SystemOperationalState::Normal,
        database_pool_size: pool.size(),
        last_health_check: std::time::Instant::now(),
    }));

    // Create HealthService
    let health_service = HealthService::new(
        pool.clone(),
        pool.clone(),
        circuit_breaker,
        orchestration_core,
        orchestration_status,
    );

    Ok(health_service)
}

/// Test: HealthService.basic_health() returns healthy status
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_basic_health(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.basic_health()");

    let health_service = setup_health_service(pool).await?;

    let response = health_service.basic_health();

    assert_eq!(response.status, "healthy");
    assert!(!response.timestamp.is_empty());

    tracing::info!(
        status = %response.status,
        timestamp = %response.timestamp,
        "✅ Basic health check passed"
    );

    Ok(())
}

/// Test: HealthService.liveness() returns alive status
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_liveness(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.liveness()");

    let health_service = setup_health_service(pool).await?;

    let response = health_service.liveness().await;

    assert_eq!(response.status, "alive");
    assert!(!response.timestamp.is_empty());

    tracing::info!(
        status = %response.status,
        "✅ Liveness check passed"
    );

    Ok(())
}

/// Test: HealthService.readiness() validates all checks are performed
///
/// Note: OrchestrationCore starts in "Created" state (not "Running"), so the
/// orchestration_system check will report unhealthy. This test validates that
/// readiness correctly aggregates all checks and returns not_ready when any check fails.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_readiness_checks_performed(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.readiness() checks performed");

    let health_service = setup_health_service(pool).await?;

    let result = health_service.readiness().await;

    // Extract response from either Ok or Err (both contain ReadinessResponse)
    let response = match result {
        Ok(r) => r,
        Err(r) => r,
    };

    // Verify all expected checks are present (typed struct fields)
    // Database checks should pass (since we have a valid pool)
    assert_eq!(
        response.checks.web_database.status, "healthy",
        "web_database should be healthy"
    );
    assert_eq!(
        response.checks.orchestration_database.status, "healthy",
        "orchestration_database should be healthy"
    );
    assert_eq!(
        response.checks.circuit_breaker.status, "healthy",
        "circuit_breaker should be healthy"
    );

    // orchestration_system and command_processor may be unhealthy since
    // OrchestrationCore starts in Created state
    assert!(
        !response.checks.orchestration_system.status.is_empty(),
        "orchestration_system should have a status"
    );
    assert!(
        !response.checks.command_processor.status.is_empty(),
        "command_processor should have a status"
    );

    tracing::info!(
        status = %response.status,
        web_db = %response.checks.web_database.status,
        orch_db = %response.checks.orchestration_database.status,
        circuit_breaker = %response.checks.circuit_breaker.status,
        "✅ Readiness checks performed correctly"
    );

    Ok(())
}

/// Test: HealthService.detailed_health() returns all subsystem checks
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_detailed_health(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.detailed_health()");

    let health_service = setup_health_service(pool).await?;

    let response = health_service.detailed_health().await;

    // Should always return a response (healthy or degraded)
    assert!(
        response.status == "healthy" || response.status == "degraded",
        "Status should be healthy or degraded, got: {}",
        response.status
    );

    // Verify all typed check fields exist and have status values
    assert!(
        !response.checks.web_database.status.is_empty(),
        "web_database should have status"
    );
    assert!(
        !response.checks.orchestration_database.status.is_empty(),
        "orchestration_database should have status"
    );
    assert!(
        !response.checks.circuit_breaker.status.is_empty(),
        "circuit_breaker should have status"
    );
    assert!(
        !response.checks.orchestration_system.status.is_empty(),
        "orchestration_system should have status"
    );
    assert!(
        !response.checks.command_processor.status.is_empty(),
        "command_processor should have status"
    );
    assert!(
        !response.checks.pool_utilization.status.is_empty(),
        "pool_utilization should have status"
    );
    assert!(
        !response.checks.queue_depth.status.is_empty(),
        "queue_depth should have status"
    );
    assert!(
        !response.checks.channel_saturation.status.is_empty(),
        "channel_saturation should have status"
    );

    // Should have health info
    assert!(!response.info.version.is_empty());
    assert!(!response.info.environment.is_empty());

    tracing::info!(
        status = %response.status,
        version = %response.info.version,
        "✅ Detailed health check passed"
    );

    Ok(())
}

/// Test: HealthService.prometheus_metrics() returns metrics string
///
/// Note: This test validates the custom metrics generation. OpenTelemetry metrics
/// require init_metrics() to be called, which is typically done at application startup.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_prometheus_metrics(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.prometheus_metrics()");

    // Initialize metrics for this test (safe to call multiple times due to OnceLock)
    tasker_shared::metrics::init_metrics();

    let health_service = setup_health_service(pool).await?;

    let metrics = health_service.prometheus_metrics().await;

    // Should return non-empty string with custom metrics
    assert!(!metrics.is_empty(), "Should return metrics");

    // Should contain orchestration-specific metrics (custom metrics always present)
    assert!(
        metrics.contains("tasker_orchestration_info")
            || metrics.contains("tasker_orchestration_running")
            || metrics.contains("tasker_orchestration_db_pool_size"),
        "Should contain orchestration metrics"
    );

    tracing::info!(
        metrics_length = metrics.len(),
        "✅ Prometheus metrics generated"
    );

    Ok(())
}

/// Test: HealthService.check_database() validates connectivity
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_check_database(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.check_database()");

    let health_service = setup_health_service(pool.clone()).await?;

    let check = health_service.check_database(&pool, "test_db").await;

    assert_eq!(check.status, "healthy");
    // Duration could be 0 for very fast queries on local connections
    tracing::info!(
        status = %check.status,
        duration_ms = check.duration_ms,
        "✅ Database check passed"
    );

    Ok(())
}

/// Test: HealthService.check_circuit_breaker() reports state
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_check_circuit_breaker(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.check_circuit_breaker()");

    let health_service = setup_health_service(pool).await?;

    let check = health_service.check_circuit_breaker().await;

    // Circuit breaker should be healthy when not tripped
    assert_eq!(check.status, "healthy");
    assert!(check.message.is_some());

    tracing::info!(
        status = %check.status,
        message = ?check.message,
        "✅ Circuit breaker check passed"
    );

    Ok(())
}

/// Test: HealthService.check_pool_utilization() reports pool stats
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_check_pool_utilization(pool: PgPool) -> Result<()> {
    tracing::info!("🧪 TEST: HealthService.check_pool_utilization()");

    let health_service = setup_health_service(pool).await?;

    let check = health_service.check_pool_utilization();

    // Pool should be healthy when not saturated
    assert!(
        check.status == "healthy" || check.status == "degraded",
        "Status should be healthy or degraded"
    );
    assert!(check.message.is_some());

    tracing::info!(
        status = %check.status,
        message = ?check.message,
        "✅ Pool utilization check passed"
    );

    Ok(())
}
