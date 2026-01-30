//! # Operational Status Types
//!
//! Types for tracking operational status of the API layer.
//! These are shared between REST and gRPC APIs.

use tasker_shared::types::web::SystemOperationalState;

/// Database pool usage statistics for monitoring (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct DatabasePoolUsageStats {
    pub pool_name: String,
    pub active_connections: u32,
    pub max_connections: u32,
    pub usage_ratio: f64,
    pub is_healthy: bool,
}

/// Operational status tracking for API integration
#[derive(Debug, Clone)]
pub struct OrchestrationStatus {
    pub running: bool,
    pub environment: String,
    pub operational_state: SystemOperationalState,
    pub database_pool_size: u32,
    pub last_health_check: std::time::Instant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_database_pool_usage_stats_construction() {
        let stats = DatabasePoolUsageStats {
            pool_name: "web_api_pool".to_string(),
            active_connections: 5,
            max_connections: 20,
            usage_ratio: 0.25,
            is_healthy: true,
        };

        assert_eq!(stats.pool_name, "web_api_pool");
        assert_eq!(stats.active_connections, 5);
        assert_eq!(stats.max_connections, 20);
        assert_eq!(stats.usage_ratio, 0.25);
        assert!(stats.is_healthy);
    }

    #[test]
    fn test_database_pool_usage_stats_unhealthy() {
        let stats = DatabasePoolUsageStats {
            pool_name: "overloaded_pool".to_string(),
            active_connections: 18,
            max_connections: 20,
            usage_ratio: 0.9,
            is_healthy: false,
        };

        assert!(!stats.is_healthy);
        assert!(stats.usage_ratio > 0.75);
    }

    #[test]
    fn test_database_pool_usage_stats_clone() {
        let stats = DatabasePoolUsageStats {
            pool_name: "test".to_string(),
            active_connections: 1,
            max_connections: 10,
            usage_ratio: 0.1,
            is_healthy: true,
        };

        let cloned = stats.clone();
        assert_eq!(stats.pool_name, cloned.pool_name);
        assert_eq!(stats.usage_ratio, cloned.usage_ratio);
    }

    #[test]
    fn test_orchestration_status_construction() {
        let status = OrchestrationStatus {
            running: true,
            environment: "test".to_string(),
            operational_state: SystemOperationalState::Normal,
            database_pool_size: 10,
            last_health_check: Instant::now(),
        };

        assert!(status.running);
        assert_eq!(status.environment, "test");
        assert!(matches!(
            status.operational_state,
            SystemOperationalState::Normal
        ));
    }

    #[test]
    fn test_orchestration_status_shutdown_state() {
        let status = OrchestrationStatus {
            running: false,
            environment: "production".to_string(),
            operational_state: SystemOperationalState::GracefulShutdown,
            database_pool_size: 20,
            last_health_check: Instant::now(),
        };

        assert!(!status.running);
        assert!(matches!(
            status.operational_state,
            SystemOperationalState::GracefulShutdown
        ));
    }

    #[test]
    fn test_orchestration_status_debug() {
        let status = OrchestrationStatus {
            running: true,
            environment: "dev".to_string(),
            operational_state: SystemOperationalState::Normal,
            database_pool_size: 5,
            last_health_check: Instant::now(),
        };

        let debug = format!("{:?}", status);
        assert!(debug.contains("running: true"));
        assert!(debug.contains("dev"));
    }
}
