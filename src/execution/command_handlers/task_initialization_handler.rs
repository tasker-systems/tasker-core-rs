//! Task Initialization Command Handler
//!
//! Handles task-related commands including task initialization and readiness checking.
//! This handler implements the core task orchestration commands that delegate to
//! the existing orchestration system for actual task management.

use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::execution::command::{Command, CommandPayload, CommandSource, CommandType, TaskBatchInfo};
use crate::execution::command_router::CommandHandler;
use crate::orchestration::types::{TaskContext};
use crate::models::core::task::{Task, NewTask};
use crate::models::core::workflow_step::{WorkflowStep, NewWorkflowStep};
use crate::models::core::task_request::TaskRequest;
use crate::orchestration::task_initializer::TaskInitializer;
use crate::orchestration::workflow_coordinator::WorkflowCoordinator;
use crate::orchestration::TaskOrchestrationResult;
use serde::{Deserialize, Serialize};
use serde_json;
use chrono;

/// Task readiness information returned by TryTaskIfReady command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessInfo {
    pub task_id: i64,
    pub ready: bool,
    pub ready_steps_count: usize,
    pub total_steps: usize,
    pub batch_info: Option<TaskBatchInfo>,
    pub error_message: Option<String>,
}

/// Task initialization result returned by InitializeTask command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInitializationResult {
    pub task_id: i64,
    pub success: bool,
    pub step_count: usize,
    pub step_mapping: HashMap<String, i64>,
    pub error_message: Option<String>,
}

/// Workflow step information for task initialization response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepInfo {
    pub step_id: i64,
    pub step_name: String,
    pub handler_class: String,
    pub dependencies: Vec<i64>,
    pub status: String,
}

/// Task Initialization Handler - handles InitializeTask and TryTaskIfReady commands
///
/// This handler manages task creation and readiness checking by integrating with
/// the existing database models and orchestration system.
pub struct TaskInitializationHandler {
    orchestration_system: std::sync::Arc<crate::ffi::shared::orchestration_system::OrchestrationSystem>,
}

impl TaskInitializationHandler {
    /// Create a new task initialization handler using shared orchestration components
    pub fn from_orchestration_system(
        orchestration_system: std::sync::Arc<crate::ffi::shared::orchestration_system::OrchestrationSystem>,
    ) -> Self {
        info!("🎯 Creating TaskInitializationHandler from shared orchestration system");
        Self {
            orchestration_system,
        }
    }

    /// Handle InitializeTask command - create a new task with workflow steps
    async fn handle_initialize_task(
        &self,
        task_request: TaskRequest,
    ) -> Result<TaskInitializationResult, Box<dyn std::error::Error + Send + Sync>> {
        info!("🎯 Handling InitializeTask command");
        debug!("Task request: {:?}", task_request);

        let task_init_result = self.orchestration_system.task_initializer.create_task_from_request(task_request).await?;

        let task_id = task_init_result.task_id;
        let step_count = task_init_result.step_count;
        let step_mapping = task_init_result.step_mapping;

        Ok(TaskInitializationResult {
            task_id,
            success: true,
            step_count,
            step_mapping,
            error_message: None,
        })
    }

    /// Handle TryTaskIfReady command - check if task has ready steps for execution
    async fn handle_try_task_if_ready(
        &self,
        task_id: i64,
    ) -> Result<TaskReadinessInfo, Box<dyn std::error::Error + Send + Sync>> {
        info!("🎯 Handling TryTaskIfReady command for task {}", task_id);

        let task_orchestration_result: TaskOrchestrationResult = self.orchestration_system.workflow_coordinator.execute_task_workflow(task_id).await?;

        match task_orchestration_result {
            TaskOrchestrationResult::Published { task_id, viable_steps_discovered, steps_published, batch_id, publication_time_ms, next_poll_delay_ms } => {
                Ok(TaskReadinessInfo {
                    task_id,
                    ready: true,
                    ready_steps_count: steps_published,
                    total_steps: viable_steps_discovered,
                    batch_info: Some(TaskBatchInfo {
                        batch_id: batch_id.unwrap_or_default(),
                        estimated_steps: steps_published,
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
}

#[async_trait]
impl CommandHandler for TaskInitializationHandler {
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        debug!("🎯 TaskInitializationHandler received command: {:?}", command.command_type);

        let result = match &command.payload {
            CommandPayload::InitializeTask { task_request } => {
                let result = self.handle_initialize_task(task_request.clone()).await?;
                let response_payload = serde_json::to_value(result)?;

                // Create response command
                Some(Command::new(
                    CommandType::TaskInitialized,
                    CommandPayload::TaskInitialized {
                        task_id: response_payload.get("task_id").and_then(|v| v.as_i64()).unwrap_or(0),
                        success: response_payload.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                        step_count: response_payload.get("step_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                        workflow_steps: response_payload.get("workflow_steps").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                        error_message: response_payload.get("error_message").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    },
                    CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                ))
            }
            CommandPayload::TryTaskIfReady { task_id } => {
                let result = self.handle_try_task_if_ready(*task_id).await?;

                // Create response command using the result directly
                Some(Command::new(
                    CommandType::TaskReadinessResult,
                    CommandPayload::TaskReadinessResult {
                        task_id: *task_id,
                        ready: result.ready,
                        batch_info: result.batch_info,
                        ready_steps_count: result.ready_steps_count,
                        error_message: result.error_message,
                    },
                    CommandSource::RustOrchestrator { id: "task_initialization_handler".to_string() },
                ))
            }
            _ => {
                warn!("TaskInitializationHandler received unsupported command: {:?}", command.command_type);
                return Err(format!("Unsupported command type: {:?}", command.command_type).into());
            }
        };

        Ok(result)
    }

    fn handler_name(&self) -> &str {
        "TaskInitializationHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![CommandType::InitializeTask, CommandType::TryTaskIfReady]
    }
}
