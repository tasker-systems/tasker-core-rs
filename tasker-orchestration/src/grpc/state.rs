//! gRPC server state management.
//!
//! `GrpcState` wraps `SharedApiServices` with gRPC-specific configuration.
//! The services are shared with the REST API when both are enabled.

use crate::services::SharedApiServices;
use std::sync::Arc;
use tasker_shared::config::GrpcConfig;

/// Shared state for gRPC services.
///
/// This struct wraps `SharedApiServices` (shared with REST API) and adds
/// gRPC-specific configuration. This ensures both APIs use the same
/// service instances and database pools.
#[derive(Clone, Debug)]
pub struct GrpcState {
    /// gRPC server configuration
    pub config: Arc<GrpcConfig>,

    /// Shared services (same instances used by REST API)
    pub services: Arc<SharedApiServices>,
}

impl GrpcState {
    /// Create GrpcState from shared services.
    ///
    /// This is the only constructor - gRPC always uses shared services,
    /// whether REST is enabled or not.
    pub fn new(services: Arc<SharedApiServices>, grpc_config: GrpcConfig) -> Self {
        Self {
            config: Arc::new(grpc_config),
            services,
        }
    }

    /// Check if authentication is enabled
    pub fn is_auth_enabled(&self) -> bool {
        self.services.is_auth_enabled()
    }

    /// Check backpressure status (delegates to shared services)
    pub fn check_backpressure(&self) -> Option<String> {
        self.services.check_backpressure()
    }
}
