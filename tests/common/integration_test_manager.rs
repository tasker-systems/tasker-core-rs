//! # Docker Integration Test Manager
//!
//! Simplified integration test manager that assumes Docker Compose services are already running.
//! This provides a lightweight approach that validates service health and sets up API clients
//! for integration testing.
//!
//! ## Configuration Precedence
//!
//! Uses 2-tier precedence (highest to lowest):
//! 1. **Environment Variables** (highest priority)
//!    - `TASKER_TEST_ORCHESTRATION_URL` - Override orchestration service URL
//!    - `TASKER_TEST_WORKER_URL` - Override worker service URL
//!    - `TASKER_TEST_SKIP_HEALTH_CHECK` - Skip health checks
//!    - `TASKER_TEST_HEALTH_TIMEOUT` - Health check timeout in seconds
//!    - `TASKER_TEST_HEALTH_RETRY_INTERVAL` - Retry interval in seconds
//! 2. **Code Defaults** (lowest priority)
//!    - Orchestration: `http://localhost:8080`
//!    - Worker: `http://localhost:8081` (Rust worker default)
//!
//! ## Docker Port Mappings
//!
//! Workers bind internally to port 8081, but Docker maps them to different external ports:
//! - **Rust worker**: `8081:8081` (external 8081 → internal 8081)
//! - **Ruby worker**: `8082:8081` (external 8082 → internal 8081)
//!
//! ## Usage
//!
//! ```bash
//! # Start the test environment first
//! docker-compose -f docker/docker-compose.test.yml up --build -d
//!
//! # Run tests (defaults to Rust worker on 8081)
//! TASKER_ENV=test cargo test --test rust_worker_e2e_integration_tests
//!
//! # Override to test Ruby worker on external port 8082
//! TASKER_TEST_WORKER_URL=http://localhost:8082 cargo test --test rust_worker_e2e_integration_tests
//!
//! # The test config (config/tasker/environments/test/worker.toml) has:
//! #   bind_address = "0.0.0.0:8081" (internal container port)
//! # Docker Compose maps this to external ports via port mappings
//! ```

#![expect(
    dead_code,
    reason = "Test module for Docker integration test management"
)]

use anyhow::Result;
use serde_json::Value;
use std::env;
use std::time::Duration;
use tokio::time::sleep;

use tasker_client::{
    OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient, WorkerApiConfig,
};

/// Integration test manager for Docker Compose-based testing
pub struct IntegrationTestManager {
    pub orchestration_client: OrchestrationApiClient,
    pub worker_client: Option<WorkerApiClient>,
    pub orchestration_url: String,
    pub worker_url: Option<String>,
}

/// Configuration for service discovery and health checks
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    pub orchestration_url: String,
    pub worker_url: Option<String>,
    pub skip_health_check: bool,
    pub health_timeout_seconds: u64,
    pub health_retry_interval_seconds: u64,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        // 2-tier precedence: ENV VAR → Code Default
        // Configuration loading removed - tests should use environment variables to override defaults

        let orchestration_url = env::var("TASKER_TEST_ORCHESTRATION_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        let worker_url = env::var("TASKER_TEST_WORKER_URL")
            .ok()
            .or_else(|| Some("http://localhost:8081".to_string()));

        Self {
            orchestration_url,
            worker_url,

            skip_health_check: env::var("TASKER_TEST_SKIP_HEALTH_CHECK")
                .ok()
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),

            health_timeout_seconds: env::var("TASKER_TEST_HEALTH_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),

            health_retry_interval_seconds: env::var("TASKER_TEST_HEALTH_RETRY_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
        }
    }
}

impl IntegrationTestManager {
    /// Set up integration test environment with default configuration
    ///
    /// Expects:
    /// - Orchestration service at http://localhost:8080
    /// - Worker service at http://localhost:8081 (optional)
    ///
    /// This is the main entry point for most integration tests.
    pub async fn setup() -> Result<Self> {
        let config = IntegrationConfig::default();
        Self::setup_with_config(config).await
    }

    /// Set up integration test environment with orchestration only
    ///
    /// Use this for API-only tests that don't require a worker service.
    pub async fn setup_orchestration_only() -> Result<Self> {
        let config = IntegrationConfig {
            worker_url: None,
            ..Default::default()
        };
        Self::setup_with_config(config).await
    }

    /// Set up integration test environment with custom configuration
    pub async fn setup_with_config(config: IntegrationConfig) -> Result<Self> {
        println!("🚀 Setting up Docker Integration Test Manager");
        println!("   Orchestration URL: {}", config.orchestration_url);

        if let Some(ref worker_url) = config.worker_url {
            println!("   Worker URL: {}", worker_url);
        } else {
            println!("   Worker URL: None (orchestration only)");
        }

        // Validate services are running and healthy
        if !config.skip_health_check {
            Self::validate_orchestration_service(&config).await?;

            if let Some(ref worker_url) = config.worker_url {
                Self::validate_worker_service(worker_url, &config).await?;
            }
        } else {
            println!("⚠️  Health checks skipped (TASKER_TEST_SKIP_HEALTH_CHECK=true)");
        }

        // Create API clients
        let orchestration_client = Self::create_orchestration_client(&config.orchestration_url)?;

        let worker_client = match &config.worker_url {
            Some(worker_url) => Some(Self::create_worker_client(worker_url)?),
            None => None,
        };

        println!("✅ Docker Integration Test Manager ready!");

        Ok(Self {
            orchestration_client,
            worker_client,
            orchestration_url: config.orchestration_url,
            worker_url: config.worker_url,
        })
    }

    /// Validate orchestration service is healthy
    async fn validate_orchestration_service(config: &IntegrationConfig) -> Result<()> {
        println!("🔍 Validating orchestration service health...");

        let start_time = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(config.health_timeout_seconds);
        let retry_interval = Duration::from_secs(config.health_retry_interval_seconds);

        while start_time.elapsed() < timeout_duration {
            match Self::check_orchestration_health(&config.orchestration_url).await {
                Ok(()) => {
                    println!("✅ Orchestration service is healthy");
                    return Ok(());
                }
                Err(e) => {
                    println!("   ⏳ Orchestration health check failed, retrying: {}", e);
                    sleep(retry_interval).await;
                }
            }
        }

        Err(anyhow::anyhow!(
            "Orchestration service at {} is not healthy after {}s. Is Docker Compose running?\n\
            Start services with: docker-compose -f docker/docker-compose.test.yml up --build -d",
            config.orchestration_url,
            config.health_timeout_seconds
        ))
    }

    /// Validate worker service is healthy
    async fn validate_worker_service(worker_url: &str, config: &IntegrationConfig) -> Result<()> {
        println!("🔍 Validating worker service health...");

        let start_time = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(config.health_timeout_seconds);
        let retry_interval = Duration::from_secs(config.health_retry_interval_seconds);

        while start_time.elapsed() < timeout_duration {
            match Self::check_worker_health(worker_url).await {
                Ok(()) => {
                    println!("✅ Worker service is healthy");
                    return Ok(());
                }
                Err(e) => {
                    println!("   ⏳ Worker health check failed, retrying: {}", e);
                    sleep(retry_interval).await;
                }
            }
        }

        Err(anyhow::anyhow!(
            "Worker service at {} is not healthy after {}s. Is Docker Compose running?\n\
            Start services with: docker-compose -f docker/docker-compose.test.yml up --build -d",
            worker_url,
            config.health_timeout_seconds
        ))
    }

    /// Check orchestration service health endpoint
    async fn check_orchestration_health(orchestration_url: &str) -> Result<()> {
        let client = reqwest::Client::new();
        let health_url = format!("{}/health", orchestration_url);

        let response = client
            .get(&health_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to connect to orchestration health endpoint: {}", e)
            })?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Orchestration health check failed with status: {}",
                response.status()
            ));
        }

        // Try to parse the health response
        let health_data: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse orchestration health response: {}", e))?;

        // Check if status indicates healthy
        if let Some(status) = health_data.get("status").and_then(|s| s.as_str()) {
            if status != "healthy" {
                return Err(anyhow::anyhow!(
                    "Orchestration service reports unhealthy status: {}",
                    status
                ));
            }
        }

        Ok(())
    }

    /// Check worker service health endpoint
    async fn check_worker_health(worker_url: &str) -> Result<()> {
        let client = reqwest::Client::new();
        let health_url = format!("{}/health", worker_url);

        let response = client
            .get(&health_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to worker health endpoint: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Worker health check failed with status: {}",
                response.status()
            ));
        }

        // Try to parse the health response
        let health_data: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse worker health response: {}", e))?;

        // Check if status indicates healthy
        if let Some(status) = health_data.get("status").and_then(|s| s.as_str()) {
            if status != "healthy" {
                return Err(anyhow::anyhow!(
                    "Worker service reports unhealthy status: {}",
                    status
                ));
            }
        }

        Ok(())
    }

    /// Create orchestration API client
    fn create_orchestration_client(orchestration_url: &str) -> Result<OrchestrationApiClient> {
        let config = OrchestrationApiConfig {
            base_url: orchestration_url.to_string(),
            timeout_ms: 10000,
            max_retries: 3,
            auth: None,
        };

        OrchestrationApiClient::new(config)
            .map_err(|e| anyhow::anyhow!("Failed to create orchestration client: {}", e))
    }

    /// Create worker API client
    fn create_worker_client(worker_url: &str) -> Result<WorkerApiClient> {
        let config = WorkerApiConfig {
            base_url: worker_url.to_string(),
            timeout_ms: 1000,
            auth: None,
            max_retries: 3,
        };

        WorkerApiClient::new(config)
            .map_err(|e| anyhow::anyhow!("Failed to create worker client: {}", e))
    }

    /// Perform a comprehensive health check of all configured services
    pub async fn health_check(&self) -> Result<()> {
        println!("🏥 Performing comprehensive health check...");

        // Check orchestration service
        Self::check_orchestration_health(&self.orchestration_url).await?;
        println!("✅ Orchestration service healthy");

        // Check worker service if configured
        if let Some(ref worker_url) = self.worker_url {
            Self::check_worker_health(worker_url).await?;
            println!("✅ Worker service healthy");
        }

        println!("🎉 All services are healthy!");
        Ok(())
    }

    /// Display helpful diagnostic information
    pub fn display_info(&self) {
        println!("\n📊 Docker Integration Test Manager Info:");
        println!("   Orchestration URL: {}", self.orchestration_url);

        if let Some(ref worker_url) = self.worker_url {
            println!("   Worker URL: {}", worker_url);
        } else {
            println!("   Worker: Not configured");
        }

        println!("   Configuration:");
        println!(
            "     TASKER_TEST_ORCHESTRATION_URL: {}",
            env::var("TASKER_TEST_ORCHESTRATION_URL").unwrap_or_else(|_| "default".to_string())
        );
        println!(
            "     TASKER_TEST_WORKER_URL: {}",
            env::var("TASKER_TEST_WORKER_URL").unwrap_or_else(|_| "default".to_string())
        );
        println!(
            "     TASKER_TEST_SKIP_HEALTH_CHECK: {}",
            env::var("TASKER_TEST_SKIP_HEALTH_CHECK").unwrap_or_else(|_| "false".to_string())
        );
    }
}

/// Convenience alias for backward compatibility
pub type ApiOnlyManager = IntegrationTestManager;

/// Helper functions for creating common test scenarios
impl IntegrationTestManager {
    /// Quick setup for API-only integration tests
    pub async fn api_only() -> Result<Self> {
        Self::setup_orchestration_only().await
    }

    /// Quick setup for full integration tests with worker
    pub async fn full_integration() -> Result<Self> {
        Self::setup().await
    }

    /// Quick setup for Ruby worker integration tests
    ///
    /// Uses the ruby-worker on port 8082 by default (unless overridden by
    /// `TASKER_TEST_RUBY_WORKER_URL` environment variable).
    ///
    /// Environment variable:
    /// - `TASKER_TEST_RUBY_WORKER_URL` - Specific Ruby worker URL (CI uses this)
    /// - Default: `http://localhost:8082`
    ///
    /// Note: This intentionally does NOT fall back to `TASKER_TEST_WORKER_URL` because
    /// that variable is typically set to the Rust worker (8081) in CI environments.
    ///
    /// Docker port mappings:
    /// - Rust worker: `8081:8081` (external 8081 → internal 8081)
    /// - Ruby worker: `8082:8081` (external 8082 → internal 8081)
    pub async fn setup_ruby_worker() -> Result<Self> {
        // Use Ruby-specific URL if set, otherwise default to Ruby worker port
        let ruby_worker_url = std::env::var("TASKER_TEST_RUBY_WORKER_URL")
            .unwrap_or_else(|_| "http://localhost:8082".to_string());

        let config = IntegrationConfig {
            worker_url: Some(ruby_worker_url),
            ..Default::default()
        };

        Self::setup_with_config(config).await
    }

    /// Set up integration test environment specifically for Python worker tests
    ///
    /// This method explicitly connects to the Python worker (default port 8083).
    /// Use this for tests that require Python handler execution and event verification.
    ///
    /// Worker URL Priority:
    /// 1. `TASKER_TEST_PYTHON_WORKER_URL` environment variable
    /// 2. `TASKER_TEST_WORKER_URL` environment variable
    /// 3. Default: `http://localhost:8083`
    ///
    /// Docker port mappings:
    /// - Rust worker: `8081:8081` (external 8081 → internal 8081)
    /// - Ruby worker: `8082:8081` (external 8082 → internal 8081)
    /// - Python worker: `8083:8081` (external 8083 → internal 8081)
    pub async fn setup_python_worker() -> Result<Self> {
        // Use Python-specific URL if set, otherwise check generic worker URL, then default to Python port
        let python_worker_url = std::env::var("TASKER_TEST_PYTHON_WORKER_URL")
            .or_else(|_| std::env::var("TASKER_TEST_WORKER_URL"))
            .unwrap_or_else(|_| "http://localhost:8083".to_string());

        let config = IntegrationConfig {
            worker_url: Some(python_worker_url),
            ..Default::default()
        };

        Self::setup_with_config(config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run when Docker services are available
    async fn test_integration_manager_setup() -> Result<()> {
        let manager = IntegrationTestManager::setup().await?;

        // Perform basic health check
        manager.health_check().await?;

        // Test orchestration client
        let health = manager.orchestration_client.get_basic_health().await?;
        assert_eq!(health.status, "healthy");

        println!("✅ Integration manager test passed");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Only run when Docker services are available
    async fn test_orchestration_only_setup() -> Result<()> {
        let manager = IntegrationTestManager::setup_orchestration_only().await?;

        // Should have orchestration client but no worker client
        assert!(manager.worker_client.is_none());

        // Test orchestration client
        let health = manager.orchestration_client.get_basic_health().await?;
        assert_eq!(health.status, "healthy");

        println!("✅ Orchestration-only test passed");
        Ok(())
    }
}
