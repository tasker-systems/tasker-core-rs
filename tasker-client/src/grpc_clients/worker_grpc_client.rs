//! # Worker gRPC Client
//!
//! gRPC client for communicating with the tasker-worker gRPC API.
//! Provides methods for health checks, templates, and configuration.
//!
//! This client mirrors the REST API client interface, returning the same domain types.

use tonic::transport::Channel;
use tracing::{debug, info};

use tasker_shared::{
    proto::{
        client::{
            WorkerConfigServiceClient, WorkerHealthServiceClient, WorkerTemplateServiceClient,
        },
        v1 as proto,
    },
    types::api::{
        orchestration::WorkerConfigResponse,
        worker::{
            BasicHealthResponse, DetailedHealthResponse, ReadinessResponse, TemplateListResponse,
            TemplateQueryParams, TemplateResponse,
        },
    },
};

use super::common::{AuthInterceptor, GrpcAuthConfig, GrpcClientConfig};
use super::conversions;
use crate::error::ClientError;

/// gRPC client for communicating with the worker system.
///
/// This client provides methods to interact with worker health, template,
/// and configuration endpoints using gRPC as the transport protocol.
/// It mirrors the REST client interface, returning the same domain types.
///
/// # Examples
///
/// ```rust,ignore
/// use tasker_client::grpc_clients::WorkerGrpcClient;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Connect with default config (port 9100)
///     let client = WorkerGrpcClient::connect("http://localhost:9100").await?;
///
///     // Check health
///     let health = client.health_check().await?;
///     println!("Worker status: {}", health.status);
///
///     // List templates
///     let templates = client.list_templates(None).await?;
///     println!("Template count: {}", templates.template_count);
///
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct WorkerGrpcClient {
    health_client: WorkerHealthServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    template_client: WorkerTemplateServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    config_client: WorkerConfigServiceClient<
        tonic::service::interceptor::InterceptedService<Channel, AuthInterceptor>,
    >,
    endpoint: String,
}

impl WorkerGrpcClient {
    /// Connect to a gRPC endpoint with default configuration.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The gRPC endpoint URL (e.g., "http://localhost:9100")
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, ClientError> {
        let endpoint_str = endpoint.into();
        let config = GrpcClientConfig::new(&endpoint_str);
        Self::with_config(config).await
    }

    /// Connect to a gRPC endpoint with authentication.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The gRPC endpoint URL
    /// * `auth` - Authentication configuration
    pub async fn connect_with_auth(
        endpoint: impl Into<String>,
        auth: GrpcAuthConfig,
    ) -> Result<Self, ClientError> {
        let config = GrpcClientConfig::new(endpoint).with_auth(auth);
        Self::with_config(config).await
    }

    /// Connect with full configuration.
    pub async fn with_config(config: GrpcClientConfig) -> Result<Self, ClientError> {
        let endpoint = config.endpoint.clone();
        let channel = config.connect().await?;
        let interceptor = AuthInterceptor::new(config.auth);

        info!(endpoint = %endpoint, "Connected to worker gRPC endpoint");

        Ok(Self {
            health_client: WorkerHealthServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            template_client: WorkerTemplateServiceClient::with_interceptor(
                channel.clone(),
                interceptor.clone(),
            ),
            config_client: WorkerConfigServiceClient::with_interceptor(channel, interceptor),
            endpoint,
        })
    }

    /// Get the configured endpoint URL.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    // ===================================================================================
    // HEALTH API METHODS
    // ===================================================================================

    /// Basic health check.
    pub async fn health_check(&self) -> Result<BasicHealthResponse, ClientError> {
        debug!("Checking worker health via gRPC");

        let response = self
            .health_client
            .clone()
            .check_health(proto::WorkerHealthRequest {})
            .await?
            .into_inner();

        Ok(conversions::proto_worker_basic_health_to_domain(response))
    }

    /// Kubernetes liveness probe.
    pub async fn liveness_probe(&self) -> Result<BasicHealthResponse, ClientError> {
        debug!("Checking worker liveness via gRPC");

        let response = self
            .health_client
            .clone()
            .check_liveness(proto::WorkerLivenessRequest {})
            .await?
            .into_inner();

        Ok(conversions::proto_worker_basic_health_to_domain(response))
    }

    /// Kubernetes readiness probe.
    pub async fn readiness_probe(&self) -> Result<ReadinessResponse, ClientError> {
        debug!("Checking worker readiness via gRPC");

        let response = self
            .health_client
            .clone()
            .check_readiness(proto::WorkerReadinessRequest {})
            .await?
            .into_inner();

        conversions::proto_worker_readiness_to_domain(response)
    }

    /// Detailed health check with subsystem status.
    pub async fn get_detailed_health(&self) -> Result<DetailedHealthResponse, ClientError> {
        debug!("Getting detailed worker health via gRPC");

        let response = self
            .health_client
            .clone()
            .check_detailed_health(proto::WorkerDetailedHealthRequest {})
            .await?
            .into_inner();

        conversions::proto_worker_detailed_health_to_domain(response)
    }

    // ===================================================================================
    // TEMPLATE API METHODS
    // ===================================================================================

    /// List supported templates and namespaces.
    ///
    /// Use `include_cache_stats=true` in query params to include cache statistics.
    pub async fn list_templates(
        &self,
        params: Option<&TemplateQueryParams>,
    ) -> Result<TemplateListResponse, ClientError> {
        debug!("Listing templates via gRPC");

        let response = self
            .template_client
            .clone()
            .list_templates(proto::WorkerListTemplatesRequest {
                namespace: params.and_then(|p| p.namespace.clone()),
                include_cache_stats: params.and_then(|p| p.include_cache_stats).unwrap_or(false),
            })
            .await?
            .into_inner();

        Ok(conversions::proto_worker_template_list_to_domain(response))
    }

    /// Get a specific task template.
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<TemplateResponse, ClientError> {
        debug!(
            namespace = %namespace,
            name = %name,
            version = %version,
            "Getting template via gRPC"
        );

        let response = self
            .template_client
            .clone()
            .get_template(proto::WorkerGetTemplateRequest {
                namespace: namespace.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            })
            .await?
            .into_inner();

        conversions::proto_worker_template_to_domain(response)
    }

    // ===================================================================================
    // CONFIG API METHODS
    // ===================================================================================

    /// Get worker configuration (secrets redacted).
    pub async fn get_config(&self) -> Result<WorkerConfigResponse, ClientError> {
        debug!("Getting worker config via gRPC");

        let response = self
            .config_client
            .clone()
            .get_config(proto::WorkerGetConfigRequest {})
            .await?
            .into_inner();

        conversions::proto_worker_config_to_domain(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_grpc_client_config() {
        // Test that default config uses port 9100
        let config = GrpcClientConfig::new("http://localhost:9100");
        assert_eq!(config.endpoint, "http://localhost:9100");
    }
}
