//! gRPC server state management.
//!
//! `GrpcState` wraps the same services as the REST `AppState`, providing
//! a consistent service layer for both API types.

use crate::orchestration::OrchestrationCore;
use crate::services::{
    AnalyticsService, HealthService, StepService, TaskService, TemplateQueryService,
};
use crate::web::state::AppState;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_shared::config::GrpcConfig;
use tasker_shared::types::SecurityService;

/// Shared state for gRPC services.
///
/// This struct provides access to the same services used by the REST API,
/// ensuring consistent behavior across both API surfaces.
#[derive(Clone, Debug)]
pub struct GrpcState {
    /// gRPC server configuration
    pub config: Arc<GrpcConfig>,

    /// Security service for JWT/API key authentication
    pub security_service: Option<Arc<SecurityService>>,

    /// Database pool for write operations
    pub write_pool: PgPool,

    /// Database pool for read operations
    pub read_pool: PgPool,

    /// Task service for task operations
    pub task_service: TaskService,

    /// Step service for step operations
    pub step_service: StepService,

    /// Health service for health checks
    pub health_service: HealthService,

    /// Template query service for template discovery
    pub template_query_service: TemplateQueryService,

    /// Analytics service for performance metrics
    pub analytics_service: Arc<AnalyticsService>,

    /// Orchestration core for backpressure checking
    pub orchestration_core: Arc<OrchestrationCore>,
}

impl GrpcState {
    /// Create GrpcState from AppState (shares the same services).
    ///
    /// This allows the gRPC server to reuse all the services already
    /// created for the REST API.
    pub fn from_app_state(app_state: &AppState, grpc_config: GrpcConfig) -> Self {
        Self {
            config: Arc::new(grpc_config),
            security_service: app_state.security_service.clone(),
            write_pool: app_state.web_db_pool.clone(),
            read_pool: app_state.orchestration_db_pool.clone(),
            task_service: app_state.task_service.clone(),
            step_service: app_state.step_service.clone(),
            health_service: app_state.health_service.clone(),
            template_query_service: app_state.template_query_service.clone(),
            analytics_service: app_state.analytics_service.clone(),
            orchestration_core: app_state.orchestration_core.clone(),
        }
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.security_service
            .as_ref()
            .map(|s| s.is_enabled())
            .unwrap_or(false)
    }

    /// Check backpressure status (delegates to orchestration core)
    pub fn check_backpressure(&self) -> Option<String> {
        self.orchestration_core
            .backpressure_checker()
            .try_check_backpressure()
            .map(|e| e.to_string())
    }
}
