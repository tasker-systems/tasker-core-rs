//! Worker Web API Response Types
//!
//! Standard response structures for worker web endpoints following the same
//! patterns as the orchestration web API for consistency.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthStatus {
    pub status: String,
    pub namespaces: Vec<NamespaceHealth>,
    pub system_metrics: WorkerSystemMetrics,
    pub uptime_seconds: u64,
    pub worker_id: String,
    pub worker_type: String,
}

/// Health status for individual namespaces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceHealth {
    pub namespace: String,
    pub queue_depth: u64,
    pub processing_rate: f64,
    pub active_steps: u32,
    pub health_status: String,
    pub last_processed: Option<DateTime<Utc>>,
}

/// System metrics for the worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSystemMetrics {
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
    pub active_commands: u32,
    pub total_commands_processed: u64,
    pub error_rate_percent: f64,
    pub last_activity: Option<DateTime<Utc>>,
}

/// Simple health check responses for Kubernetes probes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// Worker status response with detailed information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatusResponse {
    pub worker_id: String,
    pub worker_type: String,
    pub status: String,
    pub uptime_seconds: u64,
    pub configuration: WorkerConfigurationStatus,
    pub capabilities: WorkerCapabilities,
    pub performance_metrics: WorkerPerformanceMetrics,
    pub registered_handlers: Vec<RegisteredHandler>,
}

/// Configuration status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigurationStatus {
    pub environment: String,
    pub database_connected: bool,
    pub event_system_enabled: bool,
    pub command_buffer_size: usize,
    pub supported_namespaces: Vec<String>,
}

/// Worker capabilities information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub can_process_steps: bool,
    pub can_initialize_tasks: bool,
    pub event_publishing_enabled: bool,
    pub health_monitoring_enabled: bool,
    pub supported_message_types: Vec<String>,
}

/// Performance metrics for the worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPerformanceMetrics {
    pub steps_processed_total: u64,
    pub steps_processed_success: u64,
    pub steps_processed_failed: u64,
    pub average_processing_time_ms: f64,
    pub queue_processing_rate: f64,
    pub last_step_processed: Option<DateTime<Utc>>,
    pub error_details: Vec<RecentError>,
}

/// Information about registered step handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredHandler {
    pub namespace: String,
    pub handler_class: String,
    pub version: String,
    pub last_used: Option<DateTime<Utc>>,
    pub success_count: u64,
    pub failure_count: u64,
}

/// Recent error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentError {
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub message: String,
    pub namespace: Option<String>,
    pub step_name: Option<String>,
}

/// Prometheus-compatible metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResponse {
    pub metrics: HashMap<String, MetricValue>,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
}

/// Individual metric value with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub value: f64,
    pub metric_type: String,
    pub labels: HashMap<String, String>,
    pub help: String,
}

/// Standard error response following orchestration patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub request_id: Option<String>,
}