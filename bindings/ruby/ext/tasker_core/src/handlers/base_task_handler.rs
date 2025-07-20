//! # BaseTaskHandler - Ruby FFI Wrapper
//!
//! This provides a Ruby FFI wrapper that follows the complete workflow from RUBY.md.
//! Ruby developers can subclass this to implement their task logic in lifecycle
//! hooks, receiving simple Ruby arrays/hashes instead of complex Rust types.

use crate::context::ValidationConfig;
use magnus::{function, Module, method, Error, RModule, Ruby, Value, Object};
use crate::types::{TaskHandlerInitializeResult, TaskHandlerHandleResult};
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;
use tasker_core::ffi::shared::orchestration_system::execute_async;
use tasker_core::models::core::task_request::TaskRequest;
use tasker_core::models::orchestration::task_execution_context::TaskExecutionContext;
use tasker_core::orchestration::types::TaskOrchestrationResult;
use tasker_core::orchestration::task_enqueuer::{EnqueuePriority, EnqueueRequest, TaskEnqueuer};
use tasker_core::orchestration::task_initializer::{TaskInitializationConfig, TaskInitializer};
use tasker_core::models::core::task::Task;
use tracing::debug;

/// Ruby wrapper for Rust BaseTaskHandler
#[magnus::wrap(class = "TaskerCore::BaseTaskHandler")]
pub struct BaseTaskHandler {
    /// Reference to the shared orchestration handle
    shared_handle: std::sync::Arc<SharedOrchestrationHandle>,
}

impl BaseTaskHandler {
    /// Create a new BaseTaskHandler with a TaskTemplate
    pub fn new() -> magnus::error::Result<Self> {
        // 🎯 SHARED ARCHITECTURE: Use the global shared handle
        // This provides access to the shared orchestration system with persistent Arc references
        debug!("🔧 BaseTaskHandler::new() - using shared orchestration handle");
        let shared_handle = SharedOrchestrationHandle::get_global();

        Ok(Self { shared_handle })
    }

    /// Initialize task from TaskRequest - this is the critical method from RUBY.md
    pub fn initialize_task(&self, task_request_value: Value) -> magnus::error::Result<TaskHandlerInitializeResult> {
        let ruby = Ruby::get().unwrap();

        // Use strict validation for task request data
        let validation_config = ValidationConfig {
            max_string_length: 1000, // Reasonable for task request fields
            max_array_length: 100,   // Moderate array sizes
            max_object_depth: 4,     // Task requests can be somewhat nested
            max_object_keys: 30,     // Reasonable for task request structure
            max_numeric_value: 1e12, // Large but reasonable for IDs and timestamps
            min_numeric_value: -1e12,
        };

        // Convert Ruby value to JSON then to TaskRequest with validation
        let task_request_json = crate::context::ruby_value_to_json_with_validation(
            task_request_value,
            &validation_config,
        )
        .map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Task request validation failed: {}", e),
            )
        })?;

        let task_request: TaskRequest = serde_json::from_value(task_request_json).map_err(|e| {
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to parse TaskRequest: {}", e),
            )
        })?;

        // Use TaskInitializer to properly initialize the task
        let orchestration_system = self.shared_handle.orchestration_system();
        let result: Result<TaskHandlerInitializeResult, String> = execute_async(async {
            let task_initializer = TaskInitializer::with_state_manager(
                orchestration_system.database_pool().clone(),
                TaskInitializationConfig::default(),
                orchestration_system.event_publisher.clone(),
            );

            let initialization_result = task_initializer
                .create_task_from_request(task_request)
                .await
                .map_err(|e| format!("Task initialization failed: {}", e))?;

            // Get the created task for enqueuing
            let task = tasker_core::models::Task::find_by_id(
                orchestration_system.database_pool(),
                initialization_result.task_id,
            )
            .await
            .map_err(|e| format!("Failed to load created task: {}", e))?
            .ok_or_else(|| "Created task not found".to_string())?;

            // Use TaskEnqueuer to enqueue the task
            let task_enqueuer = TaskEnqueuer::new(orchestration_system.database_pool().clone());
            let enqueue_request = EnqueueRequest::new(task)
                .with_reason("Task initialized and ready for processing")
                .with_priority(EnqueuePriority::Normal);

            task_enqueuer
                .enqueue(enqueue_request)
                .await
                .map_err(|e| format!("Task enqueuing failed: {}", e))?;

              Ok(TaskHandlerInitializeResult {
                task_id: initialization_result.task_id,
                step_count: initialization_result.step_count,
                step_mapping: initialization_result.step_mapping,
                handler_config_name: initialization_result.handler_config_name,
              })
        });

        match result {
            Ok(success_data) => Ok(success_data),
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
    }

    /// Handle task execution by task_id with per-call configuration
    /// This ensures thread safety by accepting config as a parameter rather than storing it
    pub fn handle(&self, task_id: i64) -> magnus::error::Result<TaskHandlerHandleResult> {
        let orchestration_system = self.shared_handle.orchestration_system();

        // Delegate directly to the WorkflowCoordinator from our orchestration system
        let result = execute_async(async {
            let framework_integration = std::sync::Arc::new(BasicRubyFrameworkIntegration::new());
            let task_result = orchestration_system
                .workflow_coordinator
                .execute_task_workflow(task_id, framework_integration)
                .await
                .map_err(|e| format!("Task execution failed: {}", e))?;

            // Convert TaskOrchestrationResult to TaskHandlerHandleResult
            match task_result {
                TaskOrchestrationResult::Complete { task_id, steps_executed, total_execution_time_ms } => {
                    Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                        status: "complete".to_string(),
                        task_id,
                        steps_executed: Some(steps_executed),
                        total_execution_time_ms: Some(total_execution_time_ms),
                        failed_steps: None,
                        blocking_reason: None,
                        next_poll_delay_ms: None,
                        error: None,
                    })
                },
                TaskOrchestrationResult::Failed { task_id, error, failed_steps } => {
                    Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                        status: "failed".to_string(),
                        task_id,
                        steps_executed: None,
                        total_execution_time_ms: None,
                        failed_steps: Some(failed_steps),
                        blocking_reason: None,
                        next_poll_delay_ms: None,
                        error: Some(error),
                    })
                },
                TaskOrchestrationResult::InProgress { task_id, steps_executed, next_poll_delay_ms } => {
                    Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                        status: "in_progress".to_string(),
                        task_id,
                        steps_executed: Some(steps_executed),
                        total_execution_time_ms: None,
                        failed_steps: None,
                        blocking_reason: None,
                        next_poll_delay_ms: Some(next_poll_delay_ms),
                        error: None,
                    })
                },
                TaskOrchestrationResult::Blocked { task_id, blocking_reason } => {
                    Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                        status: "blocked".to_string(),
                        task_id,
                        steps_executed: None,
                        total_execution_time_ms: None,
                        failed_steps: None,
                        blocking_reason: Some(blocking_reason),
                        next_poll_delay_ms: None,
                        error: None,
                    })
                },
            }
        });

        match result {
            Ok(success_data) => Ok(success_data),
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
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

    class.define_singleton_method("new", function!(BaseTaskHandler::new, 0))?;
    class.define_method(
        "initialize_task",
        method!(BaseTaskHandler::initialize_task, 1),
    )?;
    class.define_method("handle", method!(BaseTaskHandler::handle, 1))?;
    class.define_method("capabilities", method!(BaseTaskHandler::capabilities, 0))?;
    class.define_method(
        "supports_capability?",
        method!(BaseTaskHandler::supports_capability, 1),
    )?;

    Ok(())
}

/// Basic Ruby framework integration for task execution
struct BasicRubyFrameworkIntegration {
    shared_handle: std::sync::Arc<SharedOrchestrationHandle>,
}

impl BasicRubyFrameworkIntegration {
    fn new() -> Self {
        let shared_handle = SharedOrchestrationHandle::get_global();

        Self { shared_handle }
    }
}

#[async_trait::async_trait]
impl tasker_core::orchestration::types::FrameworkIntegration for BasicRubyFrameworkIntegration {
    fn framework_name(&self) -> &'static str {
        "BasicRubyFramework"
    }

    async fn get_task_context(
        &self,
        task_id: i64,
    ) -> Result<
        tasker_core::orchestration::types::TaskContext,
        tasker_core::orchestration::errors::OrchestrationError,
    > {
        let pool = self.shared_handle.orchestration_system().database_pool();
        let task_context = TaskExecutionContext::get_for_task(pool, task_id).await?;

        if let Some(task_context) = task_context {
            Ok(tasker_core::orchestration::types::TaskContext {
                task_id,
                data: serde_json::json!({}),
                metadata: std::collections::HashMap::new(),
            })
        } else {
            Err(
                tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                    operation: "get_task_context".to_string(),
                    reason: "Task context not found".to_string(),
                },
            )
        }
    }

    async fn enqueue_task(
        &self,
        task_id: i64,
        delay: Option<std::time::Duration>,
    ) -> Result<(), tasker_core::orchestration::errors::OrchestrationError> {
        let task_enqueuer = TaskEnqueuer::new(self.shared_handle.orchestration_system().database_pool().clone());
        let task = Task::find_by_id(self.shared_handle.orchestration_system().database_pool(), task_id).await?;
        match task {
            Some(task) => {
                task_enqueuer.enqueue(EnqueueRequest::new(task)).await.map_err(|e| {
                    tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                        operation: "enqueue_task".to_string(),
                        reason: e.to_string(),
                    }
                })?;
            }
            None => {
                return Err(tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                    operation: "enqueue_task".to_string(),
                    reason: "Task not found".to_string(),
                });
            }
        }
        Ok(())
    }
}
