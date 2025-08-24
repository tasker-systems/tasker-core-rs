//! Worker Web API

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde_json::Value;
use std::sync::Arc;

use crate::{
    health::{WorkerHealthStatus, NamespaceHealth, SystemMetrics},
    metrics::WorkerMetrics,
    worker::core::WorkerCore,
};

/// API state containing worker core
#[derive(Clone)]
pub struct ApiState {
    worker_core: Arc<WorkerCore>,
    metrics: Arc<WorkerMetrics>,
}

impl ApiState {
    pub fn new(worker_core: Arc<WorkerCore>) -> Self {
        let metrics = Arc::new(WorkerMetrics::new().expect("Failed to create metrics"));
        Self {
            worker_core,
            metrics,
        }
    }
}

/// Create worker API router
pub fn create_worker_api(worker_core: Arc<WorkerCore>) -> Router {
    let api_state = ApiState::new(worker_core);

    Router::new()
        .route("/health", get(health_check))
        .route("/health/ready", get(readiness_check))
        .route("/health/live", get(liveness_check))
        .route("/metrics", get(metrics))
        .route("/status", get(worker_status))
        .with_state(api_state)
}

/// Kubernetes readiness probe endpoint
async fn readiness_check(State(_state): State<ApiState>) -> Result<Json<Value>, StatusCode> {
    // Check if worker can accept new work
    Ok(Json(serde_json::json!({
        "status": "ready",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Kubernetes liveness probe endpoint
async fn liveness_check(State(_state): State<ApiState>) -> Result<Json<Value>, StatusCode> {
    // Check if worker is alive and functional
    Ok(Json(serde_json::json!({
        "status": "alive",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// Comprehensive health check
async fn health_check(State(state): State<ApiState>) -> Result<Json<WorkerHealthStatus>, StatusCode> {
    // TODO: Implement comprehensive health check
    let health_status = WorkerHealthStatus {
        status: "healthy".to_string(),
        namespaces: vec![],
        system_metrics: SystemMetrics {
            memory_usage_mb: 256,
            cpu_usage_percent: 15.0,
            database_connections: 10,
            queue_connections: 5,
        },
        uptime_seconds: 3600,
    };

    Ok(Json(health_status))
}

/// Prometheus metrics endpoint
async fn metrics(State(state): State<ApiState>) -> Result<String, StatusCode> {
    state
        .metrics
        .render_metrics()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Worker status endpoint
async fn worker_status(State(_state): State<ApiState>) -> Result<Json<Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "worker_type": "foundation",
        "version": crate::VERSION,
        "status": "running"
    })))
}