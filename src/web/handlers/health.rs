//! # Health Check Handlers
//!
//! Kubernetes-compatible health check endpoints for monitoring and load balancing.

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, error};

use crate::web::errors::ApiError;
use crate::web::state::AppState;

#[cfg(feature = "web-api")]
// use utoipa::ToSchema;

/// Basic health check response
#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    timestamp: String,
}

/// Detailed health check response
#[derive(Serialize)]
pub struct DetailedHealthResponse {
    status: String,
    timestamp: String,
    checks: HashMap<String, HealthCheck>,
    info: HealthInfo,
}

/// Individual health check result
#[derive(Serialize)]
pub struct HealthCheck {
    status: String,
    message: Option<String>,
    duration_ms: u64,
}

/// System information for detailed health
#[derive(Serialize)]
pub struct HealthInfo {
    version: String,
    environment: String,
    operational_state: String,
    web_database_pool_size: u32,
    orchestration_database_pool_size: u32,
    circuit_breaker_state: String,
}

/// Basic health check endpoint: GET /health
///
/// Simple health check that returns OK if the service is running.
/// This endpoint is always available, even during graceful shutdown.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is running", body = crate::web::openapi::HealthResponse)
    ),
    tag = "health"
))]
pub async fn basic_health(_state: State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Kubernetes readiness probe: GET /ready
///
/// Indicates whether the service is ready to accept traffic.
/// Checks database connectivity and circuit breaker status.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = crate::web::openapi::DetailedHealthResponse),
        (status = 503, description = "Service is not ready", body = crate::web::openapi::ApiError)
    ),
    tag = "health"
))]
pub async fn readiness_probe(
    State(state): State<AppState>,
) -> Result<Json<DetailedHealthResponse>, ApiError> {
    debug!("Performing readiness probe");

    let _start_time = std::time::Instant::now();
    let mut checks = HashMap::new();
    let mut overall_healthy = true;

    // Check web database connectivity
    let web_db_check = check_database_health(&state.web_db_pool, "web_database").await;
    overall_healthy = overall_healthy && web_db_check.status == "healthy";
    checks.insert("web_database".to_string(), web_db_check);

    // Check orchestration database connectivity
    let orch_db_check =
        check_database_health(&state.orchestration_db_pool, "orchestration_database").await;
    overall_healthy = overall_healthy && orch_db_check.status == "healthy";
    checks.insert("orchestration_database".to_string(), orch_db_check);

    // Check circuit breaker status
    let cb_check = check_circuit_breaker_health(&state).await;
    overall_healthy = overall_healthy && cb_check.status == "healthy";
    checks.insert("circuit_breaker".to_string(), cb_check);

    // Check orchestration system status
    let orch_check = check_orchestration_health(&state).await;
    overall_healthy = overall_healthy && orch_check.status == "healthy";
    checks.insert("orchestration_system".to_string(), orch_check);

    let response = DetailedHealthResponse {
        status: if overall_healthy {
            "ready"
        } else {
            "not_ready"
        }
        .to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks,
        info: create_health_info(&state),
    };

    if overall_healthy {
        Ok(Json(response))
    } else {
        Err(ApiError::ServiceUnavailable)
    }
}

/// Kubernetes liveness probe: GET /live
///
/// Indicates whether the service is alive and should not be restarted.
/// This is a simpler check than readiness - mainly checks if the process is responsive.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/live",
    responses(
        (status = 200, description = "Service is alive", body = crate::web::openapi::HealthResponse)
    ),
    tag = "health"
))]
pub async fn liveness_probe(State(state): State<AppState>) -> Json<HealthResponse> {
    // Check if we can access our state (basic process health)
    let _operational_state = state.operational_state();

    Json(HealthResponse {
        status: "alive".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Detailed health status: GET /health/detailed
///
/// Comprehensive health check with detailed information about all subsystems.
/// May require authentication depending on configuration.
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/health/detailed",
    responses(
        (status = 200, description = "Detailed health information", body = crate::web::openapi::DetailedHealthResponse)
    ),
    tag = "health"
))]
pub async fn detailed_health(State(state): State<AppState>) -> Json<DetailedHealthResponse> {
    debug!("Performing detailed health check");

    let mut checks = HashMap::new();

    // Run all health checks
    checks.insert(
        "web_database".to_string(),
        check_database_health(&state.web_db_pool, "web_database").await,
    );
    checks.insert(
        "orchestration_database".to_string(),
        check_database_health(&state.orchestration_db_pool, "orchestration_database").await,
    );
    checks.insert(
        "circuit_breaker".to_string(),
        check_circuit_breaker_health(&state).await,
    );
    checks.insert(
        "orchestration_system".to_string(),
        check_orchestration_health(&state).await,
    );

    // Determine overall status
    let overall_healthy = checks.values().all(|check| check.status == "healthy");

    Json(DetailedHealthResponse {
        status: if overall_healthy {
            "healthy"
        } else {
            "degraded"
        }
        .to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks,
        info: create_health_info(&state),
    })
}

/// Prometheus metrics endpoint: GET /metrics
///
/// Returns metrics in Prometheus format for monitoring and alerting.
/// This will be a placeholder implementation for now.
pub async fn prometheus_metrics(State(_state): State<AppState>) -> Result<String, ApiError> {
    // Note: Comprehensive metrics collection integration pending
    Ok("# HELP tasker_web_api_info Web API information\n# TYPE tasker_web_api_info gauge\ntasker_web_api_info{version=\"0.1.0\"} 1\n".to_string())
}

// Helper functions for health checks

async fn check_database_health(pool: &sqlx::PgPool, name: &str) -> HealthCheck {
    let start = std::time::Instant::now();

    match sqlx::query("SELECT 1").fetch_one(pool).await {
        Ok(_) => HealthCheck {
            status: "healthy".to_string(),
            message: None,
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => {
            error!(database = name, error = %e, "Database health check failed");
            HealthCheck {
                status: "unhealthy".to_string(),
                message: Some(format!("Database connection failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
}

async fn check_circuit_breaker_health(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();

    let is_healthy = state.is_database_healthy();
    let circuit_state = state.web_db_circuit_breaker.current_state();

    HealthCheck {
        status: if is_healthy { "healthy" } else { "degraded" }.to_string(),
        message: Some(format!("Circuit breaker state: {circuit_state:?}")),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

async fn check_orchestration_health(state: &AppState) -> HealthCheck {
    let start = std::time::Instant::now();
    let status = state.orchestration_status.read();

    HealthCheck {
        status: if status.running {
            "healthy"
        } else {
            "unhealthy"
        }
        .to_string(),
        message: Some(format!("Operational state: {:?}", status.operational_state)),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn create_health_info(state: &AppState) -> HealthInfo {
    let status = state.orchestration_status.read();

    HealthInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: status.environment.clone(),
        operational_state: format!("{:?}", status.operational_state),
        web_database_pool_size: state.web_db_pool.size(),
        orchestration_database_pool_size: status.database_pool_size,
        circuit_breaker_state: format!("{:?}", state.web_db_circuit_breaker.current_state()),
    }
}
