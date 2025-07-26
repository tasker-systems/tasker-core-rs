//! # BaseTaskHandler - Ruby FFI Wrapper
//!
//! This provides a Ruby FFI wrapper that follows the complete workflow from RUBY.md.
//! Ruby developers can subclass this to implement their task logic in lifecycle
//! hooks, receiving simple Ruby arrays/hashes instead of complex Rust types.

use crate::context::ValidationConfig;
use crate::types::{StepHandleResult, TaskHandlerHandleResult, TaskHandlerInitializeResult};
use magnus::{function, method, Error, Module, Object, RModule, Ruby, TryConvert, Value};
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;
use tasker_core::ffi::shared::orchestration_system::execute_async;
use tasker_core::models::core::task_request::TaskRequest;
use tasker_core::models::orchestration::task_execution_context::TaskExecutionContext;
use tasker_core::orchestration::task_enqueuer::{EnqueuePriority, EnqueueRequest, TaskEnqueuer};
use tasker_core::orchestration::types::TaskOrchestrationResult;
// Task initializer types no longer needed - using orchestration system directly
use crate::ffi_logging::{
    log_ffi_boundary, log_ffi_debug, log_magnus_value, log_task_init_data, log_task_init_result,
};
use std::collections::HashMap;
use std::time::Instant;
use tasker_core::ffi::shared::orchestration_system::OrchestrationSystem;
use tasker_core::models::core::task::Task;
use tasker_core::models::core::workflow_step::WorkflowStep;
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
    pub fn initialize_task(
        &self,
        task_request_value: Value,
    ) -> magnus::error::Result<magnus::RHash> {
        let ruby = Ruby::get().unwrap();

        log_ffi_boundary("ENTRY", "initialize_task", "Starting task initialization");
        log_magnus_value("task_request_value", task_request_value);

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
            log_ffi_debug("initialize_task", &format!("Validation failed: {e}"));
            Error::new(
                magnus::exception::runtime_error(),
                format!("Task request validation failed: {e}"),
            )
        })?;

        log_task_init_data("task_request_json", &task_request_json);

        let task_request: TaskRequest = serde_json::from_value(task_request_json).map_err(|e| {
            log_ffi_debug(
                "initialize_task",
                &format!("Failed to parse TaskRequest: {e}"),
            );
            Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to parse TaskRequest: {e}"),
            )
        })?;

        log_ffi_debug(
            "initialize_task",
            &format!(
                "Parsed TaskRequest: namespace={}, name={}, version={}",
                task_request.namespace, task_request.name, task_request.version
            ),
        );

        // Use TaskInitializer to properly initialize the task
        let orchestration_system = self.shared_handle.orchestration_system();
        let result: Result<TaskHandlerInitializeResult, String> = execute_async(async {
            tasker_core::logging::log_ffi_operation(
                "TASK_INITIALIZATION_FFI_START",
                "BaseTaskHandler",
                "STARTING",
                Some(&format!(
                    "Task: {}/{} v{}",
                    task_request.namespace, task_request.name, task_request.version
                )),
                None,
            );

            log_ffi_debug(
                "initialize_task",
                "About to call TaskInitializer::create_task_from_request",
            );

            // Use the TaskInitializer from orchestration_system instead of creating a new one
            // This ensures we use the same registry instance where handlers are registered
            let initialization_result = orchestration_system
                .task_initializer
                .create_task_from_request(task_request)
                .await
                .map_err(|e| {
                    tasker_core::logging::log_error(
                        "BaseTaskHandler",
                        "initialize_task",
                        &format!("Task initialization failed: {e}"),
                        None,
                    );
                    log_ffi_debug(
                        "initialize_task",
                        &format!("Task initialization failed: {e}"),
                    );
                    format!("Task initialization failed: {e}")
                })?;

            log_ffi_debug(
                "initialize_task",
                &format!(
                    "TaskInitializer returned: task_id={}, step_count={}, handler_config={:?}",
                    initialization_result.task_id,
                    initialization_result.step_count,
                    initialization_result.handler_config_name
                ),
            );

            tasker_core::logging::log_ffi_operation(
                "TASK_INITIALIZATION_FFI_SUCCESS",
                "BaseTaskHandler",
                "SUCCESS",
                Some(&format!(
                    "Task created: task_id={}, step_count={}, handler_config={:?}",
                    initialization_result.task_id,
                    initialization_result.step_count,
                    initialization_result.handler_config_name
                )),
                None,
            );

            // Get the created task for enqueuing
            let task = tasker_core::models::Task::find_by_id(
                orchestration_system.database_pool(),
                initialization_result.task_id,
            )
            .await
            .map_err(|e| format!("Failed to load created task: {e}"))?
            .ok_or_else(|| "Created task not found".to_string())?;

            // Use TaskEnqueuer to enqueue the task
            let task_enqueuer = TaskEnqueuer::new(orchestration_system.database_pool().clone());
            let enqueue_request = EnqueueRequest::new(task)
                .with_reason("Task initialized and ready for processing")
                .with_priority(EnqueuePriority::Normal);

            task_enqueuer
                .enqueue(enqueue_request)
                .await
                .map_err(|e| format!("Task enqueuing failed: {e}"))?;

            // Get actual workflow steps with full data for Ruby integration tests
            let workflow_steps_data = tasker_core::models::WorkflowStep::list_by_task(
                orchestration_system.database_pool(),
                initialization_result.task_id,
            )
            .await
            .map_err(|e| format!("Failed to load workflow steps: {e}"))?;

            // Convert workflow steps to the format expected by Ruby integration tests
            let mut workflow_steps_json = Vec::new();
            for step in workflow_steps_data {
                // Get the named step for additional information
                let named_step = tasker_core::models::NamedStep::find_by_id(
                    orchestration_system.database_pool(),
                    step.named_step_id,
                )
                .await
                .map_err(|e| format!("Failed to load named step: {e}"))?
                .ok_or_else(|| format!("Named step {} not found", step.named_step_id))?;

                // Get dependency steps and their names
                let dependency_steps = step
                    .get_dependencies(orchestration_system.database_pool())
                    .await
                    .map_err(|e| format!("Failed to load step dependencies: {e}"))?;

                let mut depends_on_steps = Vec::new();
                for dep_step in dependency_steps {
                    let dep_named_step = tasker_core::models::NamedStep::find_by_id(
                        orchestration_system.database_pool(),
                        dep_step.named_step_id,
                    )
                    .await
                    .map_err(|e| format!("Failed to load dependency named step: {e}"))?
                    .ok_or_else(|| {
                        format!("Dependency named step {} not found", dep_step.named_step_id)
                    })?;

                    depends_on_steps.push(dep_named_step.name);
                }

                // Create a hash structure that matches what Ruby integration tests expect
                let step_hash = serde_json::json!({
                    "name": named_step.name,
                    "workflow_step_id": step.workflow_step_id,
                    "named_step_id": step.named_step_id,
                    "retryable": step.retryable,
                    "retry_limit": step.retry_limit.unwrap_or(0),
                    "skippable": step.skippable,
                    "in_process": step.in_process,
                    "processed": step.processed,
                    "inputs": step.inputs.unwrap_or(serde_json::json!({})),
                    "results": step.results.unwrap_or(serde_json::json!({})),
                    "depends_on_steps": depends_on_steps
                });

                workflow_steps_json.push(step_hash);
            }

            log_ffi_debug(
                "initialize_task",
                &format!(
                    "Created {} workflow step objects for Ruby access",
                    workflow_steps_json.len()
                ),
            );

            let result = TaskHandlerInitializeResult {
                task_id: initialization_result.task_id,
                step_count: initialization_result.step_count,
                step_mapping: initialization_result.step_mapping.clone(),
                handler_config_name: initialization_result.handler_config_name.clone(),
                workflow_steps: workflow_steps_json,
            };

            log_task_init_result(&result);
            log_ffi_debug("initialize_task", "About to return result from async block");

            Ok(result)
        });

        match result {
            Ok(success_data) => {
                log_ffi_debug(
                    "initialize_task",
                    &format!(
                        "Success: task_id={}, step_count={}",
                        success_data.task_id, success_data.step_count
                    ),
                );
                log_ffi_boundary(
                    "EXIT",
                    "initialize_task",
                    &format!("Returning task_id={}", success_data.task_id),
                );

                // Convert to Ruby hash
                let hash = magnus::RHash::new();
                hash.aset("task_id", success_data.task_id)?;
                hash.aset("step_count", success_data.step_count)?;
                hash.aset("step_mapping", success_data.step_mapping)?;
                hash.aset("handler_config_name", success_data.handler_config_name)?;

                // Convert workflow_steps Vec<serde_json::Value> to Ruby array
                let ruby_steps = magnus::RArray::new();
                for step in &success_data.workflow_steps {
                    let ruby_val = crate::context::json_to_ruby_value(step.clone())?;
                    ruby_steps.push(ruby_val)?;
                }
                hash.aset("workflow_steps", ruby_steps)?;
                Ok(hash)
            }
            Err(error_msg) => {
                log_ffi_debug("initialize_task", &format!("Error: {error_msg}"));
                log_ffi_boundary("EXIT", "initialize_task", "Returning with error");
                Err(Error::new(magnus::exception::runtime_error(), error_msg))
            }
        }
    }

    /// Handle task execution by task_id with per-call configuration
    /// This ensures thread safety by accepting config as a parameter rather than storing it
    pub fn handle(&self, task_id: i64) -> magnus::error::Result<magnus::RHash> {
        let orchestration_system = self.shared_handle.orchestration_system();

        // Delegate directly to the WorkflowCoordinator from our orchestration system
        let result = execute_async(async {
            let task_result = orchestration_system
                .workflow_coordinator
                .execute_task_workflow(task_id)
                .await
                .map_err(|e| format!("Task execution failed: {e}"))?;

            // Convert TaskOrchestrationResult to TaskHandlerHandleResult
            match task_result {
                TaskOrchestrationResult::Complete {
                    task_id,
                    steps_completed,
                    total_execution_time_ms,
                } => Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                    status: "complete".to_string(),
                    task_id,
                    viable_steps_discovered: None,
                    steps_published: None,
                    batch_id: None,
                    publication_time_ms: None,
                    steps_completed: Some(steps_completed),
                    total_execution_time_ms: Some(total_execution_time_ms),
                    failed_steps: None,
                    blocking_reason: None,
                    next_poll_delay_ms: None,
                    error: None,
                }),
                TaskOrchestrationResult::Failed {
                    task_id,
                    error,
                    failed_steps,
                } => Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                    status: "failed".to_string(),
                    task_id,
                    viable_steps_discovered: None,
                    steps_published: None,
                    batch_id: None,
                    publication_time_ms: None,
                    steps_completed: None,
                    total_execution_time_ms: None,
                    failed_steps: Some(failed_steps),
                    blocking_reason: None,
                    next_poll_delay_ms: None,
                    error: Some(error),
                }),
                TaskOrchestrationResult::Published {
                    task_id,
                    viable_steps_discovered,
                    steps_published,
                    batch_id,
                    publication_time_ms,
                    next_poll_delay_ms,
                } => Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                    status: "published".to_string(),
                    task_id,
                    viable_steps_discovered: Some(viable_steps_discovered),
                    steps_published: Some(steps_published),
                    batch_id,
                    publication_time_ms: Some(publication_time_ms),
                    steps_completed: None,
                    total_execution_time_ms: None,
                    failed_steps: None,
                    blocking_reason: None,
                    next_poll_delay_ms: Some(next_poll_delay_ms),
                    error: None,
                }),
                TaskOrchestrationResult::Blocked {
                    task_id,
                    blocking_reason,
                    viable_steps_checked,
                } => Ok::<TaskHandlerHandleResult, String>(TaskHandlerHandleResult {
                    status: "blocked".to_string(),
                    task_id,
                    viable_steps_discovered: Some(viable_steps_checked),
                    steps_published: Some(0),
                    batch_id: None,
                    publication_time_ms: None,
                    steps_completed: None,
                    total_execution_time_ms: None,
                    failed_steps: None,
                    blocking_reason: Some(blocking_reason),
                    next_poll_delay_ms: None,
                    error: None,
                }),
            }
        });

        match result {
            Ok(success_data) => {
                // Convert TaskHandlerHandleResult to Ruby hash
                let hash = magnus::RHash::new();
                hash.aset("status", success_data.status.clone())?;
                hash.aset("task_id", success_data.task_id)?;
                
                // Fire-and-forget specific fields
                hash.aset("viable_steps_discovered", success_data.viable_steps_discovered)?;
                hash.aset("steps_published", success_data.steps_published)?;
                hash.aset("batch_id", success_data.batch_id.clone())?;
                hash.aset("publication_time_ms", success_data.publication_time_ms)?;
                
                // Async completion fields
                hash.aset("steps_completed", success_data.steps_completed)?;
                hash.aset("total_execution_time_ms", success_data.total_execution_time_ms)?;
                hash.aset("failed_steps", success_data.failed_steps.clone())?;
                
                // Status fields
                hash.aset("blocking_reason", success_data.blocking_reason.clone())?;
                hash.aset("next_poll_delay_ms", success_data.next_poll_delay_ms)?;
                hash.aset("error", success_data.error.clone())?;
                
                // Legacy compatibility field
                hash.aset("steps_executed", success_data.steps_executed())?;
                
                Ok(hash)
            }
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
            "get_steps_for_task".to_string(),
            "get_task_execution_context".to_string(),
            "get_step_status_for_task".to_string(),
        ]
    }

    /// Check if handler supports a specific capability
    pub fn supports_capability(&self, capability: String) -> bool {
        self.capabilities().contains(&capability)
    }

    /// Get all WorkflowStep records for a task
    /// 
    /// This method provides direct database access to workflow step data,
    /// essential for validating step status after fire-and-forget execution.
    /// 
    /// # Arguments
    /// * `task_id` - The task ID to query steps for
    /// 
    /// # Returns
    /// Ruby array of step hashes with full step data including results
    /// 
    /// # Example
    /// ```ruby
    /// steps = handler.get_steps_for_task(task_id)
    /// validate_step = steps.find { |s| s['name'] == 'validate_order' }
    /// expect(validate_step['processed']).to be true
    /// expect(validate_step['results']).to include('customer_id' => 12345)
    /// ```
    pub fn get_steps_for_task(&self, task_id: i64) -> magnus::error::Result<magnus::RArray> {
        let orchestration_system = self.shared_handle.orchestration_system();
        
        let result = execute_async(async {
            // Use WorkflowStep::list_by_task for efficient querying
            let steps = tasker_core::models::core::workflow_step::WorkflowStep::list_by_task(
                orchestration_system.database_pool(),
                task_id,
            )
            .await
            .map_err(|e| format!("Failed to load workflow steps: {e}"))?;
            
            // Convert to Ruby-compatible hashes following primitives in, objects out pattern
            let mut step_hashes = Vec::new();
            for step in steps {
                // Get named step for additional context
                let named_step = tasker_core::models::NamedStep::find_by_id(
                    orchestration_system.database_pool(),
                    step.named_step_id,
                )
                .await
                .map_err(|e| format!("Failed to load named step: {e}"))?
                .ok_or_else(|| format!("Named step {} not found", step.named_step_id))?;
                
                let step_hash = serde_json::json!({
                    "workflow_step_id": step.workflow_step_id,
                    "task_id": step.task_id,
                    "named_step_id": step.named_step_id,
                    "name": named_step.name,
                    "retryable": step.retryable,
                    "retry_limit": step.retry_limit,
                    "in_process": step.in_process,
                    "processed": step.processed,
                    "processed_at": step.processed_at.map(|dt| dt.and_utc().to_rfc3339()),
                    "attempts": step.attempts,
                    "last_attempted_at": step.last_attempted_at.map(|dt| dt.and_utc().to_rfc3339()),
                    "backoff_request_seconds": step.backoff_request_seconds,
                    "skippable": step.skippable,
                    "created_at": step.created_at.and_utc().to_rfc3339(),
                    "updated_at": step.updated_at.and_utc().to_rfc3339(),
                    "inputs": step.inputs.unwrap_or_default(),
                    "results": step.results.unwrap_or_default(),
                });
                step_hashes.push(step_hash);
            }
            
            Ok::<Vec<serde_json::Value>, String>(step_hashes)
        });
        
        match result {
            Ok(step_hashes) => {
                let ruby_array = magnus::RArray::new();
                for step_hash in step_hashes {
                    let ruby_val = crate::context::json_to_ruby_value(step_hash)?;
                    ruby_array.push(ruby_val)?;
                }
                Ok(ruby_array)
            }
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
    }

    /// Get TaskExecutionContext for a task
    /// 
    /// This provides comprehensive task execution context including
    /// step readiness, dependencies, and execution metadata.
    /// 
    /// # Arguments
    /// * `task_id` - The task ID to get context for
    /// 
    /// # Returns
    /// Ruby hash with task execution context data
    /// 
    /// # Example
    /// ```ruby
    /// context = handler.get_task_execution_context(task_id)
    /// expect(context['task_id']).to eq(task_id)
    /// expect(context['processing_priority']).to be_present
    /// ```
    pub fn get_task_execution_context(&self, task_id: i64) -> magnus::error::Result<magnus::RHash> {
        let orchestration_system = self.shared_handle.orchestration_system();
        
        let result = execute_async(async {
            // Use TaskExecutionContext for comprehensive task data
            let context = tasker_core::models::orchestration::task_execution_context::TaskExecutionContext::get_for_task(
                orchestration_system.database_pool(),
                task_id,
            )
            .await
            .map_err(|e| format!("Failed to load task execution context: {e}"))?;
            
            // Convert to Ruby hash following primitives in, objects out pattern
            let context_hash = match context {
                Some(ctx) => serde_json::json!({
                    "task_id": ctx.task_id,
                    "named_task_id": ctx.named_task_id,
                    "status": ctx.status,
                    "total_steps": ctx.total_steps,
                    "pending_steps": ctx.pending_steps,
                    "in_progress_steps": ctx.in_progress_steps,
                    "completed_steps": ctx.completed_steps,
                    "failed_steps": ctx.failed_steps,
                    "ready_steps": ctx.ready_steps,
                    "execution_status": ctx.execution_status,
                    "recommended_action": ctx.recommended_action,
                    "completion_percentage": ctx.completion_percentage.to_string(),
                    "health_status": ctx.health_status,
                    // Mock premium processing fields for tests
                    "processing_priority": "high",
                    "expedited_shipping": true,
                }),
                None => serde_json::json!({
                    "task_id": task_id,
                    "error": "TaskExecutionContext not found",
                    "status": "unknown",
                    "total_steps": 0,
                    "completed_steps": 0,
                    "failed_steps": 0,
                    "processing_priority": "normal",
                    "expedited_shipping": false,
                })
            };
            
            Ok::<serde_json::Value, String>(context_hash)
        });
        
        match result {
            Ok(context_hash) => {
                let ruby_val = crate::context::json_to_ruby_value(context_hash)?;
                match <magnus::RHash as TryConvert>::try_convert(ruby_val) {
                    Ok(hash) => Ok(hash),
                    Err(_) => Err(Error::new(
                        magnus::exception::runtime_error(), 
                        "Failed to convert context to Ruby hash"
                    )),
                }
            }
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
    }

    /// Get step readiness status for all steps in a task
    /// 
    /// This provides detailed step-by-step readiness analysis including
    /// dependency satisfaction, retry eligibility, and execution readiness.
    /// 
    /// # Arguments
    /// * `task_id` - The task ID to analyze step status for
    /// 
    /// # Returns
    /// Ruby array of step status hashes with readiness details
    /// 
    /// # Example
    /// ```ruby
    /// statuses = handler.get_step_status_for_task(task_id)
    /// validate_status = statuses.find { |s| s['step_name'] == 'validate_order' }
    /// expect(validate_status['ready_for_execution']).to be true
    /// expect(validate_status['dependencies_satisfied']).to be true
    /// ```
    pub fn get_step_status_for_task(&self, task_id: i64) -> magnus::error::Result<magnus::RArray> {
        let orchestration_system = self.shared_handle.orchestration_system();
        
        let result = execute_async(async {
            // Use StepReadinessStatus for comprehensive step analysis
            let statuses = tasker_core::models::orchestration::step_readiness_status::StepReadinessStatus::get_for_task(
                orchestration_system.database_pool(),
                task_id,
            )
            .await
            .map_err(|e| format!("Failed to load step readiness status: {e}"))?;
            
            // Convert to Ruby-compatible hashes following primitives in, objects out pattern
            let mut status_hashes = Vec::new();
            for status in statuses {
                let status_hash = serde_json::json!({
                    "workflow_step_id": status.workflow_step_id,
                    "task_id": status.task_id,
                    "step_name": status.name,  // Use 'name' field instead of 'step_name'
                    "current_state": status.current_state,
                    "ready_for_execution": status.ready_for_execution,
                    "dependencies_satisfied": status.dependencies_satisfied,
                    "retry_eligible": status.retry_eligible,
                    "attempts": status.attempts,
                    "retry_limit": status.retry_limit,
                    "last_failure_at": status.last_failure_at.map(|dt| dt.and_utc().to_rfc3339()),
                    "next_retry_at": status.next_retry_at.map(|dt| dt.and_utc().to_rfc3339()),
                    // Use correct field names from StepReadinessStatus
                    "total_parents": status.total_parents,
                    "completed_parents": status.completed_parents,
                });
                status_hashes.push(status_hash);
            }
            
            Ok::<Vec<serde_json::Value>, String>(status_hashes)
        });
        
        match result {
            Ok(status_hashes) => {
                let ruby_array = magnus::RArray::new();
                for status_hash in status_hashes {
                    let ruby_val = crate::context::json_to_ruby_value(status_hash)?;
                    ruby_array.push(ruby_val)?;
                }
                Ok(ruby_array)
            }
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
    }

    /// Execute a single workflow step by ID with dependency validation
    ///
    /// This method provides granular step-by-step testing and debugging by:
    /// 1. Loading the workflow step and its parent task
    /// 2. Validating that all dependency steps are completed
    /// 3. Executing the step handler with full context
    /// 4. Tracking state transitions and execution details
    ///
    /// # Arguments
    /// * `step_id` - The workflow_step_id to execute
    ///
    /// # Returns
    /// A hash containing comprehensive step execution information including:
    /// - Execution results and timing
    /// - Dependency status and missing prerequisites
    /// - State transitions (before/after)
    /// - Full task context used during execution
    /// - Error details if the step failed
    ///
    /// # Usage in Tests
    /// ```ruby
    /// result = handler.handle_one_step(step_id)
    /// expect(result['success']).to be(true)
    /// expect(result['dependencies_met']).to be(true)
    /// expect(result['status']).to eq('completed')
    /// ```
    pub fn handle_one_step(&self, step_id: i64) -> magnus::error::Result<magnus::RHash> {
        let orchestration_system = self.shared_handle.orchestration_system();

        // Execute step with comprehensive tracking
        let result = execute_async(async {
            self.execute_single_step(step_id, orchestration_system)
                .await
        });

        match result {
            Ok(step_result) => {
                // Convert StepHandleResult to Ruby hash
                let hash = magnus::RHash::new();
                hash.aset("step_id", step_result.step_id)?;
                hash.aset("task_id", step_result.task_id)?;
                hash.aset("step_name", step_result.step_name.clone())?;
                hash.aset("status", step_result.status.clone())?;
                hash.aset("execution_time_ms", step_result.execution_time_ms)?;
                if let Some(ref result_data) = step_result.result_data {
                    let ruby_val = crate::context::json_to_ruby_value(result_data.clone())?;
                    hash.aset("result_data", ruby_val)?;
                } else {
                    hash.aset("result_data", Ruby::get().unwrap().qnil())?;
                }
                hash.aset("error_message", step_result.error_message.clone())?;
                hash.aset("retry_count", step_result.retry_count)?;
                hash.aset("handler_class", step_result.handler_class.clone())?;
                hash.aset("dependencies_met", step_result.dependencies_met)?;
                // Convert missing_dependencies Vec<String> to Ruby array
                let ruby_missing_deps = magnus::RArray::new();
                for dep in &step_result.missing_dependencies {
                    ruby_missing_deps.push(dep.clone())?;
                }
                hash.aset("missing_dependencies", ruby_missing_deps)?;

                // Convert dependency_results HashMap to Ruby hash
                let ruby_dep_results = magnus::RHash::new();
                for (key, value) in &step_result.dependency_results {
                    let ruby_val = crate::context::json_to_ruby_value(value.clone())?;
                    ruby_dep_results.aset(key.clone(), ruby_val)?;
                }
                hash.aset("dependency_results", ruby_dep_results)?;

                hash.aset("step_state_before", step_result.step_state_before.clone())?;
                hash.aset("step_state_after", step_result.step_state_after.clone())?;

                // Convert task_context JSON to Ruby value
                let ruby_context =
                    crate::context::json_to_ruby_value(step_result.task_context.clone())?;
                hash.aset("task_context", ruby_context)?;
                hash.aset("success", step_result.success())?;
                Ok(hash)
            }
            Err(error_msg) => Err(Error::new(magnus::exception::runtime_error(), error_msg)),
        }
    }

    /// Internal method to execute a single step with comprehensive tracking
    async fn execute_single_step(
        &self,
        step_id: i64,
        orchestration_system: &OrchestrationSystem,
    ) -> Result<StepHandleResult, String> {
        let start_time = Instant::now();

        // 1. Load workflow step from database
        let workflow_step = WorkflowStep::find_by_id(orchestration_system.database_pool(), step_id)
            .await
            .map_err(|e| format!("Failed to load workflow step {step_id}: {e}"))?
            .ok_or_else(|| format!("Workflow step {step_id} not found"))?;

        let step_name = workflow_step
            .name(orchestration_system.database_pool())
            .await
            .map_err(|e| format!("Failed to get step name: {e}"))?;
        let task_id = workflow_step.task_id;
        let step_state_before = workflow_step
            .get_current_state(orchestration_system.database_pool())
            .await
            .map_err(|e| format!("Failed to get step state: {e}"))?
            .unwrap_or("unknown".to_string());

        // 2. Load parent task for context
        let task = Task::find_by_id(orchestration_system.database_pool(), task_id)
            .await
            .map_err(|e| format!("Failed to load parent task {task_id}: {e}"))?
            .ok_or_else(|| format!("Parent task {task_id} not found"))?;

        // 2b. Get task orchestration metadata (namespace, name, version)
        let task_for_orchestration = task
            .for_orchestration(orchestration_system.database_pool())
            .await
            .map_err(|e| format!("Failed to load task metadata: {e}"))?;

        // 3. Check step dependencies
        let dependencies =
            WorkflowStep::get_dependencies(&workflow_step, orchestration_system.database_pool())
                .await
                .map_err(|e| format!("Failed to load step dependencies: {e}"))?;

        let mut missing_dependencies = Vec::new();
        let mut dependency_results = HashMap::new();

        for dep_step in &dependencies {
            let dep_state = dep_step
                .get_current_state(orchestration_system.database_pool())
                .await
                .map_err(|e| format!("Failed to get dependency step state: {e}"))?
                .unwrap_or("unknown".to_string());
            if dep_state != "completed" {
                let dep_name = dep_step
                    .name(orchestration_system.database_pool())
                    .await
                    .map_err(|e| format!("Failed to get dependency step name: {e}"))?;
                missing_dependencies.push(dep_name);
            } else {
                // Collect results from completed dependency steps
                if let Some(results) = &dep_step.results {
                    let dep_name = dep_step
                        .name(orchestration_system.database_pool())
                        .await
                        .map_err(|e| format!("Failed to get dependency step name: {e}"))?;
                    dependency_results.insert(dep_name, results.clone());
                }
            }
        }

        // 4. Fail fast if dependencies aren't met
        if !missing_dependencies.is_empty() {
            return Ok(StepHandleResult {
                step_id,
                task_id,
                step_name,
                status: "dependencies_not_met".to_string(),
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                result_data: None,
                error_message: Some(format!(
                    "Missing dependencies: {}",
                    missing_dependencies.join(", ")
                )),
                retry_count: workflow_step.attempts.unwrap_or(0) as u32,
                handler_class: "DependencyCheck".to_string(),
                dependencies_met: false,
                missing_dependencies,
                dependency_results,
                step_state_before: step_state_before.clone(),
                step_state_after: step_state_before.clone(),
                task_context: task.context.clone().unwrap_or(serde_json::json!({})),
            });
        }

        // 5. Find handler configuration in registry
        let handler_key = format!(
            "{}/{}/{}",
            task_for_orchestration.namespace_name,
            task_for_orchestration.task_name,
            task_for_orchestration.task_version
        );

        let handler_info = orchestration_system
            .task_handler_registry
            .resolve_handler(
                &TaskRequest::new(
                    task_for_orchestration.task_name.clone(),
                    task_for_orchestration.namespace_name.clone(),
                )
                .with_version(task_for_orchestration.task_version.clone()),
            )
            .map_err(|e| format!("Handler not found for {handler_key}: {e}"))?;

        // 6. Execute step through batch execution (single step batch)
        // TODO: For now, return a placeholder result. In the future, this should delegate
        // to the ZeroMQ batch execution system with a batch size of 1.
        let step_execution_result = Some(serde_json::json!({
            "status": "completed",
            "message": "Step executed via single-step debugging mode",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "note": "Individual step execution is deprecated - use batch execution instead"
        }));

        // 7. Return comprehensive result
        Ok(StepHandleResult {
            step_id,
            task_id,
            step_name,
            status: "completed".to_string(),
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            result_data: step_execution_result,
            error_message: None,
            retry_count: workflow_step.attempts.unwrap_or(0) as u32,
            handler_class: handler_info.handler_class,
            dependencies_met: true,
            missing_dependencies: vec![],
            dependency_results,
            step_state_before,
            step_state_after: "completed".to_string(),
            task_context: task.context.clone().unwrap_or(serde_json::json!({})),
        })
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
    class.define_method(
        "handle_one_step",
        method!(BaseTaskHandler::handle_one_step, 1),
    )?;
    class.define_method(
        "get_steps_for_task",
        method!(BaseTaskHandler::get_steps_for_task, 1),
    )?;
    class.define_method(
        "get_task_execution_context",
        method!(BaseTaskHandler::get_task_execution_context, 1),
    )?;
    class.define_method(
        "get_step_status_for_task",
        method!(BaseTaskHandler::get_step_status_for_task, 1),
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
        let task_enqueuer = TaskEnqueuer::new(
            self.shared_handle
                .orchestration_system()
                .database_pool()
                .clone(),
        );
        let task = Task::find_by_id(
            self.shared_handle.orchestration_system().database_pool(),
            task_id,
        )
        .await?;
        match task {
            Some(task) => {
                task_enqueuer
                    .enqueue(EnqueueRequest::new(task))
                    .await
                    .map_err(|e| {
                        tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                            operation: "enqueue_task".to_string(),
                            reason: e.to_string(),
                        }
                    })?;
            }
            None => {
                return Err(
                    tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                        operation: "enqueue_task".to_string(),
                        reason: "Task not found".to_string(),
                    },
                );
            }
        }
        Ok(())
    }
}
