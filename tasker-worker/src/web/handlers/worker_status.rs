//! # Worker Status Handlers
//!
//! Endpoints for detailed worker status information and diagnostics.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use tracing::debug;

use crate::web::state::WorkerWebState;
use tasker_shared::messaging::clients::MessageClient;
use tasker_shared::types::web::*;

/// Basic worker status: GET /status
///
/// Returns current worker status and basic information.
pub async fn worker_status(State(state): State<Arc<WorkerWebState>>) -> Json<WorkerStatusResponse> {
    debug!("Serving worker status");

    Json(WorkerStatusResponse {
        worker_id: state.worker_id(),
        worker_type: state.worker_type(),
        status: "active".to_string(),
        uptime_seconds: state.uptime_seconds(),
        configuration: create_configuration_status(&state).await,
        capabilities: create_capabilities_info(&state),
        performance_metrics: create_performance_metrics(&state).await,
        registered_handlers: create_registered_handlers(&state).await,
    })
}

/// Detailed worker status: GET /status/detailed
///
/// Returns comprehensive worker status with diagnostic information.
pub async fn detailed_status(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<WorkerStatusResponse> {
    debug!("Serving detailed worker status");

    // For now, detailed status is the same as basic status
    // TODO: Add more detailed diagnostic information
    worker_status(State(state)).await
}

/// Registered handlers information: GET /handlers
///
/// Returns information about registered step handlers.
pub async fn registered_handlers(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<Vec<RegisteredHandler>> {
    debug!("Serving registered handlers information");

    Json(create_registered_handlers(&state).await)
}

/// Namespace health information: GET /status/namespaces
///
/// Returns health status for each supported namespace including queue metrics.
pub async fn namespace_health(
    State(state): State<Arc<WorkerWebState>>,
) -> Json<Vec<NamespaceHealth>> {
    debug!("Serving namespace health information");

    let mut namespace_health = Vec::new();

    for namespace in state.supported_namespaces().await {
        let queue_name = state.queue_name_for_namespace(&namespace);

        let (queue_depth, health_status, metrics) =
            match state.message_client.get_queue_metrics(&queue_name).await {
                Ok(metrics) => {
                    let depth = metrics.message_count as u64;
                    let status = if depth < 100 {
                        "healthy"
                    } else if depth < 500 {
                        "warning"
                    } else {
                        "critical"
                    };
                    (depth, status.to_string(), metrics)
                }
                Err(_) => (
                    0,
                    "unknown".to_string(),
                    pgmq_notify::types::QueueMetrics::default(),
                ),
            };

        namespace_health.push(NamespaceHealth {
            namespace: namespace.clone(),
            queue_depth,
            health_status,
            queue_metrics: metrics,
        });
    }

    Json(namespace_health)
}

// Helper functions for status creation

async fn create_configuration_status(state: &WorkerWebState) -> WorkerConfigurationStatus {
    use tasker_shared::config::tasker::tasker_v2::DeploymentMode;

    WorkerConfigurationStatus {
        environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
        database_connected: true, // TODO: Check actual database status
        event_system_enabled: state
            .system_config
            .worker
            .as_ref()
            .map(|w| matches!(w.event_systems.worker.deployment_mode, DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid))
            .unwrap_or(false),
        supported_namespaces: state.supported_namespaces().await,
    }
}

fn create_capabilities_info(state: &WorkerWebState) -> WorkerCapabilities {
    WorkerCapabilities {
        can_process_steps: true,
        can_initialize_tasks: true,
        event_publishing_enabled: true, // TODO: Check actual event system status
        health_monitoring_enabled: state.config.metrics_enabled,
        supported_message_types: vec![
            "SimpleStepMessage".to_string(),
            "StepCompletionEvent".to_string(),
        ],
    }
}

async fn create_performance_metrics(state: &WorkerWebState) -> WorkerPerformanceMetrics {
    // Calculate total queue depth across all namespaces
    let mut _total_queue_depth = 0;
    let mut processing_rate = 0.0;

    for namespace in state.supported_namespaces().await {
        let queue_name = state.queue_name_for_namespace(&namespace);
        if let Ok(metrics) = state.message_client.get_queue_metrics(&queue_name).await {
            _total_queue_depth += metrics.message_count;
            // Simple processing rate estimation based on queue activity
            processing_rate += if metrics.message_count > 0 { 1.0 } else { 0.0 };
        }
    }

    WorkerPerformanceMetrics {
        steps_processed_total: 0,   // TODO: Get from database step execution history
        steps_processed_success: 0, // TODO: Get from database successful step executions
        steps_processed_failed: 0,  // TODO: Get from database failed step executions
        average_processing_time_ms: 0.0, // TODO: Calculate from recent step timing
        queue_processing_rate: processing_rate,
        last_step_processed: None, // TODO: Get timestamp from last processed step
        error_details: vec![],     // TODO: Get recent errors from database
    }
}

async fn create_registered_handlers(state: &WorkerWebState) -> Vec<RegisteredHandler> {
    // Create placeholder handlers for each supported namespace
    let mut handlers = Vec::new();

    for namespace in state.supported_namespaces().await {
        handlers.push(RegisteredHandler {
            namespace: namespace.clone(),
            handler_class: format!("{}::{}Handler", namespace, namespace),
            version: "1.0.0".to_string(),
            last_used: None,  // TODO: Get from actual usage tracking
            success_count: 0, // TODO: Get from database metrics
            failure_count: 0, // TODO: Get from database metrics
        });
    }

    // TODO: Get actual registered handlers from TaskTemplateRegistry
    // This would involve querying the registry for all registered handlers
    handlers
}
