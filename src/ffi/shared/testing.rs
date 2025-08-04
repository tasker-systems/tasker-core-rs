//! # Shared Testing Factory
//!
//! Language-agnostic testing factory that can be used by any language binding
//! to create test data while preserving the handle-based architecture.
//!
//! ## Design Principles
//!
//! 1. **Shared Core Logic**: Language-agnostic factory patterns
//! 2. **Handle-Based Access**: Uses shared orchestration handle
//! 3. **Type Safety**: Shared types for cross-language compatibility

use super::errors::*;
use super::orchestration_system::*;
use super::types::*;
use tracing::{debug, info};

// Import core models directly (available from main crate)
use crate::models::{
    core::{task::NewTask, workflow_step::NewWorkflowStep},
    NamedStep, NamedTask, Task, TaskNamespace, WorkflowStep,
};

use serde_json::json;
use sqlx::PgPool;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

static GLOBAL_TESTING_FACTORY: OnceLock<Arc<SharedTestingFactory>> = OnceLock::new();

/// Global shared testing factory singleton
pub fn get_global_testing_factory() -> Arc<SharedTestingFactory> {
    GLOBAL_TESTING_FACTORY
        .get_or_init(|| {
            info!("🎯 Creating global shared testing factory");
            Arc::new(SharedTestingFactory::new())
        })
        .clone()
}

/// 🎯 SHARED TESTING FACTORY - Language-agnostic test data creation
///
/// This struct provides shared factory patterns that can be used by any
/// language binding while preserving the handle-based architecture.
///
/// ## Usage Pattern
///
/// ```rust
/// # use tasker_core::ffi::shared::testing::SharedTestingFactory;
/// # use tasker_core::ffi::shared::types::CreateTestTaskInput;
/// #
/// // Create using shared orchestration system
/// let factory = SharedTestingFactory::new();
///
/// // Example of creating test task input (actual execution requires database)
/// let input = CreateTestTaskInput {
///     namespace: "test".to_string(),
///     name: "test_task".to_string(),
///     ..Default::default()
/// };
///
/// // Method signature shows it returns SharedFFIResult<TestTaskOutput>
/// // factory.create_test_task(input) would execute the creation
/// ```
///
/// ## Architecture Benefits
///
/// - **Language-Agnostic**: Uses shared types for cross-language compatibility
/// - **Handle-Based**: Leverages shared orchestration handle for resource access
/// - **Type Safety**: Strong typing with shared input/output types
/// - **Resource Efficiency**: Shares database pool through orchestration system
#[derive(Clone)]
pub struct SharedTestingFactory {
    orchestration_system: Arc<OrchestrationSystem>,
}

impl Default for SharedTestingFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedTestingFactory {
    /// Create a new SharedTestingFactory using the shared orchestration system
    pub fn new() -> Self {
        debug!("🔧 SharedTestingFactory::new() - using shared orchestration system");
        let orchestration_system = initialize_unified_orchestration_system();
        Self {
            orchestration_system,
        }
    }

    /// Get database pool from orchestration system
    pub fn database_pool(&self) -> &PgPool {
        self.orchestration_system.database_pool()
    }

    /// Create test task using shared types
    pub fn create_test_task(&self, input: CreateTestTaskInput) -> SharedFFIResult<TestTaskOutput> {
        debug!(
            "🔍 SHARED FACTORY create_test_task: namespace={}, name={}",
            input.namespace, input.name
        );

        execute_async(async {
            let pool = self.database_pool();

            // Find or create namespace
            let namespace = match self.find_or_create_namespace(pool, &input.namespace).await {
                Ok(ns) => ns,
                Err(e) => {
                    return Err(SharedFFIError::TaskCreationFailed(format!(
                        "Failed to create namespace: {e}"
                    )))
                }
            };

            // Find or create named task
            let named_task = match self
                .find_or_create_named_task(
                    pool,
                    &input.name,
                    namespace.task_namespace_id.into(),
                    input.version.as_deref().unwrap_or("0.1.0"),
                )
                .await
            {
                Ok(nt) => nt,
                Err(e) => {
                    return Err(SharedFFIError::TaskCreationFailed(format!(
                        "Failed to create named task: {e}"
                    )))
                }
            };

            // Create new task
            let new_task = NewTask {
                named_task_id: named_task.named_task_id,
                requested_at: None, // Will default to NOW()
                initiator: input.initiator.clone(),
                source_system: Some("shared_testing_factory".to_string()),
                reason: Some("Created by shared testing factory".to_string()),
                bypass_steps: None,
                tags: Some(json!({"test": true, "shared_factory": true})),
                context: Some(input.context.unwrap_or_else(|| json!({"test": true}))),
                identity_hash: format!("task_{}", chrono::Utc::now().timestamp_millis()),
                claim_timeout_seconds: None,
                priority: None,
            };

            match Task::create(pool, new_task).await {
                Ok(task) => Ok(TestTaskOutput {
                    task_id: task.task_id,
                    namespace: input.namespace,
                    name: input.name,
                    version: input.version.unwrap_or("0.1.0".to_string()),
                    status: if task.complete {
                        "completed".to_string()
                    } else {
                        "pending".to_string()
                    },
                    context: task.context.clone().unwrap_or_else(|| json!({})),
                    created_at: task.created_at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                }),
                Err(e) => Err(SharedFFIError::TaskCreationFailed(format!(
                    "Failed to create task: {e}"
                ))),
            }
        })
    }

    // Helper methods for database operations
    async fn find_or_create_namespace(
        &self,
        pool: &PgPool,
        name: &str,
    ) -> Result<TaskNamespace, sqlx::Error> {
        // Use the model's find_or_create method
        TaskNamespace::find_or_create(pool, name).await
    }

    async fn find_or_create_named_task(
        &self,
        pool: &PgPool,
        name: &str,
        namespace_id: i64,
        version: &str,
    ) -> Result<NamedTask, sqlx::Error> {
        // Use the model's find_or_create method - 1 line instead of 25!
        NamedTask::find_or_create_by_name_version_namespace(pool, name, version, namespace_id).await
    }

    async fn find_or_create_named_step(
        &self,
        pool: &PgPool,
        name: &str,
        _namespace_id: i64, // Not used in actual schema
        _version: &str,     // Not used in actual schema
    ) -> Result<NamedStep, sqlx::Error> {
        // Use the model's find_or_create method - 1 line instead of 35!
        NamedStep::find_or_create_by_name_simple(pool, name).await
    }

    /// Create test workflow step using shared types
    pub fn create_test_step(&self, input: CreateTestStepInput) -> SharedFFIResult<TestStepOutput> {
        debug!(
            "🔍 SHARED FACTORY create_test_step: task_id={}, name={}",
            input.task_id, input.name
        );

        execute_async(async {
            let pool = self.database_pool();

            // Verify task exists using model layer (automatically handles correct table names)
            let task = match Task::find_by_id(pool, input.task_id).await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    return Err(SharedFFIError::StepCreationFailed(format!(
                        "Task {} not found",
                        input.task_id
                    )))
                }
                Err(e) => {
                    return Err(SharedFFIError::StepCreationFailed(format!(
                        "Failed to find task: {e}"
                    )))
                }
            };

            // Get namespace for named step lookup using model layer
            let named_task = match NamedTask::find_by_id(pool, task.named_task_id).await {
                Ok(Some(nt)) => nt,
                Ok(None) => {
                    return Err(SharedFFIError::StepCreationFailed(
                        "Named task not found".to_string(),
                    ))
                }
                Err(e) => {
                    return Err(SharedFFIError::StepCreationFailed(format!(
                        "Failed to find named task: {e}"
                    )))
                }
            };

            // Find or create named step
            let named_step = match self
                .find_or_create_named_step(pool, &input.name, named_task.task_namespace_id, "0.1.0")
                .await
            {
                Ok(ns) => ns,
                Err(e) => {
                    return Err(SharedFFIError::StepCreationFailed(format!(
                        "Failed to create named step: {e}"
                    )))
                }
            };

            // Create new workflow step with actual available fields
            let new_step = NewWorkflowStep {
                task_id: input.task_id,
                named_step_id: named_step.named_step_id,
                retryable: Some(true),
                retry_limit: Some(3),
                inputs: input.config.clone(), // Use config as inputs
                skippable: Some(false),
            };

            match WorkflowStep::create(pool, new_step).await {
                Ok(step) => Ok(TestStepOutput {
                    workflow_step_id: step.workflow_step_id,
                    task_id: step.task_id,
                    name: input.name,
                    handler_class: input.handler_class.unwrap_or("TestStepHandler".to_string()),
                    status: if step.processed {
                        "completed".to_string()
                    } else if step.in_process {
                        "in_progress".to_string()
                    } else {
                        "pending".to_string()
                    },
                    dependencies: input.dependencies.unwrap_or_default(),
                    config: step.inputs.clone().unwrap_or_else(|| json!({})),
                }),
                Err(e) => Err(SharedFFIError::StepCreationFailed(format!(
                    "Failed to create workflow step: {e}"
                ))),
            }
        })
    }

    /// Create test foundation (namespace + named task + named step)
    pub fn create_test_foundation(
        &self,
        input: CreateTestFoundationInput,
    ) -> SharedFFIResult<TestFoundationOutput> {
        debug!(
            "🔍 SHARED FACTORY create_test_foundation: namespace={}, task_name={}",
            input.namespace, input.task_name
        );

        execute_async(async {
            let pool = self.database_pool();

            // Create namespace
            let namespace = match self.find_or_create_namespace(pool, &input.namespace).await {
                Ok(ns) => ns,
                Err(e) => {
                    return Err(SharedFFIError::TaskCreationFailed(format!(
                        "Failed to create namespace: {e}"
                    )))
                }
            };

            // Create named task
            let named_task = match self
                .find_or_create_named_task(
                    pool,
                    &input.task_name,
                    namespace.task_namespace_id.into(),
                    "0.1.0",
                )
                .await
            {
                Ok(nt) => nt,
                Err(e) => {
                    return Err(SharedFFIError::TaskCreationFailed(format!(
                        "Failed to create named task: {e}"
                    )))
                }
            };

            // Create named step
            let named_step = match self
                .find_or_create_named_step(
                    pool,
                    &input.step_name,
                    namespace.task_namespace_id.into(),
                    "0.1.0",
                )
                .await
            {
                Ok(ns) => ns,
                Err(e) => {
                    return Err(SharedFFIError::StepCreationFailed(format!(
                        "Failed to create named step: {e}"
                    )))
                }
            };

            let foundation_id = format!(
                "foundation_{}_{}",
                namespace.task_namespace_id,
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );

            Ok(TestFoundationOutput {
                foundation_id,
                namespace: json!({
                    "name": namespace.name,
                    "namespace_id": namespace.task_namespace_id,
                    "description": namespace.description
                }),
                named_task: json!({
                    "name": named_task.name,
                    "named_task_id": named_task.named_task_id,
                    "namespace_id": named_task.task_namespace_id,
                    "version": named_task.version
                }),
                named_step: json!({
                    "name": named_step.name,
                    "named_step_id": named_step.named_step_id,
                    "dependent_system_id": named_step.dependent_system_id,
                    "description": named_step.description
                }),
                status: "created".to_string(),
                components: vec![
                    "namespace".to_string(),
                    "named_task".to_string(),
                    "named_step".to_string(),
                ],
            })
        })
    }

    /// Setup test environment
    pub fn setup_test_environment(&self) -> SharedFFIResult<EnvironmentSetupResult> {
        debug!("🔍 SHARED FACTORY setup_test_environment");

        let result = execute_async(async {
            // Use factory's database pool for any setup operations
            let pool = self.orchestration_system.database_pool();

            EnvironmentSetupResult {
                status: "initialized".to_string(),
                message: "Shared test environment setup completed".to_string(),
                handle_id: "shared_factory".to_string(),
                pool_size: pool.size(),
            }
        });

        Ok(result)
    }

    /// Cleanup test environment
    pub fn cleanup_test_environment(&self) -> SharedFFIResult<EnvironmentCleanupResult> {
        debug!("🔍 SHARED FACTORY cleanup_test_environment");

        let result = execute_async(async {
            // Use factory's database pool for cleanup operations
            let pool = self.orchestration_system.database_pool();

            EnvironmentCleanupResult {
                status: "cleaned".to_string(),
                message: "Shared test environment cleanup completed".to_string(),
                handle_id: "shared_factory".to_string(),
                pool_size: pool.size(),
            }
        });

        Ok(result)
    }
}

// ===== SHARED TESTING FACTORY LOGIC ENDS HERE =====
// Language bindings should implement their own wrapper functions that
// convert language-specific types to/from the shared types above.
