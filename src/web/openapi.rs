//! # OpenAPI Documentation Schemas
//!
//! This module defines the OpenAPI specification and schemas for the Tasker Web API.
//! It uses utoipa to generate OpenAPI 3.0 compatible documentation with Swagger UI integration.

use utoipa::OpenApi;

// Re-export ApiError from the errors module so OpenAPI annotations can find it
pub use crate::web::response_types::ApiError;

// Import all the actual types used in handlers
use crate::database::sql_functions::{StepReadinessStatus, TaskExecutionContext};
use crate::models::core::task::PaginationInfo;
use crate::models::core::task_request::TaskRequest;

// Import all handler response types
use crate::web::handlers;
use crate::web::handlers::analytics::{
    BottleneckAnalysis, PerformanceMetrics, ResourceUtilization, SlowStepInfo, SlowTaskInfo,
};
use crate::web::handlers::health::{
    DetailedHealthResponse, HealthCheck, HealthInfo, HealthResponse,
};
use crate::web::handlers::registry::{HandlerInfo, NamespaceInfo};
use crate::web::handlers::steps::{ManualResolutionRequest, StepResponse};
use crate::web::handlers::tasks::{TaskCreationResponse, TaskListResponse, TaskResponse};

/// Main OpenAPI specification for the Tasker Web API
#[derive(OpenApi)]
#[openapi(
    paths(
        // Tasks API paths
        handlers::tasks::create_task,
        handlers::tasks::list_tasks,
        handlers::tasks::get_task,
        handlers::tasks::cancel_task,

        // Health API paths
        handlers::health::basic_health,
        handlers::health::readiness_probe,
        handlers::health::liveness_probe,
        handlers::health::detailed_health,

        // Analytics API paths
        handlers::analytics::get_performance_metrics,
        handlers::analytics::get_bottlenecks,

        // Workflow Steps API paths
        handlers::steps::list_task_steps,
        handlers::steps::get_step,
        handlers::steps::resolve_step_manually,

        // Handlers Registry API paths
        handlers::registry::list_namespaces,
        handlers::registry::list_namespace_handlers,
        handlers::registry::get_handler_info,
    ),
    components(schemas(
        // Task-related schemas
        TaskRequest,
        TaskCreationResponse,
        TaskResponse,
        TaskListResponse,
        PaginationInfo,

        // Database function types used in responses
        StepReadinessStatus,
        TaskExecutionContext,

        // Workflow Step schemas
        StepResponse,
        ManualResolutionRequest,

        // Health schemas
        HealthResponse,
        DetailedHealthResponse,
        HealthCheck,
        HealthInfo,

        // Analytics schemas
        PerformanceMetrics,
        BottleneckAnalysis,
        SlowStepInfo,
        SlowTaskInfo,
        ResourceUtilization,

        // Handler Registry schemas
        NamespaceInfo,
        HandlerInfo,

        // Error schemas (re-exported from errors module)
        ApiError,
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
        (url = "/", description = "API version 1")
    )
)]
pub struct ApiDoc;
