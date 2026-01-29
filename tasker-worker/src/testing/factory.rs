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

use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tasker_client::api_clients::orchestration_client::{
    OrchestrationApiClient, OrchestrationApiConfig,
};
use tasker_pgmq::PgmqClient;
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::models::{NamedStep, NamedTask, TaskNamespace, WorkflowStep};
use tasker_shared::types::api::orchestration::TaskResponse;
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
///
/// ## TAS-78: Split Database Support
///
/// The factory supports separate pools for Tasker tables and PGMQ queues:
/// - `database_pool`: Used for Tasker tables (tasks, steps, namespaces)
/// - `pgmq_pool`: Used for PGMQ queue operations (queue creation, messages)
///
/// For backward compatibility, if `pgmq_pool` is not specified, it defaults
/// to using the same pool as `database_pool`.
#[derive(Clone, Debug)]
pub struct WorkerTestFactory {
    /// Pool for Tasker table operations (tasks, steps, namespaces)
    database_pool: Arc<PgPool>,
    /// Pool for PGMQ queue operations (TAS-78: may be separate database)
    pgmq_pool: Arc<PgPool>,
    api_client: Option<Arc<OrchestrationApiClient>>,
}

impl WorkerTestFactory {
    /// Create a new factory with database connection (single pool mode)
    ///
    /// For backward compatibility, uses the same pool for both Tasker tables
    /// and PGMQ operations. Use `new_with_pools()` for split-database mode.
    pub fn new(database_pool: Arc<PgPool>) -> Self {
        Self {
            pgmq_pool: database_pool.clone(), // TAS-78: default to same pool
            database_pool,
            api_client: None,
        }
    }

    /// Create a new factory with separate pools for Tasker and PGMQ (TAS-78)
    ///
    /// Use this constructor when testing in split-database mode where PGMQ
    /// tables are in a separate database from Tasker tables.
    pub fn new_with_pools(database_pool: Arc<PgPool>, pgmq_pool: Arc<PgPool>) -> Self {
        Self {
            database_pool,
            pgmq_pool,
            api_client: None,
        }
    }

    /// Create a factory with orchestration API client for task initialization
    ///
    /// Uses single pool mode. For split-database mode, use `with_api_client_and_pools()`.
    pub fn with_api_client(
        database_pool: Arc<PgPool>,
        orchestration_url: String,
        auth_token: Option<String>,
    ) -> Result<Self, TestFactoryError> {
        Self::with_api_client_and_pools(
            database_pool.clone(),
            database_pool, // TAS-78: default to same pool
            orchestration_url,
            auth_token,
        )
    }

    /// Create a factory with API client and separate pools (TAS-78)
    ///
    /// Use this constructor when testing in split-database mode.
    pub fn with_api_client_and_pools(
        database_pool: Arc<PgPool>,
        pgmq_pool: Arc<PgPool>,
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
            pgmq_pool,
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
    ///
    /// Uses the Tasker pool for namespace creation and PGMQ pool for queue creation.
    pub async fn create_test_namespace(
        &self,
        name: &str,
    ) -> Result<TestNamespace, TestFactoryError> {
        // TAS-78: Use Tasker pool for namespace (Tasker table)
        let tasker_pool = self.database_pool.as_ref();

        // Find or create namespace in Tasker database
        let namespace = TaskNamespace::find_or_create(tasker_pool, name)
            .await
            .map_err(|e| {
                TestFactoryError::DatabaseError(format!("Namespace creation failed: {}", e))
            })?;

        // TAS-78: Use PGMQ pool for queue creation (PGMQ database)
        let pgmq_client = PgmqClient::new_with_pool(self.pgmq_pool.as_ref().clone()).await;
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
            "0.1.0",
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
    ///
    /// Returns the full `TaskResponse` (same shape as GET /v1/tasks/{uuid}).
    pub async fn initialize_task_via_api(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskResponse, TestFactoryError> {
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
                max_attempts: Some(config.max_attempts),
                inputs: Some(config.inputs),
            };

            let step = WorkflowStep::create(pool, new_step).await.map_err(|e| {
                TestFactoryError::DatabaseError(format!("Workflow step creation failed: {}", e))
            })?;

            created_steps.push(step);
        }

        Ok(created_steps)
    }

    /// Get Tasker database pool for custom operations
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
    }

    /// Get PGMQ database pool for custom queue operations (TAS-78)
    ///
    /// In single-database mode, this returns the same pool as `database_pool()`.
    /// In split-database mode, this returns a separate pool connected to the PGMQ database.
    pub fn pgmq_pool(&self) -> &PgPool {
        &self.pgmq_pool
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
    pub max_attempts: i32,
    pub skippable: bool,
}

impl Default for TestStepConfig {
    fn default() -> Self {
        Self {
            name: "test_step".to_string(),
            inputs: json!({}),
            retryable: true,
            max_attempts: 3,
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
#[derive(Debug)]
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
        assert_eq!(config.max_attempts, 3);
        assert!(!config.skippable);
    }
}
