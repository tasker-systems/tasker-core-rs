//! # Worker Metrics Handlers
//!
//! Prometheus-compatible metrics endpoints for worker monitoring and observability.

use axum::extract::State;
use axum::http::StatusCode;
use axum::{response::Html, Json};
use std::{collections::HashMap, sync::Arc};
use tracing::debug;

use crate::web::state::WorkerWebState;
use tasker_shared::messaging::clients::MessageClient;
use tasker_shared::metrics::channels::global_registry;
use tasker_shared::types::web::*;

/// Prometheus metrics endpoint: GET /metrics
///
/// Returns metrics in Prometheus format for scraping by monitoring systems.
pub async fn prometheus_metrics(State(state): State<Arc<WorkerWebState>>) -> Html<String> {
    debug!("Serving Prometheus metrics");

    let mut metrics = Vec::new();

    // Worker info metric
    metrics.push(format!(
        "# HELP tasker_worker_info Worker information\n# TYPE tasker_worker_info gauge\ntasker_worker_info{{version=\"{}\",worker_id=\"{}\",worker_type=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION"),
        state.worker_id(),
        state.worker_type()
    ));

    // Uptime metric
    metrics.push(format!(
        "# HELP tasker_worker_uptime_seconds Worker uptime in seconds\n# TYPE tasker_worker_uptime_seconds counter\ntasker_worker_uptime_seconds {{}} {}",
        state.uptime_seconds()
    ));

    // Database connection pool metrics
    metrics.push(format!(
        "# HELP tasker_worker_db_pool_size Database connection pool size\n# TYPE tasker_worker_db_pool_size gauge\ntasker_worker_db_pool_size {{}} {}",
        state.database_pool.size()
    ));

    // Queue depth metrics for each supported namespace
    for namespace in state.supported_namespaces().await {
        let queue_name = state.queue_name_for_namespace(&namespace);
        match state.message_client.get_queue_metrics(&queue_name).await {
            Ok(queue_metrics) => {
                metrics.push(format!(
                    "# HELP tasker_worker_queue_depth Queue depth for {} namespace\n# TYPE tasker_worker_queue_depth gauge\ntasker_worker_queue_depth{{namespace=\"{}\"}} {}",
                    namespace, namespace, queue_metrics.message_count
                ));

                if let Some(oldest_age) = queue_metrics.oldest_message_age_seconds {
                    metrics.push(format!(
                        "# HELP tasker_worker_queue_oldest_message_age_seconds Age of oldest message in queue\n# TYPE tasker_worker_queue_oldest_message_age_seconds gauge\ntasker_worker_queue_oldest_message_age_seconds{{namespace=\"{}\"}} {}",
                        namespace, oldest_age
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(namespace = %namespace, error = %e, "Failed to get queue metrics");
            }
        }
    }

    // TODO: Add step processing performance metrics:
    // - Steps processed total/success/failure counts from database
    // - Processing latency histograms from recent step executions
    // - Active commands count from WorkerProcessor

    // TAS-51: Add channel metrics
    let channel_health = global_registry()
        .get_all_health(|_channel_name| {
            // For Prometheus export, we don't need precise capacity
            // Health checks will be done via dedicated health endpoint
            Some(100) // Placeholder capacity for health reporting
        })
        .await;

    for (channel_name, component, health_status) in channel_health {
        let health_value = match health_status.as_str() {
            s if s.starts_with("healthy") => 0,
            s if s.starts_with("degraded") => 1,
            s if s.starts_with("critical") => 2,
            _ => 0,
        };

        metrics.push(format!(
            "# HELP tasker_worker_channel_health Channel health status (0=healthy, 1=degraded, 2=critical)\n# TYPE tasker_worker_channel_health gauge\ntasker_worker_channel_health{{channel_name=\"{}\",component=\"{}\"}} {}",
            channel_name, component, health_value
        ));
    }

    Html(metrics.join("\n\n"))
}

/// Worker-specific metrics endpoint: GET /metrics/worker
///
/// Returns worker metrics in JSON format for programmatic access.
pub async fn worker_metrics(
    State(state): State<Arc<WorkerWebState>>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Serving worker metrics in JSON format");

    let mut metrics = HashMap::new();

    // Basic system metrics
    metrics.insert(
        "uptime_seconds".to_string(),
        MetricValue {
            value: state.uptime_seconds() as f64,
            metric_type: "counter".to_string(),
            labels: HashMap::new(),
            help: "Worker uptime in seconds".to_string(),
        },
    );

    metrics.insert(
        "database_pool_size".to_string(),
        MetricValue {
            value: state.database_pool.size() as f64,
            metric_type: "gauge".to_string(),
            labels: HashMap::new(),
            help: "Database connection pool size".to_string(),
        },
    );

    // Queue metrics for each supported namespace
    for namespace in state.supported_namespaces().await {
        let queue_name = state.queue_name_for_namespace(&namespace);
        match state.message_client.get_queue_metrics(&queue_name).await {
            Ok(queue_metrics) => {
                let mut labels = HashMap::new();
                labels.insert("namespace".to_string(), namespace.clone());

                metrics.insert(
                    format!("queue_depth_{}", namespace),
                    MetricValue {
                        value: queue_metrics.message_count as f64,
                        metric_type: "gauge".to_string(),
                        labels: labels.clone(),
                        help: format!("Queue depth for {} namespace", namespace),
                    },
                );

                if let Some(oldest_age) = queue_metrics.oldest_message_age_seconds {
                    metrics.insert(
                        format!("queue_oldest_message_age_{}", namespace),
                        MetricValue {
                            value: oldest_age as f64,
                            metric_type: "gauge".to_string(),
                            labels,
                            help: format!("Age of oldest message in {} queue", namespace),
                        },
                    );
                }
            }
            Err(e) => {
                tracing::warn!(namespace = %namespace, error = %e, "Failed to get queue metrics for JSON response");
            }
        }
    }

    // TODO: Add step processing performance metrics:
    // - step_processing_rate from database queries
    // - error_rate from recent step executions
    // - queue_processing_latency from timing measurements
    // - active_commands_count from WorkerProcessor

    Ok(Json(MetricsResponse {
        metrics,
        timestamp: chrono::Utc::now(),
        worker_id: state.worker_id(),
    }))
}

/// Domain event statistics endpoint: GET /metrics/events
///
/// Returns statistics about domain event routing and delivery paths.
/// Used for monitoring event publishing and by E2E tests to verify
/// events were published through the expected delivery paths.
///
/// # Response
///
/// Returns statistics for:
/// - **Router stats**: durable_routed, fast_routed, broadcast_routed counts
/// - **In-process bus stats**: handler dispatches, FFI channel dispatches
///
/// # Example Response
///
/// ```json
/// {
///   "router": {
///     "total_routed": 42,
///     "durable_routed": 10,
///     "fast_routed": 30,
///     "broadcast_routed": 2
///   },
///   "in_process_bus": {
///     "total_events_dispatched": 32,
///     "rust_handler_dispatches": 20,
///     "ffi_channel_dispatches": 12
///   },
///   "captured_at": "2025-11-30T12:00:00Z",
///   "worker_id": "worker-01234567"
/// }
/// ```
pub async fn domain_event_stats(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<DomainEventStats> {
    debug!("Serving domain event statistics");

    // Use cached event components - does not lock worker core
    let stats = state.domain_event_stats().await;

    Json(stats)
}
