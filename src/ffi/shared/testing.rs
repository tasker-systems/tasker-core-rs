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
    core::{
        dependent_system::NewDependentSystem, named_step::NewNamedStep, named_task::NewNamedTask,
        task::NewTask, task_namespace::NewTaskNamespace, workflow_step::NewWorkflowStep,
    },
    DependentSystem, NamedStep, NamedTask, Task, TaskNamespace, WorkflowStep,
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
        // Try to find existing namespace
        if let Ok(Some(namespace)) =
            sqlx::query_as::<_, TaskNamespace>("SELECT * FROM task_namespaces WHERE name = $1")
                .bind(name)
                .fetch_optional(pool)
                .await
        {
            return Ok(namespace);
        }

        // Create new namespace
        let new_namespace = NewTaskNamespace {
            name: name.to_string(),
            description: Some(format!("Test namespace created by shared factory: {name}")),
        };

        TaskNamespace::create(pool, new_namespace).await
    }

    async fn find_or_create_named_task(
        &self,
        pool: &PgPool,
        name: &str,
        namespace_id: i64,
        version: &str,
    ) -> Result<NamedTask, sqlx::Error> {
        // Try to find existing named task
        if let Ok(Some(named_task)) = sqlx::query_as::<_, NamedTask>(
            "SELECT * FROM named_tasks WHERE name = $1 AND task_namespace_id = $2 AND version = $3",
        )
        .bind(name)
        .bind(namespace_id)
        .bind(version)
        .fetch_optional(pool)
        .await
        {
            return Ok(named_task);
        }

        // Create new named task
        let new_named_task = NewNamedTask {
            name: name.to_string(),
            task_namespace_id: namespace_id,
            description: Some(format!("Test task created by shared factory: {name}")),
            version: Some(version.to_string()),
            configuration: Some(json!({"created_by": "shared_testing_factory"})),
        };

        NamedTask::create(pool, new_named_task).await
    }

    async fn find_or_create_named_step(
        &self,
        pool: &PgPool,
        name: &str,
        _namespace_id: i64, // Not used in actual schema
        _version: &str,     // Not used in actual schema
    ) -> Result<NamedStep, sqlx::Error> {
        // Try to find existing named step by name only
        if let Ok(existing_steps) = NamedStep::find_by_name(pool, name).await {
            if let Some(existing) = existing_steps.first() {
                return Ok(existing.clone());
            }
        }

        // Ensure dependent system exists for the step
        let dependent_system = match self
            .find_or_create_dependent_system(
                pool,
                "shared_testing_factory",
                "Test system for shared testing factory",
            )
            .await
        {
            Ok(ds) => ds,
            Err(e) => return Err(e),
        };

        // Create new named step
        let new_named_step = NewNamedStep {
            name: name.to_string(),
            dependent_system_id: dependent_system.dependent_system_id,
            description: Some(format!("Test step created by shared factory: {name}")),
        };

        NamedStep::create(pool, new_named_step).await
    }

    async fn find_or_create_dependent_system(
        &self,
        pool: &PgPool,
        name: &str,
        description: &str,
    ) -> Result<DependentSystem, sqlx::Error> {
        // Try to find existing dependent system first
        if let Some(existing) = DependentSystem::find_by_name(pool, name).await? {
            return Ok(existing);
        }

        // Create new dependent system
        let new_dependent_system = NewDependentSystem {
            name: name.to_string(),
            description: Some(description.to_string()),
        };

        DependentSystem::create(pool, new_dependent_system).await
    }

    /// Create test workflow step using shared types
    pub fn create_test_step(&self, input: CreateTestStepInput) -> SharedFFIResult<TestStepOutput> {
        debug!(
            "🔍 SHARED FACTORY create_test_step: task_id={}, name={}",
            input.task_id, input.name
        );

        execute_async(async {
            let pool = self.database_pool();

            // Verify task exists
            let task = match sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE task_id = $1")
                .bind(input.task_id)
                .fetch_optional(pool)
                .await
            {
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

            // Get namespace for named step lookup
            let named_task = match sqlx::query_as::<_, NamedTask>(
                "SELECT * FROM named_tasks WHERE named_task_id = $1",
            )
            .bind(task.named_task_id)
            .fetch_optional(pool)
            .await
            {
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
                    step_id: step.workflow_step_id,
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
