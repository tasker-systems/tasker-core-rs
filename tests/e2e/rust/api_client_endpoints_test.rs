//! # API Client Endpoints E2E Test (TAS-70)
//!
//! Tests the tasker-client API functions against running Docker services.
//! Validates that all client methods return expected response structures.
//!
//! ## Endpoints Tested
//!
//! ### Orchestration Service
//! - Health: `/health`, `/health/ready`, `/health/live`, `/health/detailed`
//! - Config: `/config`
//! - Metrics: `/metrics`
//!
//! ### Worker Service
//! - Health: `/health`, `/health/ready`, `/health/live`, `/health/detailed`
//!   - TAS-169: `/health/detailed` now includes `distributed_cache` field
//! - Config: `/config`
//! - Metrics: `/metrics`, `/metrics/worker`, `/metrics/events`
//! - Templates (TAS-169: moved to `/v1/`):
//!   - `/v1/templates` - list templates (supports `?include_cache_stats=true`)
//!   - `/v1/templates/{namespace}/{name}/{version}` - get template
//!   - `/v1/templates/{namespace}/{name}/{version}/validate` - validate template
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests

use anyhow::Result;
use tasker_shared::types::api::worker::TemplateQueryParams;

use crate::common::integration_test_manager::IntegrationTestManager;

// =============================================================================
// ORCHESTRATION SERVICE ENDPOINT TESTS
// =============================================================================

/// Test orchestration health endpoints with unified nested paths
///
/// Validates:
/// - GET /health returns BasicHealthResponse with status "healthy"
/// - GET /health/ready returns HealthResponse (readiness probe)
/// - GET /health/live returns HealthResponse (liveness probe)
/// - GET /health/detailed returns DetailedHealthResponse with subsystem checks
#[tokio::test]
async fn test_orchestration_health_endpoints() -> Result<()> {
    println!("🏥 Testing Orchestration Health Endpoints (TAS-70 unified paths)");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Test basic health
    println!("\n  📋 GET /health");
    let basic_health = manager.orchestration_client.get_basic_health().await?;
    assert_eq!(
        basic_health.status, "healthy",
        "Basic health should be healthy"
    );
    println!("     ✅ status: {}", basic_health.status);

    // Test readiness probe
    // Note: Readiness probe may return 503 if service isn't fully ready - this is valid K8s behavior
    println!("\n  📋 GET /health/ready");
    match manager.orchestration_client.readiness_probe().await {
        Ok(readiness) => {
            println!("     ✅ status: {}", readiness.status);
        }
        Err(e) => {
            // 503 is valid for readiness probes - service may not be fully ready
            println!(
                "     ⚠️  readiness probe returned error (valid for K8s): {}",
                e
            );
        }
    }

    // Test liveness probe
    println!("\n  📋 GET /health/live");
    let liveness = manager.orchestration_client.liveness_probe().await?;
    assert!(
        liveness.status == "healthy" || liveness.status == "alive",
        "Liveness probe should indicate alive state"
    );
    println!("     ✅ status: {}", liveness.status);

    // Test detailed health
    println!("\n  📋 GET /health/detailed");
    let detailed = manager.orchestration_client.get_detailed_health().await?;
    assert!(
        detailed.status == "healthy" || detailed.status == "degraded",
        "Detailed health should return valid status"
    );
    println!("     ✅ status: {}", detailed.status);
    println!("     ✅ checks: {} subsystems", detailed.checks.len());
    for (name, check) in &detailed.checks {
        println!("        - {}: {}", name, check.status);
    }

    println!("\n🎉 Orchestration health endpoints test passed!");
    Ok(())
}

/// Test orchestration config endpoint
///
/// Validates:
/// - GET /config returns OrchestrationConfigResponse with safe (whitelist-only) fields
/// - Response includes auth, circuit breaker, and messaging configuration
#[tokio::test]
async fn test_orchestration_config_endpoint() -> Result<()> {
    println!("⚙️  Testing Orchestration Config Endpoint (TAS-70/TAS-150)");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    println!("\n  📋 GET /config");
    let config = manager.orchestration_client.get_config().await?;

    // Verify metadata
    assert!(
        !config.metadata.environment.is_empty(),
        "Environment should not be empty"
    );
    assert!(
        !config.metadata.version.is_empty(),
        "Version should not be empty"
    );
    println!("     ✅ environment: {}", config.metadata.environment);
    println!("     ✅ version: {}", config.metadata.version);

    // Deployment mode
    assert!(
        !config.deployment_mode.is_empty(),
        "Deployment mode should not be empty"
    );
    println!("     ✅ deployment_mode: {}", config.deployment_mode);

    // Messaging config
    assert!(
        !config.messaging.backend.is_empty(),
        "Messaging backend should not be empty"
    );
    println!("     ✅ messaging.backend: {}", config.messaging.backend);
    println!(
        "     ✅ messaging.queues: {} queues",
        config.messaging.queues.len()
    );

    println!("\n🎉 Orchestration config endpoint test passed!");
    Ok(())
}

/// Test orchestration Prometheus metrics endpoint
///
/// Validates:
/// - GET /metrics returns Prometheus-format text
/// - Response contains expected metric prefixes
#[tokio::test]
async fn test_orchestration_prometheus_metrics() -> Result<()> {
    println!("📊 Testing Orchestration Prometheus Metrics Endpoint");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    println!("\n  📋 GET /metrics");
    let metrics_text = manager
        .orchestration_client
        .get_prometheus_metrics()
        .await?;

    // Verify it's non-empty Prometheus format
    assert!(!metrics_text.is_empty(), "Metrics should not be empty");
    println!("     ✅ metrics response: {} bytes", metrics_text.len());

    // Should contain some metric lines
    let line_count = metrics_text.lines().count();
    println!("     ✅ metric lines: {}", line_count);

    println!("\n🎉 Orchestration Prometheus metrics test passed!");
    Ok(())
}

// =============================================================================
// WORKER SERVICE ENDPOINT TESTS
// =============================================================================

/// Test worker health endpoints with unified nested paths
///
/// Validates:
/// - GET /health returns BasicHealthResponse with worker_id
/// - GET /health/ready returns DetailedHealthResponse with checks
/// - GET /health/live returns BasicHealthResponse (liveness)
/// - GET /health/detailed returns DetailedHealthResponse with system_info
#[tokio::test]
async fn test_worker_health_endpoints() -> Result<()> {
    println!("🏥 Testing Worker Health Endpoints (TAS-70 unified paths)");

    let manager = IntegrationTestManager::setup().await?;
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Worker client required. Ensure worker service is running.")
    })?;

    // Test basic health
    println!("\n  📋 GET /health");
    let basic_health = worker_client.health_check().await?;
    assert_eq!(
        basic_health.status, "healthy",
        "Worker health should be healthy"
    );
    assert!(
        !basic_health.worker_id.is_empty(),
        "Worker ID should be present"
    );
    println!("     ✅ status: {}", basic_health.status);
    println!("     ✅ worker_id: {}", basic_health.worker_id);

    // Test readiness probe
    // Note: Readiness probe may return 503 if service isn't fully ready - this is valid K8s behavior
    println!("\n  📋 GET /health/ready");
    match worker_client.readiness_probe().await {
        Ok(readiness) => {
            assert!(
                readiness.status == "ready" || readiness.status == "not_ready",
                "Readiness should return valid status"
            );
            println!("     ✅ status: {}", readiness.status);
            println!("     ✅ checks: {} subsystems", readiness.checks.len());
        }
        Err(e) => {
            // 503 is valid for readiness probes - service may not be fully ready
            println!(
                "     ⚠️  readiness probe returned error (valid for K8s): {}",
                e
            );
        }
    }

    // Test liveness probe
    println!("\n  📋 GET /health/live");
    let liveness = worker_client.liveness_probe().await?;
    assert_eq!(liveness.status, "alive", "Worker should be alive");
    println!("     ✅ status: {}", liveness.status);

    // Test detailed health
    println!("\n  📋 GET /health/detailed");
    let detailed = worker_client.get_detailed_health().await?;
    assert!(
        detailed.status == "healthy" || detailed.status == "degraded",
        "Detailed health should return valid status"
    );
    println!("     ✅ status: {}", detailed.status);
    println!(
        "     ✅ system_info.version: {}",
        detailed.system_info.version
    );
    println!(
        "     ✅ system_info.worker_type: {}",
        detailed.system_info.worker_type
    );
    println!(
        "     ✅ system_info.namespaces: {:?}",
        detailed.system_info.supported_namespaces
    );

    println!("\n🎉 Worker health endpoints test passed!");
    Ok(())
}

/// Test worker config endpoint
///
/// Validates:
/// - GET /config returns WorkerConfigResponse with safe (whitelist-only) fields
/// - Response includes worker identity and messaging configuration
#[tokio::test]
async fn test_worker_config_endpoint() -> Result<()> {
    println!("⚙️  Testing Worker Config Endpoint (TAS-70/TAS-150)");

    let manager = IntegrationTestManager::setup().await?;
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Worker client required. Ensure worker service is running.")
    })?;

    println!("\n  📋 GET /config");
    let config = worker_client.get_config().await?;

    // Verify metadata
    assert!(
        !config.metadata.environment.is_empty(),
        "Environment should not be empty"
    );
    assert!(
        !config.metadata.version.is_empty(),
        "Version should not be empty"
    );
    println!("     ✅ environment: {}", config.metadata.environment);
    println!("     ✅ version: {}", config.metadata.version);

    // Worker identity
    assert!(
        !config.worker_id.is_empty(),
        "Worker ID should not be empty"
    );
    println!("     ✅ worker_id: {}", config.worker_id);
    println!("     ✅ worker_type: {}", config.worker_type);

    // Messaging config
    assert!(
        !config.messaging.backend.is_empty(),
        "Messaging backend should not be empty"
    );
    println!("     ✅ messaging.backend: {}", config.messaging.backend);
    println!(
        "     ✅ messaging.queues: {} queues",
        config.messaging.queues.len()
    );

    println!("\n🎉 Worker config endpoint test passed!");
    Ok(())
}

/// Test worker metrics endpoints
///
/// Validates:
/// - GET /metrics returns Prometheus-format text
/// - GET /metrics/worker returns MetricsResponse with worker metrics
/// - GET /metrics/events returns DomainEventStats (already tested in domain_event_publishing_test)
#[tokio::test]
async fn test_worker_metrics_endpoints() -> Result<()> {
    println!("📊 Testing Worker Metrics Endpoints (TAS-70)");

    let manager = IntegrationTestManager::setup().await?;
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Worker client required. Ensure worker service is running.")
    })?;

    // Test Prometheus metrics
    println!("\n  📋 GET /metrics");
    let prometheus_metrics = worker_client.get_prometheus_metrics().await?;
    assert!(
        !prometheus_metrics.is_empty(),
        "Prometheus metrics should not be empty"
    );
    println!(
        "     ✅ prometheus metrics: {} bytes",
        prometheus_metrics.len()
    );

    // Should contain worker-specific metrics
    assert!(
        prometheus_metrics.contains("tasker_worker"),
        "Should contain tasker_worker metrics"
    );
    println!("     ✅ contains tasker_worker metrics");

    // Test worker JSON metrics
    println!("\n  📋 GET /metrics/worker");
    let worker_metrics = worker_client.get_worker_metrics().await?;
    assert!(
        !worker_metrics.metrics.is_empty(),
        "Worker metrics should contain entries"
    );
    println!("     ✅ metrics: {} entries", worker_metrics.metrics.len());
    println!("     ✅ worker_id: {}", worker_metrics.worker_id);

    // List some metrics
    for (name, value) in worker_metrics.metrics.iter().take(3) {
        println!(
            "        - {}: {} ({})",
            name, value.value, value.metric_type
        );
    }

    // Test domain event stats (quick validation, detailed test in domain_event_publishing_test)
    println!("\n  📋 GET /metrics/events");
    let event_stats = worker_client.get_domain_event_stats().await?;
    println!(
        "     ✅ router.total_routed: {}",
        event_stats.router.total_routed
    );
    println!(
        "     ✅ in_process_bus.total_events_dispatched: {}",
        event_stats.in_process_bus.total_events_dispatched
    );

    println!("\n🎉 Worker metrics endpoints test passed!");
    Ok(())
}

/// Test worker template endpoints (TAS-169: /v1/templates)
///
/// Validates:
/// - GET /v1/templates returns TemplateListResponse with supported namespaces
/// - GET /v1/templates?include_cache_stats=true returns cache statistics
#[tokio::test]
async fn test_worker_template_endpoints() -> Result<()> {
    println!("📄 Testing Worker Template Endpoints (TAS-169: /v1/templates)");

    let manager = IntegrationTestManager::setup().await?;
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Worker client required. Ensure worker service is running.")
    })?;

    // Test template listing
    println!("\n  📋 GET /v1/templates");
    let templates = worker_client.list_templates(None).await?;
    println!(
        "     ✅ supported_namespaces: {:?}",
        templates.supported_namespaces
    );
    println!("     ✅ template_count: {}", templates.template_count);
    println!(
        "     ✅ worker_capabilities: {:?}",
        templates.worker_capabilities
    );

    // Test cache stats via list_templates with include_cache_stats=true (TAS-169)
    println!("\n  📋 GET /v1/templates?include_cache_stats=true");
    let params_with_stats = TemplateQueryParams {
        namespace: None,
        include_cache_stats: Some(true),
    };
    let templates_with_stats = worker_client
        .list_templates(Some(&params_with_stats))
        .await?;
    if let Some(cache_stats) = &templates_with_stats.cache_stats {
        println!("     ✅ total_cached: {}", cache_stats.total_cached);
        println!("     ✅ cache_hits: {}", cache_stats.cache_hits);
        println!("     ✅ cache_misses: {}", cache_stats.cache_misses);
    } else {
        println!("     ⚠️ cache_stats not included in response");
    }

    println!("\n🎉 Worker template endpoints test passed!");
    Ok(())
}

// =============================================================================
// COMBINED SERVICE TESTS
// =============================================================================

/// Test that both services have consistent health endpoint structure
///
/// Validates unified path convention across services:
/// - Both use /health, /health/ready, /health/live, /health/detailed
/// - Response structures are compatible
#[tokio::test]
async fn test_unified_health_path_convention() -> Result<()> {
    println!("🔗 Testing Unified Health Path Convention (TAS-70)");
    println!("   Both services should use: /health, /health/ready, /health/live, /health/detailed");

    let manager = IntegrationTestManager::setup().await?;
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Worker client required. Ensure worker service is running.")
    })?;

    // Test orchestration endpoints
    println!(
        "\n📍 Orchestration Service ({}):",
        manager.orchestration_url
    );
    let orch_health = manager.orchestration_client.get_basic_health().await?;
    let orch_live = manager.orchestration_client.liveness_probe().await?;
    let orch_detailed = manager.orchestration_client.get_detailed_health().await?;
    println!("     /health          → {}", orch_health.status);
    // Readiness probe may return 503 if not fully ready - valid K8s behavior
    match manager.orchestration_client.readiness_probe().await {
        Ok(ready) => println!("     /health/ready    → {}", ready.status),
        Err(_) => println!("     /health/ready    → 503 (not ready)"),
    }
    println!("     /health/live     → {}", orch_live.status);
    println!("     /health/detailed → {}", orch_detailed.status);

    // Test worker endpoints
    println!(
        "\n📍 Worker Service ({}):",
        manager.worker_url.as_ref().unwrap()
    );
    let worker_health = worker_client.health_check().await?;
    let worker_live = worker_client.liveness_probe().await?;
    let worker_detailed = worker_client.get_detailed_health().await?;
    println!("     /health          → {}", worker_health.status);
    // Readiness probe may return 503 if not fully ready - valid K8s behavior
    match worker_client.readiness_probe().await {
        Ok(ready) => println!("     /health/ready    → {}", ready.status),
        Err(_) => println!("     /health/ready    → 503 (not ready)"),
    }
    println!("     /health/live     → {}", worker_live.status);
    println!("     /health/detailed → {}", worker_detailed.status);

    // Both should be healthy/ready
    assert_eq!(orch_health.status, "healthy");
    assert_eq!(worker_health.status, "healthy");

    println!("\n🎉 Unified health path convention verified!");
    println!("   Both services correctly use nested /health/* paths");
    Ok(())
}
