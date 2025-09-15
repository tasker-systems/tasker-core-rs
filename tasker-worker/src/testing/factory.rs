//! # Worker Testing Factory
//!
//! Pure Rust testing factory for worker components, inspired by
//! tasker-orchestration/src/ffi/shared/testing.rs patterns but
//! without OrchestrationCore or FFI dependencies.
//!
//! ## Key Patterns
//!
//! - Type-safe test data creation
//! - Factory patterns for common test objects
//! - Database-backed test data persistence
//! - Namespace and queue setup for testing

use pgmq_notify::PgmqClient;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_client::api_clients::orchestration_client::{
    OrchestrationApiClient, OrchestrationApiConfig,
};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::models::{NamedStep, NamedTask, TaskNamespace, WorkflowStep};
use tasker_shared::types::api::orchestration::TaskCreationResponse;
use uuid::Uuid;

/// Test-specific error type for factory operations
#[derive(Debug, thiserror::Error)]
pub enum TestFactoryError {
    #[error("Database operation failed: {0}")]
    DatabaseError(String),

    #[error("API client error: {0}")]
    ApiClientError(String),

    #[error("Test data creation failed: {0}")]
    CreationError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Worker-specific testing factory for creating test data
///
/// Provides factory methods for creating test tasks, steps, and related
/// objects without FFI dependencies.
#[derive(Clone)]
pub struct WorkerTestFactory {
    database_pool: Arc<PgPool>,
    api_client: Option<Arc<OrchestrationApiClient>>,
}

impl WorkerTestFactory {
    /// Create a new factory with database connection
    pub fn new(database_pool: Arc<PgPool>) -> Self {
        Self {
            database_pool,
            api_client: None,
        }
    }

    /// Create a factory with orchestration API client for task initialization
    pub fn with_api_client(
        database_pool: Arc<PgPool>,
        orchestration_url: String,
        auth_token: Option<String>,
    ) -> Result<Self, TestFactoryError> {
        let auth = auth_token.map(|token| tasker_shared::config::WebAuthConfig {
            enabled: true,
            api_key: token,
            ..Default::default()
        });

        let config = OrchestrationApiConfig {
            base_url: orchestration_url,
            auth,
            ..OrchestrationApiConfig::default()
        };

        let client = OrchestrationApiClient::new(config)
            .map_err(|e| TestFactoryError::ApiClientError(e.to_string()))?;

        Ok(Self {
            database_pool,
            api_client: Some(Arc::new(client)),
        })
    }

    /// Create a test task request
    pub fn create_test_task_request(
        &self,
        namespace: &str,
        name: &str,
        test_id: &str,
    ) -> TaskRequest {
        TaskRequest::new(name.to_string(), namespace.to_string())
            .with_context(json!({
                "test": true,
                "test_id": test_id,
                "created_by": "worker_test_factory",
            }))
            .with_initiator("test_factory".to_string())
            .with_source_system("worker_testing".to_string())
            .with_reason(format!("Testing {} workflow", name))
            .with_tags(vec!["test".to_string(), "worker".to_string()])
            .with_priority(5)
    }

    /// Create test namespace and related infrastructure
    pub async fn create_test_namespace(
        &self,
        name: &str,
    ) -> Result<TestNamespace, TestFactoryError> {
        let pool = self.database_pool.as_ref();

        // Find or create namespace
        let namespace = TaskNamespace::find_or_create(pool, name)
            .await
            .map_err(|e| {
                TestFactoryError::DatabaseError(format!("Namespace creation failed: {}", e))
            })?;

        // Create pgmq queue for namespace
        let pgmq_client = PgmqClient::new_with_pool(pool.clone()).await;
        let queue_name = format!("worker_{}_queue", name);

        pgmq_client.create_queue(&queue_name).await.map_err(|e| {
            TestFactoryError::CreationError(format!("Queue creation failed: {}", e))
        })?;

        Ok(TestNamespace {
            namespace,
            queue_name,
        })
    }

    /// Create a complete test foundation (namespace + named task + named step)
    pub async fn create_test_foundation(
        &self,
        namespace_name: &str,
        task_name: &str,
        step_name: &str,
    ) -> Result<TestFoundation, TestFactoryError> {
        let pool = self.database_pool.as_ref();

        // Create namespace
        let test_namespace = self.create_test_namespace(namespace_name).await?;

        // Create named task
        let named_task = NamedTask::find_or_create_by_name_version_namespace(
            pool,
            task_name,
            "1.0.0",
            test_namespace.namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| {
            TestFactoryError::DatabaseError(format!("Named task creation failed: {}", e))
        })?;

        // Create named step
        let named_step = NamedStep::find_or_create_by_name_simple(pool, step_name)
            .await
            .map_err(|e| {
                TestFactoryError::DatabaseError(format!("Named step creation failed: {}", e))
            })?;

        Ok(TestFoundation {
            namespace: test_namespace,
            named_task,
            named_step,
        })
    }

    /// Initialize a task via orchestration API (if client configured)
    pub async fn initialize_task_via_api(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskCreationResponse, TestFactoryError> {
        let client = self.api_client.as_ref().ok_or_else(|| {
            TestFactoryError::ConfigError("API client not configured".to_string())
        })?;

        client
            .create_task(task_request)
            .await
            .map_err(|e| TestFactoryError::ApiClientError(format!("Task creation failed: {}", e)))
    }

    /// Create test step message for worker processing
    pub async fn create_test_step_message(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        payload: serde_json::Value,
    ) -> Result<TestStepMessage, TestFactoryError> {
        // This creates the data structure that would be read from a queue
        Ok(TestStepMessage {
            step_uuid,
            task_uuid,
            step_name: step_name.to_string(),
            payload,
            created_at: chrono::Utc::now(),
        })
    }

    /// Create multiple test workflow steps for a task
    pub async fn create_test_workflow_steps(
        &self,
        task_uuid: Uuid,
        step_configs: Vec<TestStepConfig>,
    ) -> Result<Vec<WorkflowStep>, TestFactoryError> {
        let pool = self.database_pool.as_ref();
        let mut created_steps = Vec::new();

        for config in step_configs {
            // Get or create named step
            let named_step = NamedStep::find_or_create_by_name_simple(pool, &config.name)
                .await
                .map_err(|e| {
                    TestFactoryError::DatabaseError(format!("Named step creation failed: {}", e))
                })?;

            // Create workflow step
            let new_step = tasker_shared::models::core::workflow_step::NewWorkflowStep {
                task_uuid,
                named_step_uuid: named_step.named_step_uuid,
                retryable: Some(config.retryable),
                retry_limit: Some(config.retry_limit),
                inputs: Some(config.inputs),
                skippable: Some(config.skippable),
            };

            let step = WorkflowStep::create(pool, new_step).await.map_err(|e| {
                TestFactoryError::DatabaseError(format!("Workflow step creation failed: {}", e))
            })?;

            created_steps.push(step);
        }

        Ok(created_steps)
    }

    /// Get database pool for custom operations
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
    }
}

/// Test namespace with associated queue
#[derive(Debug, Clone)]
pub struct TestNamespace {
    pub namespace: TaskNamespace,
    pub queue_name: String,
}

/// Complete test foundation with namespace, named task, and named step
#[derive(Debug, Clone)]
pub struct TestFoundation {
    pub namespace: TestNamespace,
    pub named_task: NamedTask,
    pub named_step: NamedStep,
}

/// Test step configuration for creating workflow steps
#[derive(Debug, Clone)]
pub struct TestStepConfig {
    pub name: String,
    pub inputs: serde_json::Value,
    pub retryable: bool,
    pub retry_limit: i32,
    pub skippable: bool,
}

impl Default for TestStepConfig {
    fn default() -> Self {
        Self {
            name: "test_step".to_string(),
            inputs: json!({}),
            retryable: true,
            retry_limit: 3,
            skippable: false,
        }
    }
}

/// Test step message structure
#[derive(Debug, Clone)]
pub struct TestStepMessage {
    pub step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub payload: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Common test data builder for worker tests
pub struct WorkerTestData {
    pub namespace_name: String,
    pub task_name: String,
    pub test_id: String,
    pub foundation: Option<TestFoundation>,
}

impl WorkerTestData {
    /// Create a new test data builder
    pub fn new(test_id: &str) -> Self {
        Self {
            namespace_name: format!("test_ns_{}", test_id),
            task_name: format!("test_task_{}", test_id),
            test_id: test_id.to_string(),
            foundation: None,
        }
    }

    /// Build test data with factory
    pub async fn build_with_factory(
        self,
        factory: &WorkerTestFactory,
    ) -> Result<Self, TestFactoryError> {
        let foundation = factory
            .create_test_foundation(
                &self.namespace_name,
                &self.task_name,
                &format!("step_{}", self.test_id),
            )
            .await?;

        Ok(Self {
            foundation: Some(foundation),
            ..self
        })
    }

    /// Get the test foundation
    pub fn foundation(&self) -> Option<&TestFoundation> {
        self.foundation.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_config_defaults() {
        let config = TestStepConfig::default();

        assert_eq!(config.name, "test_step");
        assert!(config.retryable);
        assert_eq!(config.retry_limit, 3);
        assert!(!config.skippable);
    }
}
