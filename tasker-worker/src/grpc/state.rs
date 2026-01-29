//! gRPC server state management for Worker.
//!
//! `WorkerGrpcState` wraps `SharedWorkerServices` with gRPC-specific configuration.
//! The services are shared with the REST API when both are enabled.
//!
//! TAS-177: Refactored to use `SharedWorkerServices` directly instead of
//! extracting services from `WorkerWebState`.

use crate::worker::services::{
    ConfigQueryService, HealthService, SharedWorkerServices, TemplateQueryService,
};
use std::sync::Arc;
use tasker_shared::config::GrpcConfig;
use tasker_shared::types::SecurityService;

/// Shared state for Worker gRPC services.
///
/// This struct wraps the services from `SharedWorkerServices` and adds
/// gRPC-specific configuration. This ensures both APIs use the same
/// service instances.
#[derive(Clone, Debug)]
pub struct WorkerGrpcState {
    /// gRPC server configuration
    pub config: Arc<GrpcConfig>,

    /// TAS-177: Shared services (also used by REST API)
    shared_services: Arc<SharedWorkerServices>,
}

impl WorkerGrpcState {
    /// Create WorkerGrpcState from SharedWorkerServices.
    ///
    /// This wraps the shared services with gRPC-specific configuration.
    pub fn new(shared_services: Arc<SharedWorkerServices>, grpc_config: GrpcConfig) -> Self {
        Self {
            config: Arc::new(grpc_config),
            shared_services,
        }
    }

    /// Get the shared services (for access to all service instances)
    pub fn shared_services(&self) -> &Arc<SharedWorkerServices> {
        &self.shared_services
    }

    /// Get the health service
    pub fn health_service(&self) -> &Arc<HealthService> {
        &self.shared_services.health_service
    }

    /// Get the template query service
    pub fn template_query_service(&self) -> &Arc<TemplateQueryService> {
        &self.shared_services.template_query_service
    }

    /// Get the config query service
    pub fn config_query_service(&self) -> &Arc<ConfigQueryService> {
        &self.shared_services.config_query_service
    }

    /// Get the security service (if authentication is enabled)
    pub fn security_service(&self) -> Option<&Arc<SecurityService>> {
        self.shared_services.security_service.as_ref()
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.shared_services.security_service.is_some()
    }
}
