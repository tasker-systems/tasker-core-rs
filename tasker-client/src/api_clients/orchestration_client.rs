//! # Orchestration API Client
//!
//! Comprehensive HTTP client for communicating with the tasker-orchestration web API.
//! Provides methods for all available endpoints including tasks, workflow steps, templates,
//! analytics, and health monitoring.
//!
//! TAS-76: Legacy /v1/handlers endpoints replaced with /v1/templates.

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
            BottleneckAnalysis, BottleneckQuery, DetailedHealthResponse, HealthResponse,
            MetricsQuery, PerformanceMetrics, StepAuditResponse, StepManualAction, StepResponse,
            TaskListResponse, TaskResponse,
        },
        api::templates::{TemplateDetail, TemplateListResponse},
        auth::JwtAuthenticator,
    },
};

/// Configuration for the orchestration API client
///
/// This struct contains all the necessary configuration options for connecting
/// to and communicating with the Tasker orchestration service.
///
/// # Examples
///
/// ```rust
/// use tasker_client::OrchestrationApiConfig;
/// use tasker_shared::config::WebAuthConfig;
///
/// // Basic configuration with defaults
/// let config = OrchestrationApiConfig::default();
/// assert_eq!(config.base_url, "http://localhost:8080");
/// assert_eq!(config.timeout_ms, 30000);
/// assert_eq!(config.max_retries, 3);
///
/// // Custom configuration with authentication
/// let config = OrchestrationApiConfig {
///     base_url: "https://orchestration.example.com".to_string(),
///     timeout_ms: 60000,
///     max_retries: 5,
///     auth: Some(WebAuthConfig {
///         enabled: true,
///         api_key: "secret-key".to_string(),
///         ..Default::default()
///     }),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct OrchestrationApiConfig {
    /// Base URL for the orchestration API (e.g., "<http://orchestration:8080>")
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

// TAS-61 Phase 6D: Convert from canonical worker.orchestration_client config
impl From<&tasker_shared::config::tasker::OrchestrationClientConfig> for OrchestrationApiConfig {
    fn from(config: &tasker_shared::config::tasker::OrchestrationClientConfig) -> Self {
        Self {
            base_url: config.base_url.clone(),
            timeout_ms: config.timeout_ms as u64,
            max_retries: config.max_retries,
            auth: config.auth.clone().map(|a| a.into()),
        }
    }
}

impl OrchestrationApiConfig {
    /// Create `OrchestrationApiConfig` from `TaskerConfig` with proper configuration loading
    ///
    /// This method replaces manual configuration with values loaded from the
    /// centralized `TaskerConfig` system. It properly extracts web configuration
    /// including authentication settings and network bindings.
    ///
    /// # Arguments
    ///
    /// * `config` - Reference to the loaded `TaskerConfig` containing orchestration settings
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tasker_shared::config::TaskerConfig;
    /// use tasker_client::OrchestrationApiConfig;
    ///
    /// let tasker_config = TaskerConfig::load_for_environment("test").unwrap();
    /// let client_config = OrchestrationApiConfig::from_tasker_config(&tasker_config);
    ///
    /// assert!(client_config.base_url.starts_with("http://"));
    /// ```
    #[must_use]
    pub fn from_tasker_config(config: &tasker_shared::config::tasker::TaskerConfig) -> Self {
        // TAS-61 Phase 6C/6D: V2 configuration access
        // Handle optional web configuration with sensible defaults
        let orchestration_web_config = config
            .orchestration
            .as_ref()
            .and_then(|o| o.web.as_ref())
            .expect("Orchestration web configuration required for client");

        Self {
            base_url: format!("http://{}", orchestration_web_config.bind_address.clone()),
            timeout_ms: orchestration_web_config.request_timeout_ms as u64,
            max_retries: 3,
            auth: orchestration_web_config.auth.clone().map(|a| a.into()),
        }
    }
}

/// HTTP client for communicating with the orchestration system
///
/// This client provides methods to interact with all orchestration API endpoints
/// including task management, workflow steps, performance analytics, and health monitoring.
/// It handles authentication, retries, and proper error handling automatically.
///
/// # Examples
///
/// ```rust,ignore
/// use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
/// use tasker_shared::models::core::task_request::TaskRequest;
///
/// let config = OrchestrationApiConfig::default();
/// let client = OrchestrationApiClient::new(config)?;
///
/// // Create a new task
/// let task_request = TaskRequest {
///     name: "example_task".to_string(),
///     namespace: "default".to_string(),
///     version: "1.0.0".to_string(),
///     context: serde_json::json!({"key": "value"}),
/// };
///
/// let response = client.create_task(task_request).await?;
/// println!("Created task with ID: {}", response.task_id);
/// ```
#[derive(Clone)]
pub struct OrchestrationApiClient {
    client: Client,
    config: OrchestrationApiConfig,
    base_url: Url,
}

impl std::fmt::Debug for OrchestrationApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationApiClient")
            .field("base_url", &self.base_url.as_str())
            .field("timeout_ms", &self.config.timeout_ms)
            .field("max_retries", &self.config.max_retries)
            .field(
                "auth_enabled",
                &self
                    .config
                    .auth
                    .as_ref()
                    .map(|a| a.enabled)
                    .unwrap_or(false),
            )
            .finish()
    }
}

impl OrchestrationApiClient {
    /// Create a new orchestration API client with the given configuration
    ///
    /// This constructor validates the configuration, sets up HTTP client with
    /// proper timeouts and authentication headers, and prepares the client
    /// for making requests to the orchestration API.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration settings for the API client
    ///
    /// # Returns
    ///
    /// Returns a configured `OrchestrationApiClient` instance or an error if
    /// the configuration is invalid (e.g., malformed base URL).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
    ///
    /// let config = OrchestrationApiConfig {
    ///     base_url: "http://localhost:8080".to_string(),
    ///     timeout_ms: 30000,
    ///     max_retries: 3,
    ///     auth: None,
    /// };
    ///
    /// let client = OrchestrationApiClient::new(config).unwrap();
    /// ```
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
                // Priority: bearer_token > api_key > JWT key generation
                if !web_auth_config.bearer_token.is_empty() {
                    // Use pre-existing Bearer token (e.g., from environment variable)
                    default_headers.insert(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", web_auth_config.bearer_token)
                            .parse()
                            .map_err(|e| {
                                TaskerError::ConfigurationError(format!(
                                    "Invalid bearer token: {}",
                                    e
                                ))
                            })?,
                    );

                    debug!("Configured Bearer token authentication");
                } else if !web_auth_config.api_key.is_empty() {
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
                    let jwt_authenticator = JwtAuthenticator::from_config(web_auth_config)
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
                                .map(std::string::ToString::to_string)
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
    ///
    /// Returns the same `TaskResponse` shape as `get_task`, following REST best practices
    /// where create operations return the same representation as read operations.
    pub async fn create_task(&self, request: TaskRequest) -> TaskerResult<TaskResponse> {
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
                        match resp.json::<TaskResponse>().await {
                            Ok(task_response) => {
                                info!(
                                    task_uuid = %task_response.task_uuid,
                                    status = %task_response.status,
                                    total_steps = task_response.total_steps,
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
    /// GET /`v1/tasks/{task_uuid`}
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
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Get the configured timeout for debugging/logging
    #[must_use]
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
    /// DELETE /`v1/tasks/{task_uuid`}
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
    /// GET /`v1/tasks/{task_uuid}/workflow_steps`
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
    /// GET /`v1/tasks/{task_uuid}/workflow_steps/{step_uuid`}
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
    /// PATCH /`v1/tasks/{task_uuid}/workflow_steps/{step_uuid`}
    pub async fn resolve_step_manually(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        action: StepManualAction,
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
            "Performing manual step action via orchestration API"
        );

        let response = self
            .client
            .patch(url.clone())
            .json(&action)
            .send()
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
            })?;

        self.handle_response(response, "resolve step manually")
            .await
    }

    /// Get audit history for a workflow step (TAS-62)
    ///
    /// Returns SOC2-compliant audit trail with worker attribution (worker_uuid, correlation_id)
    /// and execution details. Full execution results are retrieved via JOIN to transition metadata.
    ///
    /// GET `/v1/tasks/{task_uuid}/workflow_steps/{step_uuid}/audit`
    ///
    /// # Arguments
    ///
    /// * `task_uuid` - UUID of the parent task
    /// * `step_uuid` - UUID of the workflow step to get audit history for
    ///
    /// # Returns
    ///
    /// Vec of audit records ordered by recorded_at DESC (most recent first)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let audit_history = client.get_step_audit_history(task_uuid, step_uuid).await?;
    /// for record in audit_history {
    ///     println!("Worker: {:?}, Success: {}, Time: {:?}ms",
    ///         record.worker_uuid,
    ///         record.success,
    ///         record.execution_time_ms
    ///     );
    /// }
    /// ```
    pub async fn get_step_audit_history(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
    ) -> TaskerResult<Vec<StepAuditResponse>> {
        let url = self
            .base_url
            .join(&format!(
                "/v1/tasks/{}/workflow_steps/{}/audit",
                task_uuid, step_uuid
            ))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            "Getting step audit history via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get step audit history")
            .await
    }

    // ===================================================================================
    // TEMPLATES API METHODS (TAS-76: Replaces legacy handlers API)
    // ===================================================================================

    /// List all available templates
    ///
    /// GET /v1/templates
    ///
    /// Returns a list of all registered task templates with namespace summaries.
    /// Optionally filter by namespace using the `namespace` query parameter.
    pub async fn list_templates(
        &self,
        namespace: Option<&str>,
    ) -> TaskerResult<TemplateListResponse> {
        let mut url = self.base_url.join("/v1/templates").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        if let Some(ns) = namespace {
            url.query_pairs_mut().append_pair("namespace", ns);
        }

        debug!(url = %url, namespace = ?namespace, "Listing templates via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list templates").await
    }

    /// Get details about a specific template
    ///
    /// GET /v1/templates/{namespace}/{name}/{version}
    ///
    /// Returns detailed information about a template including its step definitions.
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<TemplateDetail> {
        let url = self
            .base_url
            .join(&format!("/v1/templates/{}/{}/{}", namespace, name, version))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            namespace = %namespace,
            template_name = %name,
            version = %version,
            "Getting template details via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get template").await
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
    /// GET /health/ready
    pub async fn readiness_probe(&self) -> TaskerResult<HealthResponse> {
        let url = self.base_url.join("/health/ready").map_err(|e| {
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
    /// GET /health/live
    pub async fn liveness_probe(&self) -> TaskerResult<HealthResponse> {
        let url = self.base_url.join("/health/live").map_err(|e| {
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
    // CONFIG API METHODS
    // ===================================================================================

    /// Get orchestration configuration (secrets redacted)
    ///
    /// GET /config
    pub async fn get_config(
        &self,
    ) -> TaskerResult<tasker_shared::types::api::orchestration::OrchestrationConfigResponse> {
        let url = self.base_url.join("/config").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Getting orchestration config via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get config").await
    }

    // ===================================================================================
    // DLQ API METHODS (TAS-49)
    // ===================================================================================

    /// List DLQ entries with optional filtering
    ///
    /// GET /v1/dlq
    pub async fn list_dlq_entries(
        &self,
        params: Option<&tasker_shared::models::orchestration::DlqListParams>,
    ) -> TaskerResult<Vec<tasker_shared::models::orchestration::DlqEntry>> {
        let mut url = self.base_url.join("/v1/dlq").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        // Add query parameters if provided
        if let Some(dlq_params) = params {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(status) = &dlq_params.resolution_status {
                query_pairs.append_pair("resolution_status", status.as_str());
            }
            query_pairs.append_pair("limit", &dlq_params.limit.to_string());
            query_pairs.append_pair("offset", &dlq_params.offset.to_string());
            drop(query_pairs);
        }

        debug!(url = %url, "Listing DLQ entries via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "list DLQ entries").await
    }

    /// Get DLQ entry by task UUID
    ///
    /// GET /v1/dlq/task/{task_uuid}
    pub async fn get_dlq_entry(
        &self,
        task_uuid: Uuid,
    ) -> TaskerResult<tasker_shared::models::orchestration::DlqEntry> {
        let url = self
            .base_url
            .join(&format!("/v1/dlq/task/{}", task_uuid))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            task_uuid = %task_uuid,
            "Getting DLQ entry via orchestration API"
        );

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get DLQ entry").await
    }

    /// Update DLQ investigation status
    ///
    /// PATCH /v1/dlq/entry/{dlq_entry_uuid}
    pub async fn update_dlq_investigation(
        &self,
        dlq_entry_uuid: Uuid,
        update: tasker_shared::models::orchestration::DlqInvestigationUpdate,
    ) -> TaskerResult<()> {
        let url = self
            .base_url
            .join(&format!("/v1/dlq/entry/{}", dlq_entry_uuid))
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        debug!(
            url = %url,
            dlq_entry_uuid = %dlq_entry_uuid,
            "Updating DLQ investigation via orchestration API"
        );

        let response = self
            .client
            .patch(url.clone())
            .json(&update)
            .send()
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
            })?;

        if response.status().is_success() {
            info!(dlq_entry_uuid = %dlq_entry_uuid, "Successfully updated DLQ investigation");
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Failed to update DLQ investigation");
            Err(TaskerError::OrchestrationError(format!(
                "HTTP {}: {}",
                status, error_text
            )))
        }
    }

    /// Get DLQ statistics
    ///
    /// GET /v1/dlq/stats
    pub async fn get_dlq_stats(
        &self,
    ) -> TaskerResult<Vec<tasker_shared::models::orchestration::DlqStats>> {
        let url = self.base_url.join("/v1/dlq/stats").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        debug!(url = %url, "Getting DLQ statistics via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get DLQ statistics").await
    }

    /// Get DLQ investigation queue
    ///
    /// GET /v1/dlq/investigation-queue
    ///
    /// Returns a prioritized queue of pending DLQ entries for operator triage.
    /// Entries are ordered by priority score (higher = more urgent).
    ///
    /// # Arguments
    ///
    /// * `limit` - Optional limit for number of entries (default: 100)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let queue = client.get_investigation_queue(Some(50)).await?;
    /// for entry in queue {
    ///     println!("Priority {}: Task {} in DLQ for {} min",
    ///         entry.priority_score,
    ///         entry.task_uuid,
    ///         entry.minutes_in_dlq
    ///     );
    /// }
    /// ```
    pub async fn get_investigation_queue(
        &self,
        limit: Option<i64>,
    ) -> TaskerResult<Vec<tasker_shared::models::orchestration::DlqInvestigationQueueEntry>> {
        let mut url = self
            .base_url
            .join("/v1/dlq/investigation-queue")
            .map_err(|e| {
                TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
            })?;

        // Add query parameter if provided
        if let Some(limit_val) = limit {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("limit", &limit_val.to_string());
            drop(query_pairs);
        }

        debug!(url = %url, limit = ?limit, "Getting DLQ investigation queue via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get investigation queue")
            .await
    }

    /// Get staleness monitoring data
    ///
    /// GET /v1/dlq/staleness
    ///
    /// Returns real-time staleness monitoring for active tasks in waiting states.
    /// Provides proactive visibility into tasks approaching staleness thresholds,
    /// enabling prevention of DLQ entries through early intervention.
    ///
    /// # Arguments
    ///
    /// * `limit` - Optional limit for number of tasks (default: 100)
    ///
    /// # Health Status
    ///
    /// - `healthy` - Task < 80% of staleness threshold
    /// - `warning` - Task at 80-99% of threshold (alerting recommended)
    /// - `stale` - Task ≥ 100% of threshold (DLQ candidate)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let monitoring = client.get_staleness_monitoring(Some(50)).await?;
    /// for task in monitoring {
    ///     if task.health_status.needs_attention() {
    ///         println!("Task {} is {} in state {} for {} min (threshold: {} min)",
    ///             task.task_uuid,
    ///             task.health_status,
    ///             task.current_state,
    ///             task.time_in_state_minutes,
    ///             task.staleness_threshold_minutes
    ///         );
    ///     }
    /// }
    /// ```
    pub async fn get_staleness_monitoring(
        &self,
        limit: Option<i64>,
    ) -> TaskerResult<Vec<tasker_shared::models::orchestration::StalenessMonitoring>> {
        let mut url = self.base_url.join("/v1/dlq/staleness").map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to construct URL: {}", e))
        })?;

        // Add query parameter if provided
        if let Some(limit_val) = limit {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("limit", &limit_val.to_string());
            drop(query_pairs);
        }

        debug!(url = %url, limit = ?limit, "Getting staleness monitoring via orchestration API");

        let response = self.client.get(url.clone()).send().await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Failed to send request: {}", e))
        })?;

        self.handle_response(response, "get staleness monitoring")
            .await
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
    use tasker_shared::types::api::orchestration::ManualCompletionData;

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
        // Task creation now returns TaskResponse (same shape as GET /v1/tasks/{uuid})
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
            "correlation_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
            "execution_status": "has_ready_steps",
            "recommended_action": "execute_ready_steps",
            "completion_percentage": 0.0,
            "health_status": "healthy",
            "steps": []
        });

        let response: TaskResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.task_uuid, "123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(response.name, "test_task");
        assert_eq!(response.status, "pending");
        assert_eq!(response.total_steps, 3);
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
            "correlation_id": "a1b2c3d4-e5f6-7890-1234-567890abcdef",
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
            "max_attempts": 3,
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
    async fn test_template_detail_deserialization() {
        // TAS-76: Updated from legacy handler info to template detail
        let json_response = json!({
            "name": "test_template",
            "namespace": "test",
            "version": "1.0.0",
            "description": "Test template",
            "configuration": null,
            "steps": [
                {
                    "name": "step1",
                    "description": null,
                    "default_retryable": true,
                    "default_max_attempts": 3
                },
                {
                    "name": "step2",
                    "description": "Second step",
                    "default_retryable": false,
                    "default_max_attempts": 1
                }
            ]
        });

        let response: TemplateDetail = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.name, "test_template");
        assert_eq!(response.namespace, "test");
        assert_eq!(response.version, "1.0.0");
        assert_eq!(response.steps.len(), 2);
        assert_eq!(response.steps[0].name, "step1");
        assert!(response.steps[0].default_retryable);
    }

    #[tokio::test]
    async fn test_template_list_response_deserialization() {
        // TAS-76: Updated from legacy namespace info to template list
        let json_response = json!({
            "namespaces": [
                {
                    "name": "test",
                    "description": "Test namespace",
                    "template_count": 5
                }
            ],
            "templates": [
                {
                    "name": "template1",
                    "namespace": "test",
                    "version": "1.0.0",
                    "description": "First template",
                    "step_count": 3
                }
            ],
            "total_count": 5
        });

        let response: TemplateListResponse = serde_json::from_value(json_response).unwrap();
        assert_eq!(response.namespaces.len(), 1);
        assert_eq!(response.namespaces[0].name, "test");
        assert_eq!(response.namespaces[0].template_count, 5);
        assert_eq!(response.templates.len(), 1);
        assert_eq!(response.total_count, 5);
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
        // TAS-76: Updated to use typed DetailedHealthChecks struct
        let json_response = json!({
            "status": "healthy",
            "timestamp": "2023-12-01T12:00:00Z",
            "checks": {
                "web_database": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 5
                },
                "orchestration_database": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 3
                },
                "circuit_breaker": {
                    "status": "healthy",
                    "message": "closed",
                    "duration_ms": 0
                },
                "orchestration_system": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 1
                },
                "command_processor": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 1
                },
                "pool_utilization": {
                    "status": "healthy",
                    "message": "10/20 connections in use",
                    "duration_ms": 0
                },
                "queue_depth": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 0
                },
                "channel_saturation": {
                    "status": "healthy",
                    "message": null,
                    "duration_ms": 0
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
        assert_eq!(response.checks.web_database.status, "healthy");
        assert_eq!(response.checks.orchestration_database.status, "healthy");
        assert_eq!(response.checks.circuit_breaker.status, "healthy");
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
    fn test_step_manual_action_reset_for_retry_serialization() {
        let action = StepManualAction::ResetForRetry {
            reset_by: "admin".to_string(),
            reason: "Database connection restored".to_string(),
        };

        let serialized = serde_json::to_string(&action).unwrap();
        assert!(serialized.contains("action_type"));
        assert!(serialized.contains("reset_for_retry"));
        assert!(serialized.contains("reset_by"));
        assert!(serialized.contains("reason"));
    }

    #[test]
    fn test_step_manual_action_resolve_manually_serialization() {
        let action = StepManualAction::ResolveManually {
            resolved_by: "admin".to_string(),
            reason: "Non-critical step, bypassing".to_string(),
        };

        let serialized = serde_json::to_string(&action).unwrap();
        assert!(serialized.contains("action_type"));
        assert!(serialized.contains("resolve_manually"));
        assert!(serialized.contains("resolved_by"));
        assert!(serialized.contains("reason"));
    }

    #[test]
    fn test_step_manual_action_complete_manually_serialization() {
        let completion_data = ManualCompletionData {
            result: json!({"validated": true, "score": 95}),
            metadata: Some(json!({"manually_verified": true})),
        };

        let action = StepManualAction::CompleteManually {
            completion_data,
            reason: "Manual verification completed".to_string(),
            completed_by: "admin".to_string(),
        };

        let serialized = serde_json::to_string(&action).unwrap();
        assert!(serialized.contains("action_type"));
        assert!(serialized.contains("complete_manually"));
        assert!(serialized.contains("completion_data"));
        assert!(serialized.contains("completed_by"));
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

    // ===================================================================================
    // DLQ API TESTS (TAS-49)
    // ===================================================================================

    #[test]
    fn test_dlq_list_params_default() {
        use tasker_shared::models::orchestration::DlqListParams;

        let params = DlqListParams::default();
        assert_eq!(params.limit, 50);
        assert_eq!(params.offset, 0);
        assert!(params.resolution_status.is_none());
    }

    #[test]
    fn test_dlq_investigation_update_creation() {
        use tasker_shared::models::orchestration::{DlqInvestigationUpdate, DlqResolutionStatus};

        let update = DlqInvestigationUpdate {
            resolution_status: Some(DlqResolutionStatus::ManuallyResolved),
            resolution_notes: Some("Fixed by recreating upstream dependency".to_string()),
            resolved_by: Some("operator@example.com".to_string()),
            metadata: None,
        };

        assert!(update.resolution_status.is_some());
        assert!(update.resolution_notes.is_some());
        assert!(update.resolved_by.is_some());
    }

    #[tokio::test]
    async fn test_dlq_entry_deserialization() {
        let json_response = json!({
            "dlq_entry_uuid": "550e8400-e29b-41d4-a716-446655440000",
            "task_uuid": "650e8400-e29b-41d4-a716-446655440000",
            "original_state": "BlockedByFailures",
            "dlq_reason": "MaxRetriesExceeded",
            "dlq_timestamp": "2023-12-01T12:00:00",
            "resolution_status": "Pending",
            "resolution_timestamp": null,
            "resolution_notes": null,
            "resolved_by": null,
            "task_snapshot": {},
            "metadata": null,
            "created_at": "2023-12-01T12:00:00",
            "updated_at": "2023-12-01T12:00:00"
        });

        let entry: tasker_shared::models::orchestration::DlqEntry =
            serde_json::from_value(json_response).unwrap();
        assert_eq!(
            entry.dlq_entry_uuid.to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            entry.task_uuid.to_string(),
            "650e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(entry.original_state, "BlockedByFailures");
    }

    #[tokio::test]
    async fn test_dlq_stats_deserialization() {
        let json_response = json!({
            "dlq_reason": "MaxRetriesExceeded",
            "total_entries": 42,
            "pending": 10,
            "manually_resolved": 25,
            "permanent_failures": 7,
            "cancelled": 3,
            "oldest_entry": "2023-12-01T10:00:00",
            "newest_entry": "2023-12-01T15:00:00",
            "avg_resolution_time_minutes": 45.5
        });

        let stats: tasker_shared::models::orchestration::DlqStats =
            serde_json::from_value(json_response).unwrap();
        assert_eq!(stats.total_entries, 42);
        assert_eq!(stats.pending, 10);
        assert_eq!(stats.manually_resolved, 25);
        assert_eq!(stats.permanent_failures, 7);
        assert_eq!(stats.cancelled, 3);
        assert!(stats.oldest_entry.is_some());
        assert!(stats.newest_entry.is_some());
        assert_eq!(stats.avg_resolution_time_minutes, Some(45.5));
    }
}
