//! # TAS-40 WorkerCore with Command Pattern Integration
//! # TAS-43 Event-Driven Message Processing Integration
//!
//! This module provides the main worker system core that integrates the
//! TAS-40 command pattern architecture with TAS-43 event-driven processing.
//!
//! ## Key Features
//!
//! - **Command Pattern Integration**: Uses WorkerProcessor for all worker operations
//! - **Event-Driven Processing**: Real-time message processing via PostgreSQL LISTEN/NOTIFY
//! - **Hybrid Reliability**: Event-driven with fallback polling for guaranteed message processing
//! - **Step Execution**: Manages step message processing and result publishing
//! - **TaskTemplate Management**: Local template caching and validation
//! - **Orchestration API**: HTTP client for orchestration service communication
//! - **Health Monitoring**: Worker-specific health checks and status reporting

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;

use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use super::command_processor::{WorkerCommand, WorkerProcessor, WorkerStatus};
use super::event_driven_processor::{
    EventDrivenConfig, EventDrivenMessageProcessor, EventDrivenStats,
};
use super::task_template_manager::TaskTemplateManager;
use crate::api_clients::orchestration_client::{OrchestrationApiClient, OrchestrationApiConfig};
use crate::health::WorkerHealthStatus;

/// TAS-40 Command Pattern WorkerCore with TAS-43 Event-Driven Integration
///
/// Focused on worker-specific operations with real-time event processing:
/// - Event-driven step message processing via PostgreSQL LISTEN/NOTIFY
/// - Hybrid reliability with fallback polling for guaranteed processing
/// - Command pattern integration for all worker operations
/// - Publishing results back to orchestration
/// - Managing local task template cache
/// - Health monitoring and metrics
pub struct WorkerCore {
    /// System context for dependency injection
    pub context: Arc<SystemContext>,

    /// Command sender for worker operations
    command_sender: mpsc::Sender<WorkerCommand>,

    /// Worker processor (handles commands in background)
    /// Kept alive for the lifetime of WorkerCore to ensure background task runs
    #[allow(dead_code)]
    processor: Option<WorkerProcessor>,

    /// Event-driven message processor for real-time message handling
    event_driven_processor: Option<EventDrivenMessageProcessor>,

    /// Task template manager for local caching
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// Orchestration API client for service communication
    pub orchestration_client: Arc<OrchestrationApiClient>,

    /// System status
    pub status: WorkerCoreStatus,

    /// Core configuration
    pub core_id: Uuid,

    /// Worker namespace for message filtering
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerCoreStatus {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl WorkerCore {
    /// Create new WorkerCore with TAS-43 event-driven processing integration
    pub async fn new(
        context: Arc<SystemContext>,
        orchestration_config: OrchestrationApiConfig,
        namespace: String,
        event_driven_enabled: Option<bool>,
    ) -> TaskerResult<Self> {
        Self::new_with_event_system(context, orchestration_config, namespace, event_driven_enabled, None).await
    }

    /// Create new WorkerCore with external event system
    pub async fn new_with_event_system(
        context: Arc<SystemContext>,
        orchestration_config: OrchestrationApiConfig,
        namespace: String,
        event_driven_enabled: Option<bool>,
        event_system: Option<Arc<tasker_shared::events::WorkerEventSystem>>,
    ) -> TaskerResult<Self> {
        info!(
            namespace = %namespace,
            "Creating WorkerCore with TAS-40 command pattern and TAS-43 event-driven integration"
        );

        // Create worker-specific components
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let orchestration_client = Arc::new(
            OrchestrationApiClient::new(orchestration_config).map_err(|e| {
                TaskerError::ConfigurationError(format!(
                    "Failed to create orchestration client: {}",
                    e
                ))
            })?,
        );

        // Create WorkerProcessor with command pattern and database operations capability
        let (mut processor, command_sender) = WorkerProcessor::new(
            namespace.clone(),
            context.clone(),
            task_template_manager.clone(),
            1000, // Command buffer size
        );

        // Start the processor with event integration
        if let Some(ref event_system) = event_system {
            processor.start_with_events_and_system(Some(event_system.clone())).await?;
        } else {
            processor.start_with_events().await?;
        }

        // Create EventDrivenMessageProcessor for TAS-43 integration
        let event_driven_config = EventDrivenConfig {
            event_driven_enabled: event_driven_enabled.unwrap_or(true),
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
        };

        let event_driven_processor = EventDrivenMessageProcessor::new(
            event_driven_config,
            context.clone(),
            task_template_manager.clone(),
            command_sender.clone(),
        )
        .await
        .map_err(|e| {
            TaskerError::WorkerError(format!("Failed to create event-driven processor: {e}"))
        })?;

        let core_id = Uuid::now_v7();

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            event_driven_processor: Some(event_driven_processor),
            task_template_manager,
            orchestration_client,
            status: WorkerCoreStatus::Created,
            core_id,
            namespace,
        })
    }

    /// Start the worker core with event-driven processing
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            namespace = %self.namespace,
            "Starting WorkerCore with TAS-40 command pattern and TAS-43 event-driven integration"
        );

        self.status = WorkerCoreStatus::Starting;

        // Start event-driven message processor for real-time processing
        if let Some(ref mut event_driven_processor) = self.event_driven_processor {
            event_driven_processor.start().await.map_err(|e| {
                TaskerError::WorkerError(format!("Failed to start event-driven processor: {e}"))
            })?;

            info!(
                core_id = %self.core_id,
                namespace = %self.namespace,
                "Event-driven message processor started successfully"
            );
        }

        // The WorkerProcessor is already started in new(), just update status
        self.status = WorkerCoreStatus::Running;

        info!(
            core_id = %self.core_id,
            namespace = %self.namespace,
            "WorkerCore started successfully with TAS-40 command pattern and TAS-43 event-driven integration"
        );

        Ok(())
    }

    /// Stop the worker core and event-driven processing
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            namespace = %self.namespace,
            "Stopping WorkerCore with event-driven processing"
        );

        self.status = WorkerCoreStatus::Stopping;

        // Stop event-driven message processor first
        if let Some(ref mut event_driven_processor) = self.event_driven_processor {
            if let Err(e) = event_driven_processor.stop().await {
                warn!(
                    core_id = %self.core_id,
                    error = %e,
                    "Failed to stop event-driven processor cleanly"
                );
            } else {
                info!(
                    core_id = %self.core_id,
                    namespace = %self.namespace,
                    "Event-driven message processor stopped successfully"
                );
            }
        }

        // Send shutdown command to worker processor
        let (resp_tx, resp_rx) = oneshot::channel();
        if let Err(e) = self
            .command_sender
            .send(WorkerCommand::Shutdown { resp: resp_tx })
            .await
        {
            warn!("Failed to send shutdown command: {e}");
        } else {
            // Wait for shutdown acknowledgment
            if let Err(e) = tokio::time::timeout(Duration::from_secs(10), resp_rx).await {
                warn!("Shutdown acknowledgment timeout: {e}");
            }
        }

        self.status = WorkerCoreStatus::Stopped;

        info!(
            core_id = %self.core_id,
            namespace = %self.namespace,
            "WorkerCore stopped successfully"
        );

        Ok(())
    }

    /// Get worker health status including event-driven processor status
    pub async fn get_health(&self) -> TaskerResult<WorkerHealthStatus> {
        // Use GetWorkerStatus to get worker information, then convert to health status
        let worker_status = self.get_processing_stats().await?;

        // Get event-driven processor stats if available
        let event_driven_connected = if let Some(ref processor) = self.event_driven_processor {
            let stats = processor.get_stats().await;
            stats.listener_connected && processor.is_running()
        } else {
            false
        };

        // Convert WorkerStatus to WorkerHealthStatus
        Ok(WorkerHealthStatus {
            status: worker_status.status.clone(),
            database_connected: event_driven_connected, // Event-driven processor includes DB connectivity
            orchestration_api_reachable: true,          // TODO: Could check actual API connectivity
            supported_namespaces: vec![worker_status.namespace.clone()],
            cached_templates: 0, // TODO: Get from task_template_manager
            total_messages_processed: worker_status.steps_executed,
            successful_executions: worker_status.steps_succeeded,
            failed_executions: worker_status.steps_failed,
        })
    }

    /// Get processing statistics via command pattern
    pub async fn get_processing_stats(&self) -> TaskerResult<WorkerStatus> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(WorkerCommand::GetWorkerStatus { resp: resp_tx })
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to send stats command: {e}")))?;

        resp_rx
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Stats response error: {e}")))?
    }

    /// Process step message via command pattern
    pub async fn process_step_message(
        &self,
        _message: tasker_shared::messaging::message::SimpleStepMessage,
    ) -> TaskerResult<tasker_shared::messaging::StepExecutionResult> {
        let (_resp_tx, _resp_rx) =
            oneshot::channel::<TaskerResult<tasker_shared::messaging::StepExecutionResult>>();

        // TODO: This method needs to be updated to use TaskSequenceStep instead of SimpleStepMessage
        // For now, we'll return an error to indicate this needs to be implemented properly
        Err(TaskerError::WorkerError(
            "process_step_message needs to be updated to use TaskSequenceStep pattern".to_string(),
        ))
    }

    /// Refresh task template cache via command pattern
    pub async fn refresh_template_cache(&self, namespace: Option<String>) -> TaskerResult<()> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(WorkerCommand::RefreshTemplateCache {
                namespace,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to send cache refresh command: {e}"))
            })?;

        resp_rx
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Cache refresh response error: {e}")))?
    }

    /// Get current worker core status
    pub fn status(&self) -> &WorkerCoreStatus {
        &self.status
    }

    /// Get core ID
    pub fn core_id(&self) -> &Uuid {
        &self.core_id
    }

    /// Get supported namespaces
    pub fn supported_namespaces(&self) -> &[String] {
        self.task_template_manager.supported_namespaces()
    }

    /// Check if a namespace is supported
    pub fn is_namespace_supported(&self, namespace: &str) -> bool {
        self.task_template_manager.is_namespace_supported(namespace)
    }

    /// Get the command sender for communicating with the worker processor
    pub fn command_sender(&self) -> &mpsc::Sender<WorkerCommand> {
        &self.command_sender
    }

    /// Get the worker namespace
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Get event-driven processor statistics
    pub async fn get_event_driven_stats(&self) -> Option<EventDrivenStats> {
        if let Some(ref processor) = self.event_driven_processor {
            Some(processor.get_stats().await)
        } else {
            None
        }
    }

    /// Check if event-driven processing is enabled and running
    pub fn is_event_driven_enabled(&self) -> bool {
        if let Some(ref processor) = self.event_driven_processor {
            processor.is_running()
        } else {
            false
        }
    }
}

impl std::fmt::Display for WorkerCoreStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerCoreStatus::Created => write!(f, "Created"),
            WorkerCoreStatus::Starting => write!(f, "Starting"),
            WorkerCoreStatus::Running => write!(f, "Running"),
            WorkerCoreStatus::Stopping => write!(f, "Stopping"),
            WorkerCoreStatus::Stopped => write!(f, "Stopped"),
            WorkerCoreStatus::Error(e) => write!(f, "Error: {e}"),
        }
    }
}
