// Test Helper Module - Integration Testing Infrastructure for Rust Workers
//
// Provides comprehensive test setup and utilities that mirror the Ruby SharedTestLoop
// functionality but adapted for native Rust integration testing.

use anyhow::{Context, Result};
use dotenvy::dotenv;
use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use chrono::Utc;
use tasker_client::api_clients::orchestration_client::{
    OrchestrationApiClient, OrchestrationApiConfig,
};
use tasker_orchestration::orchestration::bootstrap::{
    BootstrapConfig as OrchestrationBootstrapConfig, OrchestrationBootstrap,
    OrchestrationSystemHandle,
};
use tasker_shared::config::ConfigManager;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_worker::bootstrap::{WorkerBootstrap, WorkerBootstrapConfig, WorkerSystemHandle};

/// Shared test setup for Rust integration tests
/// Mirrors the Ruby SharedTestLoop functionality for native Rust testing
pub struct SharedTestSetup {
    pub orchestration_handle: Option<Arc<RwLock<OrchestrationSystemHandle>>>,
    pub worker_handle: Option<Arc<RwLock<WorkerSystemHandle>>>,
    pub orchestration_client: Arc<OrchestrationApiClient>,
    pub test_id: String,
}

impl SharedTestSetup {
    /// Create a new test setup instance
    pub fn new() -> Result<Self> {
        dotenv().ok();

        // For integration tests, use default orchestration API config
        // This avoids the complex TaskerConfig loading which requires all components
        let orchestration_api_config = OrchestrationApiConfig {
            base_url: "http://localhost:8080".to_string(), // Default orchestration web server
            timeout_ms: 30000,
            max_retries: 3,
            auth: None, // No auth for testing
        };

        let orchestration_client = Arc::new(
            OrchestrationApiClient::new(orchestration_api_config)
                .context("Failed to create OrchestrationApiClient")?,
        );

        Ok(Self {
            orchestration_handle: None,
            worker_handle: None,
            orchestration_client,
            test_id: Uuid::new_v4().to_string(),
        })
    }

    pub fn set_worker_handle(&mut self, worker_handle: WorkerSystemHandle) {
        self.worker_handle = Some(Arc::new(RwLock::new(worker_handle)));
    }

    /// Initialize both orchestration and worker systems with web APIs enabled
    /// This is the key difference - both systems need web APIs for integration testing
    pub async fn initialize_systems(&mut self, namespaces: Vec<&str>) -> Result<()> {
        info!(
            "🔧  Initializing orchestration and worker systems for test {}",
            self.test_id
        );
        info!("Namespaces: {:?}", namespaces);

        // Check if systems are already initialized
        if self.orchestration_handle.is_none() {
            self.initialize_orchestration(namespaces.clone()).await?;
        }

        if self.worker_handle.is_none() {
            self.initialize_worker(namespaces).await?;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("✅ Both systems initialized and ready");

        Ok(())
    }

    /// Create and initialize a task using the orchestration web API
    /// Equivalent to Ruby's TaskerCore.initialize_task_embedded
    pub async fn create_task(&self, task_request: TaskRequest) -> Result<String> {
        info!(
            "📋 Creating task for namespace '{}', name '{}' via orchestration web API",
            task_request.namespace, task_request.name
        );

        // Use the real orchestration client to hit the /v1/tasks endpoint
        let response = self
            .orchestration_client
            .create_task(task_request)
            .await
            .context("Failed to create task via orchestration API")?;

        let task_uuid = response.task_uuid.to_string();
        info!("✅ Task created successfully via web API: {}", task_uuid);
        Ok(task_uuid)
    }

    /// Wait for task completion with timeout
    /// Simplified version for demonstration
    pub async fn wait_for_completion(
        &self,
        task_uuid: &str,
        timeout_seconds: u64,
    ) -> Result<TaskExecutionSummary> {
        info!(
            "⏳ Waiting for task {} to complete (timeout: {}s)",
            task_uuid, timeout_seconds
        );

        let start_time = Instant::now();
        let timeout = Duration::from_secs(timeout_seconds);

        loop {
            if start_time.elapsed() > timeout {
                error!("❌ Task {} timed out after {}s", task_uuid, timeout_seconds);
                return Err(anyhow::anyhow!(
                    "Task execution timed out after {} seconds",
                    timeout_seconds
                ));
            }

            // Check real task status via orchestration web API
            let task_uuid_parsed = task_uuid
                .parse::<Uuid>()
                .context("Invalid task UUID format")?;

            match self.orchestration_client.get_task(task_uuid_parsed).await {
                Ok(status_response) => {
                    // Check if task is completed based on completed_at field or status/completion_percentage
                    let is_completed = status_response.completed_at.is_some()
                        || status_response.status == "completed"
                        || status_response.completion_percentage >= 100.0;

                    if is_completed {
                        info!("✅ Task {} completed successfully", task_uuid);
                        return Ok(TaskExecutionSummary {
                            task_uuid: task_uuid.to_string(),
                            status: status_response.status,
                            completion_percentage: status_response.completion_percentage,
                            total_steps: status_response.total_steps as u32,
                            completed_steps: status_response.completed_steps as u32,
                            failed_steps: status_response.failed_steps as u32,
                            execution_time: start_time.elapsed(),
                        });
                    } else {
                        warn!(
                            "⚠️ Task {} not completed yet: {}% (status: {}, completed: {}/{}, failed: {})",
                            task_uuid,
                            status_response.completion_percentage,
                            status_response.status,
                            status_response.completed_steps,
                            status_response.total_steps,
                            status_response.failed_steps
                        );
                    }
                }
                Err(e) => {
                    warn!("⚠️ Error checking task status via API: {}", e);
                }
            }

            // Sleep before next check (equivalent to Ruby's sleep 1)
            thread::sleep(Duration::from_millis(1000));
        }
    }

    /// Run complete integration test workflow
    /// Equivalent to Ruby's complete workflow execution pattern
    pub async fn run_integration_test(
        &mut self,
        task_request: TaskRequest,
        namespace: &str,
        timeout_seconds: u64,
    ) -> Result<TaskExecutionSummary> {
        info!(
            "🧪 Running integration test for {}/{}:{}",
            task_request.namespace, task_request.name, task_request.version
        );

        // Initialize both orchestration and worker systems with web APIs enabled
        self.initialize_systems(vec![namespace]).await?;

        // Create and execute the task via web API
        let task_uuid = self.create_task(task_request).await?;

        // Wait for completion
        let summary = self
            .wait_for_completion(&task_uuid, timeout_seconds)
            .await?;

        info!("✅ Integration test completed: {}", summary);
        Ok(summary)
    }

    /// Initialize orchestration system only (separate from initialize_systems)
    pub async fn initialize_orchestration(&mut self, namespaces: Vec<&str>) -> Result<()> {
        info!(
            "🔧  Initializing orchestration system only for test {}",
            self.test_id
        );
        info!("Namespaces: {:?}", namespaces);

        // Check if orchestration is already initialized
        if self.orchestration_handle.is_some() {
            warn!(
                "Orchestration already initialized for test {}",
                self.test_id
            );
            return Ok(());
        }

        let config_manager = ConfigManager::load_from_env("test").unwrap();

        let orchestration_bootstrap_config = OrchestrationBootstrapConfig::from_config_manager(
            &config_manager,
            namespaces
                .clone()
                .into_iter()
                .map(|ns| ns.to_string())
                .collect(),
        );

        let orchestration_handle: OrchestrationSystemHandle =
            OrchestrationBootstrap::bootstrap(orchestration_bootstrap_config).await?;

        self.orchestration_handle = Some(Arc::new(RwLock::new(orchestration_handle)));

        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("✅  Orchestration system initialized");
        Ok(())
    }

    /// Initialize workers for the orchestration system
    /// Equivalent to Ruby's worker initialization pattern
    pub async fn initialize_worker(&mut self, namespaces: Vec<&str>) -> Result<()> {
        info!(
            "🔧  Worker initialization for namespaces '{:?}'",
            namespaces
        );

        // Check if workers are already initialized
        if self.worker_handle.is_some() {
            warn!("Workers already initialized for test {}", self.test_id);
            return Ok(());
        }

        let worker_id = Uuid::new_v4();
        let config_manager = ConfigManager::load_from_env("test").unwrap();

        let mut worker_bootstrap_config = WorkerBootstrapConfig::from_config_manager(
            &config_manager,
            worker_id.to_string(),
            namespaces
                .clone()
                .into_iter()
                .map(|ns| ns.to_string())
                .collect(),
        );

        worker_bootstrap_config.enable_web_api = false;

        let worker_handle: WorkerSystemHandle =
            WorkerBootstrap::bootstrap(worker_bootstrap_config).await?;

        self.worker_handle = Some(Arc::new(RwLock::new(worker_handle)));

        tokio::time::sleep(Duration::from_millis(100)).await;
        info!("✅  Worker initialized");
        Ok(())
    }

    /// Clean up resources
    /// Equivalent to Ruby's SharedTestLoop cleanup
    pub async fn cleanup(&mut self) -> Result<()> {
        info!("🧹 Cleaning up test resources for test {}", self.test_id);

        // Stop worker system first
        if let Some(worker_handle_arc) = self.worker_handle.take() {
            let mut worker_handle = worker_handle_arc.write().await;
            worker_handle
                .stop()
                .context("Failed to stop worker system")?;
            info!("✅ Worker system stopped successfully");
        }

        // Then stop orchestration system
        if let Some(orchestration_handle_arc) = self.orchestration_handle.take() {
            let mut orchestration_handle = orchestration_handle_arc.write().await;
            orchestration_handle
                .stop()
                .await
                .context("Failed to stop orchestration")?;
            info!("✅ Orchestration stopped successfully");
        }

        info!("✅ Cleanup completed successfully");
        Ok(())
    }
}

/// Summary of task execution results
/// Equivalent to Ruby's task execution context information
#[derive(Debug)]
pub struct TaskExecutionSummary {
    pub task_uuid: String,
    pub status: String,
    pub completion_percentage: f64,
    pub total_steps: u32,
    pub completed_steps: u32,
    pub failed_steps: u32,
    pub execution_time: Duration,
}

impl std::fmt::Display for TaskExecutionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TaskExecutionSummary {{ uuid: {}, status: {}, completion: {}%, steps: {}/{}, failed: {}, time: {:?} }}",
            self.task_uuid,
            self.status,
            self.completion_percentage,
            self.completed_steps,
            self.total_steps,
            self.failed_steps,
            self.execution_time
        )
    }
}

/// Create a test task request with standard test data
/// Equivalent to Ruby's TaskRequest creation pattern
pub fn create_test_task_request(
    namespace: &str,
    name: &str,
    version: &str,
    context: Value,
    test_description: &str,
) -> TaskRequest {
    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        context,
        status: "PENDING".to_string(),
        initiator: format!("rust_integration_test_{}", Uuid::new_v4()),
        source_system: "rust_integration_testing".to_string(),
        reason: test_description.to_string(),
        complete: false,
        tags: vec![],
        bypass_steps: vec![],
        requested_at: Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        claim_timeout_seconds: Some(300),
    }
}

/// Create test context for mathematical workflows
/// Equivalent to Ruby's test_input creation
pub fn create_mathematical_test_context(even_number: i64) -> Value {
    json!({
        "even_number": even_number,
        "test_run_id": Uuid::new_v4().to_string()
    })
}

/// Create test context for business workflows
/// Equivalent to Ruby's order fulfillment test data
pub fn create_business_test_context() -> Value {
    json!({
        "customer": {
            "id": 12345,
            "name": "Test Customer",
            "email": "test@example.com"
        },
        "items": [
            {
                "sku": "WIDGET-001",
                "quantity": 2,
                "price": 29.99
            },
            {
                "sku": "GADGET-002",
                "quantity": 1,
                "price": 49.99
            }
        ],
        "payment": {
            "method": "credit_card",
            "token": "tok_test_12345"
        },
        "shipping": {
            "method": "standard",
            "address": {
                "street": "123 Test Street",
                "city": "Test City",
                "state": "TS",
                "zip": "12345"
            }
        },
        "test_run_id": Uuid::new_v4().to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_shared_setup_creation() {
        let setup = SharedTestSetup::new().expect("Should create test setup");
        assert!(!setup.test_id.is_empty());
    }

    #[test]
    fn test_task_request_creation() {
        let context = create_mathematical_test_context(6);
        let request = create_test_task_request(
            "linear_workflow",
            "mathematical_sequence",
            "1.0.0",
            context,
            "Test task request creation",
        );

        assert_eq!(request.namespace, "linear_workflow");
        assert_eq!(request.name, "mathematical_sequence");
        assert_eq!(request.version, "1.0.0");
        assert_eq!(request.priority, Some(5));
    }

    #[test]
    fn test_mathematical_context_creation() {
        let context = create_mathematical_test_context(8);

        assert!(context.is_object());
        assert_eq!(context["even_number"], 8);
        assert!(context["test_run_id"].is_string());
    }

    #[test]
    fn test_business_context_creation() {
        let context = create_business_test_context();

        assert!(context.is_object());
        assert!(context["customer"].is_object());
        assert!(context["items"].is_array());
        assert!(context["payment"].is_object());
        assert!(context["shipping"].is_object());
        assert!(context["test_run_id"].is_string());
    }
}
