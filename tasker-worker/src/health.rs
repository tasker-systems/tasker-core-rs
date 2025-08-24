//! Worker health monitoring utilities

use serde::Serialize;

/// Worker health status
#[derive(Debug, Clone, Serialize)]
pub struct WorkerHealthStatus {
    pub status: String,
    pub namespaces: Vec<NamespaceHealth>,
    pub system_metrics: SystemMetrics,
    pub uptime_seconds: u64,
}

/// Namespace health information
#[derive(Debug, Clone, Serialize)]
pub struct NamespaceHealth {
    pub namespace: String,
    pub queue_depth: u64,
    pub processing_rate: f64,
    pub active_executors: u32,
    pub health_status: String,
}

/// System metrics
#[derive(Debug, Clone, Serialize)]
pub struct SystemMetrics {
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
    pub database_connections: u32,
    pub queue_connections: u32,
}