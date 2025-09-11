//! # Docker Test Suite Manager
//!
//! Manages Docker services for the entire integration test suite, replacing the
//! per-test container startup pattern with a shared service approach.
//!
//! ## Architecture
//!
//! - **DockerTestSuiteManager**: Singleton that manages Docker service lifecycle
//! - **DockerTestClient**: Lightweight per-test client for interacting with shared services
//!
//! ## Usage Pattern
//!
//! ```rust
//! // In test suite setup (once)
//! let suite_manager = DockerTestSuiteManager::get_or_start().await?;
//!
//! // In individual tests (many)
//! let test_client = DockerTestClient::new("my_test_name").await?;
//! let task_uuid = test_client.create_task(request).await?;
//! let result = test_client.wait_for_completion(&task_uuid, 30).await?;
//! ```
//!
//! ## Benefits
//!
//! - **Fast test execution**: No container startup/shutdown per test
//! - **Resource efficient**: Single set of containers for all tests
//! - **Parallel testing**: Multiple tests can run concurrently
//! - **Production-like**: Same Docker setup used in deployment

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

use tasker_client::api_clients::orchestration_client::{
    OrchestrationApiClient, OrchestrationApiConfig,
};
use tasker_client::api_clients::worker_client::{WorkerApiClient, WorkerApiConfig};
use tasker_shared::models::core::task_request::TaskRequest;

/// Singleton manager for Docker services shared across all integration tests
///
/// This replaces the per-test Docker container pattern with a single set of
/// long-running containers that serve the entire test suite.
pub struct DockerTestSuiteManager {
    /// Docker Compose project name for the test suite
    pub compose_project: String,

    /// API client for orchestration service
    pub orchestration_client: OrchestrationApiClient,

    /// API client for worker service
    pub worker_client: WorkerApiClient,

    /// Service URLs for reference
    pub service_urls: HashMap<String, String>,

    /// Timestamp when services were started
    started_at: Instant,

    /// Tracks if services are healthy
    services_healthy: bool,
}

/// Lightweight client for individual tests to interact with shared Docker services
///
/// This replaces DockerIntegrationTestSetup for individual tests, providing
/// a simple interface without any service lifecycle management.
pub struct DockerTestClient {
    /// Unique test identifier for logging/tracing
    pub test_id: String,

    /// Reference to shared suite manager
    suite_manager: &'static DockerTestSuiteManager,
}

/// Results from individual test execution
#[derive(Debug, Clone)]
pub struct DockerTestResult {
    pub test_id: String,
    pub task_uuid: String,
    pub status: String,
    pub completion_percentage: f64,
    pub total_steps: u32,
    pub completed_steps: u32,
    pub failed_steps: u32,
    pub execution_time: Duration,
}

// Global state management for suite manager
static SUITE_MANAGER_INIT: Once = Once::new();
static mut SUITE_MANAGER: Option<Arc<DockerTestSuiteManager>> = None;
static SUITE_MANAGER_MUTEX: Mutex<()> = Mutex::new(());

impl DockerTestSuiteManager {
    /// Get existing suite manager or start new one
    ///
    /// This uses std::sync::Once to ensure Docker services are started only once
    /// per test suite run, regardless of how many tests call this method.
    pub async fn get_or_start() -> Result<&'static DockerTestSuiteManager> {
        let _guard = SUITE_MANAGER_MUTEX.lock().unwrap();

        SUITE_MANAGER_INIT.call_once(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let manager = rt.block_on(Self::start_new_suite()).unwrap();
            unsafe {
                SUITE_MANAGER = Some(Arc::new(manager));
            }
        });

        unsafe { Ok(SUITE_MANAGER.as_ref().unwrap().as_ref()) }
    }

    /// Start new Docker test suite services
    ///
    /// This is called only once per test suite run by get_or_start().
    async fn start_new_suite() -> Result<DockerTestSuiteManager> {
        let compose_project = format!(
            "tasker-integration-suite-{}",
            Uuid::new_v4().to_string()[..8].to_string()
        );

        info!(
            "🚀 Starting Docker test suite services: {}",
            compose_project
        );

        // Create API clients
        let orchestration_client = OrchestrationApiClient::new(OrchestrationApiConfig {
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        })
        .context("Failed to create OrchestrationApiClient")?;

        let worker_client = WorkerApiClient::new(WorkerApiConfig {
            base_url: "http://localhost:8081".to_string(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None,
        })
        .context("Failed to create WorkerApiClient")?;

        let mut service_urls = HashMap::new();
        service_urls.insert(
            "postgres".to_string(),
            "postgresql://tasker:tasker@localhost:5432/tasker_integration".to_string(),
        );
        service_urls.insert(
            "orchestration".to_string(),
            "http://localhost:8080".to_string(),
        );
        service_urls.insert("worker".to_string(), "http://localhost:8081".to_string());

        let mut manager = DockerTestSuiteManager {
            compose_project,
            orchestration_client,
            worker_client,
            service_urls,
            started_at: Instant::now(),
            services_healthy: false,
        };

        // Start Docker services
        manager.start_services().await?;

        info!("✅ Docker test suite ready - services will be shared across all tests");
        Ok(manager)
    }

    /// Start Docker services for the test suite
    async fn start_services(&mut self) -> Result<()> {
        info!(
            "📦 Starting shared Docker services: {}",
            self.compose_project
        );

        // Use docker-compose to start all services
        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                "docker/docker-compose.integration.yml",
                "-p",
                &self.compose_project,
                "up",
                "-d",
                "--build",
            ])
            .current_dir("..") // Go up from src/test_helpers to project root
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute docker-compose up command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Docker Compose failed to start services: {}",
                stderr
            ));
        }

        info!("🔍 Waiting for service health checks...");
        self.wait_for_service_health().await?;

        self.services_healthy = true;
        info!("✅ All shared Docker services are healthy and ready");

        Ok(())
    }

    /// Wait for all services to pass health checks
    async fn wait_for_service_health(&self) -> Result<()> {
        let max_wait = Duration::from_secs(90); // Longer timeout for suite startup
        let start_time = Instant::now();

        loop {
            if start_time.elapsed() > max_wait {
                return Err(anyhow::anyhow!(
                    "Services failed to become healthy within {} seconds",
                    max_wait.as_secs()
                ));
            }

            let postgres_healthy = self.check_postgres_health().await.unwrap_or(false);
            let orchestration_healthy = self.check_orchestration_health().await.unwrap_or(false);
            let worker_healthy = self.check_worker_health().await.unwrap_or(false);

            if postgres_healthy && orchestration_healthy && worker_healthy {
                info!("✅ All services healthy: postgres=✓ orchestration=✓ worker=✓");
                return Ok(());
            }

            info!(
                "⏳ Service health: postgres={} orchestration={} worker={}",
                if postgres_healthy { "✓" } else { "✗" },
                if orchestration_healthy { "✓" } else { "✗" },
                if worker_healthy { "✓" } else { "✗" }
            );

            sleep(Duration::from_secs(3)).await;
        }
    }

    /// Check PostgreSQL health
    async fn check_postgres_health(&self) -> Result<bool> {
        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                "docker/docker-compose.integration.yml",
                "-p",
                &self.compose_project,
                "exec",
                "-T",
                "postgres",
                "pg_isready",
                "-U",
                "tasker",
                "-d",
                "tasker_integration",
            ])
            .current_dir("..")
            .output();

        match output {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false),
        }
    }

    /// Check orchestration service health
    async fn check_orchestration_health(&self) -> Result<bool> {
        match self.orchestration_client.get_basic_health().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Check worker service health
    async fn check_worker_health(&self) -> Result<bool> {
        match self.worker_client.get_worker_status("health-check").await {
            Ok(_) => Ok(true),
            Err(e) => {
                // 404 means service is up but no workers - that's healthy
                if e.to_string().contains("404") || e.to_string().contains("not found") {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// Get service uptime
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Check if services are healthy
    pub fn are_services_healthy(&self) -> bool {
        self.services_healthy
    }

    /// Get service logs for debugging
    pub async fn get_service_logs(&self, service: &str) -> Result<String> {
        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                "docker/docker-compose.integration.yml",
                "-p",
                &self.compose_project,
                "logs",
                "--tail=100",
                service,
            ])
            .current_dir("..")
            .output()
            .context("Failed to get service logs")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!(
                "Failed to get logs for {}: {}",
                service,
                stderr
            ))
        }
    }

    /// Clean up Docker resources (called when test suite completes)
    pub async fn cleanup(&self) -> Result<()> {
        info!(
            "🧹 Cleaning up shared Docker test suite: {}",
            self.compose_project
        );

        let output = Command::new("docker-compose")
            .args(&[
                "-f",
                "docker/docker-compose.integration.yml",
                "-p",
                &self.compose_project,
                "down",
                "-v",
                "--remove-orphans",
            ])
            .current_dir("..")
            .output()
            .context("Failed to stop Docker services")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Docker cleanup had issues: {}", stderr);
        } else {
            info!("✅ Docker test suite cleaned up successfully");
        }

        Ok(())
    }
}

impl DockerTestClient {
    /// Create new test client connected to shared Docker services
    ///
    /// This is lightweight - no Docker service management, just a client interface.
    pub async fn new(test_name: &str) -> Result<Self> {
        let test_id = format!(
            "{}_{}",
            test_name,
            Uuid::new_v4().to_string()[..8].to_string()
        );

        // Ensure shared services are running
        let suite_manager = DockerTestSuiteManager::get_or_start().await?;

        info!(
            "🧪 Created test client: {} (using shared services)",
            test_id
        );

        Ok(Self {
            test_id,
            suite_manager,
        })
    }

    /// Create a task using shared orchestration service
    pub async fn create_task(&self, task_request: TaskRequest) -> Result<String> {
        info!(
            "[{}] 📋 Creating task: {}/{}",
            self.test_id, task_request.namespace, task_request.name
        );

        let response = self
            .suite_manager
            .orchestration_client
            .create_task(task_request)
            .await
            .context("Failed to create task via shared orchestration service")?;

        let task_uuid = response.task_uuid.to_string();
        info!("[{}] ✅ Task created: {}", self.test_id, task_uuid);
        Ok(task_uuid)
    }

    /// Wait for task completion
    pub async fn wait_for_completion(
        &self,
        task_uuid: &str,
        timeout_seconds: u64,
    ) -> Result<DockerTestResult> {
        info!(
            "[{}] ⏳ Waiting for task {} completion ({}s timeout)",
            self.test_id, task_uuid, timeout_seconds
        );

        let start_time = Instant::now();
        let timeout = Duration::from_secs(timeout_seconds);

        loop {
            if start_time.elapsed() > timeout {
                error!("[{}] ❌ Task {} timed out", self.test_id, task_uuid);
                return Err(anyhow::anyhow!(
                    "Task execution timed out after {} seconds",
                    timeout_seconds
                ));
            }

            let task_uuid_parsed = task_uuid
                .parse::<Uuid>()
                .context("Invalid task UUID format")?;

            match self
                .suite_manager
                .orchestration_client
                .get_task(task_uuid_parsed)
                .await
            {
                Ok(status_response) => {
                    let is_completed = status_response.completed_at.is_some()
                        || status_response.status == "completed"
                        || status_response.completion_percentage >= 100.0;

                    if is_completed {
                        info!("[{}] ✅ Task {} completed", self.test_id, task_uuid);

                        return Ok(DockerTestResult {
                            test_id: self.test_id.clone(),
                            task_uuid: task_uuid.to_string(),
                            status: status_response.status,
                            completion_percentage: status_response.completion_percentage,
                            total_steps: status_response.total_steps as u32,
                            completed_steps: status_response.completed_steps as u32,
                            failed_steps: status_response.failed_steps as u32,
                            execution_time: start_time.elapsed(),
                        });
                    }

                    // Log progress
                    if start_time.elapsed().as_secs() % 5 == 0 {
                        info!(
                            "[{}] ⏳ Task {} progress: {}% ({}/{})",
                            self.test_id,
                            task_uuid,
                            status_response.completion_percentage,
                            status_response.completed_steps,
                            status_response.total_steps
                        );
                    }
                }
                Err(e) => {
                    warn!("[{}] ⚠️ Error checking task status: {}", self.test_id, e);
                }
            }

            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Run complete integration test
    pub async fn run_integration_test(
        &self,
        task_request: TaskRequest,
        timeout_seconds: u64,
    ) -> Result<DockerTestResult> {
        info!(
            "[{}] 🧪 Running integration test: {}/{}:{}",
            self.test_id, task_request.namespace, task_request.name, task_request.version
        );

        let task_uuid = self.create_task(task_request).await?;
        let result = self
            .wait_for_completion(&task_uuid, timeout_seconds)
            .await?;

        info!(
            "[{}] ✅ Integration test completed: {:?}",
            self.test_id, result
        );
        Ok(result)
    }

    /// Get shared service logs for debugging
    pub async fn get_service_logs(&self, service: &str) -> Result<String> {
        self.suite_manager.get_service_logs(service).await
    }
}

// Utility functions for creating test data

/// Create test task request
pub fn create_test_task_request(
    namespace: &str,
    name: &str,
    version: &str,
    context: Value,
    test_description: &str,
) -> TaskRequest {
    use chrono::Utc;

    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        context,
        status: "PENDING".to_string(),
        initiator: format!("docker_suite_test_{}", Uuid::new_v4()),
        source_system: "docker_suite_testing".to_string(),
        reason: test_description.to_string(),
        complete: false,
        tags: vec!["docker_suite_test".to_string()],
        bypass_steps: vec![],
        requested_at: Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
    }
}

/// Create mathematical test context
pub fn create_mathematical_test_context(even_number: u128) -> Value {
    use serde_json::json;

    json!({
        "even_number": even_number,
        "test_run_id": format!("suite_test_{}", Uuid::new_v4())
    })
}

pub fn create_order_fulfillment_test_context(order_id: &str, customer_id: i64) -> Value {
    use serde_json::json;

    json!({
        "customer_info": {
            "id": customer_id,
            "name": "Test Customer",
            "email": "test@example.com"
        },
        "order_items": [
            {
                "product_id": 101,
                "quantity": 2,
                "price": 29.99
            },
            {
                "product_id": 102,
                "quantity": 1,
                "price": 49.99
            }
        ],
        "payment_info": {
            "method": "credit_card",
            "token": "tok_test_visa_4242"
        },
        "shipping_info": {
            "method": "standard",
            "address": {
                "street": "123 Test St",
                "city": "Test City",
                "state": "TS",
                "zip": "12345",
                "country": "US"
            }
        },
        "order_id": order_id,
        "test_run_id": format!("order_test_{}", Uuid::new_v4())
    })
}

/// Manual cleanup function for test suite teardown
///
/// Call this in test suite teardown to clean up shared Docker services.
/// Usually called via a test harness or manual cleanup.
pub async fn cleanup_shared_services() -> Result<()> {
    if let Some(manager) = unsafe { SUITE_MANAGER.as_ref() } {
        manager.cleanup().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_suite_manager_singleton() {
        // Multiple calls should return the same instance
        let manager1 = DockerTestSuiteManager::get_or_start().await.unwrap();
        let manager2 = DockerTestSuiteManager::get_or_start().await.unwrap();

        // Should be same instance (same compose project)
        assert_eq!(manager1.compose_project, manager2.compose_project);
    }

    #[tokio::test]
    async fn test_client_creation() {
        let client = DockerTestClient::new("test_client").await.unwrap();
        assert!(client.test_id.contains("test_client"));
    }
}
