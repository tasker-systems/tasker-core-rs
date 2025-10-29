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
use tracing::{info, warn};
use uuid::Uuid;

use tasker_shared::monitoring::ChannelMonitor; // TAS-51: Channel monitoring
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::orchestration::command_processor::{
    OrchestrationCommand, OrchestrationProcessingStats, OrchestrationProcessor, SystemHealth,
};
// use crate::orchestration::lifecycle::result_processing::OrchestrationResultProcessor;
// use crate::orchestration::lifecycle::step_enqueuer_services::StepEnqueuerService;
// use crate::orchestration::lifecycle::task_finalization::TaskFinalizer;
// use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
// use crate::orchestration::lifecycle::task_request_processor::{
//     TaskRequestProcessor, TaskRequestProcessorConfig,
// };

/// TAS-40 Command Pattern OrchestrationCore
///
/// Replaces complex polling-based coordinator/executor system with simple command pattern
/// while preserving all sophisticated orchestration logic through delegation.
#[derive(Debug)]
pub struct OrchestrationCore {
    /// System context for dependency injection
    pub context: Arc<SystemContext>,

    /// Command sender for orchestration operations
    command_sender: mpsc::Sender<OrchestrationCommand>,

    /// Orchestration processor (handles commands in background)
    /// Kept alive for the lifetime of OrchestrationCore to ensure background task runs
    /// Future: Will be used for reaper/sweeper processes for missed messages
    #[allow(dead_code)]
    processor: Option<OrchestrationProcessor>,

    /// System status
    pub status: OrchestrationCoreStatus,

    /// Channel monitor for command channel observability (TAS-51)
    command_channel_monitor: ChannelMonitor,
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
    /// Create new OrchestrationCore with actor-based command pattern (TAS-46)
    pub async fn new(context: Arc<SystemContext>) -> TaskerResult<Self> {
        info!("Creating OrchestrationCore with TAS-46 actor-based command pattern");

        // Create ActorRegistry with all actors (TAS-46)
        let actors = Arc::new(crate::actors::ActorRegistry::build(context.clone()).await?);

        // // Create sophisticated delegation components using unified claim system
        // let task_request_processor = Self::create_task_request_processor(&context).await?;
        // let result_processor = Self::create_result_processor(&context).await?;
        // let task_claim_step_enqueuer = Self::create_task_claim_step_enqueuer(&context).await?;

        // Create OrchestrationProcessor with actor registry (TAS-46)
        // TAS-51: Use configured buffer size for command processor
        let command_buffer_size = context
            .tasker_config
            .mpsc_channels
            .orchestration
            .command_processor
            .command_buffer_size;

        // TAS-51: Initialize channel monitor for observability
        let channel_monitor =
            ChannelMonitor::new("orchestration_command_channel", command_buffer_size);

        let (mut processor, command_sender) = OrchestrationProcessor::new(
            context.clone(),
            actors,
            // task_request_processor,
            // result_processor,
            // task_claim_step_enqueuer,
            context.message_client(),
            command_buffer_size,
            channel_monitor.clone(), // Pass clone to processor
        );

        // Start the processor
        processor.start().await?;

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            status: OrchestrationCoreStatus::Created,
            command_channel_monitor: channel_monitor, // Store monitor for sender instrumentation
        })
    }

    /// Get command sender for external components
    pub fn command_sender(&self) -> mpsc::Sender<OrchestrationCommand> {
        self.command_sender.clone()
    }

    /// Get command channel monitor for send instrumentation (TAS-51)
    pub fn command_channel_monitor(&self) -> ChannelMonitor {
        self.command_channel_monitor.clone()
    }

    /// Start the orchestration core
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!("Starting OrchestrationCore with command pattern architecture");

        self.status = OrchestrationCoreStatus::Starting;

        // The processor is already started in new(), just update status
        self.status = OrchestrationCoreStatus::Running;

        info!(
            processor_uuid = %self.context.processor_uuid(),
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
            processor_uuid = %self.context.processor_uuid(),
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

    /// Get current orchestration core status
    pub fn status(&self) -> &OrchestrationCoreStatus {
        &self.status
    }

    /// Get core ID
    pub fn processor_uuid(&self) -> Uuid {
        self.context.processor_uuid()
    }

    ///// Create sophisticated TaskRequestProcessor for delegation
    // async fn create_task_request_processor(
    //     context: &Arc<SystemContext>,
    // ) -> TaskerResult<Arc<TaskRequestProcessor>> {
    //     info!("Creating sophisticated TaskRequestProcessor for command pattern delegation");

    //     let config = TaskRequestProcessorConfig::default();
    //     let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
    //     let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

    //     // Create TaskInitializer with step enqueuer for immediate step enqueuing
    //     let task_initializer = Arc::new(TaskInitializer::new(
    //         context.clone(),
    //         task_claim_step_enqueuer,
    //     ));

    //     Ok(Arc::new(TaskRequestProcessor::new(
    //         context.message_client.clone(),
    //         context.task_handler_registry.clone(),
    //         task_initializer,
    //         config,
    //     )))
    // }

    // /// Create sophisticated OrchestrationResultProcessor for delegation
    // async fn create_result_processor(
    //     context: &Arc<SystemContext>,
    // ) -> TaskerResult<Arc<OrchestrationResultProcessor>> {
    //     info!("Creating sophisticated OrchestrationResultProcessor with unified claim system");

    //     // Create TaskClaimStepEnqueuer with the shared processor UUID
    //     let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
    //     let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

    //     let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer);

    //     Ok(Arc::new(OrchestrationResultProcessor::new(
    //         task_finalizer,
    //         context.clone(),
    //     )))
    // }

    // /// Create sophisticated TaskClaimStepEnqueuer for delegation
    // async fn create_task_claim_step_enqueuer(
    //     context: &Arc<SystemContext>,
    // ) -> TaskerResult<Arc<StepEnqueuerService>> {
    //     info!(
    //         "Creating sophisticated TaskClaimStepEnqueuer for TAS-43 command pattern integration"
    //     );

    //     let enqueuer = StepEnqueuerService::new(context.clone()).await?;

    //     Ok(Arc::new(enqueuer))
    // }
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

    #[tokio::test]
    async fn test_orchestration_core_lifecycle() {
        // This test will be implemented once we have proper test infrastructure
        // for the command pattern architecture
        // This test will be implemented once we have proper test infrastructure
    }

    #[tokio::test]
    async fn test_command_pattern_integration() {
        // This test will verify that the command pattern properly delegates
        // to sophisticated orchestration components
        // This test will verify that the command pattern properly delegates
    }
}
