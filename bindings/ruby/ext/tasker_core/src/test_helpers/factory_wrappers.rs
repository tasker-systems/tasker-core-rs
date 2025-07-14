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
    Task, WorkflowStep, TaskNamespace, NamedTask, NamedStep,
    core::{
        task::NewTask,
        workflow_step::NewWorkflowStep, 
        task_namespace::NewTaskNamespace,
        named_task::NewNamedTask,
        named_step::NewNamedStep,
    }
};

use sqlx::PgPool;
use serde_json::json;

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

async fn create_dummy_named_step(pool: &PgPool) -> Result<NamedStep, sqlx::Error> {
    // Try to find existing dummy step first
    let existing_steps = NamedStep::find_by_name(pool, "dummy_step").await?;
    if let Some(existing) = existing_steps.first() {
        return Ok(existing.clone());
    }
    
    // Create new dummy step
    let new_named_step = NewNamedStep {
        name: "dummy_step".to_string(),
        dependent_system_id: 1, // Default system ID
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

/// Create test task using core models (mimicking existing TaskFactory pattern)
fn create_test_task_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
    
    // Create runtime for async operations
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for factory operations",
            )
        })?;

    let result = runtime.block_on(async {
        // Get database URL from options or environment
        let database_url = options.get("database_url")
            .and_then(|v| v.as_str())
            .unwrap_or("postgresql://localhost/tasker_test");
            
        // Create database pool
        let pool_result = PgPool::connect(database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return json!({
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
                });
            }
        };

        // Create or find dummy named task (mimicking factory pattern)
        let named_task_result = create_dummy_named_task(&pool).await;
        let named_task = match named_task_result {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        // Extract options with defaults (mimicking TaskFactory defaults)
        let context = options.get("context").cloned().unwrap_or_else(|| json!({
            "test": true,
            "created_by": "ruby_test_helper"
        }));
        
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
        match Task::create(&pool, new_task).await {
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
                "created_by": "factory_delegation"
            }),
            Err(e) => json!({
                "error": format!("Failed to create task: {}", e)
            })
        }
    });

    json_to_ruby_value(result)
}

/// Create test workflow step using core models (mimicking existing WorkflowStepFactory pattern)
fn create_test_workflow_step_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
    
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for factory operations",
            )
        })?;

    let result = runtime.block_on(async {
        // Get database URL from options or environment
        let database_url = options.get("database_url")
            .and_then(|v| v.as_str())
            .unwrap_or("postgresql://localhost/tasker_test");
            
        // Create database pool
        let pool_result = PgPool::connect(database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return json!({
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
                });
            }
        };

        // Get or create task_id (mimicking WorkflowStepFactory pattern)
        let task_id = if let Some(id) = options.get("task_id").and_then(|v| v.as_i64()) {
            id
        } else {
            // Create a dummy task if none provided
            match create_dummy_task(&pool).await {
                Ok(task) => task.task_id,
                Err(e) => {
                    return json!({
                        "error": format!("Failed to create task for step: {}", e)
                    });
                }
            }
        };

        // Create or find dummy named step
        let named_step_result = create_dummy_named_step(&pool).await;
        let named_step = match named_step_result {
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
        match WorkflowStep::create(&pool, new_step).await {
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
                "created_by": "factory_delegation"
            }),
            Err(e) => json!({
                "error": format!("Failed to create workflow step: {}", e)
            })
        }
    });

    json_to_ruby_value(result)
}

/// Create test foundation data using core models (mimicking foundation factory patterns)
fn create_test_foundation_with_factory_wrapper(options_value: Value) -> Result<Value, Error> {
    let options = ruby_value_to_json(options_value).unwrap_or_else(|_| json!({}));
    
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for factory operations",
            )
        })?;

    let result = runtime.block_on(async {
        // Get database URL from options or environment
        let database_url = options.get("database_url")
            .and_then(|v| v.as_str())
            .unwrap_or("postgresql://localhost/tasker_test");
            
        // Create database pool
        let pool_result = PgPool::connect(database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return json!({
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
                });
            }
        };

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
        let namespace_result = create_or_find_namespace(&pool, namespace_name, "Test namespace created by Ruby helper").await;
        let namespace = match namespace_result {
            Ok(ns) => ns,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create namespace: {}", e)
                });
            }
        };

        let named_task_result = create_dummy_named_task(&pool).await;
        let named_task = match named_task_result {
            Ok(nt) => nt,
            Err(e) => {
                return json!({
                    "error": format!("Failed to create named task: {}", e)
                });
            }
        };

        let named_step_result = create_dummy_named_step(&pool).await;
        let named_step = match named_step_result {
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
            "created_by": "factory_delegation"
        })
    });

    json_to_ruby_value(result)
}


/// Register factory wrapper functions
pub fn register_factory_functions(module: RModule) -> Result<(), Error> {
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