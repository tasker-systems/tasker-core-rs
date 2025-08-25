//! # TAS-40 WorkerCore with Command Pattern Integration
//!
//! This module provides the main worker system core that integrates the
//! TAS-40 command pattern architecture with worker-specific processing components.
//!
//! ## Key Features
//!
//! - **Command Pattern Integration**: Uses WorkerProcessor for all worker operations
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

use crate::api_clients::orchestration_client::{OrchestrationApiClient, OrchestrationApiConfig};
use crate::command_processor::{WorkerCommand, WorkerProcessor, WorkerStatus};
use crate::health::WorkerHealthStatus;
use crate::task_template_manager::TaskTemplateManager;

/// TAS-40 Command Pattern WorkerCore
///
/// Similar to OrchestrationCore but focused on worker-specific operations:
/// - Processing step messages from queues
/// - Executing step handlers
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

    /// Task template manager for local caching
    pub task_template_manager: Arc<TaskTemplateManager>,

    /// Orchestration API client for service communication
    pub orchestration_client: Arc<OrchestrationApiClient>,

    /// System status
    pub status: WorkerCoreStatus,

    /// Core configuration
    pub core_id: Uuid,
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
    /// Create new WorkerCore with command pattern integration
    pub async fn new(
        context: Arc<SystemContext>,
        orchestration_config: OrchestrationApiConfig,
    ) -> TaskerResult<Self> {
        info!("Creating WorkerCore with TAS-40 command pattern integration");

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

        // Create WorkerProcessor with command pattern - use a default namespace for now
        // TODO: Make namespace configurable in WorkerCore constructor
        let default_namespace = "default_worker".to_string();
        let (mut processor, command_sender) = WorkerProcessor::new(
            default_namespace,
            context.clone(),
            1000, // Command buffer size
        );

        // Start the processor
        processor.start().await?;

        let core_id = Uuid::now_v7();

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            task_template_manager,
            orchestration_client,
            status: WorkerCoreStatus::Created,
            core_id,
        })
    }

    /// Start the worker core
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!("Starting WorkerCore with command pattern architecture");

        self.status = WorkerCoreStatus::Starting;

        // The processor is already started in new(), just update status
        self.status = WorkerCoreStatus::Running;

        info!(
            core_id = %self.core_id,
            "WorkerCore started successfully with TAS-40 command pattern"
        );

        Ok(())
    }

    /// Stop the worker core
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!("Stopping WorkerCore");

        self.status = WorkerCoreStatus::Stopping;

        // Send shutdown command to processor
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
            "WorkerCore stopped successfully"
        );

        Ok(())
    }

    /// Get worker health status via command pattern
    pub async fn get_health(&self) -> TaskerResult<WorkerHealthStatus> {
        // Use GetWorkerStatus to get worker information, then convert to health status
        let worker_status = self.get_processing_stats().await?;

        // Convert WorkerStatus to WorkerHealthStatus
        Ok(WorkerHealthStatus {
            status: worker_status.status.clone(),
            database_connected: true, // TODO: Could check actual DB connectivity
            orchestration_api_reachable: true, // TODO: Could check actual API connectivity
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
        message: tasker_shared::messaging::message::SimpleStepMessage,
    ) -> TaskerResult<tasker_shared::messaging::StepExecutionResult> {
        let (resp_tx, resp_rx) = oneshot::channel();

        self.command_sender
            .send(WorkerCommand::ExecuteStep {
                message,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to send step processing command: {e}"))
            })?;

        resp_rx
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Step processing response error: {e}")))?
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
