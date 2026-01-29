//! # Unified Transport Abstraction
//!
//! Provides a transport-agnostic interface for orchestration and worker clients.
//! This module enables code (especially tests) to work with either REST or gRPC
//! transports without modification.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_client::{ClientConfig, UnifiedOrchestrationClient};
//!
//! // Create client based on configured transport
//! let config = ClientConfig::load()?;
//! let client = UnifiedOrchestrationClient::from_config(&config).await?;
//!
//! // Works the same regardless of transport
//! let health = client.health_check().await?;
//! ```

use async_trait::async_trait;
use uuid::Uuid;

use tasker_shared::{
    models::core::task_request::TaskRequest,
    types::api::orchestration::{
        DetailedHealthResponse, HealthResponse, StepAuditResponse, StepManualAction, StepResponse,
        TaskListResponse, TaskResponse,
    },
    types::api::templates::{TemplateDetail, TemplateListResponse},
};

use crate::config::{ClientConfig, Transport};
use crate::error::{ClientError, ClientResult};
use crate::{OrchestrationApiClient, OrchestrationApiConfig};

#[cfg(feature = "grpc")]
use crate::grpc_clients::{GrpcClientConfig, OrchestrationGrpcClient};

/// Common interface for orchestration clients regardless of transport.
///
/// This trait defines the operations that both REST and gRPC clients support,
/// enabling transport-agnostic code.
#[async_trait]
pub trait OrchestrationClient: Send + Sync {
    /// Get the transport name for debugging/logging.
    fn transport_name(&self) -> &'static str;

    /// Get the endpoint URL.
    fn endpoint(&self) -> &str;

    // ===================================================================================
    // TASK OPERATIONS
    // ===================================================================================

    /// Create a new task.
    async fn create_task(&self, request: TaskRequest) -> ClientResult<TaskResponse>;

    /// Get a task by UUID.
    async fn get_task(&self, task_uuid: Uuid) -> ClientResult<TaskResponse>;

    /// List tasks with pagination.
    async fn list_tasks(
        &self,
        limit: i32,
        offset: i32,
        namespace: Option<&str>,
        status: Option<&str>,
    ) -> ClientResult<TaskListResponse>;

    /// Cancel a task.
    async fn cancel_task(&self, task_uuid: Uuid) -> ClientResult<()>;

    // ===================================================================================
    // STEP OPERATIONS
    // ===================================================================================

    /// List workflow steps for a task.
    async fn list_task_steps(&self, task_uuid: Uuid) -> ClientResult<Vec<StepResponse>>;

    /// Get a specific workflow step.
    async fn get_step(&self, task_uuid: Uuid, step_uuid: Uuid) -> ClientResult<StepResponse>;

    /// Manually resolve a workflow step.
    async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> ClientResult<StepResponse>;

    /// Get audit history for a workflow step.
    async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> ClientResult<Vec<StepAuditResponse>>;

    // ===================================================================================
    // TEMPLATE OPERATIONS
    // ===================================================================================

    /// List all available templates.
    async fn list_templates(&self, namespace: Option<&str>) -> ClientResult<TemplateListResponse>;

    /// Get details about a specific template.
    async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> ClientResult<TemplateDetail>;

    // ===================================================================================
    // HEALTH OPERATIONS
    // ===================================================================================

    /// Check if the orchestration API is healthy.
    async fn health_check(&self) -> ClientResult<()>;

    /// Get basic health status.
    async fn get_basic_health(&self) -> ClientResult<HealthResponse>;

    /// Get detailed health status.
    async fn get_detailed_health(&self) -> ClientResult<DetailedHealthResponse>;
}

// ===================================================================================
// REST TRANSPORT IMPLEMENTATION
// ===================================================================================

/// REST transport wrapper that implements the unified interface.
#[derive(Debug)]
pub struct RestOrchestrationClient {
    inner: OrchestrationApiClient,
}

impl RestOrchestrationClient {
    /// Create a new REST orchestration client.
    pub fn new(config: OrchestrationApiConfig) -> ClientResult<Self> {
        let inner = OrchestrationApiClient::new(config)
            .map_err(|e| ClientError::Internal(format!("Failed to create REST client: {}", e)))?;
        Ok(Self { inner })
    }

    /// Create from ClientConfig.
    pub fn from_client_config(config: &ClientConfig) -> ClientResult<Self> {
        let api_config = OrchestrationApiConfig {
            base_url: config.orchestration.base_url.clone(),
            timeout_ms: config.orchestration.timeout_ms,
            max_retries: config.orchestration.max_retries,
            auth: config.orchestration.resolve_web_auth_config(),
        };
        Self::new(api_config)
    }
}

#[async_trait]
impl OrchestrationClient for RestOrchestrationClient {
    fn transport_name(&self) -> &'static str {
        "REST"
    }

    fn endpoint(&self) -> &str {
        self.inner.base_url()
    }

    async fn create_task(&self, request: TaskRequest) -> ClientResult<TaskResponse> {
        self.inner
            .create_task(request)
            .await
            .map_err(|e| ClientError::Internal(format!("Create task failed: {}", e)))
    }

    async fn get_task(&self, task_uuid: Uuid) -> ClientResult<TaskResponse> {
        self.inner
            .get_task(task_uuid)
            .await
            .map_err(|e| ClientError::Internal(format!("Get task failed: {}", e)))
    }

    async fn list_tasks(
        &self,
        limit: i32,
        offset: i32,
        namespace: Option<&str>,
        status: Option<&str>,
    ) -> ClientResult<TaskListResponse> {
        use tasker_shared::models::core::task::TaskListQuery;

        // Convert limit/offset to page/per_page
        let per_page = limit as u32;
        let page = if per_page > 0 {
            ((offset as u32) / per_page) + 1
        } else {
            1
        };

        let query = TaskListQuery {
            page,
            per_page,
            namespace: namespace.map(String::from),
            status: status.map(String::from),
            ..Default::default()
        };

        self.inner
            .list_tasks(&query)
            .await
            .map_err(|e| ClientError::Internal(format!("List tasks failed: {}", e)))
    }

    async fn cancel_task(&self, task_uuid: Uuid) -> ClientResult<()> {
        self.inner
            .cancel_task(task_uuid)
            .await
            .map_err(|e| ClientError::Internal(format!("Cancel task failed: {}", e)))
    }

    async fn list_task_steps(&self, task_uuid: Uuid) -> ClientResult<Vec<StepResponse>> {
        self.inner
            .list_task_steps(task_uuid)
            .await
            .map_err(|e| ClientError::Internal(format!("List steps failed: {}", e)))
    }

    async fn get_step(&self, task_uuid: Uuid, step_uuid: Uuid) -> ClientResult<StepResponse> {
        self.inner
            .get_step(task_uuid, step_uuid)
            .await
            .map_err(|e| ClientError::Internal(format!("Get step failed: {}", e)))
    }

    async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> ClientResult<StepResponse> {
        self.inner
            .resolve_step_manually(task_uuid, step_uuid, action)
            .await
            .map_err(|e| ClientError::Internal(format!("Resolve step failed: {}", e)))
    }

    async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> ClientResult<Vec<StepAuditResponse>> {
        self.inner
            .get_step_audit_history(task_uuid, step_uuid)
            .await
            .map_err(|e| ClientError::Internal(format!("Get step audit failed: {}", e)))
    }

    async fn list_templates(&self, namespace: Option<&str>) -> ClientResult<TemplateListResponse> {
        self.inner
            .list_templates(namespace)
            .await
            .map_err(|e| ClientError::Internal(format!("List templates failed: {}", e)))
    }

    async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> ClientResult<TemplateDetail> {
        self.inner
            .get_template(namespace, name, version)
            .await
            .map_err(|e| ClientError::Internal(format!("Get template failed: {}", e)))
    }

    async fn health_check(&self) -> ClientResult<()> {
        self.inner
            .health_check()
            .await
            .map_err(|e| ClientError::Internal(format!("Health check failed: {}", e)))
    }

    async fn get_basic_health(&self) -> ClientResult<HealthResponse> {
        self.inner
            .get_basic_health()
            .await
            .map_err(|e| ClientError::Internal(format!("Get basic health failed: {}", e)))
    }

    async fn get_detailed_health(&self) -> ClientResult<DetailedHealthResponse> {
        self.inner
            .get_detailed_health()
            .await
            .map_err(|e| ClientError::Internal(format!("Get detailed health failed: {}", e)))
    }
}

// ===================================================================================
// GRPC TRANSPORT IMPLEMENTATION
// ===================================================================================

#[cfg(feature = "grpc")]
/// gRPC transport wrapper that implements the unified interface.
#[derive(Debug)]
pub struct GrpcOrchestrationClient {
    inner: OrchestrationGrpcClient,
    endpoint: String,
}

#[cfg(feature = "grpc")]
impl GrpcOrchestrationClient {
    /// Create a new gRPC orchestration client.
    pub async fn connect(endpoint: impl Into<String>) -> ClientResult<Self> {
        let endpoint = endpoint.into();
        let inner = OrchestrationGrpcClient::connect(endpoint.clone()).await?;
        Ok(Self { inner, endpoint })
    }

    /// Create with full configuration.
    pub async fn with_config(config: GrpcClientConfig) -> ClientResult<Self> {
        let endpoint = config.endpoint.clone();
        let inner = OrchestrationGrpcClient::with_config(config).await?;
        Ok(Self { inner, endpoint })
    }

    /// Create from ClientConfig.
    pub async fn from_client_config(config: &ClientConfig) -> ClientResult<Self> {
        use crate::grpc_clients::GrpcAuthConfig;

        let endpoint = config.orchestration.base_url.clone();

        let grpc_config = if let Some(ref auth) = config.orchestration.auth {
            let auth_config = match &auth.method {
                crate::config::ClientAuthMethod::ApiKey { key, header_name } => {
                    GrpcAuthConfig::with_api_key_header(key.clone(), header_name.clone())
                }
                crate::config::ClientAuthMethod::BearerToken(token) => {
                    GrpcAuthConfig::with_bearer_token(token.clone())
                }
            };
            GrpcClientConfig::new(endpoint.clone()).with_auth(auth_config)
        } else {
            GrpcClientConfig::new(endpoint.clone())
        };

        let inner = OrchestrationGrpcClient::with_config(grpc_config).await?;
        Ok(Self { inner, endpoint })
    }
}

#[cfg(feature = "grpc")]
#[async_trait]
impl OrchestrationClient for GrpcOrchestrationClient {
    fn transport_name(&self) -> &'static str {
        "gRPC"
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn create_task(&self, request: TaskRequest) -> ClientResult<TaskResponse> {
        self.inner.create_task(request).await
    }

    async fn get_task(&self, task_uuid: Uuid) -> ClientResult<TaskResponse> {
        self.inner.get_task(task_uuid).await
    }

    async fn list_tasks(
        &self,
        limit: i32,
        offset: i32,
        namespace: Option<&str>,
        status: Option<&str>,
    ) -> ClientResult<TaskListResponse> {
        self.inner
            .list_tasks(limit, offset, namespace, status)
            .await
    }

    async fn cancel_task(&self, task_uuid: Uuid) -> ClientResult<()> {
        self.inner.cancel_task(task_uuid).await
    }

    async fn list_task_steps(&self, task_uuid: Uuid) -> ClientResult<Vec<StepResponse>> {
        self.inner.list_task_steps(task_uuid).await
    }

    async fn get_step(&self, task_uuid: Uuid, step_uuid: Uuid) -> ClientResult<StepResponse> {
        self.inner.get_step(task_uuid, step_uuid).await
    }

    async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> ClientResult<StepResponse> {
        self.inner
            .resolve_step_manually(task_uuid, step_uuid, action)
            .await
    }

    async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> ClientResult<Vec<StepAuditResponse>> {
        self.inner
            .get_step_audit_history(task_uuid, step_uuid)
            .await
    }

    async fn list_templates(&self, namespace: Option<&str>) -> ClientResult<TemplateListResponse> {
        self.inner.list_templates(namespace).await
    }

    async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> ClientResult<TemplateDetail> {
        self.inner.get_template(namespace, name, version).await
    }

    async fn health_check(&self) -> ClientResult<()> {
        self.inner.health_check().await
    }

    async fn get_basic_health(&self) -> ClientResult<HealthResponse> {
        self.inner.get_basic_health().await
    }

    async fn get_detailed_health(&self) -> ClientResult<DetailedHealthResponse> {
        self.inner.get_detailed_health().await
    }
}

// ===================================================================================
// UNIFIED CLIENT
// ===================================================================================

/// Unified orchestration client that selects transport based on configuration.
///
/// This enum wraps either a REST or gRPC client and dispatches to the appropriate
/// implementation. Use this when you want transport-agnostic code.
#[derive(Debug)]
pub enum UnifiedOrchestrationClient {
    /// REST transport (boxed to reduce enum size)
    Rest(Box<RestOrchestrationClient>),
    /// gRPC transport (requires `grpc` feature)
    /// Boxed to reduce enum size (GrpcOrchestrationClient is ~1728 bytes, REST ~440 bytes)
    #[cfg(feature = "grpc")]
    Grpc(Box<GrpcOrchestrationClient>),
}

impl UnifiedOrchestrationClient {
    /// Create a client from configuration, selecting transport automatically.
    pub async fn from_config(config: &ClientConfig) -> ClientResult<Self> {
        match config.transport {
            Transport::Rest => {
                let client = RestOrchestrationClient::from_client_config(config)?;
                Ok(UnifiedOrchestrationClient::Rest(Box::new(client)))
            }
            #[cfg(feature = "grpc")]
            Transport::Grpc => {
                let client = GrpcOrchestrationClient::from_client_config(config).await?;
                Ok(UnifiedOrchestrationClient::Grpc(Box::new(client)))
            }
            #[cfg(not(feature = "grpc"))]
            Transport::Grpc => Err(ClientError::Internal(
                "gRPC transport requested but 'grpc' feature is not enabled".to_string(),
            )),
        }
    }

    /// Create a REST client directly.
    pub fn rest(config: OrchestrationApiConfig) -> ClientResult<Self> {
        let client = RestOrchestrationClient::new(config)?;
        Ok(UnifiedOrchestrationClient::Rest(Box::new(client)))
    }

    /// Create a gRPC client directly.
    #[cfg(feature = "grpc")]
    pub async fn grpc(endpoint: impl Into<String>) -> ClientResult<Self> {
        let client = GrpcOrchestrationClient::connect(endpoint).await?;
        Ok(UnifiedOrchestrationClient::Grpc(Box::new(client)))
    }

    /// Get a reference to the trait object for polymorphic use.
    pub fn as_client(&self) -> &dyn OrchestrationClient {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.as_ref(),
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.as_ref(),
        }
    }
}

#[async_trait]
impl OrchestrationClient for UnifiedOrchestrationClient {
    fn transport_name(&self) -> &'static str {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.transport_name(),
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.transport_name(),
        }
    }

    fn endpoint(&self) -> &str {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.endpoint(),
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.endpoint(),
        }
    }

    async fn create_task(&self, request: TaskRequest) -> ClientResult<TaskResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.create_task(request).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.create_task(request).await,
        }
    }

    async fn get_task(&self, task_uuid: Uuid) -> ClientResult<TaskResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.get_task(task_uuid).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.get_task(task_uuid).await,
        }
    }

    async fn list_tasks(
        &self,
        limit: i32,
        offset: i32,
        namespace: Option<&str>,
        status: Option<&str>,
    ) -> ClientResult<TaskListResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => {
                c.list_tasks(limit, offset, namespace, status).await
            }
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => {
                c.list_tasks(limit, offset, namespace, status).await
            }
        }
    }

    async fn cancel_task(&self, task_uuid: Uuid) -> ClientResult<()> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.cancel_task(task_uuid).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.cancel_task(task_uuid).await,
        }
    }

    async fn list_task_steps(&self, task_uuid: Uuid) -> ClientResult<Vec<StepResponse>> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.list_task_steps(task_uuid).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.list_task_steps(task_uuid).await,
        }
    }

    async fn get_step(&self, task_uuid: Uuid, step_uuid: Uuid) -> ClientResult<StepResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.get_step(task_uuid, step_uuid).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.get_step(task_uuid, step_uuid).await,
        }
    }

    async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
    ) -> ClientResult<StepResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => {
                c.resolve_step_manually(task_uuid, step_uuid, action).await
            }
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => {
                c.resolve_step_manually(task_uuid, step_uuid, action).await
            }
        }
    }

    async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> ClientResult<Vec<StepAuditResponse>> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => {
                c.get_step_audit_history(task_uuid, step_uuid).await
            }
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => {
                c.get_step_audit_history(task_uuid, step_uuid).await
            }
        }
    }

    async fn list_templates(&self, namespace: Option<&str>) -> ClientResult<TemplateListResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.list_templates(namespace).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.list_templates(namespace).await,
        }
    }

    async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> ClientResult<TemplateDetail> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.get_template(namespace, name, version).await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.get_template(namespace, name, version).await,
        }
    }

    async fn health_check(&self) -> ClientResult<()> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.health_check().await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.health_check().await,
        }
    }

    async fn get_basic_health(&self) -> ClientResult<HealthResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.get_basic_health().await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.get_basic_health().await,
        }
    }

    async fn get_detailed_health(&self) -> ClientResult<DetailedHealthResponse> {
        match self {
            UnifiedOrchestrationClient::Rest(c) => c.get_detailed_health().await,
            #[cfg(feature = "grpc")]
            UnifiedOrchestrationClient::Grpc(c) => c.get_detailed_health().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rest_client_creation() {
        let config = OrchestrationApiConfig::default();
        let client = RestOrchestrationClient::new(config).unwrap();
        assert_eq!(client.transport_name(), "REST");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[test]
    fn test_unified_client_rest() {
        let config = OrchestrationApiConfig::default();
        let client = UnifiedOrchestrationClient::rest(config).unwrap();
        assert_eq!(client.transport_name(), "REST");
    }

    #[tokio::test]
    async fn test_unified_from_config_rest() {
        let config = ClientConfig::default();
        let client = UnifiedOrchestrationClient::from_config(&config)
            .await
            .unwrap();
        assert_eq!(client.transport_name(), "REST");
    }

    #[cfg(feature = "grpc")]
    #[test]
    fn test_grpc_transport_name() {
        // Can't test actual connection without a server, but we can verify types compile
        assert_eq!(
            std::any::type_name::<GrpcOrchestrationClient>(),
            "tasker_client::transport::GrpcOrchestrationClient"
        );
    }
}
