//! # Factory Wrappers - Test Helpers Only
//!
//! **DEVELOPMENT ONLY**: These functions wrap the existing Rust factory system
//! to provide Ruby test access without reimplementing database operations.
//!
//! ## Design Principles
//!
//! 1. **Delegate to Existing Factories**: All functions call the established
//!    factory system in `tests/factories/` instead of reimplementing database logic
//!
//! 2. **Thin Wrappers**: Minimal FFI conversion between Ruby arguments and Rust factory calls
//!
//! 3. **Same Patterns**: Use identical patterns as Rust integration tests

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::{Error, RModule, Ruby, Value};

// Import core models directly (available from main crate)
use tasker_core::models::{
    Task, WorkflowStep, TaskNamespace, NamedTask, NamedStep, DependentSystem,
    core::{
        task::NewTask,
        workflow_step::NewWorkflowStep,
        task_namespace::NewTaskNamespace,
        named_task::NewNamedTask,
        named_step::NewNamedStep,
        dependent_system::NewDependentSystem,
    }
};

use sqlx::{PgPool, Row};
use serde_json::json;
use std::sync::OnceLock;
use std::sync::Arc;

static GLOBAL_TESTING_FACTORY: OnceLock<Arc<TestingFactory>> = OnceLock::new();

/// 🎯 UNIFIED TESTING FACTORY - References OrchestrationSystem
///
/// This struct follows the unified architecture pattern where:
/// - TestingFactory references the OrchestrationSystem (not embedded in it)
/// - Database pool access goes through orchestration system
/// - Single source of truth for all orchestration resources
///
/// ## New Architecture Usage
///
/// ```rust
/// // Get from unified orchestration system (recommended)
/// let factory = TestingFactory::from_orchestration_system();
///
/// // Or inject orchestration system reference directly
/// let orchestration = initialize_unified_orchestration_system();
/// let factory = TestingFactory::with_orchestration_system(orchestration);
///
/// // Legacy: direct pool injection (discouraged)
/// let factory = TestingFactory::with_pool(my_pool);
/// ```
///
/// ## Architecture Benefits
///
/// - **Unified Resource Access**: Pool accessed through orchestration system
/// - **Single Initialization**: Prevents multiple pool creation
/// - **Configuration-Driven**: Inherits configuration from orchestration system
/// - **Proper Separation**: TestingFactory references orchestration, not embedded
#[derive(Clone)]
pub struct TestingFactory {
    orchestration_system: Option<Arc<crate::globals::OrchestrationSystem>>,
    pool: Option<PgPool>,  // Legacy direct pool support
}

impl TestingFactory {
    /// Create a new TestingFactory with the unified orchestration system
    pub fn new() -> Self {
        // 🎯 CRITICAL FIX: Use existing orchestration system instead of re-initializing
        // This prevents excessive calls to initialize_unified_orchestration_system
        println!("🔧 TestingFactory::new() - reusing existing orchestration system singleton");
        let orchestration_system = crate::globals::get_global_orchestration_system();
        Self {
            orchestration_system: Some(orchestration_system),
            pool: None
        }
    }

    /// Create TestingFactory with orchestration system reference
    ///
    /// This constructor allows explicit injection of an orchestration system reference.
    pub fn with_orchestration_system(orchestration_system: Arc<crate::globals::OrchestrationSystem>) -> Self {
        Self {
            orchestration_system: Some(orchestration_system),
            pool: None
        }
    }

    /// Get database pool reference - unified access method
    ///
    /// This method provides unified access to the database pool regardless of
    /// how the TestingFactory was constructed.
    fn pool(&self) -> &PgPool {
        if let Some(orchestration_system) = &self.orchestration_system {
            orchestration_system.database_pool()
        } else if let Some(pool) = &self.pool {
            pool
        } else {
            panic!("TestingFactory not properly initialized - no pool or orchestration system available")
        }
    }

    /// Create a test task with the given options
    ///
    /// This method encapsulates all the logic for creating a test task,
    /// including creating dependent entities (named tasks, namespaces) as needed.
    pub async fn create_task(&self, options: serde_json::Value) -> serde_json::Value {
        // Create or find dummy named task (mimicking factory pattern)
        let named_task = match create_dummy_named_task(self.pool()).await {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        // Extract options with defaults (mimicking TaskFactory defaults)
        let mut context = json!({
            "test": true,
            "created_by": "ruby_test_helper"
        });

        // Merge provided context with defaults
        if let Some(provided_context) = options.get("context") {
            if let Some(provided_obj) = provided_context.as_object() {
                if let Some(default_obj) = context.as_object_mut() {
                    for (key, value) in provided_obj {
                        default_obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        let tags = options.get("tags").cloned().unwrap_or_else(|| json!({
            "test": true,
            "auto_generated": true
        }));

        // Create NewTask (mimicking TaskFactory::create)
        let new_task = NewTask {
            named_task_id: named_task.named_task_id,
            requested_at: None, // Will default to NOW()
            initiator: options.get("initiator").and_then(|v| v.as_str()).map(|s| s.to_string()),
            source_system: Some("ruby_test_helpers".to_string()),
            reason: Some("Created by Ruby test helper".to_string()),
            bypass_steps: None,
            tags: Some(tags),
            context: Some(context),
            identity_hash: generate_test_identity_hash(),
        };

        // Create task using core model
        match Task::create(self.pool(), new_task).await {
            Ok(task) => json!({
                "task_id": task.task_id,
                "named_task_id": task.named_task_id,
                "complete": task.complete,
                "context": task.context,
                "tags": task.tags,
                "status": if task.complete { "complete" } else { "pending" },
                "initiator": task.initiator,
                "source_system": task.source_system,
                "reason": task.reason,
                "requested_at": task.requested_at.and_utc().to_rfc3339(),
                "identity_hash": task.identity_hash,
                "created_by": "testing_factory"
            }),
            Err(e) => json!({
                "error": format!("Failed to create task: {}", e)
            })
        }
    }

    /// Create a test workflow step with the given options
    ///
    /// This method encapsulates all the logic for creating a test workflow step,
    /// including creating dependent entities (tasks, named steps) as needed.
    pub async fn create_workflow_step(&self, options: serde_json::Value) -> serde_json::Value {
        // Get or create task_id (mimicking WorkflowStepFactory pattern)
        let task_id = if let Some(id) = options.get("task_id").and_then(|v| v.as_i64()) {
            id
        } else {
            // Create a dummy task if none provided
            match create_dummy_task(self.pool()).await {
                Ok(task) => task.task_id,
                Err(e) => {
                    return json!({
                        "error": format!("Failed to create task for step: {}", e)
                    });
                }
            }
        };

        // Create or find dummy named step
        let named_step = match create_dummy_named_step(self.pool()).await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named step: {}", e)
                });
            }
        };

        // Extract options with defaults (mimicking WorkflowStepFactory defaults)
        let inputs = options.get("inputs").cloned().unwrap_or_else(|| json!({
            "test_input": true,
            "created_by": "ruby_test_helper"
        }));

        // Create NewWorkflowStep (mimicking WorkflowStepFactory::create)
        let new_step = NewWorkflowStep {
            task_id,
            named_step_id: named_step.named_step_id,
            retryable: Some(true),
            retry_limit: Some(3),
            inputs: Some(inputs),
            skippable: Some(false),
        };

        // Create workflow step using core model
        match WorkflowStep::create(self.pool(), new_step).await {
            Ok(step) => json!({
                "workflow_step_id": step.workflow_step_id,
                "task_id": step.task_id,
                "named_step_id": step.named_step_id,
                "retryable": step.retryable,
                "retry_limit": step.retry_limit,
                "inputs": step.inputs,
                "results": step.results,
                "in_process": step.in_process,
                "processed": step.processed,
                "skippable": step.skippable,
                "created_by": "testing_factory"
            }),
            Err(e) => json!({
                "error": format!("Failed to create workflow step: {}", e)
            })
        }
    }

    /// Create test foundation data with the given options
    ///
    /// This method creates the basic foundation entities needed for testing:
    /// namespaces, named tasks, and named steps.
    pub async fn create_foundation(&self, options: serde_json::Value) -> serde_json::Value {
        // Extract options with defaults
        let namespace_name = options.get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let task_name = options.get("task_name")
            .and_then(|v| v.as_str())
            .unwrap_or("dummy_task");
        let step_name = options.get("step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("dummy_step");

        // Create foundation data (mimicking StandardFoundation pattern)
        let namespace = match create_or_find_namespace(self.pool(), namespace_name, "Test namespace created by Ruby helper").await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create namespace: {}", e)
                });
            }
        };

        let named_task = match create_dummy_named_task(self.pool()).await {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        let named_step = match create_dummy_named_step(self.pool()).await {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named step: {}", e)
                });
            }
        };

        json!({
            "namespace": {
                "task_namespace_id": namespace.task_namespace_id,
                "name": namespace.name,
                "description": namespace.description
            },
            "named_task": {
                "named_task_id": named_task.named_task_id,
                "name": named_task.name,
                "task_namespace_id": named_task.task_namespace_id,
                "version": named_task.version,
                "description": named_task.description
            },
            "named_step": {
                "named_step_id": named_step.named_step_id,
                "name": named_step.name,
                "description": named_step.description
            },
            "created_by": "testing_factory"
        })
    }
}

// Helper functions (mimicking factory utility functions)

fn generate_test_identity_hash() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    format!("test_task_{}", timestamp)
}

async fn create_dummy_named_task(pool: &PgPool) -> Result<NamedTask, sqlx::Error> {
    // Create or find default namespace first
    let namespace = create_or_find_namespace(pool, "default", "Default test namespace").await?;

    // Try to find existing dummy task first (find-or-create pattern)
    if let Some(existing) = NamedTask::find_by_name_version_namespace(
        pool,
        "dummy_task",
        "0.1.0",
        namespace.task_namespace_id as i64
    ).await? {
        return Ok(existing);
    }

    // Create new dummy task
    let new_named_task = NewNamedTask {
        name: "dummy_task".to_string(),
        task_namespace_id: namespace.task_namespace_id as i64,
        version: Some("0.1.0".to_string()),
        description: Some("Dummy task for testing".to_string()),
        configuration: Some(json!({"test": true})),
    };

    NamedTask::create(pool, new_named_task).await
}

async fn create_or_find_dependent_system(pool: &PgPool, name: &str, description: &str) -> Result<DependentSystem, sqlx::Error> {
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

async fn create_dummy_named_step(pool: &PgPool) -> Result<NamedStep, sqlx::Error> {
    // Try to find existing dummy step first
    let existing_steps = NamedStep::find_by_name(pool, "dummy_step").await?;
    if let Some(existing) = existing_steps.first() {
        return Ok(existing.clone());
    }

    // Ensure dependent system exists
    let dependent_system = create_or_find_dependent_system(pool, "test_system", "Test system for Ruby helpers").await?;

    // Create new dummy step
    let new_named_step = NewNamedStep {
        name: "dummy_step".to_string(),
        dependent_system_id: dependent_system.dependent_system_id as i32,
        description: Some("Dummy step for testing".to_string()),
    };

    NamedStep::create(pool, new_named_step).await
}

async fn create_or_find_namespace(pool: &PgPool, name: &str, description: &str) -> Result<TaskNamespace, sqlx::Error> {
    // Try to find existing namespace first
    if let Some(existing) = TaskNamespace::find_by_name(pool, name).await? {
        return Ok(existing);
    }

    // Create new namespace
    let new_namespace = NewTaskNamespace {
        name: name.to_string(),
        description: Some(description.to_string()),
    };

    TaskNamespace::create(pool, new_namespace).await
}

async fn create_dummy_task(pool: &PgPool) -> Result<Task, sqlx::Error> {
    let named_task = create_dummy_named_task(pool).await?;

    let new_task = NewTask {
        named_task_id: named_task.named_task_id,
        requested_at: None,
        initiator: Some("test_helper".to_string()),
        source_system: Some("ruby_test_helpers".to_string()),
        reason: Some("Auto-created for step testing".to_string()),
        bypass_steps: None,
        tags: Some(json!({"test": true, "auto_created": true})),
        context: Some(json!({"test": true, "created_by": "helper"})),
        identity_hash: generate_test_identity_hash(),
    };

    Task::create(pool, new_task).await
}

pub fn get_global_testing_factory() -> Arc<TestingFactory> {
  println!("🎯 UNIFIED ENTRY: initialize_global_testing_factory called");
  GLOBAL_TESTING_FACTORY.get_or_init(|| {
      // Use the unified orchestration system to create TestingFactory
      println!("🎯 UNIFIED ENTRY: Creating TestingFactory from orchestration system");
      let testing_factory = TestingFactory::new();
      Arc::new(testing_factory)
  }).clone()
}


/// Create test task using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_task_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the task
        factory.create_task(options).await
    });

    json_to_ruby_value(result)
}


/// Create test workflow step using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_workflow_step_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the workflow step
        factory.create_workflow_step(options).await
    });

    json_to_ruby_value(result)
}


/// Create test foundation data using TestingFactory
/// FFI wrapper that uses the TestingFactory struct for cleaner architecture
fn create_test_foundation_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));

    let result = crate::globals::execute_async(async {
        // Get TestingFactory from unified orchestration system
        let factory = get_global_testing_factory();

        // Use the factory to create the foundation data
        factory.create_foundation(options).await
    });

    json_to_ruby_value(result)
}

/// Setup the factory singleton - triggers Rust-side singleton initialization
/// This ensures the `get_global_testing_factory()` singleton is created and ready
fn setup_factory_singleton_wrapper() -> Result<Value, Error> {
    println!("🎯 FACTORY SINGLETON: Initializing factory singleton from Ruby coordination");
    
    // Trigger the singleton initialization by calling get_global_testing_factory
    let _factory = get_global_testing_factory();
    
    json_to_ruby_value(serde_json::json!({
        "status": "initialized",
        "type": "TestingFactorySingleton",
        "message": "Rust-side TestingFactory singleton initialized successfully",
        "singleton_active": true
    }))
}

/// Register factory wrapper functions
pub fn register_factory_functions(module: RModule) -> Result<(), Error> {
    // Factory singleton coordination
    module.define_module_function(
        "setup_factory_singleton",
        magnus::function!(setup_factory_singleton_wrapper, 0),
    )?;
    
    // Factory operation functions
    module.define_module_function(
        "create_test_task_with_factory",
        magnus::function!(create_test_task_with_factory_wrapper, 1),
    )?;
    module.define_module_function(
        "create_test_workflow_step_with_factory",
        magnus::function!(create_test_workflow_step_with_factory_wrapper, 1),
    )?;
    module.define_module_function(
        "create_test_foundation_with_factory",
        magnus::function!(create_test_foundation_with_factory_wrapper, 1),
    )?;
    Ok(())
}
