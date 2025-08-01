//! # Task Initialization Handler
//!
//! Handles InitializeTask and TryTaskIfReady commands for the command-based
//! orchestration system. This replaces direct FFI calls with async command processing.

use crate::execution::command::{Command, CommandPayload, CommandSource, CommandType};
use crate::execution::CommandHandler;
use crate::ffi::shared::orchestration_system::OrchestrationSystem;
use crate::orchestration::TaskOrchestrationResult;
use crate::models::core::task_request::TaskRequest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn, error};

/// Handler for task initialization commands
#[derive(Clone)]
pub struct TaskInitializationHandler {
    orchestration_system: Arc<OrchestrationSystem>,
}

/// Result of task initialization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInitializationResult {
    pub task_id: i64,
    pub success: bool,
    pub step_count: usize,
    pub step_mapping: HashMap<String, i64>,
    pub workflow_steps: Vec<serde_json::Value>,
    pub error_message: Option<String>,
}

/// Information about task readiness for step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessInfo {
    pub task_id: i64,
    pub ready: bool,
    pub ready_steps_count: usize,
    pub total_steps: usize,
    pub batch_info: Option<BatchInfo>,
    pub error_message: Option<String>,
}

/// Batch execution information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInfo {
    pub batch_id: String,
    pub publication_time_ms: u64,
    pub next_poll_delay_ms: u64,
}

impl TaskInitializationHandler {
    /// Create new task initialization handler
    pub fn new(orchestration_system: Arc<OrchestrationSystem>) -> Self {
        Self {
            orchestration_system,
        }
    }

    /// Handle InitializeTask command - create a new task based on the request
    async fn handle_initialize_task(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, Box<dyn std::error::Error + Send + Sync>> {
        info!("🎯 TASK_INIT_HANDLER: Handling InitializeTask command");
        info!("🎯 TASK_INIT_HANDLER: Task request: namespace='{}', name='{}', version='{}'",
              task_request.namespace, task_request.name, task_request.version);
        debug!("🎯 TASK_INIT_HANDLER: Full task request: {:?}", task_request);

        info!("🎯 TASK_INIT_HANDLER: Calling orchestration_system.task_initializer.create_task_from_request");
        let task_init_result = self.orchestration_system.task_initializer.create_task_from_request(task_request).await?;

        info!("🎯 TASK_INIT_HANDLER: Task initializer returned: task_id={}, step_count={}, handler_config_name={:?}",
              task_init_result.task_id, task_init_result.step_count, task_init_result.handler_config_name);

        let task_id = task_init_result.task_id;
        let step_count = task_init_result.step_count;
        let step_mapping = task_init_result.step_mapping;

        // Load the actual workflow steps that were created
        let workflow_steps = self.load_workflow_steps_for_task(task_id).await?;

        info!("🎯 TASK_INIT_HANDLER: Loaded {} workflow steps for task {}", workflow_steps.len(), task_id);

        Ok(TaskInitializationResult {
            task_id,
            success: true,
            step_count,
            step_mapping,
            workflow_steps,
            error_message: None,
        })
    }

    /// Load workflow steps for a task and convert them to JSON values for the response
    async fn load_workflow_steps_for_task(
        &self,
        task_id: i64,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        use crate::models::core::workflow_step::WorkflowStep;

        // Load all workflow steps for the task
        let workflow_steps = WorkflowStep::list_by_task(&self.orchestration_system.database_pool(), task_id).await?;

        // Convert workflow steps to JSON values
        let workflow_steps_json: Vec<serde_json::Value> = workflow_steps
            .into_iter()
            .map(|step| {
                serde_json::json!({
                    "workflow_step_id": step.workflow_step_id,
                    "task_id": step.task_id,
                    "named_step_id": step.named_step_id,
                    "retryable": step.retryable,
                    "retry_limit": step.retry_limit,
                    "in_process": step.in_process,
                    "processed": step.processed,
                    "processed_at": step.processed_at,
                    "attempts": step.attempts,
                    "last_attempted_at": step.last_attempted_at,
                    "backoff_request_seconds": step.backoff_request_seconds,
                    "inputs": step.inputs,
                    "results": step.results,
                    "skippable": step.skippable,
                    "created_at": step.created_at,
                    "updated_at": step.updated_at,
                })
            })
            .collect();

        Ok(workflow_steps_json)
    }

    /// Handle TryTaskIfReady command - check if task has ready steps for execution
    async fn handle_try_task_if_ready(
        &self,
        task_id: i64,
    ) -> Result<TaskReadinessInfo, Box<dyn std::error::Error + Send + Sync>> {
        info!("Handling TryTaskIfReady command for task {}", task_id);

        let task_orchestration_result: TaskOrchestrationResult = self.orchestration_system.workflow_coordinator.execute_task_workflow(task_id).await?;

        match task_orchestration_result {
            TaskOrchestrationResult::Published { task_id, viable_steps_discovered, steps_published, batch_id, publication_time_ms, next_poll_delay_ms } => {
                Ok(TaskReadinessInfo {
                    task_id,
                    ready: true,
                    ready_steps_count: steps_published,
                    total_steps: viable_steps_discovered,
                    batch_info: Some(BatchInfo {
                        batch_id: batch_id.unwrap_or("unknown_batch".to_string()),
                        publication_time_ms,
                        next_poll_delay_ms,
                    }),
                    error_message: None,
                })
            }
            _ => {
                Ok(TaskReadinessInfo {
                    task_id,
                    ready: false,
                    ready_steps_count: 0,
                    total_steps: 0,
                    batch_info: None,
                    error_message: None,
                })
            }
        }
    }

    /// Create from orchestration system (for backward compatibility)
    pub fn from_orchestration_system(orchestration_system: Arc<OrchestrationSystem>) -> Self {
        Self::new(orchestration_system.clone())
    }
}

#[async_trait::async_trait]
impl CommandHandler for TaskInitializationHandler {
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        info!("🎯 TASK_INIT_HANDLER: handle_command called with type={:?}, id={}", command.command_type, command.command_id);
        debug!("TaskInitializationHandler received command: {:?}", command.command_type);

        let result = match &command.payload {
            CommandPayload::InitializeTask { task_request } => {
                info!("🎯 TASK_INIT_HANDLER: Matched InitializeTask payload, about to handle");

                // 🎯 PROPER ERROR HANDLING: Always return a response, never let errors break command flow
                let result = match self.handle_initialize_task(task_request.clone()).await {
                    Ok(result) => {
                        info!("🎯 TASK_INIT_HANDLER: handle_initialize_task succeeded: task_id={}, step_count={}", result.task_id, result.step_count);
                        result
                    }
                    Err(e) => {
                        let error_msg = format!("Task initialization failed: {}", e);
                        error!("🎯 TASK_INIT_HANDLER: {}", error_msg);

                        // Return proper error command type
                        let response = Command::new(
                            CommandType::Error,
                            CommandPayload::Error {
                                error_type: "TaskInitializationError".to_string(),
                                message: error_msg,
                                details: Some(HashMap::from([
                                    ("task_name".to_string(), serde_json::Value::String(task_request.name.clone())),
                                    ("namespace".to_string(), serde_json::Value::String(task_request.namespace.clone())),
                                    ("version".to_string(), serde_json::Value::String(task_request.version.clone())),
                                ])),
                                retryable: false, // Task initialization errors are typically not retryable
                            },
                            CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                        );

                        info!("🎯 TASK_INIT_HANDLER: Sending error response for task initialization failure");
                        return Ok(Some(response));
                    }
                };

                // Create success response command for successful initialization
                let response = Command::new(
                    CommandType::TaskInitialized,
                    CommandPayload::TaskInitialized {
                        task_id: result.task_id,
                        success: true, // Always true for success case
                        step_count: result.step_count,
                        workflow_steps: serde_json::to_value(result.workflow_steps).unwrap_or(serde_json::Value::Array(vec![])),
                        error_message: None, // No error message for success
                    },
                    CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                );

                info!("🎯 TASK_INIT_HANDLER: Created TaskInitialized response: task_id={}, step_count={}",
                      result.task_id, result.step_count);
                Some(response)
            }
            CommandPayload::TryTaskIfReady { task_id } => {
                // 🎯 PROPER ERROR HANDLING: Always return a response, never let errors break command flow
                let result = match self.handle_try_task_if_ready(*task_id).await {
                    Ok(result) => result,
                    Err(e) => {
                        let error_msg = format!("Task readiness check failed for task_id {}: {}", task_id, e);
                        warn!("🎯 TASK_INIT_HANDLER: {}", error_msg);
                        TaskReadinessInfo {
                            task_id: *task_id,
                            ready: false,
                            ready_steps_count: 0,
                            total_steps: 0,
                            batch_info: None,
                            error_message: Some(error_msg),
                        }
                    }
                };

                // Convert BatchInfo to TaskBatchInfo if available
                let batch_info = result.batch_info.map(|batch| {
                    crate::execution::command::TaskBatchInfo {
                        batch_id: batch.batch_id,
                        estimated_steps: result.ready_steps_count, // Use ready steps count as estimate
                        publication_time_ms: batch.publication_time_ms,
                        next_poll_delay_ms: batch.next_poll_delay_ms,
                    }
                });

                // Create response command using the result directly
                Some(Command::new(
                    CommandType::TaskReadinessResult,
                    CommandPayload::TaskReadinessResult {
                        task_id: result.task_id,
                        ready: result.ready,
                        ready_steps_count: result.ready_steps_count,
                        batch_info,
                        error_message: result.error_message,
                    },
                    CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                ))
            }
            _ => {
                warn!("TaskInitializationHandler received unsupported command: {:?}", command.command_type);

                Some(command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "UnsupportedCommand".to_string(),
                        message: format!("TaskInitializationHandler does not support command type: {:?}", command.command_type),
                        details: None,
                        retryable: false,
                    },
                    CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                ))
            }
        };

        Ok(result)
    }

    fn handler_name(&self) -> &str {
        "TaskInitializationHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![
            CommandType::InitializeTask,
            CommandType::TryTaskIfReady,
        ]
    }
}
