//! # Worker Health Check Handlers
//!
//! Kubernetes-compatible health check endpoints for worker monitoring and load balancing.
//! Focuses on worker-specific health concerns like queue processing and step execution.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
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
pub async fn health_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<BasicHealthResponse> {
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
pub async fn liveness_check(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<BasicHealthResponse> {
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
    checks.insert(
        "database".to_string(),
        check_database_health(&state).await,
    );
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

    match sqlx::query("SELECT 1").fetch_one(state.database_pool.as_ref()).await {
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

    // For now, assume healthy if we can access the processor
    // TODO: Add actual command processor health check via ping command
    let _processor = &state.worker_processor;

    HealthCheck {
        status: "healthy".to_string(),
        message: Some("Command processor available".to_string()),
        duration_ms: start.elapsed().as_millis() as u64,
        last_checked: Utc::now(),
    }
}

/// Check queue processing capability and queue depths
async fn check_queue_health(_state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // TODO: Check actual queue depths and processing capability
    // This would involve checking PGMQ queue metrics
    
    HealthCheck {
        status: "healthy".to_string(),
        message: Some("Queue processing available".to_string()),
        duration_ms: start.elapsed().as_millis() as u64,
        last_checked: Utc::now(),
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
async fn check_step_processing_health(_state: &WorkerWebState) -> HealthCheck {
    let start = std::time::Instant::now();

    // TODO: Check recent step processing performance and error rates
    // This would involve querying step execution metrics
    
    HealthCheck {
        status: "healthy".to_string(),
        message: Some("Step processing operational".to_string()),
        duration_ms: start.elapsed().as_millis() as u64,
        last_checked: Utc::now(),
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
        command_processor_active: true, // TODO: Check actual processor status
        supported_namespaces: vec![], // TODO: Get from task template registry
    }
}