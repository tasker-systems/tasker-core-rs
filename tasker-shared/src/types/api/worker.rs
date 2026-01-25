//! # Worker API Types
//!
//! Types for worker web API endpoints including templates, health checks, and cache operations.

use crate::types::base::CacheStats;
use crate::{models::core::task_template::ResolvedTaskTemplate, types::HandlerMetadata};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Template Types
// =============================================================================

/// Query parameters for template listing
#[derive(Debug, Deserialize)]
pub struct TemplateQueryParams {
    /// Filter by namespace
    pub namespace: Option<String>,
    /// Include cache statistics
    pub include_cache_stats: Option<bool>,
}

/// Path parameters for template operations
#[derive(Debug, Deserialize)]
pub struct TemplatePathParams {
    pub namespace: String,
    pub name: String,
    pub version: String,
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

/// Detailed health check response with subsystem checks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::ToSchema))]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
    #[cfg_attr(feature = "web-api", schema(value_type = Object))]
    pub checks: HashMap<String, HealthCheck>,
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
