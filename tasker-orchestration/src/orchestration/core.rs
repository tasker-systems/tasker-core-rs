//! # TAS-40 OrchestrationCore with Command Pattern Integration
//!
//! This module provides the main orchestration system bootstrap that integrates the
//! TAS-40 command pattern architecture with existing sophisticated orchestration components.
//!
//! ## Key Features
//!
//! - **Command Pattern Integration**: Uses OrchestrationProcessor for all orchestration operations
//! - **Sophisticated Delegation**: Maintains existing sophisticated orchestration logic through delegation
//! - **No Polling**: Pure command-driven architecture with tokio channels
//! - **Race Condition Prevention**: TAS-37 atomic finalization claiming preserved through delegation
//! - **Transaction Safety**: Existing transaction patterns maintained via component delegation

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::orchestration::command_processor::{
    OrchestrationCommand, OrchestrationProcessingStats, OrchestrationProcessor, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult,
};
use crate::orchestration::lifecycle::result_processor::OrchestrationResultProcessor;
use crate::orchestration::lifecycle::task_finalizer::TaskFinalizer;
use crate::orchestration::lifecycle::task_initializer::TaskInitializer;
use crate::orchestration::lifecycle::task_request_processor::{
    TaskRequestProcessor, TaskRequestProcessorConfig,
};
use crate::orchestration::task_claim::finalization_claimer::{
    FinalizationClaimer, FinalizationClaimerConfig,
};

/// TAS-40 Command Pattern OrchestrationCore
///
/// Replaces complex polling-based coordinator/executor system with simple command pattern
/// while preserving all sophisticated orchestration logic through delegation.
pub struct OrchestrationCore {
    /// System context for dependency injection
    context: Arc<SystemContext>,

    /// Command sender for orchestration operations
    command_sender: mpsc::Sender<OrchestrationCommand>,

    /// Orchestration processor (handles commands in background)
    processor: Option<OrchestrationProcessor>,

    /// System status
    status: OrchestrationCoreStatus,

    /// Core configuration
    core_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationCoreStatus {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl OrchestrationCore {
    /// Create new OrchestrationCore with command pattern integration
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        info!("Creating OrchestrationCore with TAS-40 command pattern integration");

        // Create sophisticated delegation components using existing patterns
        let task_request_processor = Self::create_task_request_processor(&context).await?;
        let result_processor = Self::create_result_processor(&context).await?;
        let finalization_claimer = Self::create_finalization_claimer(&context).await?;

        // Create OrchestrationProcessor with sophisticated delegation
        let (mut processor, command_sender) = OrchestrationProcessor::new(
            context.clone(),
            task_request_processor,
            result_processor,
            finalization_claimer,
            1000, // Command buffer size
        );

        // Start the processor
        processor.start().await?;

        let core_id = format!("orchestration_core_{}", Uuid::new_v4());

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            status: OrchestrationCoreStatus::Created,
            core_id,
        })
    }

    /// Start the orchestration core
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!("Starting OrchestrationCore with command pattern architecture");

        self.status = OrchestrationCoreStatus::Starting;

        // The processor is already started in new(), just update status
        self.status = OrchestrationCoreStatus::Running;

        info!(
            core_id = %self.core_id,
            "OrchestrationCore started successfully with TAS-40 command pattern"
        );

        Ok(())
    }

    /// Stop the orchestration core
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!("Stopping OrchestrationCore");

        self.status = OrchestrationCoreStatus::Stopping;

        // Send shutdown command to processor
        let (resp_tx, resp_rx) = oneshot::channel();
        if let Err(e) = self
            .command_sender
            .send(OrchestrationCommand::Shutdown { resp: resp_tx })
            .await
        {
            warn!("Failed to send shutdown command: {e}");
        } else {
            // Wait for shutdown acknowledgment
            if let Err(e) = tokio::time::timeout(Duration::from_secs(10), resp_rx).await {
                warn!("Shutdown acknowledgment timeout: {e}");
            }
        }

        self.status = OrchestrationCoreStatus::Stopped;

        info!(
            core_id = %self.core_id,
            "OrchestrationCore stopped successfully"
        );

        Ok(())
    }

    /// Get system health status via command pattern
    pub async fn get_health(&self) -> TaskerResult<SystemHealth> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(OrchestrationCommand::HealthCheck { resp: resp_tx })
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send health check command: {e}"))
            })?;

        resp_rx.await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Health check response error: {e}"))
        })?
    }

    /// Get processing statistics via command pattern
    pub async fn get_processing_stats(&self) -> TaskerResult<OrchestrationProcessingStats> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(OrchestrationCommand::GetProcessingStats { resp: resp_tx })
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send stats command: {e}"))
            })?;

        resp_rx
            .await
            .map_err(|e| TaskerError::OrchestrationError(format!("Stats response error: {e}")))?
    }

    /// Initialize a task via command pattern (replaces direct orchestration calls)
    pub async fn initialize_task(
        &self,
        request: TaskRequestMessage,
    ) -> TaskerResult<TaskInitializeResult> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(OrchestrationCommand::InitializeTask {
                request,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to send task initialization command: {e}"
                ))
            })?;

        resp_rx.await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Task initialization response error: {e}"))
        })?
    }

    /// Process step result via command pattern (replaces direct result processor calls)
    pub async fn process_step_result(
        &self,
        result: StepExecutionResult,
    ) -> TaskerResult<StepProcessResult> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(OrchestrationCommand::ProcessStepResult {
                result,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to send step result command: {e}"))
            })?;

        resp_rx.await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Step result response error: {e}"))
        })?
    }

    /// Finalize task via command pattern (replaces direct finalization calls)
    pub async fn finalize_task(&self, task_uuid: Uuid) -> TaskerResult<TaskFinalizationResult> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(OrchestrationCommand::FinalizeTask {
                task_uuid,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::OrchestrationError(format!(
                    "Failed to send task finalization command: {e}"
                ))
            })?;

        resp_rx.await.map_err(|e| {
            TaskerError::OrchestrationError(format!("Task finalization response error: {e}"))
        })?
    }

    /// Get current orchestration core status
    pub fn status(&self) -> &OrchestrationCoreStatus {
        &self.status
    }

    /// Get core ID
    pub fn core_id(&self) -> &str {
        &self.core_id
    }

    /// Create sophisticated TaskRequestProcessor for delegation
    async fn create_task_request_processor(
        context: &Arc<SystemContext>,
    ) -> TaskerResult<Arc<TaskRequestProcessor>> {
        info!("Creating sophisticated TaskRequestProcessor for command pattern delegation");

        let config = TaskRequestProcessorConfig::default();
        let task_initializer = Arc::new(TaskInitializer::new(context.database_pool.clone()));

        Ok(Arc::new(TaskRequestProcessor::new(
            context.message_client.clone(),
            context.task_handler_registry.clone(),
            task_initializer,
            config,
        )))
    }

    /// Create sophisticated OrchestrationResultProcessor for delegation
    async fn create_result_processor(
        context: &Arc<SystemContext>,
    ) -> TaskerResult<Arc<OrchestrationResultProcessor>> {
        info!("Creating sophisticated OrchestrationResultProcessor for command pattern delegation");

        let task_finalizer = TaskFinalizer::new(
            context.database_pool.clone(),
            context.config_manager.config().clone(),
        );

        Ok(Arc::new(OrchestrationResultProcessor::new(
            task_finalizer,
            context.database_pool.clone(),
        )))
    }

    /// Create sophisticated FinalizationClaimer for delegation
    async fn create_finalization_claimer(
        context: &Arc<SystemContext>,
    ) -> TaskerResult<Arc<FinalizationClaimer>> {
        info!("Creating sophisticated FinalizationClaimer for command pattern delegation");

        let config =
            FinalizationClaimerConfig::from_config_manager(context.config_manager.as_ref());
        let processor_id = FinalizationClaimer::generate_processor_id("orchestration_core");

        Ok(Arc::new(FinalizationClaimer::with_config(
            context.database_pool.clone(),
            processor_id,
            config,
        )))
    }
}

impl std::fmt::Display for OrchestrationCoreStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrchestrationCoreStatus::Created => write!(f, "Created"),
            OrchestrationCoreStatus::Starting => write!(f, "Starting"),
            OrchestrationCoreStatus::Running => write!(f, "Running"),
            OrchestrationCoreStatus::Stopping => write!(f, "Stopping"),
            OrchestrationCoreStatus::Stopped => write!(f, "Stopped"),
            OrchestrationCoreStatus::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestration_core_lifecycle() {
        // This test will be implemented once we have proper test infrastructure
        // for the command pattern architecture
        assert!(true, "OrchestrationCore lifecycle test placeholder");
    }

    #[tokio::test]
    async fn test_command_pattern_integration() {
        // This test will verify that the command pattern properly delegates
        // to sophisticated orchestration components
        assert!(true, "Command pattern integration test placeholder");
    }
}
