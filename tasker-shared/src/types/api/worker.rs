use crate::types::base::CacheStats;
use crate::{models::core::task_template::ResolvedTaskTemplate, types::HandlerMetadata};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Serialize)]
pub struct TemplateResponse {
    pub template: ResolvedTaskTemplate,
    pub handler_metadata: HandlerMetadata,
    pub cached: bool,
    pub cache_age_seconds: Option<u64>,
    pub access_count: Option<u64>,
}

/// Response for template listing
#[derive(Debug, Serialize)]
pub struct TemplateListResponse {
    pub supported_namespaces: Vec<String>,
    pub template_count: usize,
    pub cache_stats: Option<CacheStats>,
    pub worker_capabilities: Vec<String>,
}

/// Response for cache operations
#[derive(Debug, Serialize)]
pub struct CacheOperationResponse {
    pub operation: String,
    pub success: bool,
    pub cache_stats: CacheStats,
}

/// Response for template validation
#[derive(Debug, Serialize)]
pub struct TemplateValidationResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub step_handlers: Vec<String>,
}

/// Basic health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
}

/// Detailed health check response with subsystem checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedHealthResponse {
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub worker_id: String,
    pub checks: HashMap<String, HealthCheck>,
    pub system_info: WorkerSystemInfo,
}

/// Individual health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: u64,
    pub last_checked: DateTime<Utc>,
}

/// Worker system information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSystemInfo {
    pub version: String,
    pub environment: String,
    pub uptime_seconds: u64,
    pub worker_type: String,
    pub database_pool_size: u32,
    pub command_processor_active: bool,
    pub supported_namespaces: Vec<String>,
}

/// Worker status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatusResponse {
    pub worker_id: String,
    pub status: String,
    pub uptime_seconds: u64,
    pub namespaces: Vec<String>,
    pub current_tasks: u32,
    pub completed_tasks: u32,
    pub failed_tasks: u32,
    pub last_activity: Option<DateTime<Utc>>,
    pub version: String,
    pub environment: String,
}

/// Worker health response (alias for detailed health)
pub type WorkerHealthResponse = DetailedHealthResponse;

/// Worker information for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub status: String,
    pub namespaces: Vec<String>,
    pub last_seen: DateTime<Utc>,
    pub version: String,
}

/// Worker list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerListResponse {
    pub workers: Vec<WorkerInfo>,
    pub total_count: usize,
    pub active_count: usize,
    pub timestamp: DateTime<Utc>,
}
