//! # OpenAPI Documentation Schemas
//!
//! This module defines the OpenAPI specification and schemas for the Tasker Web API.
//! It uses utoipa to generate OpenAPI 3.0 compatible documentation with Swagger UI integration.

use utoipa::OpenApi;

// Re-export ApiError from the errors module so OpenAPI annotations can find it
pub use tasker_shared::types::web::ApiError;

// Shared security scheme modifier
use tasker_shared::types::openapi_security::SecurityAddon;

// Import all the actual types used in handlers
use tasker_shared::database::sql_functions::{StepReadinessStatus, TaskExecutionContext};
use tasker_shared::models::core::task::PaginationInfo;
use tasker_shared::models::core::task_request::TaskRequest;

// Import all handler response types
use crate::web::handlers;
// DLQ handler types (TAS-49)
use crate::web::handlers::dlq::{UpdateInvestigationRequest, UpdateInvestigationResponse};

use tasker_shared::types::api::orchestration::{
    BottleneckAnalysis, ConfigMetadata, DetailedHealthResponse, HealthCheck, HealthInfo,
    HealthResponse, ManualCompletionData, OrchestrationConfigResponse, PerformanceMetrics,
    ResourceUtilization, SafeAuthConfig, SafeCircuitBreakerConfig, SafeDatabasePoolConfig,
    SafeMessagingConfig, SlowStepInfo, SlowTaskInfo, StepAuditResponse, StepManualAction,
    StepResponse, TaskCreationResponse, TaskListResponse, TaskResponse,
};

// TAS-76: Template API types
use tasker_shared::types::api::templates::{
    NamespaceSummary, StepDefinition, TemplateDetail, TemplateListResponse, TemplateSummary,
};

// TAS-49: DLQ model types
use tasker_shared::models::orchestration::dlq::{
    DlqEntry, DlqInvestigationQueueEntry, DlqResolutionStatus, DlqStats, StalenessMonitoring,
};
use tasker_shared::models::orchestration::StalenessHealthStatus;

/// Main OpenAPI specification for the Tasker Web API
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
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
        handlers::steps::get_step_audit,

        // TAS-76: Templates API paths (replaces legacy handlers registry)
        handlers::templates::list_templates,
        handlers::templates::get_template,

        // TAS-49: DLQ API paths
        handlers::dlq::list_dlq_entries,
        handlers::dlq::get_dlq_entry,
        handlers::dlq::update_dlq_investigation,
        handlers::dlq::get_dlq_stats,
        handlers::dlq::get_investigation_queue,
        handlers::dlq::get_staleness_monitoring,

        // Config API paths - unified endpoint
        handlers::config::get_config,
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
        StepAuditResponse,
        StepManualAction,
        ManualCompletionData,

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

        // TAS-76: Template schemas (replaces legacy handler registry)
        TemplateListResponse,
        TemplateSummary,
        TemplateDetail,
        NamespaceSummary,
        StepDefinition,

        // Config schemas (TAS-150: whitelist-only safe response types)
        OrchestrationConfigResponse,
        ConfigMetadata,
        SafeAuthConfig,
        SafeCircuitBreakerConfig,
        SafeDatabasePoolConfig,
        SafeMessagingConfig,

        // TAS-49: DLQ schemas
        DlqEntry,
        DlqStats,
        DlqResolutionStatus,
        DlqInvestigationQueueEntry,
        StalenessMonitoring,
        StalenessHealthStatus,
        UpdateInvestigationRequest,
        UpdateInvestigationResponse,

        // Error schemas (re-exported from errors module)
        ApiError,
    )),
    tags(
        (name = "tasks", description = "Task management operations"),
        (name = "workflow_steps", description = "Workflow step operations"),
        (name = "health", description = "Health check and monitoring"),
        (name = "analytics", description = "Performance analytics and metrics"),
        (name = "templates", description = "Template discovery and information"),
        (name = "dlq", description = "Dead Letter Queue investigation and monitoring"),
        (name = "config", description = "Runtime configuration observability (safe fields only)"),
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
#[derive(Debug)]
pub struct ApiDoc;
