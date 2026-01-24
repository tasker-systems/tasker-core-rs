//! # Worker API Client
//!
//! HTTP client for communicating with tasker-worker web APIs.
//! Provides methods for worker health checks, metrics, templates, and configuration.

use reqwest::{Client, Url};
use std::time::Duration;
use tracing::{debug, error, info};

use tasker_shared::{
    config::web::WebAuthConfig,
    errors::{TaskerError, TaskerResult},
    types::api::{
        orchestration::WorkerConfigResponse,
        worker::{
            BasicHealthResponse, CacheOperationResponse, DetailedHealthResponse,
            TemplateListResponse, TemplateQueryParams, TemplateResponse,
            TemplateValidationResponse,
        },
    },
    types::base::CacheStats,
    types::web::{DomainEventStats, MetricsResponse},
};

/// Configuration for the worker API client
///
/// # Examples
///
/// ```rust
/// use tasker_client::WorkerApiConfig;
///
/// // Default configuration
/// let config = WorkerApiConfig::default();
/// assert_eq!(config.base_url, "http://localhost:8081");
///
/// // Custom configuration
/// let config = WorkerApiConfig {
///     base_url: "http://worker.internal:8081".to_string(),
///     timeout_ms: 60000,
///     max_retries: 5,
///     auth: None,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct WorkerApiConfig {
    /// Base URL for the worker API (e.g., "<http://worker:8081>")
    pub base_url: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// API authentication configuration (if required)
    pub auth: Option<WebAuthConfig>,
}

impl Default for WorkerApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8081".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        }
    }
}

/// HTTP client for worker API operations
///
/// Provides methods to interact with worker health checks, metrics, templates,
/// and configuration endpoints. Handles authentication and proper error reporting.
///
/// # Examples
///
/// ```rust
/// use tasker_client::{WorkerApiClient, WorkerApiConfig};
///
/// let config = WorkerApiConfig::default();
/// let client = WorkerApiClient::new(config).unwrap();
///
/// // Check worker base URL
/// assert_eq!(client.base_url(), "http://localhost:8081/");
/// ```
///
/// ```rust,ignore
/// use tasker_client::{WorkerApiClient, WorkerApiConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = WorkerApiConfig::default();
/// let client = WorkerApiClient::new(config)?;
///
/// // Health check
/// let health = client.health_check().await?;
/// println!("Worker status: {}", health.status);
///
/// // Get metrics
/// let metrics = client.get_worker_metrics().await?;
/// println!("Steps processed: {}", metrics.steps_processed);
/// # Ok(())
/// # }
/// ```
pub struct WorkerApiClient {
    client: Client,
    base_url: Url,
    config: WorkerApiConfig,
}

impl std::fmt::Debug for WorkerApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerApiClient")
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

impl WorkerApiClient {
    /// Create new worker API client with the given configuration
    pub fn new(config: WorkerApiConfig) -> TaskerResult<Self> {
        let base_url = Url::parse(&config.base_url).map_err(|e| {
            TaskerError::ConfigurationError(format!(
                "Invalid base URL '{}': {}",
                config.base_url, e
            ))
        })?;

        let timeout = Duration::from_millis(config.timeout_ms);
        let mut client_builder = Client::builder()
            .timeout(timeout)
            .user_agent(format!("tasker-client/{}", env!("CARGO_PKG_VERSION")));

        // Add authentication header if configured (following orchestration client pattern)
        if let Some(ref auth_config) = config.auth {
            if auth_config.enabled {
                let mut default_headers = reqwest::header::HeaderMap::new();

                if !auth_config.bearer_token.is_empty() {
                    // Use pre-existing Bearer token
                    default_headers.insert(
                        reqwest::header::AUTHORIZATION,
                        format!("Bearer {}", auth_config.bearer_token)
                            .parse()
                            .map_err(|e| {
                                TaskerError::ConfigurationError(format!(
                                    "Invalid bearer token: {}",
                                    e
                                ))
                            })?,
                    );
                } else if !auth_config.api_key.is_empty() {
                    // Use API key authentication
                    let header_name = if auth_config.api_key_header.is_empty() {
                        "X-API-Key"
                    } else {
                        &auth_config.api_key_header
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
                        auth_config.api_key.parse().map_err(|e| {
                            TaskerError::ConfigurationError(format!("Invalid API key: {}", e))
                        })?,
                    );
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
            "Created WorkerApiClient for base_url: {}, timeout: {}ms",
            base_url, config.timeout_ms
        );

        Ok(Self {
            client,
            base_url,
            config,
        })
    }

    // =========================================================================
    // HEALTH API METHODS
    // =========================================================================

    /// Basic health check
    ///
    /// GET /health
    pub async fn health_check(&self) -> TaskerResult<BasicHealthResponse> {
        let url = self
            .base_url
            .join("/health")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Performing worker service health check: {}", url);

        let response =
            self.client.get(url).send().await.map_err(|e| {
                TaskerError::WorkerError(format!("Health check request failed: {}", e))
            })?;

        self.handle_response(response, "health check").await
    }

    /// Kubernetes readiness probe
    ///
    /// GET /health/ready
    pub async fn readiness_probe(&self) -> TaskerResult<DetailedHealthResponse> {
        let url = self
            .base_url
            .join("/health/ready")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Checking worker readiness: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Readiness check failed: {}", e)))?;

        self.handle_response(response, "readiness probe").await
    }

    /// Kubernetes liveness probe
    ///
    /// GET /health/live
    pub async fn liveness_probe(&self) -> TaskerResult<BasicHealthResponse> {
        let url = self
            .base_url
            .join("/health/live")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Checking worker liveness: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Liveness check failed: {}", e)))?;

        self.handle_response(response, "liveness probe").await
    }

    /// Detailed health check with subsystem status
    ///
    /// GET /health/detailed
    pub async fn get_detailed_health(&self) -> TaskerResult<DetailedHealthResponse> {
        let url = self
            .base_url
            .join("/health/detailed")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting detailed worker health: {}", url);

        let response = self.client.get(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Detailed health check failed: {}", e))
        })?;

        self.handle_response(response, "detailed health").await
    }

    // =========================================================================
    // METRICS API METHODS
    // =========================================================================

    /// Get Prometheus-format metrics
    ///
    /// GET /metrics
    pub async fn get_prometheus_metrics(&self) -> TaskerResult<String> {
        let url = self
            .base_url
            .join("/metrics")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting Prometheus metrics: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Metrics request failed: {}", e)))?;

        if response.status().is_success() {
            let metrics_text = response.text().await.map_err(|e| {
                TaskerError::WorkerError(format!("Failed to read metrics response: {}", e))
            })?;
            debug!("Successfully retrieved Prometheus metrics");
            Ok(metrics_text)
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                "Prometheus metrics request failed: {} - {}",
                status, error_text
            );
            Err(TaskerError::WorkerError(format!(
                "Prometheus metrics request failed: {} - {}",
                status, error_text
            )))
        }
    }

    /// Get worker metrics in JSON format
    ///
    /// GET /metrics/worker
    pub async fn get_worker_metrics(&self) -> TaskerResult<MetricsResponse> {
        let url = self
            .base_url
            .join("/metrics/worker")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting worker metrics: {}", url);

        let response = self.client.get(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Worker metrics request failed: {}", e))
        })?;

        self.handle_response(response, "worker metrics").await
    }

    /// Get domain event statistics
    ///
    /// GET /metrics/events
    ///
    /// Returns statistics about domain event routing and delivery paths.
    /// Used by E2E tests to verify events were actually published.
    pub async fn get_domain_event_stats(&self) -> TaskerResult<DomainEventStats> {
        let url = self
            .base_url
            .join("/metrics/events")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting domain event stats: {}", url);

        let response = self.client.get(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Domain event stats request failed: {}", e))
        })?;

        self.handle_response(response, "domain event stats").await
    }

    // =========================================================================
    // TEMPLATE API METHODS
    // =========================================================================

    /// List supported templates and namespaces
    ///
    /// GET /templates
    pub async fn list_templates(
        &self,
        params: Option<&TemplateQueryParams>,
    ) -> TaskerResult<TemplateListResponse> {
        let mut url = self
            .base_url
            .join("/templates")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        // Add query parameters if provided
        if let Some(query_params) = params {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(namespace) = &query_params.namespace {
                query_pairs.append_pair("namespace", namespace);
            }
            if let Some(include_stats) = query_params.include_cache_stats {
                query_pairs.append_pair("include_cache_stats", &include_stats.to_string());
            }
            drop(query_pairs);
        }

        debug!("Listing templates: {}", url);

        let response = self.client.get(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("List templates request failed: {}", e))
        })?;

        self.handle_response(response, "list templates").await
    }

    /// Get a specific task template
    ///
    /// GET /templates/{namespace}/{name}/{version}
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<TemplateResponse> {
        let url = self
            .base_url
            .join(&format!("/templates/{}/{}/{}", namespace, name, version))
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting template: {}", url);

        let response =
            self.client.get(url).send().await.map_err(|e| {
                TaskerError::WorkerError(format!("Get template request failed: {}", e))
            })?;

        self.handle_response(response, "get template").await
    }

    /// Validate a template for worker execution
    ///
    /// POST /templates/{namespace}/{name}/{version}/validate
    pub async fn validate_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<TemplateValidationResponse> {
        let url = self
            .base_url
            .join(&format!(
                "/templates/{}/{}/{}/validate",
                namespace, name, version
            ))
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Validating template: {}", url);

        let response = self.client.post(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Validate template request failed: {}", e))
        })?;

        self.handle_response(response, "validate template").await
    }

    /// Refresh a specific template in cache
    ///
    /// POST /templates/{namespace}/{name}/{version}/refresh
    pub async fn refresh_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TaskerResult<CacheOperationResponse> {
        let url = self
            .base_url
            .join(&format!(
                "/templates/{}/{}/{}/refresh",
                namespace, name, version
            ))
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Refreshing template: {}", url);

        let response = self.client.post(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Refresh template request failed: {}", e))
        })?;

        self.handle_response(response, "refresh template").await
    }

    /// Clear template cache
    ///
    /// DELETE /templates/cache
    pub async fn clear_template_cache(&self) -> TaskerResult<CacheOperationResponse> {
        let url = self
            .base_url
            .join("/templates/cache")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Clearing template cache: {}", url);

        let response =
            self.client.delete(url).send().await.map_err(|e| {
                TaskerError::WorkerError(format!("Clear cache request failed: {}", e))
            })?;

        self.handle_response(response, "clear template cache").await
    }

    /// Get template cache statistics
    ///
    /// GET /templates/cache/stats
    pub async fn get_cache_stats(&self) -> TaskerResult<CacheStats> {
        let url = self
            .base_url
            .join("/templates/cache/stats")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting cache stats: {}", url);

        let response =
            self.client.get(url).send().await.map_err(|e| {
                TaskerError::WorkerError(format!("Cache stats request failed: {}", e))
            })?;

        self.handle_response(response, "cache stats").await
    }

    /// Perform template cache maintenance
    ///
    /// POST /templates/cache/maintain
    pub async fn maintain_template_cache(&self) -> TaskerResult<CacheOperationResponse> {
        let url = self
            .base_url
            .join("/templates/cache/maintain")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Maintaining template cache: {}", url);

        let response = self.client.post(url).send().await.map_err(|e| {
            TaskerError::WorkerError(format!("Cache maintain request failed: {}", e))
        })?;

        self.handle_response(response, "maintain template cache")
            .await
    }

    // =========================================================================
    // CONFIG API METHODS
    // =========================================================================

    /// Get worker configuration (secrets redacted)
    ///
    /// GET /config
    pub async fn get_config(&self) -> TaskerResult<WorkerConfigResponse> {
        let url = self
            .base_url
            .join("/config")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting worker config: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Config request failed: {}", e)))?;

        self.handle_response(response, "get config").await
    }

    // =========================================================================
    // UTILITY METHODS
    // =========================================================================

    /// Get the base URL of the worker API
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }

    /// Get the configured timeout in milliseconds
    #[must_use]
    pub fn timeout_ms(&self) -> u64 {
        self.config.timeout_ms
    }

    /// Handle HTTP response with proper error handling and deserialization
    async fn handle_response<T>(
        &self,
        response: reqwest::Response,
        operation: &str,
    ) -> TaskerResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let status = response.status();
        if status.is_success() {
            let result: T = response.json().await.map_err(|e| {
                TaskerError::WorkerError(format!("Failed to parse {} response: {}", operation, e))
            })?;

            debug!("{} request successful", operation);
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("{} request failed: {} - {}", operation, status, error_text);
            Err(TaskerError::WorkerError(format!(
                "{} request failed: {} - {}",
                operation, status, error_text
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_api_config_default() {
        let config = WorkerApiConfig::default();
        assert_eq!(config.base_url, "http://localhost:8081");
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_worker_api_client_creation() {
        let config = WorkerApiConfig::default();
        let client = WorkerApiClient::new(config).unwrap();
        assert_eq!(client.base_url(), "http://localhost:8081/");
        assert_eq!(client.timeout_ms(), 30000);
    }

    #[test]
    fn test_worker_api_config_with_auth() {
        let config = WorkerApiConfig {
            base_url: "http://worker:8081".to_string(),
            timeout_ms: 15000,
            max_retries: 5,
            auth: None,
        };

        let client = WorkerApiClient::new(config).unwrap();
        assert_eq!(client.base_url(), "http://worker:8081/");
        assert_eq!(client.timeout_ms(), 15000);
    }

    #[test]
    fn test_invalid_base_url() {
        let config = WorkerApiConfig {
            base_url: "invalid-url".to_string(),
            ..Default::default()
        };

        let result = WorkerApiClient::new(config);
        assert!(result.is_err());
    }
}
