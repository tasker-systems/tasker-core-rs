//! # BaseTaskHandler - Ruby FFI Wrapper
//!
//! This provides a Ruby FFI wrapper that follows the complete workflow from RUBY.md.
//! Ruby developers can subclass this to implement their task logic in lifecycle
//! hooks, receiving simple Ruby arrays/hashes instead of complex Rust types.

use crate::globals::{get_global_orchestration_system, execute_async};
use crate::context::{json_to_ruby_value, ValidationConfig};
use magnus::value::ReprValue;
use magnus::{function, method, Error, Module, Object, RModule, Ruby, Value};
use tracing::debug;
use tasker_core::orchestration::task_initializer::{TaskInitializer, TaskInitializationConfig};
use tasker_core::orchestration::task_enqueuer::{TaskEnqueuer, EnqueueRequest, EnqueuePriority};
use tasker_core::orchestration::config::TaskTemplate;
use tasker_core::models::core::task_request::TaskRequest;


/// Ruby wrapper for Rust BaseTaskHandler
#[magnus::wrap(class = "TaskerCore::BaseTaskHandler")]
pub struct BaseTaskHandler {
    /// Task template for this handler
    task_template: TaskTemplate,
    /// Reference to the unified orchestration system
    orchestration_system: std::sync::Arc<crate::globals::OrchestrationSystem>,
}

impl BaseTaskHandler {
        /// Create a new BaseTaskHandler with a TaskTemplate
    pub fn new(task_template: TaskTemplate) -> magnus::error::Result<Self> {
        // 🎯 CRITICAL FIX: Use the singleton orchestration system 
        // This prevents the "37 calls to initialize_unified_orchestration_system" problem
        // by reusing the same orchestration system instance across all BaseTaskHandler instances
        debug!("🔧 BaseTaskHandler::new() - reusing existing orchestration system singleton");
        let orchestration_system = get_global_orchestration_system();

        Ok(Self {
            task_template,
            orchestration_system,
        })
    }

    /// Create a new BaseTaskHandler from JSON (for Ruby FFI)
    pub fn new_from_json(task_template_json: Value) -> magnus::error::Result<Self> {
        // Use strict validation for task template configuration
        let validation_config = ValidationConfig {
            max_string_length: 500,     // Reasonable for configuration fields
            max_array_length: 50,       // Moderate array sizes for configs
            max_object_depth: 3,        // Task templates shouldn't be deeply nested
            max_object_keys: 20,        // Reasonable for task template fields
            max_numeric_value: 1e9,     // Large but reasonable for configuration values
            min_numeric_value: -1e9,
        };
        
        let task_template_data = crate::context::ruby_value_to_json_with_validation(task_template_json, &validation_config)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Task template validation failed: {}", e)))?;

        let task_template: TaskTemplate = serde_json::from_value(task_template_data)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to parse TaskTemplate: {}", e)))?;

        Self::new(task_template)
    }

    /// Initialize task from TaskRequest - this is the critical method from RUBY.md
    pub fn initialize_task(&self, task_request_value: Value) -> magnus::error::Result<Value> {
        let ruby = Ruby::get().unwrap();

        // Use strict validation for task request data
        let validation_config = ValidationConfig {
            max_string_length: 1000,    // Reasonable for task request fields
            max_array_length: 100,      // Moderate array sizes
            max_object_depth: 4,        // Task requests can be somewhat nested
            max_object_keys: 30,        // Reasonable for task request structure
            max_numeric_value: 1e12,    // Large but reasonable for IDs and timestamps
            min_numeric_value: -1e12,
        };
        
        // Convert Ruby value to JSON then to TaskRequest with validation
        let task_request_json = crate::context::ruby_value_to_json_with_validation(task_request_value, &validation_config)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Task request validation failed: {}", e)))?;

        let task_request: TaskRequest = serde_json::from_value(task_request_json)
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Failed to parse TaskRequest: {}", e)))?;

        // Use TaskInitializer to properly initialize the task
        let orchestration_system = &self.orchestration_system;
        let result = execute_async(async {
            let task_initializer = TaskInitializer::with_state_manager(
                orchestration_system.database_pool.clone(),
                TaskInitializationConfig::default(),
                orchestration_system.event_publisher.clone()
            );

            let initialization_result = task_initializer.create_task_from_request(task_request).await
                .map_err(|e| format!("Task initialization failed: {}", e))?;

            // Get the created task for enqueuing
            let task = tasker_core::models::Task::find_by_id(
                &orchestration_system.database_pool,
                initialization_result.task_id
            ).await.map_err(|e| format!("Failed to load created task: {}", e))?
            .ok_or_else(|| "Created task not found".to_string())?;

            // Use TaskEnqueuer to enqueue the task
            let task_enqueuer = TaskEnqueuer::new(orchestration_system.database_pool.clone());
            let enqueue_request = EnqueueRequest::new(task)
                .with_reason("Task initialized and ready for processing")
                .with_priority(EnqueuePriority::Normal);

            task_enqueuer.enqueue(enqueue_request).await
                .map_err(|e| format!("Task enqueuing failed: {}", e))?;

            Ok::<serde_json::Value, String>(serde_json::json!({
                "status": "initialized",
                "task_id": initialization_result.task_id,
                "step_count": initialization_result.step_count,
                "step_mapping": initialization_result.step_mapping,
                "handler_config_name": initialization_result.handler_config_name
            }))
        });

        match result {
            Ok(success_data) => json_to_ruby_value(success_data),
            Err(error_msg) => {
                Err(Error::new(magnus::exception::runtime_error(), error_msg))
            }
        }
    }

    /// Handle task execution by task_id - delegates directly to WorkflowCoordinator
    pub fn handle(&self, task_id: i64) -> magnus::error::Result<Value> {
        let orchestration_system = &self.orchestration_system;

        // Delegate directly to the WorkflowCoordinator from our orchestration system
        let result = execute_async(async {
            let framework_integration = std::sync::Arc::new(BasicRubyFrameworkIntegration::new());
            let task_result = orchestration_system.workflow_coordinator.execute_task_workflow(task_id, framework_integration).await
                .map_err(|e| format!("Task execution failed: {}", e))?;

            // Convert TaskOrchestrationResult to JSON
            let result_json = match task_result {
                tasker_core::orchestration::types::TaskOrchestrationResult::Complete {
                    task_id,
                    steps_executed,
                    total_execution_time_ms
                } => serde_json::json!({
                    "status": "complete",
                    "task_id": task_id,
                    "steps_executed": steps_executed,
                    "total_execution_time_ms": total_execution_time_ms
                }),
                tasker_core::orchestration::types::TaskOrchestrationResult::Failed {
                    task_id,
                    error,
                    failed_steps
                } => serde_json::json!({
                    "status": "failed",
                    "task_id": task_id,
                    "error": error,
                    "failed_steps": failed_steps
                }),
                tasker_core::orchestration::types::TaskOrchestrationResult::InProgress {
                    task_id,
                    steps_executed,
                    next_poll_delay_ms
                } => serde_json::json!({
                    "status": "in_progress",
                    "task_id": task_id,
                    "steps_executed": steps_executed,
                    "next_poll_delay_ms": next_poll_delay_ms
                }),
                tasker_core::orchestration::types::TaskOrchestrationResult::Blocked {
                    task_id,
                    blocking_reason
                } => serde_json::json!({
                    "status": "blocked",
                    "task_id": task_id,
                    "blocking_reason": blocking_reason
                })
            };

            Ok::<serde_json::Value, String>(result_json)
        });

        match result {
            Ok(success_data) => json_to_ruby_value(success_data),
            Err(error_msg) => {
                Err(Error::new(magnus::exception::runtime_error(), error_msg))
            }
        }
    }

    /// Hook for Ruby developers to override - called before task execution
    pub fn initialize(&self, task_hash: Value) -> magnus::error::Result<Value> {
        // Default implementation - Ruby subclasses will override this
        let _ = task_hash; // Silence unused warning
        let ruby = Ruby::get().unwrap();
        Ok(ruby.qtrue().as_value())
    }

    /// Hook for Ruby developers to override - called before task execution
    pub fn before_execute(&self, task_hash: Value) -> magnus::error::Result<Value> {
        // Default implementation - Ruby subclasses will override this
        let _ = task_hash; // Silence unused warning
        let ruby = Ruby::get().unwrap();
        Ok(ruby.qtrue().as_value())
    }

    /// Hook for Ruby developers to override - called after task execution
    pub fn after_execute(&self, task_hash: Value) -> magnus::error::Result<Value> {
        // Default implementation - Ruby subclasses will override this
        let _ = task_hash; // Silence unused warning
        let ruby = Ruby::get().unwrap();
        Ok(ruby.qtrue().as_value())
    }

    /// Get handler capabilities - simple array of strings
    pub fn capabilities(&self) -> Vec<String> {
        vec![
            "initialize_task".to_string(),
            "handle".to_string(),
            "initialize".to_string(),
            "before_execute".to_string(),
            "after_execute".to_string(),
        ]
    }

    /// Check if handler supports a specific capability
    pub fn supports_capability(&self, capability: String) -> bool {
        self.capabilities().contains(&capability)
    }
}

/// Register the BaseTaskHandler class with Ruby
pub fn register_base_task_handler(ruby: &Ruby, module: &RModule) -> magnus::error::Result<()> {
    let class = module.define_class("BaseTaskHandler", ruby.class_object())?;

    class.define_singleton_method("new", function!(BaseTaskHandler::new_from_json, 1))?;
    class.define_method("initialize_task", method!(BaseTaskHandler::initialize_task, 1))?;
    class.define_method("handle", method!(BaseTaskHandler::handle, 1))?;
    class.define_method("initialize", method!(BaseTaskHandler::initialize, 1))?;
    class.define_method("before_execute", method!(BaseTaskHandler::before_execute, 1))?;
    class.define_method("after_execute", method!(BaseTaskHandler::after_execute, 1))?;
    class.define_method("capabilities", method!(BaseTaskHandler::capabilities, 0))?;
    class.define_method("supports_capability?", method!(BaseTaskHandler::supports_capability, 1))?;

    Ok(())
}

/// Basic Ruby framework integration for task execution
struct BasicRubyFrameworkIntegration;

impl BasicRubyFrameworkIntegration {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl tasker_core::orchestration::types::FrameworkIntegration for BasicRubyFrameworkIntegration {
    async fn execute_single_step(
        &self,
        step: &tasker_core::orchestration::types::ViableStep,
        _task_context: &tasker_core::orchestration::types::TaskContext,
    ) -> Result<tasker_core::orchestration::types::StepResult, tasker_core::orchestration::errors::OrchestrationError> {
        // Basic implementation - in production this would call back to Ruby handlers
        Ok(tasker_core::orchestration::types::StepResult {
            step_id: step.step_id,
            status: tasker_core::orchestration::types::StepStatus::Completed,
            output: serde_json::json!({"message": "Basic Ruby framework execution"}),
            execution_duration: std::time::Duration::from_millis(100),
            error_message: None,
            retry_after: None,
            error_code: None,
            error_context: None,
        })
    }

    fn framework_name(&self) -> &'static str {
        "BasicRubyFramework"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<tasker_core::orchestration::types::TaskContext, tasker_core::orchestration::errors::OrchestrationError> {
        Ok(tasker_core::orchestration::types::TaskContext {
            task_id,
            data: serde_json::json!({}),
            metadata: std::collections::HashMap::new(),
        })
    }

    async fn enqueue_task(&self, _task_id: i64, _delay: Option<std::time::Duration>) -> Result<(), tasker_core::orchestration::errors::OrchestrationError> {
        // Basic implementation - no-op
        Ok(())
    }

    async fn mark_task_failed(&self, _task_id: i64, _error: &str) -> Result<(), tasker_core::orchestration::errors::OrchestrationError> {
        // Basic implementation - no-op
        Ok(())
    }

    async fn update_step_state(&self, _step_id: i64, _state: &str, _result: Option<&serde_json::Value>) -> Result<(), tasker_core::orchestration::errors::OrchestrationError> {
        // Basic implementation - no-op
        Ok(())
    }
}
