//! # OpenAPI Documentation Schemas
//!
//! This module defines the OpenAPI specification and schemas for the Tasker Web API.
//! It uses utoipa to generate OpenAPI 3.0 compatible documentation with Swagger UI integration.

use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Main OpenAPI specification for the Tasker Web API
#[derive(OpenApi)]
#[openapi(
    paths(
        // Tasks API paths
        crate::web::handlers::tasks::create_task,
        crate::web::handlers::tasks::list_tasks,
        crate::web::handlers::tasks::get_task,
        crate::web::handlers::tasks::cancel_task,
        
        // Health API paths
        crate::web::handlers::health::basic_health,
        crate::web::handlers::health::readiness_probe,
        crate::web::handlers::health::liveness_probe,
        crate::web::handlers::health::detailed_health,
        
        // Analytics API paths
        crate::web::handlers::analytics::get_performance_metrics,
        crate::web::handlers::analytics::get_bottlenecks,
        
        // Workflow Steps API paths
        crate::web::handlers::steps::list_task_steps,
        crate::web::handlers::steps::get_step,
        crate::web::handlers::steps::resolve_step_manually,
        
        // Handlers Registry API paths
        crate::web::handlers::registry::list_namespaces,
        crate::web::handlers::registry::list_namespace_handlers,
        crate::web::handlers::registry::get_handler_info,
    ),
    components(schemas(
        // Task-related schemas
        TaskRequest,
        TaskResponse,
        TaskListResponse,
        TaskStatus,
        
        // Workflow Step schemas
        WorkflowStepResponse,
        WorkflowStepListResponse,
        StepManualResolutionRequest,
        StepStatus,
        
        // Health schemas
        HealthResponse,
        DetailedHealthResponse,
        HealthStatus,
        
        // Analytics schemas
        PerformanceMetricsResponse,
        BottleneckAnalysisResponse,
        MetricEntry,
        
        // Handler Registry schemas
        NamespaceListResponse,
        HandlerListResponse,
        HandlerInfoResponse,
        
        // Common schemas
        ApiError,
        PaginationMeta,
        TimeRange,
    )),
    tags(
        (name = "tasks", description = "Task management operations"),
        (name = "workflow_steps", description = "Workflow step operations"),
        (name = "health", description = "Health check and monitoring"),
        (name = "analytics", description = "Performance analytics and metrics"),
        (name = "handlers", description = "Handler registry and discovery"),
    ),
    info(
        title = "Tasker Web API",
        version = "1.0.0",
        description = "REST API for the Tasker workflow orchestration system",
        contact(
            name = "Tasker Systems",
            email = "support@tasker-systems.com"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    servers(
        (url = "/v1", description = "API version 1")
    )
)]
pub struct ApiDoc;

// Task-related schemas

/// Request to create a new task
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskRequest {
    /// Task namespace (e.g., "order_fulfillment", "inventory_management")
    #[schema(example = "order_fulfillment")]
    pub namespace: String,
    
    /// Task name within the namespace
    #[schema(example = "process_order")]
    pub name: String,
    
    /// Task version for handler compatibility
    #[schema(example = "1.0.0")]
    pub version: String,
    
    /// Task context data as JSON object
    #[schema(example = json!({"order_id": 12345, "customer_id": "cust_123"}))]
    pub context: serde_json::Value,
    
    /// System or user that initiated the task
    #[schema(example = "order_service")]
    pub initiator: String,
    
    /// Source system for auditing
    #[schema(example = "ecommerce_api")]
    pub source_system: String,
    
    /// Human-readable reason for task creation
    #[schema(example = "Process new customer order #12345")]
    pub reason: String,
    
    /// Task priority (1-10, higher = more important)
    #[schema(example = 5, minimum = 1, maximum = 10)]
    pub priority: i32,
    
    /// Maximum time in seconds a worker can claim this task
    #[schema(example = 300)]
    pub claim_timeout_seconds: i32,
}

/// Task response with full details
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    /// Unique task identifier
    pub task_uuid: Uuid,
    
    /// Task namespace
    pub namespace: String,
    
    /// Task name
    pub name: String,
    
    /// Task version
    pub version: String,
    
    /// Current task status
    pub status: TaskStatus,
    
    /// Task context data
    pub context: serde_json::Value,
    
    /// Task initiator
    pub initiator: String,
    
    /// Source system
    pub source_system: String,
    
    /// Task creation reason
    pub reason: String,
    
    /// Task priority
    pub priority: i32,
    
    /// Claim timeout in seconds
    pub claim_timeout_seconds: i32,
    
    /// Task creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    
    /// Task completion timestamp (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// List of tasks with pagination
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskListResponse {
    /// List of tasks
    pub tasks: Vec<TaskResponse>,
    
    /// Pagination metadata
    pub pagination: PaginationMeta,
}

/// Task execution status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is ready to be processed
    Pending,
    /// Task is currently being processed
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed with errors
    Failed,
    /// Task was cancelled
    Cancelled,
}

// Workflow Step schemas

/// Workflow step response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepResponse {
    /// Unique step identifier
    pub step_uuid: Uuid,
    
    /// Parent task UUID
    pub task_uuid: Uuid,
    
    /// Step name from workflow definition
    pub step_name: String,
    
    /// Current step status
    pub status: StepStatus,
    
    /// Step input data
    pub input_data: serde_json::Value,
    
    /// Step output data (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_data: Option<serde_json::Value>,
    
    /// Step error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    
    /// Step creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Step completion timestamp (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// List of workflow steps
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepListResponse {
    /// List of workflow steps
    pub steps: Vec<WorkflowStepResponse>,
    
    /// Pagination metadata
    pub pagination: PaginationMeta,
}

/// Request to manually resolve a failed step
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepManualResolutionRequest {
    /// Resolution action to take
    pub action: String,
    
    /// Optional output data for successful resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_data: Option<serde_json::Value>,
    
    /// Reason for manual intervention
    pub reason: String,
}

/// Workflow step execution status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Step is ready to be processed
    Pending,
    /// Step is currently being processed
    Running,
    /// Step completed successfully
    Completed,
    /// Step failed and needs attention
    Failed,
    /// Step was skipped due to conditions
    Skipped,
}

// Health schemas

/// Basic health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Overall health status
    pub status: HealthStatus,
    
    /// Response timestamp
    pub timestamp: DateTime<Utc>,
}

/// Detailed health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetailedHealthResponse {
    /// Overall health status
    pub status: HealthStatus,
    
    /// Response timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Database connectivity status
    pub database: HealthStatus,
    
    /// Circuit breaker status
    pub circuit_breaker: HealthStatus,
    
    /// Orchestration system status
    pub orchestration: HealthStatus,
    
    /// System resource information
    pub system_info: serde_json::Value,
}

/// Health status enumeration
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// System is healthy and operational
    Healthy,
    /// System is partially degraded but functional
    Degraded,
    /// System is unhealthy and may not function properly
    Unhealthy,
}

// Analytics schemas

/// Performance metrics response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetricsResponse {
    /// Time range for the metrics
    pub time_range: TimeRange,
    
    /// List of performance metrics
    pub metrics: Vec<MetricEntry>,
    
    /// Summary statistics
    pub summary: serde_json::Value,
}

/// Bottleneck analysis response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BottleneckAnalysisResponse {
    /// Time range for the analysis
    pub time_range: TimeRange,
    
    /// Identified bottlenecks
    pub bottlenecks: Vec<serde_json::Value>,
    
    /// Recommendations for optimization
    pub recommendations: Vec<String>,
}

/// Individual metric entry
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MetricEntry {
    /// Metric name
    pub name: String,
    
    /// Metric value
    pub value: f64,
    
    /// Metric unit (e.g., "ms", "count", "percentage")
    pub unit: String,
    
    /// Metric timestamp
    pub timestamp: DateTime<Utc>,
}

// Handler Registry schemas

/// List of available namespaces
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceListResponse {
    /// Available namespaces
    pub namespaces: Vec<String>,
}

/// List of handlers in a namespace
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HandlerListResponse {
    /// Namespace name
    pub namespace: String,
    
    /// Available handlers in the namespace
    pub handlers: Vec<String>,
}

/// Detailed handler information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HandlerInfoResponse {
    /// Handler namespace
    pub namespace: String,
    
    /// Handler name
    pub name: String,
    
    /// Handler version
    pub version: String,
    
    /// Handler description
    pub description: String,
    
    /// Handler configuration schema
    pub config_schema: serde_json::Value,
    
    /// Handler status
    pub status: String,
}

// Common schemas

/// API error response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// Error code
    pub error: String,
    
    /// Human-readable error message
    pub message: String,
    
    /// Request ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    
    /// Error timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Pagination metadata
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    /// Current page number (0-based)
    pub page: u32,
    
    /// Number of items per page
    pub per_page: u32,
    
    /// Total number of items
    pub total_items: u64,
    
    /// Total number of pages
    pub total_pages: u32,
    
    /// Whether there are more pages
    pub has_next: bool,
    
    /// Whether there are previous pages
    pub has_prev: bool,
}

/// Time range for analytics queries
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    /// Start timestamp
    pub start: DateTime<Utc>,
    
    /// End timestamp
    pub end: DateTime<Utc>,
    
    /// Human-readable description
    pub description: String,
}