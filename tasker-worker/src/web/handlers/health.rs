//! # Worker Health Check Handlers
//!
//! Kubernetes-compatible health check endpoints for worker monitoring and load balancing.
//! Focuses on worker-specific health concerns like queue processing and step execution.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use tracing::{debug, error, warn};

use crate::web::{response_types::*, state::WorkerWebState};

/// Basic health check response
#[derive(Debug, Clone, Serialize)]
pub struct BasicHealthResponse {
    status: String,
    timestamp: DateTime<Utc>,
    worker_id: String,
}

/// Detailed health check response with subsystem checks
#[derive(Debug, Clone, Serialize)]
pub struct DetailedHealthResponse {
    status: String,
    timestamp: DateTime<Utc>,
    worker_id: String,
    checks: HashMap<String, HealthCheck>,
    system_info: WorkerSystemInfo,
}

/// Individual health check result
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    status: String,
    message: Option<String>,
    duration_ms: u64,
    last_checked: DateTime<Utc>,
}

/// Worker system information
#[derive(Debug, Clone, Serialize)]
pub struct WorkerSystemInfo {
    version: String,
    environment: String,
    uptime_seconds: u64,
    worker_type: String,
    database_pool_size: u32,
    command_processor_active: bool,
    supported_namespaces: Vec<String>,
}

/// Basic health check endpoint: GET /health
///
/// Simple health check that returns OK if the worker service is running.
/// Always available, even during graceful shutdown.
pub async fn health_check(State(state): State<Arc<WorkerWebState>>) -> Json<BasicHealthResponse> {
    Json(BasicHealthResponse {
        status: "ok".to_string(),
        timestamp: Utc::now(),
        worker_id: state.worker_id(),
    })
}

/// Kubernetes readiness probe: GET /health/ready
///
/// Indicates whether the worker is ready to process steps.
/// Checks database connectivity, command processor status, and queue health.
pub async fn readiness_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<DetailedHealthResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Performing worker readiness probe");

    let mut checks = HashMap::new();
    let mut overall_healthy = true;

    // Check database connectivity
    let db_check = check_database_health(&state).await;
    overall_healthy = overall_healthy && db_check.status == "healthy";
    checks.insert("database".to_string(), db_check);

    // Check command processor health
    let processor_check = check_command_processor_health(&state).await;
    overall_healthy = overall_healthy && processor_check.status == "healthy";
    checks.insert("command_processor".to_string(), processor_check);

    // Check queue processing capability
    let queue_check = check_queue_health(&state).await;
    overall_healthy = overall_healthy && queue_check.status == "healthy";
    checks.insert("queue_processing".to_string(), queue_check);

    let response = DetailedHealthResponse {
        status: if overall_healthy {
            "ready"
        } else {
            "not_ready"
        }
        .to_string(),
        timestamp: Utc::now(),
        worker_id: state.worker_id(),
        checks,
        system_info: create_system_info(&state),
    };

    if overall_healthy {
        Ok(Json(response))
    } else {
        warn!("Worker readiness check failed");
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "service_unavailable".to_string(),
                message: "Worker is not ready to process steps".to_string(),
                timestamp: Utc::now(),
                request_id: None,
            }),
        ))
    }
}

/// Kubernetes liveness probe: GET /health/live
///
/// Indicates whether the worker is alive and should not be restarted.
/// Simple check focusing on basic process responsiveness.
pub async fn liveness_check(State(state): State<Arc<WorkerWebState>>) -> Json<BasicHealthResponse> {
    // Check if we can access our state (basic process health)
    let _uptime = state.uptime_seconds();

    Json(BasicHealthResponse {
        status: "alive".to_string(),
        timestamp: Utc::now(),
        worker_id: state.worker_id(),
    })
}

/// Comprehensive health check: GET /health/detailed
///
/// Detailed health information about all worker subsystems.
/// Includes performance metrics and diagnostic information.
pub async fn detailed_health_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<DetailedHealthResponse> {
    debug!("Performing detailed worker health check");

    let mut checks = HashMap::new();

    // Run all health checks
    checks.insert("database".to_string(), check_database_health(&state).await);
    checks.insert(
        "command_processor".to_string(),
        check_command_processor_health(&state).await,
    );
    checks.insert(
        "queue_processing".to_string(),
        check_queue_health(&state).await,
    );
    checks.insert(
        "event_system".to_string(),
        check_event_system_health(&state).await,
    );
    checks.insert(
        "step_processing".to_string(),
        check_step_processing_health(&state).await,
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
        timestamp: Utc::now(),
        worker_id: state.worker_id(),
        checks,
        system_info: create_system_info(&state),
    })
}

// Helper functions for health checks

/// Check database connectivity and basic query performance
async fn check_database_health(state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    match sqlx::query("SELECT 1")
        .fetch_one(state.database_pool.as_ref())
        .await
    {
        Ok(_) => HealthCheck {
            status: "healthy".to_string(),
            message: Some("Database connection successful".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
            last_checked: Utc::now(),
        },
        Err(e) => {
            error!(error = %e, "Worker database health check failed");
            HealthCheck {
                status: "unhealthy".to_string(),
                message: Some(format!("Database connection failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
    }
}

/// Check command processor availability and responsiveness
async fn check_command_processor_health(state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // Use WorkerCore's get_health method directly
    match state.worker_core.get_health().await {
        Ok(health_status) => {
            let is_healthy = health_status.status == "healthy"
                && health_status.database_connected
                && health_status.orchestration_api_reachable;

            HealthCheck {
                status: if is_healthy { "healthy" } else { "degraded" }.to_string(),
                message: Some(format!(
                    "Worker status: {}, DB connected: {}, API reachable: {}",
                    health_status.status,
                    health_status.database_connected,
                    health_status.orchestration_api_reachable
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to get worker health status");
            HealthCheck {
                status: "unhealthy".to_string(),
                message: Some(format!("Worker health check failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
    }
}

/// Check queue processing capability and queue depths
async fn check_queue_health(state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // Check PGMQ queue metrics for worker namespaces
    match sqlx::query(
        "SELECT queue_name, queue_length FROM pgmq.metrics_all() WHERE queue_name LIKE '%_queue'",
    )
    .fetch_all(state.database_pool.as_ref())
    .await
    {
        Ok(rows) => {
            let total_queue_depth: i64 = rows
                .iter()
                .filter_map(|row| row.try_get::<i64, _>("queue_length").ok())
                .sum();

            // Consider healthy if total queue depth is reasonable
            let status = if total_queue_depth < 10000 {
                "healthy"
            } else if total_queue_depth < 50000 {
                "degraded"
            } else {
                "unhealthy"
            };

            HealthCheck {
                status: status.to_string(),
                message: Some(format!(
                    "Queue processing available, total depth: {total_queue_depth}"
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to check queue metrics");
            HealthCheck {
                status: "unhealthy".to_string(),
                message: Some(format!("Queue metrics unavailable: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
    }
}

/// Check event system health and connectivity
async fn check_event_system_health(_state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // TODO: Check event publisher/subscriber health
    // For now, assume healthy if configuration allows events

    HealthCheck {
        status: "healthy".to_string(),
        message: Some("Event system available".to_string()),
        duration_ms: start.elapsed().as_millis() as u64,
        last_checked: Utc::now(),
    }
}

/// Check step processing performance and error rates
async fn check_step_processing_health(state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // Use WorkerCore's get_processing_stats method directly
    match state.worker_core.get_processing_stats().await {
        Ok(worker_status) => {
            // Calculate error rate and determine health
            let total_executions = worker_status.steps_executed;
            let error_rate = if total_executions > 0 {
                (worker_status.steps_failed as f64 / total_executions as f64) * 100.0
            } else {
                0.0
            };

            let status = if error_rate < 5.0 {
                "healthy"
            } else if error_rate < 15.0 {
                "degraded"
            } else {
                "unhealthy"
            };

            HealthCheck {
                status: status.to_string(),
                message: Some(format!(
                    "Step processing operational - {} executed, {:.1}% error rate",
                    total_executions, error_rate
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to get worker processing stats");
            HealthCheck {
                status: "unhealthy".to_string(),
                message: Some(format!("Step processing stats failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
    }
}

/// Create system information summary
fn create_system_info(state: &WorkerWebState) -> WorkerSystemInfo {
    WorkerSystemInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
        uptime_seconds: state.uptime_seconds(),
        worker_type: state.worker_type(),
        database_pool_size: state.database_pool.size(),
        command_processor_active: matches!(
            state.worker_core.status(),
            crate::worker::core::WorkerCoreStatus::Running
        ),
        supported_namespaces: state.worker_core.supported_namespaces().to_vec(),
    }
}
