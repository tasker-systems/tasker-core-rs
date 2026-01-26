//! # Worker OpenAPI Documentation
//!
//! OpenAPI specification for the Worker Web API with security scheme definitions.
//! TAS-169: Simplified template API under /v1/templates.

use utoipa::OpenApi;

use tasker_shared::types::api::orchestration::{
    ConfigMetadata, SafeAuthConfig, SafeMessagingConfig, WorkerConfigResponse,
};
use tasker_shared::types::api::worker::{
    BasicHealthResponse, DetailedHealthResponse, DistributedCacheInfo, HealthCheck,
    ReadinessResponse, TemplateListResponse, TemplateResponse, TemplateValidationResponse,
    WorkerDetailedChecks, WorkerReadinessChecks, WorkerSystemInfo,
};
use tasker_shared::types::base::CacheStats;
use tasker_shared::types::openapi_security::SecurityAddon;
use tasker_shared::types::web::ApiError;

use crate::web::handlers;

/// Worker API OpenAPI specification
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        // Health API paths (public, no auth required)
        handlers::health::health_check,
        handlers::health::readiness_check,
        handlers::health::liveness_check,
        handlers::health::detailed_health_check,

        // Template API paths (auth required) - TAS-169: moved to /v1/templates
        handlers::templates::list_templates,
        handlers::templates::get_template,
        handlers::templates::validate_template,

        // Config API paths (auth required)
        handlers::config::get_config,
    ),
    components(schemas(
        // Health schemas (TAS-76: typed health checks)
        BasicHealthResponse,
        DetailedHealthResponse,
        ReadinessResponse,
        DistributedCacheInfo,
        HealthCheck,
        WorkerReadinessChecks,
        WorkerDetailedChecks,
        WorkerSystemInfo,

        // Template schemas
        TemplateResponse,
        TemplateListResponse,
        TemplateValidationResponse,
        CacheStats,

        // Config schemas (TAS-150: whitelist-only safe response types)
        WorkerConfigResponse,
        ConfigMetadata,
        SafeAuthConfig,
        SafeMessagingConfig,

        // Error schemas
        ApiError,
    )),
    tags(
        (name = "health", description = "Health check and monitoring (includes distributed cache status)"),
        (name = "templates", description = "Task template retrieval and validation"),
        (name = "config", description = "Runtime configuration observability (safe fields only)"),
    ),
    info(
        title = "Tasker Worker API",
        version = "1.0.0",
        description = "REST API for the Tasker worker process. TAS-169: Template routes under /v1/templates, cache operations removed (restart worker to refresh).",
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
        (url = "/", description = "Worker API")
    )
)]
#[derive(Debug)]
pub struct ApiDoc;
