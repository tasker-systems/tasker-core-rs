//! # Worker Health Status
//!
//! Health status types and utilities for worker monitoring

use serde::{Deserialize, Serialize};

/// Worker health status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthStatus {
    /// Overall status (healthy, degraded, unhealthy)
    pub status: String,
    
    /// Database connectivity status
    pub database_connected: bool,
    
    /// Orchestration API reachability
    pub orchestration_api_reachable: bool,
    
    /// Namespaces this worker supports
    pub supported_namespaces: Vec<String>,
    
    /// Number of cached templates
    pub cached_templates: usize,
    
    /// Total messages processed
    pub total_messages_processed: u64,
    
    /// Successful step executions
    pub successful_executions: u64,
    
    /// Failed step executions
    pub failed_executions: u64,
}

impl Default for WorkerHealthStatus {
    fn default() -> Self {
        Self {
            status: "unknown".to_string(),
            database_connected: false,
            orchestration_api_reachable: false,
            supported_namespaces: Vec::new(),
            cached_templates: 0,
            total_messages_processed: 0,
            successful_executions: 0,
            failed_executions: 0,
        }
    }
}

impl WorkerHealthStatus {
    /// Create a healthy status
    pub fn healthy() -> Self {
        Self {
            status: "healthy".to_string(),
            database_connected: true,
            orchestration_api_reachable: true,
            ..Default::default()
        }
    }

    /// Create a degraded status (some issues but still functional)
    pub fn degraded(reason: &str) -> Self {
        Self {
            status: format!("degraded: {}", reason),
            ..Default::default()
        }
    }

    /// Create an unhealthy status
    pub fn unhealthy(reason: &str) -> Self {
        Self {
            status: format!("unhealthy: {}", reason),
            database_connected: false,
            orchestration_api_reachable: false,
            ..Default::default()
        }
    }

    /// Check if the worker is considered healthy
    pub fn is_healthy(&self) -> bool {
        self.status == "healthy" && self.database_connected && self.orchestration_api_reachable
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_messages_processed == 0 {
            100.0 // No failures if no processing
        } else {
            (self.successful_executions as f64 / self.total_messages_processed as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_health_status_creation() {
        let healthy = WorkerHealthStatus::healthy();
        assert!(healthy.is_healthy());
        assert_eq!(healthy.status, "healthy");

        let degraded = WorkerHealthStatus::degraded("high latency");
        assert!(!degraded.is_healthy());
        assert!(degraded.status.contains("degraded"));

        let unhealthy = WorkerHealthStatus::unhealthy("database down");
        assert!(!unhealthy.is_healthy());
        assert!(unhealthy.status.contains("unhealthy"));
    }

    #[test]
    fn test_success_rate_calculation() {
        let mut status = WorkerHealthStatus::default();
        status.total_messages_processed = 100;
        status.successful_executions = 95;
        status.failed_executions = 5;

        assert_eq!(status.success_rate(), 95.0);

        // Test with no messages processed
        let empty_status = WorkerHealthStatus::default();
        assert_eq!(empty_status.success_rate(), 100.0);
    }
}