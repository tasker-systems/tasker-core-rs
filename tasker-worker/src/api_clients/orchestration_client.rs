//! # Orchestration API Client
//!
//! HTTP client for communicating with the tasker-orchestration web API.
//! Used by workers to delegate task initialization and coordinate with the orchestration layer.

use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{
    errors::{TaskerError, TaskerResult},
    models::core::task_request::TaskRequest,
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
    pub auth_token: Option<String>,
    /// Enable circuit breaker protection
    pub circuit_breaker_enabled: bool,
}

impl Default for OrchestrationApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth_token: None,
            circuit_breaker_enabled: true,
        }
    }
}

/// Response from the orchestration API task creation endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreationResponse {
    pub task_uuid: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
    pub step_count: usize,
    pub step_mapping: HashMap<String, String>,
    pub handler_config_name: Option<String>,
}

/// Task status response from orchestration API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_uuid: String,
    pub name: String,
    pub namespace: String,
    pub version: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: Option<String>,
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

        // Configure authentication if provided
        if let Some(ref token) = config.auth_token {
            let mut default_headers = reqwest::header::HeaderMap::new();
            default_headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token).parse().map_err(|e| {
                    TaskerError::ConfigurationError(format!("Invalid auth token: {}", e))
                })?,
            );
            client_builder = client_builder.default_headers(default_headers);
        }

        let client = client_builder.build().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to create HTTP client: {}", e))
        })?;

        info!(
            base_url = %config.base_url,
            timeout_ms = config.timeout_ms,
            auth_enabled = config.auth_token.is_some(),
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

    /// Get task status via the orchestration API
    ///
    /// GET /v1/tasks/{task_uuid}
    pub async fn get_task_status(&self, task_uuid: Uuid) -> TaskerResult<TaskStatusResponse> {
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
            let task_status = response.json::<TaskStatusResponse>().await.map_err(|e| {
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
        assert!(config.auth_token.is_none());
        assert!(config.circuit_breaker_enabled);
    }

    #[test]
    fn test_orchestration_client_creation() {
        let config = OrchestrationApiConfig::default();
        let client = OrchestrationApiClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_orchestration_client_creation_invalid_url() {
        let config = OrchestrationApiConfig {
            base_url: "invalid-url".to_string(),
            ..Default::default()
        };
        let client = OrchestrationApiClient::new(config);
        assert!(client.is_err());
    }

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
}
