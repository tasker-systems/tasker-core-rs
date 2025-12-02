//! # Worker API Client
//!
//! HTTP client for communicating with tasker-worker web APIs.
//! Provides methods for worker status monitoring, health checks, and management operations.

use reqwest::{Client, Url};
use std::time::Duration;
use tracing::{debug, error, info};

use tasker_shared::{
    config::web::WebAuthConfig,
    errors::{TaskerError, TaskerResult},
    types::api::worker::{WorkerHealthResponse, WorkerListResponse, WorkerStatusResponse},
};

/// Configuration for the worker API client
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

                if !auth_config.api_key.is_empty() {
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

    /// Get the status of a specific worker
    pub async fn get_worker_status(&self, worker_id: &str) -> TaskerResult<WorkerStatusResponse> {
        let url = self
            .base_url
            .join(&format!("/v1/workers/{}/status", worker_id))
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting worker status from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Request failed: {}", e)))?;

        let status = response.status();
        if status.is_success() {
            let worker_status: WorkerStatusResponse = response.json().await.map_err(|e| {
                TaskerError::ValidationError(format!("Failed to parse response: {}", e))
            })?;

            info!(
                "Retrieved worker status for {}: {:?}",
                worker_id, worker_status
            );
            Ok(worker_status)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Worker status request failed: {} - {}", status, error_text);
            Err(TaskerError::WorkerError(format!(
                "Worker status request failed: {} - {}",
                status, error_text
            )))
        }
    }

    /// List all workers, optionally filtered by namespace
    pub async fn list_workers(&self, namespace: Option<&str>) -> TaskerResult<WorkerListResponse> {
        let mut url = self
            .base_url
            .join("/v1/workers")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        // Add namespace filter if provided
        if let Some(ns) = namespace {
            url.query_pairs_mut().append_pair("namespace", ns);
        }

        debug!("Listing workers from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Request failed: {}", e)))?;

        let status = response.status();
        if status.is_success() {
            let worker_list: WorkerListResponse = response.json().await.map_err(|e| {
                TaskerError::ValidationError(format!("Failed to parse response: {}", e))
            })?;

            info!("Retrieved {} workers", worker_list.workers.len());
            Ok(worker_list)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Worker list request failed: {} - {}", status, error_text);
            Err(TaskerError::WorkerError(format!(
                "Worker list request failed: {} - {}",
                status, error_text
            )))
        }
    }

    /// Check the health of a specific worker
    pub async fn worker_health(&self, worker_id: &str) -> TaskerResult<WorkerHealthResponse> {
        let url = self
            .base_url
            .join(&format!("/v1/workers/{}/health", worker_id))
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Checking worker health from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Request failed: {}", e)))?;

        let status = response.status();
        if status.is_success() {
            let health_response: WorkerHealthResponse = response.json().await.map_err(|e| {
                TaskerError::ValidationError(format!("Failed to parse response: {}", e))
            })?;

            info!(
                "Retrieved worker health for {}: {:?}",
                worker_id, health_response
            );
            Ok(health_response)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("Worker health request failed: {} - {}", status, error_text);
            Err(TaskerError::WorkerError(format!(
                "Worker health request failed: {} - {}",
                status, error_text
            )))
        }
    }

    /// Check the overall health of the worker service
    pub async fn health_check(&self) -> TaskerResult<()> {
        let url = self
            .base_url
            .join("/health")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Performing worker service health check: {}", url);

        let response =
            self.client.get(url).send().await.map_err(|e| {
                TaskerError::WorkerError(format!("Health check request failed: {}", e))
            })?;

        if response.status().is_success() {
            info!("Worker service health check passed");
            Ok(())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!(
                "Worker service health check failed: {} - {}",
                status, error_text
            );
            Err(TaskerError::WorkerError(format!(
                "Worker service health check failed: {} - {}",
                status, error_text
            )))
        }
    }

    // =========================================================================
    // TAS-65: Domain Event Metrics Endpoints
    // =========================================================================

    /// Get domain event statistics from the worker
    ///
    /// Returns statistics about domain event routing and delivery paths.
    /// Used by E2E tests to verify events were actually published.
    ///
    /// # Returns
    ///
    /// `DomainEventStats` containing:
    /// - Router stats (durable_routed, fast_routed, broadcast_routed)
    /// - In-process bus stats (total_events_dispatched, handler counts)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Capture stats before running a task
    /// let stats_before = client.get_domain_event_stats().await?;
    ///
    /// // Run task that publishes events
    /// orchestration_client.create_task(request).await?;
    /// wait_for_completion().await;
    ///
    /// // Capture stats after task completes
    /// let stats_after = client.get_domain_event_stats().await?;
    ///
    /// // Verify events were published
    /// let durable_published = stats_after.router.durable_routed - stats_before.router.durable_routed;
    /// assert!(durable_published > 0, "Expected durable events");
    /// ```
    pub async fn get_domain_event_stats(
        &self,
    ) -> TaskerResult<tasker_shared::types::web::DomainEventStats> {
        let url = self
            .base_url
            .join("/metrics/events")
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL: {}", e)))?;

        debug!("Getting domain event stats from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Request failed: {}", e)))?;

        let status = response.status();
        if status.is_success() {
            let stats: tasker_shared::types::web::DomainEventStats =
                response.json().await.map_err(|e| {
                    TaskerError::ValidationError(format!("Failed to parse response: {}", e))
                })?;

            debug!(
                "Retrieved domain event stats: durable={}, fast={}, broadcast={}",
                stats.router.durable_routed,
                stats.router.fast_routed,
                stats.router.broadcast_routed
            );
            Ok(stats)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!(
                "Domain event stats request failed: {} - {}",
                status, error_text
            );
            Err(TaskerError::WorkerError(format!(
                "Domain event stats request failed: {} - {}",
                status, error_text
            )))
        }
    }

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

    /// Utility method to make generic API requests to the worker
    pub async fn make_request<T>(&self, method: reqwest::Method, path: &str) -> TaskerResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| TaskerError::WorkerError(format!("Invalid URL path '{}': {}", path, e)))?;

        debug!("Making {} request to: {}", method, url);

        let response = self
            .client
            .request(method.clone(), url)
            .send()
            .await
            .map_err(|e| TaskerError::WorkerError(format!("{} request failed: {}", method, e)))?;

        let status = response.status();
        if status.is_success() {
            let result: T = response.json().await.map_err(|e| {
                TaskerError::ValidationError(format!("Failed to parse response: {}", e))
            })?;

            debug!("{} request successful", method);
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            error!("{} request failed: {} - {}", method, status, error_text);
            Err(TaskerError::WorkerError(format!(
                "{} request failed: {} - {}",
                method, status, error_text
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
