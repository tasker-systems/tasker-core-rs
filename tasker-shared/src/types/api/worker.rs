//! # Worker API Types
//!
//! Types for worker web API endpoints including templates, health checks, and cache operations.
//!
//! TAS-76: Common template types moved to `types::api::templates`. Worker-specific
//! types (with cache info, handler metadata) remain here.

use crate::types::base::CacheStats;
use crate::{models::core::task_template::ResolvedTaskTemplate, types::HandlerMetadata};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export shared template types for convenience
pub use super::templates::{TemplatePathParams, TemplateQueryParams as BaseTemplateQueryParams};

// =============================================================================
// Template Types (Worker-Specific)
// =============================================================================

/// Query parameters for worker template listing
///
/// Extends base query params with worker-specific options.
#[derive(Debug, Default, Deserialize)]
pub struct TemplateQueryParams {
    /// Filter by namespace
    pub namespace: Option<String>,
    /// Include cache statistics (worker-specific)
    pub include_cache_stats: Option<bool>,
}

/// Response for template retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct TemplateResponse {
    #[cfg_attr(feature = "web-api", schema(value_type = Object))]
    pub template: ResolvedTaskTemplate,
    pub handler_metadata: HandlerMetadata,
    pub cached: bool,
    pub cache_age_seconds: Option<u64>,
    pub access_count: Option<u64>,
}

/// Response for template listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct TemplateListResponse {
    pub supported_namespaces: Vec<String>,
    pub template_count: usize,
    pub cache_stats: Option<CacheStats>,
    pub worker_capabilities: Vec<String>,
}

/// Response for cache operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct CacheOperationResponse {
    pub operation: String,
    pub success: bool,
    pub cache_stats: CacheStats,
}

/// Response for template validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct TemplateValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub step_handlers: Vec<String>,
}

// =============================================================================
// Health Check Types
// =============================================================================

/// Basic health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct BasicHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
}

/// TAS-76: Typed readiness checks for worker readiness probe
///
/// These are the core checks required to determine if the worker can accept work.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct WorkerReadinessChecks {
    pub database: HealthCheck,
    pub command_processor: HealthCheck,
    pub queue_processing: HealthCheck,
}

impl WorkerReadinessChecks {
    /// Check if all readiness checks passed
    pub fn all_healthy(&self) -> bool {
        self.database.is_healthy()
            && self.command_processor.is_healthy()
            && self.queue_processing.is_healthy()
    }
}

/// TAS-76: Typed detailed checks for worker health
///
/// Comprehensive health checks for all worker subsystems.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct WorkerDetailedChecks {
    pub database: HealthCheck,
    pub command_processor: HealthCheck,
    pub queue_processing: HealthCheck,
    pub event_system: HealthCheck,
    pub step_processing: HealthCheck,
    pub circuit_breakers: HealthCheck,
}

impl WorkerDetailedChecks {
    /// Check if all detailed checks passed
    pub fn all_healthy(&self) -> bool {
        self.database.is_healthy()
            && self.command_processor.is_healthy()
            && self.queue_processing.is_healthy()
            && self.event_system.is_healthy()
            && self.step_processing.is_healthy()
            && self.circuit_breakers.is_healthy()
    }
}

/// Worker readiness response with typed checks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct ReadinessResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
    pub checks: WorkerReadinessChecks,
    pub system_info: WorkerSystemInfo,
}

/// Detailed health check response with typed subsystem checks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
    pub checks: WorkerDetailedChecks,
    pub system_info: WorkerSystemInfo,
    /// TAS-169: Distributed cache status (moved from /templates/cache/distributed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distributed_cache: Option<DistributedCacheInfo>,
}

/// TAS-169: Distributed cache information for health response
///
/// Moved from /templates/cache/distributed to /health/detailed.
/// Reports the status of the distributed template cache (Redis).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct DistributedCacheInfo {
    /// Whether distributed caching is enabled
    pub enabled: bool,
    /// Cache provider name ("redis" or "noop")
    pub provider: String,
    /// Whether the cache backend is healthy
    pub healthy: bool,
}

/// Individual health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct HealthCheck {
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: u64,
    pub last_checked: DateTime<Utc>,
}

impl HealthCheck {
    /// Create a healthy check result
    pub fn healthy(message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            status: "healthy".to_string(),
            message: Some(message.into()),
            duration_ms,
            last_checked: Utc::now(),
        }
    }

    /// Create a degraded check result
    pub fn degraded(message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            status: "degraded".to_string(),
            message: Some(message.into()),
            duration_ms,
            last_checked: Utc::now(),
        }
    }

    /// Create an unhealthy check result
    pub fn unhealthy(message: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            status: "unhealthy".to_string(),
            message: Some(message.into()),
            duration_ms,
            last_checked: Utc::now(),
        }
    }

    /// Check if this health check indicates healthy status
    pub fn is_healthy(&self) -> bool {
        self.status == "healthy"
    }
}

/// Worker system information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct WorkerSystemInfo {
    pub version: String,
    pub environment: String,
    pub uptime_seconds: u64,
    pub worker_type: String,
    pub database_pool_size: u32,
    pub command_processor_active: bool,
    pub supported_namespaces: Vec<String>,
    /// Connection pool utilization details (TAS-164)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_utilization: Option<super::health::PoolUtilizationInfo>,
}

// =============================================================================
// Circuit Breaker Types (TAS-75)
// =============================================================================

/// Circuit breaker state as string for API responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerState {
    /// Normal operation - all calls allowed
    Closed,
    /// Failure mode - calls fail fast
    Open,
    /// Testing recovery - limited calls allowed
    HalfOpen,
}

impl std::fmt::Display for CircuitBreakerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerState::Closed => write!(f, "closed"),
            CircuitBreakerState::Open => write!(f, "open"),
            CircuitBreakerState::HalfOpen => write!(f, "half_open"),
        }
    }
}

/// Circuit breaker health status for API responses
///
/// TAS-75: Provides visibility into circuit breaker state for monitoring and alerting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStatus {
    /// Circuit breaker name/identifier
    pub name: String,
    /// Current state of the circuit
    pub state: CircuitBreakerState,
    /// Whether the circuit is allowing calls (closed or half-open with capacity)
    pub is_healthy: bool,
    /// Total successful operations
    pub success_count: u64,
    /// Total failed operations
    pub failure_count: u64,
    /// Current consecutive failure count
    pub consecutive_failures: u64,
    /// Total calls through the circuit
    pub total_calls: u64,
    /// Number of rejections due to circuit being open
    pub circuit_open_rejections: u64,
    /// Additional metrics specific to the circuit breaker type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_metrics: Option<HashMap<String, serde_json::Value>>,
}

impl Default for CircuitBreakerStatus {
    fn default() -> Self {
        Self {
            name: "unknown".to_string(),
            state: CircuitBreakerState::Closed,
            is_healthy: true,
            success_count: 0,
            failure_count: 0,
            consecutive_failures: 0,
            total_calls: 0,
            circuit_open_rejections: 0,
            additional_metrics: None,
        }
    }
}

/// Aggregated circuit breaker health for worker
///
/// TAS-75: Provides summary of all circuit breakers in the worker.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CircuitBreakersHealth {
    /// Overall health (true if all circuit breakers are healthy)
    pub all_healthy: bool,
    /// Number of circuit breakers in closed state
    pub closed_count: usize,
    /// Number of circuit breakers in open state
    pub open_count: usize,
    /// Number of circuit breakers in half-open state
    pub half_open_count: usize,
    /// Individual circuit breaker statuses
    pub circuit_breakers: Vec<CircuitBreakerStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_healthy() {
        let check = HealthCheck::healthy("all good", 42);
        assert_eq!(check.status, "healthy");
        assert_eq!(check.message, Some("all good".to_string()));
        assert_eq!(check.duration_ms, 42);
    }

    #[test]
    fn test_health_check_degraded() {
        let check = HealthCheck::degraded("slow response", 150);
        assert_eq!(check.status, "degraded");
        assert_eq!(check.message, Some("slow response".to_string()));
        assert_eq!(check.duration_ms, 150);
    }

    #[test]
    fn test_health_check_unhealthy() {
        let check = HealthCheck::unhealthy("connection refused", 5000);
        assert_eq!(check.status, "unhealthy");
        assert_eq!(check.message, Some("connection refused".to_string()));
        assert_eq!(check.duration_ms, 5000);
    }

    #[test]
    fn test_health_check_is_healthy() {
        let healthy = HealthCheck::healthy("ok", 10);
        assert!(healthy.is_healthy());

        let degraded = HealthCheck::degraded("slow", 100);
        assert!(!degraded.is_healthy());

        let unhealthy = HealthCheck::unhealthy("down", 5000);
        assert!(!unhealthy.is_healthy());
    }

    #[test]
    fn test_worker_readiness_checks_all_healthy() {
        let checks = WorkerReadinessChecks {
            database: HealthCheck::healthy("db ok", 5),
            command_processor: HealthCheck::healthy("cmd ok", 3),
            queue_processing: HealthCheck::healthy("queue ok", 2),
        };
        assert!(checks.all_healthy());
    }

    #[test]
    fn test_worker_readiness_checks_not_all_healthy() {
        let checks = WorkerReadinessChecks {
            database: HealthCheck::healthy("db ok", 5),
            command_processor: HealthCheck::degraded("cmd slow", 200),
            queue_processing: HealthCheck::healthy("queue ok", 2),
        };
        assert!(!checks.all_healthy());
    }

    #[test]
    fn test_worker_detailed_checks_all_healthy() {
        let checks = WorkerDetailedChecks {
            database: HealthCheck::healthy("db ok", 5),
            command_processor: HealthCheck::healthy("cmd ok", 3),
            queue_processing: HealthCheck::healthy("queue ok", 2),
            event_system: HealthCheck::healthy("events ok", 1),
            step_processing: HealthCheck::healthy("steps ok", 4),
            circuit_breakers: HealthCheck::healthy("circuits ok", 1),
        };
        assert!(checks.all_healthy());
    }

    #[test]
    fn test_worker_detailed_checks_not_all_healthy() {
        let checks = WorkerDetailedChecks {
            database: HealthCheck::healthy("db ok", 5),
            command_processor: HealthCheck::healthy("cmd ok", 3),
            queue_processing: HealthCheck::healthy("queue ok", 2),
            event_system: HealthCheck::unhealthy("events down", 5000),
            step_processing: HealthCheck::healthy("steps ok", 4),
            circuit_breakers: HealthCheck::healthy("circuits ok", 1),
        };
        assert!(!checks.all_healthy());
    }

    #[test]
    fn test_circuit_breaker_state_display() {
        assert_eq!(CircuitBreakerState::Closed.to_string(), "closed");
        assert_eq!(CircuitBreakerState::Open.to_string(), "open");
        assert_eq!(CircuitBreakerState::HalfOpen.to_string(), "half_open");
    }

    #[test]
    fn test_circuit_breaker_status_default() {
        let status = CircuitBreakerStatus::default();
        assert_eq!(status.name, "unknown");
        assert_eq!(status.state, CircuitBreakerState::Closed);
        assert!(status.is_healthy);
        assert_eq!(status.success_count, 0);
        assert_eq!(status.failure_count, 0);
        assert_eq!(status.consecutive_failures, 0);
        assert_eq!(status.total_calls, 0);
        assert_eq!(status.circuit_open_rejections, 0);
        assert!(status.additional_metrics.is_none());
    }

    #[test]
    fn test_circuit_breakers_health_default() {
        let health = CircuitBreakersHealth::default();
        assert!(!health.all_healthy);
        assert_eq!(health.closed_count, 0);
        assert_eq!(health.open_count, 0);
        assert_eq!(health.half_open_count, 0);
        assert!(health.circuit_breakers.is_empty());
    }
}
