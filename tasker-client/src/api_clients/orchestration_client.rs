//! # Orchestration API Client
//!
//! Comprehensive HTTP client for communicating with the tasker-orchestration web API.
//! Provides methods for all available endpoints including tasks, workflow steps, handlers,
//! analytics, and health monitoring.

use reqwest::{Client, Url};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{
    config::WebAuthConfig,
    errors::{TaskerError, TaskerResult},
    models::core::{task::TaskListQuery, task_request::TaskRequest},
    types::{
        api::orchestration::{
            BottleneckAnalysis, BottleneckQuery, DetailedHealthResponse, HandlerInfo,
            HealthResponse, ManualResolutionRequest, MetricsQuery, NamespaceInfo,
            PerformanceMetrics, StepResponse, TaskCreationResponse, TaskListResponse, TaskResponse,
        },
        auth::JwtAuthenticator,
        web::AuthConfig,
    },
};

/// Configuration for the orchestration API client
#[derive(Debug, Clone)]
pub struct OrchestrationApiConfig {
    /// Base URL for the orchestration API (e.g., "http://orchestration:8080")
    pub base_url: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// API authentication token (if required)
    pub auth: Option<WebAuthConfig>,
}

impl Default for OrchestrationApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        }
    }
}

impl OrchestrationApiConfig {
    /// Create OrchestrationApiConfig from TaskerConfig with proper configuration loading
    ///
    /// TAS-43: This method replaces ::default() usage with configuration-driven setup
    pub fn from_tasker_config(config: &tasker_shared::config::TaskerConfig) -> Self {
        // Handle optional web configuration with sensible defaults
        let orchestration_web_config = config.orchestration.web_config();

        Self {
            base_url: format!("http://{}", orchestration_web_config.bind_address.clone()),
            timeout_ms: orchestration_web_config.request_timeout_ms,
            max_retries: 3,
            auth: Some(orchestration_web_config.auth.clone()),
        }
    }
}

/// HTTP client for communicating with the orchestration system
#[derive(Clone)]
pub struct OrchestrationApiClient {
    client: Client,
    config: OrchestrationApiConfig,
    base_url: Url,
}

impl OrchestrationApiClient {
    /// Create a new orchestration API client with the given configuration
    pub fn new(config: OrchestrationApiConfig) -> TaskerResult<Self> {
        let base_url = Url::parse(&config.base_url)
            .map_err(|e| TaskerError::ConfigurationError(format!("Invalid base URL: {}", e)))?;

        let mut client_builder = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent(format!("tasker-worker/{}", env!("CARGO_PKG_VERSION")));

        // Configure authentication if provided and enabled
        if let Some(ref web_auth_config) = &config.auth {
            if web_auth_config.enabled {
                let mut default_headers = reqwest::header::HeaderMap::new();

                // Determine authentication method based on available credentials
                if !web_auth_config.api_key.is_empty() {
                    // Use API key authentication
                    let header_name = if web_auth_config.api_key_header.is_empty() {
                        "X-API-Key"
                    } else {
                        &web_auth_config.api_key_header
                    };

                    default_headers.insert(
                        reqwest::header::HeaderName::from_bytes(header_name.as_bytes()).map_err(
                            |e| {
                                TaskerError::ConfigurationError(format!(
                                    "Invalid API key header name: {}",
                                    e
                                ))
                            },
                        )?,
                        web_auth_config.api_key.parse().map_err(|e| {
                            TaskerError::ConfigurationError(format!("Invalid API key: {}", e))
                        })?,
                    );

                    debug!(
                        "Configured API key authentication with header: {}",
                        header_name
                    );
                } else if !web_auth_config.jwt_public_key.is_empty()
                    && !web_auth_config.jwt_private_key.is_empty()
                {
                    // Create JWT authenticator and generate a worker token
                    let jwt_authenticator = JwtAuthenticator::from_config(&web_auth_config)
                        .map_err(|e| {
                            TaskerError::ConfigurationError(format!(
                                "Failed to create JWT authenticator: {}",
                                e
                            ))
                        })?;

                    // For the orchestration client, we need to generate a token for this worker
                    // We'll use some default permissions for orchestration communication
                    let worker_token = jwt_authenticator
                        .generate_worker_token(
                            "orchestration-client",
                            vec!["*".to_string()], // Allow access to all namespaces
                            vec!["orchestration:read", "orchestration:write"]
                                .into_iter()
                                .map(|s| s.to_string())
                                .collect(),
                        )
                        .map_err(|e| {
                            TaskerError::ConfigurationError(format!(
                                "Failed to generate worker token: {}",
                                e
                            ))
                        })?;

                    default_headers.insert(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", worker_token).parse().map_err(|e| {
                            TaskerError::ConfigurationError(format!("Invalid JWT token: {}", e))
                        })?,
                    );

                    debug!("Configured JWT authentication with worker token");
                } else {
                    warn!("Authentication enabled but no valid credentials (API key or JWT keys) configured");
                }

                if !default_headers.is_empty() {
                    client_builder = client_builder.default_headers(default_headers);
                }
            }
        }

        let client = client_builder.build().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to create HTTP client: {}", e))
        })?;

        info!(
            base_url = %config.base_url,
            timeout_ms = config.timeout_ms,
            auth_enabled = config.auth.is_some(),
            "Created orchestration API client"
        );

        Ok(Self {
            client,
            config,
            base_url,
        })
    }

    /// Create a new task via the orchestration API
    ///
    /// POST /v1/tasks
    pub async fn create_task(&self, request: TaskRequest) -> TaskerResult<TaskCreationResponse> {
        let url = self.base_url.join("/v1/tasks").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(
            url = %url,
            namespace = %request.namespace,
            task_name = %request.name,
            "Creating task via orchestration API"
        );

        let mut retries = 0;
        loop {
            let response = self.client.post(url.clone()).json(&request).send().await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<TaskCreationResponse>().await {
                            Ok(task_response) => {
                                info!(
                                    task_uuid = %task_response.task_uuid,
                                    step_count = task_response.step_count,
                                    "Successfully created task via orchestration API"
                                );
                                return Ok(task_response);
                            }
                            Err(e) => {
                                error!(error = %e, "Failed to parse task creation response");
                                return Err(TaskerError::OrchestrationError(format!(
                                    "Invalid response format: {}",
                                    e
                                )));
                            }
                        }
                    } else {
                        let status = resp.status();
                        let error_text = resp
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        // Don't retry client errors (4xx)
                        if status.is_client_error() {
                            error!(status = %status, error = %error_text, "Client error creating task");
                            return Err(TaskerError::OrchestrationError(format!(
                                "HTTP {}: {}",
                                status, error_text
                            )));
                        }

                        warn!(
                            status = %status,
                            error = %error_text,
                            retry = retries + 1,
                            max_retries = self.config.max_retries,
                            "Server error creating task, will retry"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        retry = retries + 1,
                        max_retries = self.config.max_retries,
                        "Network error creating task, will retry"
                    );
                }
            }

            retries += 1;
            if retries >= self.config.max_retries {
                error!(
                    retries = retries,
                    max_retries = self.config.max_retries,
                    "Exhausted all retries for task creation"
                );
                return Err(TaskerError::OrchestrationError(
                    "Failed to create task after all retries".to_string(),
                ));
            }

            // Exponential backoff: 1s, 2s, 4s, ...
            let delay = Duration::from_secs(1 << retries);
            tokio::time::sleep(delay).await;
        }
    }

    /// Get task via the orchestration API
    ///
    /// GET /v1/tasks/{task_uuid}
    pub async fn get_task(&self, task_uuid: Uuid) -> TaskerResult<TaskResponse> {
        let url = self
            .base_url
            .join(&format!("/v1/tasks/{}", task_uuid))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            "Getting task status via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        if response.status().is_success() {
            let task_status = response.json::<TaskResponse>().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to parse task status response: {}",
                    e
                ))
            })?;

            debug!(
                task_uuid = %task_uuid,
                status = %task_status.status,
                "Successfully retrieved task status"
            );

            Ok(task_status)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Failed to get task status");
            Err(TaskerError::OrchestrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }

    /// Check if the orchestration API is healthy
    ///
    /// GET /health
    pub async fn health_check(&self) -> TaskerResult<()> {
        let url = self.base_url.join("/health").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Checking orchestration API health");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Health check request failed: {}", e))
        })?;

        if response.status().is_success() {
            debug!("Orchestration API health check passed");
            Ok(())
        } else {
            let status = response.status();
            warn!(status = %status, "Orchestration API health check failed");
            Err(TaskerError::OrchestrationError(format!(
                "Health check failed with status: {}",
                status
            )))
        }
    }

    /// Get the configured base URL for debugging/logging
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Get the configured timeout for debugging/logging
    pub fn timeout_ms(&self) -> u64 {
        self.config.timeout_ms
    }

    // ===================================================================================
    // TASKS API METHODS
    // ===================================================================================

    /// List tasks with pagination and filtering
    ///
    /// GET /v1/tasks
    pub async fn list_tasks(&self, query: &TaskListQuery) -> TaskerResult<TaskListResponse> {
        let mut url = self.base_url.join("/v1/tasks").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        // Build query parameters
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.append_pair("page", &query.page.to_string());
        query_pairs.append_pair("per_page", &query.per_page.to_string());

        if let Some(ref namespace) = query.namespace {
            query_pairs.append_pair("namespace", namespace);
        }
        if let Some(ref status) = query.status {
            query_pairs.append_pair("status", status);
        }
        if let Some(ref initiator) = query.initiator {
            query_pairs.append_pair("initiator", initiator);
        }
        if let Some(ref source_system) = query.source_system {
            query_pairs.append_pair("source_system", source_system);
        }
        drop(query_pairs);

        debug!(url = %url, "Listing tasks via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list tasks").await
    }

    /// Cancel a task
    ///
    /// DELETE /v1/tasks/{task_uuid}
    pub async fn cancel_task(&self, task_uuid: Uuid) -> TaskerResult<()> {
        let url = self
            .base_url
            .join(&format!("/v1/tasks/{}", task_uuid))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            "Canceling task via orchestration API"
        );

        let response = self.client.delete(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        if response.status().is_success() {
            info!(task_uuid = %task_uuid, "Successfully canceled task");
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Failed to cancel task");
            Err(TaskerError::OrchestrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }

    // ===================================================================================
    // WORKFLOW STEPS API METHODS
    // ===================================================================================

    /// List workflow steps for a task
    ///
    /// GET /v1/tasks/{task_uuid}/workflow_steps
    pub async fn list_task_steps(&self, task_uuid: Uuid) -> TaskerResult<Vec<StepResponse>> {
        let url = self
            .base_url
            .join(&format!("/v1/tasks/{}/workflow_steps", task_uuid))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            "Listing task steps via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list task steps").await
    }

    /// Get a specific workflow step
    ///
    /// GET /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}
    pub async fn get_step(&self, task_uuid: Uuid, step_uuid: Uuid) -> TaskerResult<StepResponse> {
        let url = self
            .base_url
            .join(&format!(
                "/v1/tasks/{}/workflow_steps/{}",
                task_uuid, step_uuid
            ))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            "Getting step via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get step").await
    }

    /// Manually resolve a workflow step
    ///
    /// PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}
    pub async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        request: ManualResolutionRequest,
    ) -> TaskerResult<StepResponse> {
        let url = self
            .base_url
            .join(&format!(
                "/v1/tasks/{}/workflow_steps/{}",
                task_uuid, step_uuid
            ))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            resolved_by = %request.resolved_by,
            "Manually resolving step via orchestration API"
        );

        let response = self
            .client
            .patch(url.clone())
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
            })?;

        self.handle_response(response, "resolve step manually")
            .await
    }

    // ===================================================================================
    // HANDLERS API METHODS
    // ===================================================================================

    /// List all available namespaces
    ///
    /// GET /v1/handlers
    pub async fn list_namespaces(&self) -> TaskerResult<Vec<NamespaceInfo>> {
        let url = self.base_url.join("/v1/handlers").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Listing namespaces via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list namespaces").await
    }

    /// List handlers in a specific namespace
    ///
    /// GET /v1/handlers/{namespace}
    pub async fn list_namespace_handlers(&self, namespace: &str) -> TaskerResult<Vec<HandlerInfo>> {
        let url = self
            .base_url
            .join(&format!("/v1/handlers/{}", namespace))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            namespace = %namespace,
            "Listing namespace handlers via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list namespace handlers")
            .await
    }

    /// Get information about a specific handler
    ///
    /// GET /v1/handlers/{namespace}/{name}
    pub async fn get_handler_info(&self, namespace: &str, name: &str) -> TaskerResult<HandlerInfo> {
        let url = self
            .base_url
            .join(&format!("/v1/handlers/{}/{}", namespace, name))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            namespace = %namespace,
            handler_name = %name,
            "Getting handler info via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get handler info").await
    }

    // ===================================================================================
    // ANALYTICS API METHODS
    // ===================================================================================

    /// Get performance metrics
    ///
    /// GET /v1/analytics/performance
    pub async fn get_performance_metrics(
        &self,
        query: Option<&MetricsQuery>,
    ) -> TaskerResult<PerformanceMetrics> {
        let mut url = self
            .base_url
            .join("/v1/analytics/performance")
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        // Add query parameters if provided
        if let Some(metrics_query) = query {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(hours) = metrics_query.hours {
                query_pairs.append_pair("hours", &hours.to_string());
            }
            drop(query_pairs);
        }

        debug!(url = %url, "Getting performance metrics via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get performance metrics")
            .await
    }

    /// Get bottleneck analysis
    ///
    /// GET /v1/analytics/bottlenecks
    pub async fn get_bottlenecks(
        &self,
        query: Option<&BottleneckQuery>,
    ) -> TaskerResult<BottleneckAnalysis> {
        let mut url = self
            .base_url
            .join("/v1/analytics/bottlenecks")
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        // Add query parameters if provided
        if let Some(bottleneck_query) = query {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(limit) = bottleneck_query.limit {
                query_pairs.append_pair("limit", &limit.to_string());
            }
            if let Some(min_executions) = bottleneck_query.min_executions {
                query_pairs.append_pair("min_executions", &min_executions.to_string());
            }
            drop(query_pairs);
        }

        debug!(url = %url, "Getting bottleneck analysis via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get bottleneck analysis")
            .await
    }

    // ===================================================================================
    // HEALTH API METHODS
    // ===================================================================================

    /// Get basic health status
    ///
    /// GET /health
    pub async fn get_basic_health(&self) -> TaskerResult<HealthResponse> {
        let url = self.base_url.join("/health").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Getting basic health via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get basic health").await
    }

    /// Get detailed health status
    ///
    /// GET /health/detailed
    pub async fn get_detailed_health(&self) -> TaskerResult<DetailedHealthResponse> {
        let url = self.base_url.join("/health/detailed").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Getting detailed health via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get detailed health").await
    }

    /// Check readiness probe (Kubernetes readiness)
    ///
    /// GET /ready
    pub async fn readiness_probe(&self) -> TaskerResult<HealthResponse> {
        let url = self.base_url.join("/ready").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Checking readiness probe via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "readiness probe").await
    }

    /// Check liveness probe (Kubernetes liveness)
    ///
    /// GET /live
    pub async fn liveness_probe(&self) -> TaskerResult<HealthResponse> {
        let url = self.base_url.join("/live").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Checking liveness probe via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "liveness probe").await
    }

    /// Get Prometheus metrics
    ///
    /// GET /metrics
    pub async fn get_prometheus_metrics(&self) -> TaskerResult<String> {
        let url = self.base_url.join("/metrics").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Getting Prometheus metrics via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        if response.status().is_success() {
            let metrics_text = response.text().await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to parse metrics response: {}", e))
            })?;

            debug!("Successfully retrieved Prometheus metrics");
            Ok(metrics_text)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Failed to get Prometheus metrics");
            Err(TaskerError::OrchestrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }

    // ===================================================================================
    // UTILITY METHODS
    // ===================================================================================

    /// Handle HTTP response with proper error handling and deserialization
    async fn handle_response<T>(
        &self,
        response: reqwest::Response,
        operation: &str,
    ) -> TaskerResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        if response.status().is_success() {
            let result = response.json::<T>().await.map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to parse {} response: {}",
                    operation, e
                ))
            })?;

            debug!("Successfully completed operation: {}", operation);
            Ok(result)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Failed operation: {}", operation);
            Err(TaskerError::OrchestrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_orchestration_api_config_default() {
        let config = OrchestrationApiConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_orchestration_client_creation() {
        let config = OrchestrationApiConfig::default();
        let client = OrchestrationApiClient::new(config);
        assert!(client.is_ok());
    }

    // ===================================================================================
    // DESERIALIZATION TESTS
    // ===================================================================================

    #[tokio::test]
    async fn test_task_creation_response_deserialization() {
        let json_response = json!({
            "task_uuid": "123e4567-e89b-12d3-a456-426614174000",
            "status": "initialized",
            "created_at": "2023-12-01T12:00:00Z",
            "estimated_completion": null,
            "step_count": 3,
            "step_mapping": {
                "step1": "uuid1",
                "step2": "uuid2"
            },
            "handler_config_name": "default"
        });

        let response: TaskCreationResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.task_uuid, "123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(response.status, "initialized");
        assert_eq!(response.step_count, 3);
        assert_eq!(response.step_mapping.len(), 2);
    }

    #[tokio::test]
    async fn test_task_response_deserialization() {
        let json_response = json!({
            "task_uuid": "123e4567-e89b-12d3-a456-426614174000",
            "name": "test_task",
            "namespace": "test",
            "version": "1.0.0",
            "status": "pending",
            "created_at": "2023-12-01T12:00:00Z",
            "updated_at": "2023-12-01T12:00:00Z",
            "completed_at": null,
            "context": {},
            "initiator": "test_user",
            "source_system": "test_system",
            "reason": "test_reason",
            "priority": 1,
            "tags": ["test"],
            "total_steps": 3,
            "pending_steps": 3,
            "in_progress_steps": 0,
            "completed_steps": 0,
            "failed_steps": 0,
            "ready_steps": 1,
            "execution_status": "pending",
            "recommended_action": "wait",
            "completion_percentage": 0.0,
            "health_status": "healthy",
            "steps": []
        });

        let response: TaskResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.task_uuid, "123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(response.name, "test_task");
        assert_eq!(response.namespace, "test");
        assert_eq!(response.total_steps, 3);
    }

    #[tokio::test]
    async fn test_step_response_deserialization() {
        let json_response = json!({
            "step_uuid": "step-uuid-123",
            "task_uuid": "task-uuid-123",
            "name": "test_step",
            "created_at": "2023-12-01T12:00:00Z",
            "updated_at": "2023-12-01T12:00:00Z",
            "completed_at": null,
            "results": null,
            "current_state": "pending",
            "dependencies_satisfied": true,
            "retry_eligible": true,
            "ready_for_execution": true,
            "total_parents": 0,
            "completed_parents": 0,
            "attempts": 0,
            "retry_limit": 3,
            "last_failure_at": null,
            "next_retry_at": null,
            "last_attempted_at": null
        });

        let response: StepResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.step_uuid, "step-uuid-123");
        assert_eq!(response.task_uuid, "task-uuid-123");
        assert_eq!(response.name, "test_step");
        assert_eq!(response.current_state, "pending");
    }

    #[tokio::test]
    async fn test_handler_info_deserialization() {
        let json_response = json!({
            "name": "test_handler",
            "namespace": "test",
            "version": "1.0.0",
            "description": "Test handler",
            "step_templates": ["step1", "step2"]
        });

        let response: HandlerInfo = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.name, "test_handler");
        assert_eq!(response.namespace, "test");
        assert_eq!(response.step_templates.len(), 2);
    }

    #[tokio::test]
    async fn test_namespace_info_deserialization() {
        let json_response = json!({
            "name": "test",
            "description": "Test namespace",
            "handler_count": 5
        });

        let response: NamespaceInfo = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.name, "test");
        assert_eq!(response.handler_count, 5);
    }

    #[tokio::test]
    async fn test_performance_metrics_deserialization() {
        let json_response = json!({
            "total_tasks": 100,
            "active_tasks": 10,
            "completed_tasks": 85,
            "failed_tasks": 5,
            "completion_rate": 0.85,
            "error_rate": 0.05,
            "average_task_duration_seconds": 120.5,
            "average_step_duration_seconds": 30.2,
            "tasks_per_hour": 50,
            "steps_per_hour": 200,
            "system_health_score": 0.95,
            "analysis_period_start": "2023-12-01T00:00:00Z",
            "calculated_at": "2023-12-01T12:00:00Z"
        });

        let response: PerformanceMetrics = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.total_tasks, 100);
        assert_eq!(response.completion_rate, 0.85);
    }

    #[tokio::test]
    async fn test_health_response_deserialization() {
        let json_response = json!({
            "status": "healthy",
            "timestamp": "2023-12-01T12:00:00Z"
        });

        let response: HealthResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.status, "healthy");
    }

    #[tokio::test]
    async fn test_detailed_health_response_deserialization() {
        let json_response = json!({
            "status": "healthy",
            "timestamp": "2023-12-01T12:00:00Z",
            "checks": {
                "database": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 5
                }
            },
            "info": {
                "version": "1.0.0",
                "environment": "test",
                "operational_state": "normal",
                "web_database_pool_size": 10,
                "orchestration_database_pool_size": 20,
                "circuit_breaker_state": "closed"
            }
        });

        let response: DetailedHealthResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.status, "healthy");
        assert_eq!(response.info.version, "1.0.0");
        assert!(response.checks.contains_key("database"));
    }

    // ===================================================================================
    // QUERY PARAMETER TESTS
    // ===================================================================================

    #[test]
    fn test_task_list_query_default() {
        let query = TaskListQuery::default();
        assert_eq!(query.page, 1);
        assert_eq!(query.per_page, 25);
        assert!(query.namespace.is_none());
        assert!(query.status.is_none());
    }

    #[test]
    fn test_manual_resolution_request_serialization() {
        let request = ManualResolutionRequest {
            resolution_data: json!({"result": "success"}),
            resolved_by: "admin".to_string(),
            reason: "Manual intervention required".to_string(),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("resolution_data"));
        assert!(serialized.contains("resolved_by"));
        assert!(serialized.contains("reason"));
    }

    // ===================================================================================
    // URL CONSTRUCTION TESTS
    // ===================================================================================

    #[test]
    fn test_base_url_construction() {
        let config = OrchestrationApiConfig::default();
        let client = OrchestrationApiClient::new(config).unwrap();
        assert_eq!(client.base_url(), "http://localhost:8080");
    }

    #[test]
    fn test_timeout_configuration() {
        let config = OrchestrationApiConfig {
            timeout_ms: 60000,
            ..Default::default()
        };
        let client = OrchestrationApiClient::new(config).unwrap();
        assert_eq!(client.timeout_ms(), 60000);
    }

    // ===================================================================================
    // CONFIGURATION TESTS
    // ===================================================================================
}
