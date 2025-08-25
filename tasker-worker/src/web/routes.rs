//! Worker Web API Routes
//!
//! Route definitions for all worker web endpoints organized by functionality.

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;

use crate::web::{handlers, state::WorkerWebState};

/// Health check routes for monitoring and Kubernetes probes
pub fn health_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/health/ready", get(handlers::health::readiness_check))
        .route("/health/live", get(handlers::health::liveness_check))
}

/// Metrics routes for Prometheus and monitoring systems
pub fn metrics_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/metrics", get(handlers::metrics::prometheus_metrics))
        .route("/metrics/worker", get(handlers::metrics::worker_metrics))
}

/// Worker status and information routes
pub fn worker_status_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/status", get(handlers::worker_status::worker_status))
        .route("/status/detailed", get(handlers::worker_status::detailed_status))
        .route("/status/namespaces", get(handlers::worker_status::namespace_health))
        .route("/handlers", get(handlers::worker_status::registered_handlers))
}

/// Task template management routes
pub fn template_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        // Template retrieval and listing
        .route("/templates", get(handlers::templates::list_templates))
        .route("/templates/:namespace/:name/:version", get(handlers::templates::get_template))
        // Template validation
        .route("/templates/:namespace/:name/:version/validate", post(handlers::templates::validate_template))
        // Cache management
        .route("/templates/cache", delete(handlers::templates::clear_cache))
        .route("/templates/cache/stats", get(handlers::templates::get_cache_stats))
        .route("/templates/cache/maintain", post(handlers::templates::maintain_cache))
        // Template refresh
        .route("/templates/:namespace/:name/:version/refresh", post(handlers::templates::refresh_template))
}