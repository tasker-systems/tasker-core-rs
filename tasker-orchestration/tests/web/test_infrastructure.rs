//! # Web API Test Infrastructure
//!
//! Common testing utilities and infrastructure for web API integration tests.

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tasker_orchestration::web::{create_test_app, state::AppState};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::SystemContext;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// Test configuration for web API integration tests
#[derive(Debug, Clone)]
pub struct WebTestConfig {
    pub base_url: String,
    pub bind_address: String,
    pub port: u16,
    pub use_tls: bool,
}

impl Default for WebTestConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            use_tls: false,
        }
    }
}

/// Test client wrapper with authentication and request utilities
#[derive(Debug)]
pub struct WebTestClient {
    client: Client,
    config: WebTestConfig,
    jwt_token: Option<String>,
}

/// Test server instance that manages a running web server for tests
#[derive(Debug)]
pub struct TestServer {
    pub config: WebTestConfig,
    pub handle: JoinHandle<()>,
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl TestServer {
    /// Start a test server with dynamic port allocation
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Find an available port
        let port = find_available_port().await?;

        // Create test configuration
        let config = WebTestConfig {
            port,
            base_url: format!("http://localhost:{port}"),
            bind_address: format!("127.0.0.1:{port}"),
            ..Default::default()
        };

        // Set up test environment
        std::env::set_var("TASKER_ENV", "test");

        // Create system context for orchestration
        let system_context = Arc::new(
            SystemContext::new_for_orchestration()
                .await
                .map_err(|e| format!("Failed to create system context: {e}"))?,
        );

        // Create orchestration core (required for AppState)
        let orchestration_core =
            tasker_orchestration::orchestration::core::OrchestrationCore::new(system_context)
                .await
                .map_err(|e| format!("Failed to create orchestration core: {e}"))?;

        // Create app state from orchestration core
        let app_state = AppState::from_orchestration_core(Arc::new(orchestration_core))
            .await
            .map_err(|e| format!("Failed to create app state: {e}"))?;

        // Create the Axum app
        let app = create_test_app(app_state);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Bind to the selected port
        let listener = TcpListener::bind(&config.bind_address).await?;

        // Spawn the server
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("Server failed to start");
        });

        // Give the server a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(TestServer {
            config,
            handle,
            shutdown_tx,
        })
    }

    /// Get the base URL for making requests to this test server
    // pub fn base_url(&self) -> &str {
    //     &self.config.base_url
    // }
    /// Shutdown the test server
    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());

        // Wait for server to shutdown with timeout
        tokio::time::timeout(Duration::from_secs(5), self.handle).await??;

        Ok(())
    }
}

impl WebTestClient {
    /// Create a new test client with default configuration
    pub fn new(config: WebTestConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true) // For self-signed certificates in tests
            .build()?;

        Ok(Self {
            client,
            config,
            jwt_token: None,
        })
    }

    /// Create a new test client connected to a running test server
    pub fn for_server(test_server: &TestServer) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new(test_server.config.clone())
    }

    /// Set JWT token for authenticated requests
    pub fn with_jwt_token(&mut self, token: String) {
        self.jwt_token = Some(token);
    }

    /// Make an unauthenticated GET request
    pub async fn get(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.config.base_url, path);
        self.client.get(&url).send().await
    }

    /// Make an authenticated GET request
    pub async fn get_authenticated(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.config.base_url, path);
        let mut request = self.client.get(&url);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }

        request.send().await
    }

    /// Make an authenticated POST request with JSON body
    pub async fn post_json_authenticated(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.config.base_url, path);
        let mut request = self.client.post(&url).json(body);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }

        request.send().await
    }

    /// Make an authenticated PATCH request with JSON body
    pub async fn patch_authenticated(
        &self,
        path: &str,
        body: Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.config.base_url, path);
        let mut request = self.client.patch(&url).json(&body);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }

        request.send().await
    }

    /// Get the base URL for this test client
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }
}

/// Find an available port for testing
pub async fn find_available_port() -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Create test data for API requests using proper TaskRequest struct
pub fn create_test_task_request() -> TaskRequest {
    TaskRequest {
        namespace: "test_web_integration".to_string(),
        name: "test_task".to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::json!({
            "test_data": "integration_test",
            "test_id": uuid::Uuid::new_v4().to_string()
        }),
        initiator: "web_integration_test".to_string(),
        source_system: "test_suite".to_string(),
        reason: "Web API integration testing".to_string(),
        tags: vec![],
        bypass_steps: vec![],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        correlation_id: uuid::Uuid::now_v7(),
        parent_correlation_id: None,
    }
}

/// Create test data as JSON for legacy compatibility
pub fn create_test_task_request_json() -> Value {
    let task_request = create_test_task_request();
    serde_json::to_value(task_request).expect("Failed to serialize TaskRequest")
}

/// Assert that response contains expected JSON structure
pub async fn assert_json_response(
    response: reqwest::Response,
    expected_status: u16,
    expected_fields: &[&str],
) -> Result<Value, Box<dyn std::error::Error>> {
    assert_eq!(
        response.status().as_u16(),
        expected_status,
        "Unexpected status code"
    );

    let json: Value = response.json().await?;

    for field in expected_fields {
        assert!(
            json.get(field).is_some(),
            "Expected field '{}' not found in response: {}",
            field,
            serde_json::to_string_pretty(&json)?
        );
    }

    Ok(json)
}

/// Assert that response is an error with expected message pattern
pub async fn assert_error_response(
    response: reqwest::Response,
    expected_status: u16,
    expected_error_pattern: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    assert_eq!(
        response.status().as_u16(),
        expected_status,
        "Unexpected status code"
    );

    let json: Value = response.json().await?;

    if let Some(error_message) = json.get("error").and_then(|e| e.as_str()) {
        assert!(
            error_message.contains(expected_error_pattern),
            "Error message '{error_message}' does not contain expected pattern '{expected_error_pattern}'"
        );
    } else {
        panic!(
            "Expected error field not found in response: {}",
            serde_json::to_string_pretty(&json)?
        );
    }

    Ok(json)
}

/// Test helper to generate authentication headers
pub fn create_auth_headers(token: &str) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_find_available_port() {
        let port = find_available_port().await.unwrap();
        // Port is u16, so it's always >= 0 and <= 65535 by definition
        // Just verify it's a reasonable port number (not 0 which is reserved)
        assert!(port > 0, "Port should not be 0 (reserved)");
    }

    #[test]
    fn test_web_test_config_default() {
        let config = WebTestConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.bind_address, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(!config.use_tls);
    }

    #[test]
    fn test_create_test_task_request() {
        let request = create_test_task_request();
        assert_eq!(request.namespace, "test_web_integration");
        assert_eq!(request.name, "test_task");
        assert_eq!(request.version, "1.0.0");
        assert_eq!(request.priority, Some(5));
        assert_eq!(request.initiator, "web_integration_test");
        assert_eq!(request.source_system, "test_suite");
    }

    #[test]
    fn test_create_auth_headers() {
        let headers = create_auth_headers("test-token-123");
        assert_eq!(
            headers.get("Authorization").unwrap(),
            "Bearer test-token-123"
        );
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
    }
}
