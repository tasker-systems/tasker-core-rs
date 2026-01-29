//! Configuration service gRPC implementation.
//!
//! Provides safe runtime configuration information via gRPC.
//! Only whitelisted operational metadata is returned — no secrets.

use crate::grpc::conversions::datetime_to_timestamp;
use crate::grpc::interceptors::AuthInterceptor;
use crate::grpc::state::GrpcState;
use chrono::Utc;
use tasker_shared::proto::v1::{
    self as proto, config_service_server::ConfigService as ConfigServiceTrait,
};
use tasker_shared::types::{Permission, SecurityContext};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC Config service implementation.
#[derive(Debug)]
pub struct ConfigServiceImpl {
    state: GrpcState,
    auth_interceptor: AuthInterceptor,
}

impl ConfigServiceImpl {
    /// Create a new config service.
    pub fn new(state: GrpcState) -> Self {
        let auth_interceptor = AuthInterceptor::new(state.services.security_service.clone());
        Self {
            state,
            auth_interceptor,
        }
    }

    /// Authenticate the request and check permissions.
    async fn authenticate_and_authorize<T>(
        &self,
        request: &Request<T>,
        required_permission: Permission,
    ) -> Result<SecurityContext, Status> {
        let ctx = self.auth_interceptor.authenticate(request).await?;

        // Check permission
        if !ctx.has_permission(&required_permission) {
            return Err(Status::permission_denied(
                "Insufficient permissions for this operation",
            ));
        }

        Ok(ctx)
    }
}

#[tonic::async_trait]
impl ConfigServiceTrait for ConfigServiceImpl {
    /// Get current configuration (safe fields only).
    async fn get_config(
        &self,
        request: Request<proto::GetConfigRequest>,
    ) -> Result<Response<proto::GetConfigResponse>, Status> {
        // Authenticate and authorize
        let _ctx = self
            .authenticate_and_authorize(&request, Permission::SystemConfigRead)
            .await?;

        debug!("gRPC get config");

        // Build response from tasker_config (same logic as REST handler)
        let tasker_config = &self.state.services.orchestration_core.context.tasker_config;
        let web_config = tasker_config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref());
        let auth = web_config.and_then(|w| w.auth.as_ref());
        let queues = &tasker_config.common.queues;
        let cb = &tasker_config.common.circuit_breakers;

        let deployment_mode = tasker_config
            .orchestration
            .as_ref()
            .map(|o| format!("{:?}", o.event_systems.orchestration.deployment_mode))
            .unwrap_or_else(|| "Unknown".to_string());

        // Build metadata
        let metadata = proto::ConfigMetadata {
            timestamp: Some(datetime_to_timestamp(Utc::now())),
            environment: tasker_config.common.execution.environment.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Build safe auth config
        let auth_config = proto::SafeAuthConfig {
            enabled: auth.map(|a| a.enabled).unwrap_or(false),
            verification_method: auth
                .map(|a| a.jwt_verification_method.clone())
                .unwrap_or_else(|| "none".to_string()),
            jwt_issuer: auth.map(|a| a.jwt_issuer.clone()).unwrap_or_default(),
            jwt_audience: auth.map(|a| a.jwt_audience.clone()).unwrap_or_default(),
            api_key_header: auth
                .map(|a| a.api_key_header.clone())
                .unwrap_or_else(|| "X-API-Key".to_string()),
            api_key_count: auth.map(|a| a.api_keys.len() as i32).unwrap_or(0),
            strict_validation: auth.map(|a| a.strict_validation).unwrap_or(true),
            allowed_algorithms: auth
                .map(|a| a.jwt_allowed_algorithms.clone())
                .unwrap_or_else(|| vec!["RS256".to_string()]),
        };

        // Build circuit breaker config
        let circuit_breakers = proto::SafeCircuitBreakerConfig {
            enabled: cb.enabled,
            failure_threshold: cb.default_config.failure_threshold,
            timeout_seconds: cb.default_config.timeout_seconds,
            success_threshold: cb.default_config.success_threshold,
        };

        // Build database pools config
        let database_pools = web_config
            .map(|w| &w.database_pools)
            .map(|p| proto::SafeDatabasePoolConfig {
                web_api_pool_size: p.web_api_pool_size,
                web_api_max_connections: p.web_api_max_connections,
            })
            .unwrap_or(proto::SafeDatabasePoolConfig {
                web_api_pool_size: 0,
                web_api_max_connections: 0,
            });

        // Build messaging config
        let messaging = proto::SafeMessagingConfig {
            backend: queues.backend.clone(),
            queues: vec![
                queues.orchestration_queues.task_requests.clone(),
                queues.orchestration_queues.task_finalizations.clone(),
                queues.orchestration_queues.step_results.clone(),
            ],
        };

        let response = proto::GetConfigResponse {
            metadata: Some(metadata),
            auth: Some(auth_config),
            circuit_breakers: Some(circuit_breakers),
            database_pools: Some(database_pools),
            deployment_mode,
            messaging: Some(messaging),
        };

        Ok(Response::new(response))
    }
}
