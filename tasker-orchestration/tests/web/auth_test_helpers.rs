//! # Auth Test Helpers
//!
//! TAS-150: Utilities for auth-enabled E2E tests.
//! Provides JWT token generators, API key constants, and an auth-enabled test server.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tasker_orchestration::web::{create_test_app, state::AppState};
use tasker_shared::SystemContext;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::test_infrastructure::WebTestConfig;

// =============================================================================
// Test API Key Constants
// =============================================================================

/// API key with wildcard (*) permissions - full access
pub const TEST_API_KEY_FULL: &str = "test-api-key-full-access";

/// API key with read-only permissions
pub const TEST_API_KEY_READ_ONLY: &str = "test-api-key-read-only";

/// API key with tasks:* permissions only
pub const TEST_API_KEY_TASKS_ONLY: &str = "test-api-key-tasks-only";

/// API key with no permissions
pub const TEST_API_KEY_NO_PERMS: &str = "test-api-key-no-permissions";

/// Invalid API key (not configured)
pub const INVALID_API_KEY: &str = "invalid-api-key-not-configured";

/// API key header name
pub const API_KEY_HEADER: &str = "X-API-Key";

// =============================================================================
// Test JWT Constants
// =============================================================================

const TEST_JWT_ISSUER: &str = "tasker-core-test";
const TEST_JWT_AUDIENCE: &str = "tasker-api-test";

// =============================================================================
// JWT Claims for Tests
// =============================================================================

/// JWT claims structure for auth E2E tests
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthTestClaims {
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub permissions: Vec<String>,
}

// =============================================================================
// Token Generators
// =============================================================================

/// Get the path to the test private key fixture
fn test_private_key_path() -> std::path::PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("tests/fixtures/auth/jwt-private-key-test.pem")
}

/// Read the test private key from the fixture file
fn read_test_private_key() -> String {
    let key_path = test_private_key_path();
    std::fs::read_to_string(&key_path).unwrap_or_else(|e| {
        panic!(
            "Failed to read test private key at {}: {e}",
            key_path.display()
        )
    })
}

/// Generate a valid JWT with the given permissions.
///
/// Uses test issuer/audience, 60-minute expiry, and the fixture private key.
pub fn generate_jwt(permissions: &[&str]) -> String {
    let private_key = read_test_private_key();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;

    let claims = AuthTestClaims {
        sub: "test-user".to_string(),
        iss: TEST_JWT_ISSUER.to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: now + 3600, // 60 minutes
        iat: now,
        permissions: permissions.iter().map(|s| s.to_string()).collect(),
    };

    let header = Header::new(Algorithm::RS256);
    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .expect("Failed to create encoding key from test private key");
    encode(&header, &claims, &encoding_key).expect("Failed to encode JWT")
}

/// Generate an expired JWT with the given permissions.
///
/// Token expired 1 hour ago.
pub fn generate_expired_jwt(permissions: &[&str]) -> String {
    let private_key = read_test_private_key();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;

    let claims = AuthTestClaims {
        sub: "test-user-expired".to_string(),
        iss: TEST_JWT_ISSUER.to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: now - 3600, // Expired 1 hour ago
        iat: now - 7200, // Issued 2 hours ago
        permissions: permissions.iter().map(|s| s.to_string()).collect(),
    };

    let header = Header::new(Algorithm::RS256);
    let encoding_key =
        EncodingKey::from_rsa_pem(private_key.as_bytes()).expect("Failed to create encoding key");
    encode(&header, &claims, &encoding_key).expect("Failed to encode expired JWT")
}

/// Generate a JWT with the wrong issuer.
pub fn generate_jwt_wrong_issuer(permissions: &[&str]) -> String {
    let private_key = read_test_private_key();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;

    let claims = AuthTestClaims {
        sub: "test-user-wrong-iss".to_string(),
        iss: "wrong-issuer".to_string(),
        aud: TEST_JWT_AUDIENCE.to_string(),
        exp: now + 3600,
        iat: now,
        permissions: permissions.iter().map(|s| s.to_string()).collect(),
    };

    let header = Header::new(Algorithm::RS256);
    let encoding_key =
        EncodingKey::from_rsa_pem(private_key.as_bytes()).expect("Failed to create encoding key");
    encode(&header, &claims, &encoding_key).expect("Failed to encode JWT")
}

/// Generate a JWT with the wrong audience.
pub fn generate_jwt_wrong_audience(permissions: &[&str]) -> String {
    let private_key = read_test_private_key();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_secs() as i64;

    let claims = AuthTestClaims {
        sub: "test-user-wrong-aud".to_string(),
        iss: TEST_JWT_ISSUER.to_string(),
        aud: "wrong-audience".to_string(),
        exp: now + 3600,
        iat: now,
        permissions: permissions.iter().map(|s| s.to_string()).collect(),
    };

    let header = Header::new(Algorithm::RS256);
    let encoding_key =
        EncodingKey::from_rsa_pem(private_key.as_bytes()).expect("Failed to create encoding key");
    encode(&header, &claims, &encoding_key).expect("Failed to encode JWT")
}

// =============================================================================
// Auth-Enabled Test Server
// =============================================================================

/// Test server configured with auth enabled via auth-test.toml.
///
/// Sets TASKER_CONFIG_PATH and TASKER_JWT_PUBLIC_KEY_PATH before creating the
/// system context, then creates the Axum app with auth middleware active.
#[derive(Debug)]
pub struct AuthTestServer {
    pub config: WebTestConfig,
    pub handle: JoinHandle<()>,
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

impl AuthTestServer {
    /// Start an auth-enabled test server with dynamic port allocation.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let port = super::test_infrastructure::find_available_port().await?;

        let config = WebTestConfig {
            port,
            base_url: format!("http://localhost:{port}"),
            bind_address: format!("127.0.0.1:{port}"),
            ..Default::default()
        };

        // Set environment for auth-enabled test
        std::env::set_var("TASKER_ENV", "test");

        // Resolve paths relative to workspace root
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
        let workspace_root = std::path::Path::new(&manifest_dir)
            .parent()
            .unwrap_or(std::path::Path::new("."));

        let config_path = workspace_root.join("config/tasker/auth-test.toml");
        let public_key_path = workspace_root.join("tests/fixtures/auth/jwt-public-key-test.pem");

        std::env::set_var("TASKER_CONFIG_PATH", config_path.to_str().unwrap());
        std::env::set_var(
            "TASKER_JWT_PUBLIC_KEY_PATH",
            public_key_path.to_str().unwrap(),
        );
        std::env::set_var("TASKER_WEB_BIND_ADDRESS", &config.bind_address);

        // Create system context with auth config
        let system_context = Arc::new(
            SystemContext::new_for_orchestration()
                .await
                .map_err(|e| format!("Failed to create system context: {e}"))?,
        );

        let orchestration_core =
            tasker_orchestration::orchestration::core::OrchestrationCore::new(system_context)
                .await
                .map_err(|e| format!("Failed to create orchestration core: {e}"))?;

        let app_state = AppState::from_orchestration_core(Arc::new(orchestration_core))
            .await
            .map_err(|e| format!("Failed to create app state: {e}"))?;

        let app = create_test_app(app_state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let listener = TcpListener::bind(&config.bind_address).await?;

        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("Auth test server failed to start");
        });

        // Allow server to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(AuthTestServer {
            config,
            handle,
            shutdown_tx,
        })
    }

    /// Shutdown the auth test server.
    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(5), self.handle).await??;
        Ok(())
    }
}

// =============================================================================
// Extended Web Test Client with API Key Support
// =============================================================================

/// HTTP client for auth E2E tests supporting both JWT and API key auth.
#[derive(Debug)]
pub struct AuthWebTestClient {
    client: Client,
    base_url: String,
    jwt_token: Option<String>,
    api_key: Option<String>,
}

impl AuthWebTestClient {
    /// Create a new auth test client for the given server.
    pub fn for_server(server: &AuthTestServer) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: server.config.base_url.clone(),
            jwt_token: None,
            api_key: None,
        }
    }

    /// Set JWT bearer token for subsequent requests.
    pub fn with_jwt(&mut self, token: &str) -> &mut Self {
        self.jwt_token = Some(token.to_string());
        self.api_key = None;
        self
    }

    /// Set API key for subsequent requests.
    pub fn with_api_key(&mut self, key: &str) -> &mut Self {
        self.api_key = Some(key.to_string());
        self.jwt_token = None;
        self
    }

    /// Clear all credentials.
    pub fn without_auth(&mut self) -> &mut Self {
        self.jwt_token = None;
        self.api_key = None;
        self
    }

    /// Send a GET request.
    pub async fn get(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.get(&url);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = &self.api_key {
            request = request.header(API_KEY_HEADER, key.as_str());
        }

        request.send().await
    }

    /// Send a POST request with JSON body.
    pub async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(body);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = &self.api_key {
            request = request.header(API_KEY_HEADER, key.as_str());
        }

        request.send().await
    }

    /// Send a DELETE request.
    pub async fn delete(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.delete(&url);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = &self.api_key {
            request = request.header(API_KEY_HEADER, key.as_str());
        }

        request.send().await
    }

    /// Send a PATCH request with JSON body.
    pub async fn patch_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.patch(&url).json(body);

        if let Some(token) = &self.jwt_token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = &self.api_key {
            request = request.header(API_KEY_HEADER, key.as_str());
        }

        request.send().await
    }
}
