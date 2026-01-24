//! # Worker OpenAPI Documentation
//!
//! OpenAPI specification for the Worker Web API with security scheme definitions.

use utoipa::OpenApi;

use tasker_shared::types::api::orchestration::{
    ConfigMetadata, SafeAuthConfig, SafeMessagingConfig, WorkerConfigResponse,
};
use tasker_shared::types::api::worker::{
    BasicHealthResponse, CacheOperationResponse, DetailedHealthResponse, TemplateListResponse,
    TemplateResponse, TemplateValidationResponse,
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

        // Template API paths (auth required)
        handlers::templates::list_templates,
        handlers::templates::get_template,
        handlers::templates::validate_template,
        handlers::templates::clear_cache,
        handlers::templates::get_cache_stats,
        handlers::templates::maintain_cache,
        handlers::templates::refresh_template,

        // Config API paths (auth required)
        handlers::config::get_config,
    ),
    components(schemas(
        // Health schemas
        BasicHealthResponse,
        DetailedHealthResponse,

        // Template schemas
        TemplateResponse,
        TemplateListResponse,
        TemplateValidationResponse,
        CacheOperationResponse,
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
        (name = "health", description = "Health check and monitoring"),
        (name = "templates", description = "Task template management and cache operations"),
        (name = "config", description = "Runtime configuration observability (safe fields only)"),
    ),
    info(
        title = "Tasker Worker API",
        version = "1.0.0",
        description = "REST API for the Tasker worker process",
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
