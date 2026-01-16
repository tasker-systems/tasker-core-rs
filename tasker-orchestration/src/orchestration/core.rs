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
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use uuid::Uuid;

use tasker_shared::monitoring::ChannelMonitor; // TAS-51: Channel monitoring
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::health::{BackpressureChecker, HealthStatusCaches, StatusEvaluator};
use crate::orchestration::channels::OrchestrationCommandSender;
use crate::orchestration::command_processor::{
    OrchestrationCommand, OrchestrationProcessingStats, OrchestrationProcessor, SystemHealth,
};
use crate::orchestration::staleness_detector::StalenessDetector;
use crate::web::circuit_breaker::WebDatabaseCircuitBreaker;

/// TAS-40 Command Pattern OrchestrationCore
///
/// Replaces complex polling-based coordinator/executor system with simple command pattern
/// while preserving all sophisticated orchestration logic through delegation.
pub struct OrchestrationCore {
    /// System context for dependency injection
    pub context: Arc<SystemContext>,

    /// Command sender for orchestration operations
    /// TAS-133: Uses NewType wrapper for type-safe channel communication
    command_sender: OrchestrationCommandSender,

    /// Orchestration processor (handles commands in background)
    /// Kept alive for the lifetime of OrchestrationCore to ensure background task runs
    /// Future: Will be used for reaper/sweeper processes for missed messages
    processor: Option<OrchestrationProcessor>,

    /// System status - shared via Arc\<RwLock\> so web layer can read without locking the core
    pub status: Arc<RwLock<OrchestrationCoreStatus>>,

    /// Channel monitor for command channel observability (TAS-51)
    command_channel_monitor: ChannelMonitor,

    /// TAS-49: Staleness detector background service
    staleness_detector_handle: Option<JoinHandle<()>>,

    /// TAS-75: Health status caches for backpressure monitoring
    health_caches: HealthStatusCaches,

    /// TAS-75: Backpressure checker for API operations
    backpressure_checker: BackpressureChecker,

    /// TAS-75: Status evaluator background service handle
    status_evaluator_handle: Option<JoinHandle<()>>,

    /// TAS-75: Web database circuit breaker (shared with web layer)
    web_circuit_breaker: Arc<WebDatabaseCircuitBreaker>,
}

// Manual Debug implementation since JoinHandle doesn't implement Debug
impl std::fmt::Debug for OrchestrationCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationCore")
            .field("context", &self.context)
            .field("command_sender", &self.command_sender)
            .field("processor", &self.processor)
            .field("status", &self.status)
            .field("command_channel_monitor", &self.command_channel_monitor)
            .field(
                "staleness_detector_handle",
                &self
                    .staleness_detector_handle
                    .as_ref()
                    .map(|_| "JoinHandle"),
            )
            .field("health_caches", &self.health_caches)
            .field("backpressure_checker", &self.backpressure_checker)
            .field(
                "status_evaluator_handle",
                &self.status_evaluator_handle.as_ref().map(|_| "JoinHandle"),
            )
            .field("web_circuit_breaker", &self.web_circuit_breaker)
            .finish()
    }
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
        // TAS-61 V2: Access mpsc_channels from orchestration context
        let command_buffer_size = context
            .tasker_config
            .orchestration
            .as_ref()
            .map(|o| o.mpsc_channels.command_processor.command_buffer_size)
            .unwrap_or(5000);

        // TAS-51: Initialize channel monitor for observability
        let channel_monitor = ChannelMonitor::new(
            "orchestration_command_channel",
            command_buffer_size as usize,
        );

        // TAS-75: Initialize health monitoring infrastructure BEFORE processor
        // (so processor can receive cached health data for HealthCheck command)
        let health_caches = HealthStatusCaches::new();
        let backpressure_checker =
            BackpressureChecker::with_default_threshold(health_caches.clone());

        let (mut processor, command_sender) = OrchestrationProcessor::new(
            context.clone(),
            actors,
            context.message_client(),
            health_caches.clone(), // TAS-75: Pass health caches to processor
            command_buffer_size as usize,
            channel_monitor.clone(), // Pass clone to processor
        );

        // Start the processor
        processor.start().await?;

        // TAS-75: Create web circuit breaker (shared with web layer)
        // Get resilience config from orchestration context
        let web_circuit_breaker = Arc::new(WebDatabaseCircuitBreaker::new(
            5,                       // failure_threshold
            Duration::from_secs(30), // recovery_timeout
            "orchestration_db",      // component_name
        ));

        Ok(Self {
            context,
            command_sender,
            processor: Some(processor),
            status: Arc::new(RwLock::new(OrchestrationCoreStatus::Created)),
            command_channel_monitor: channel_monitor, // Store monitor for sender instrumentation
            staleness_detector_handle: None,          // TAS-49: Started in start()
            health_caches,                            // TAS-75: Health status caches
            backpressure_checker,                     // TAS-75: Backpressure checker
            status_evaluator_handle: None,            // TAS-75: Started in start()
            web_circuit_breaker,                      // TAS-75: Web circuit breaker
        })
    }

    /// Get command sender for external components
    /// TAS-133: Returns NewType wrapper for type-safe channel communication
    pub fn command_sender(&self) -> OrchestrationCommandSender {
        self.command_sender.clone()
    }

    /// Get command channel monitor for send instrumentation (TAS-51)
    pub fn command_channel_monitor(&self) -> ChannelMonitor {
        self.command_channel_monitor.clone()
    }

    /// Start the orchestration core
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!("Starting OrchestrationCore with command pattern architecture");

        *self.status.write().await = OrchestrationCoreStatus::Starting;

        // The processor is already started in new()

        // TAS-49: Start background services
        self.start_background_services().await?;

        *self.status.write().await = OrchestrationCoreStatus::Running;

        info!(
            processor_uuid = %self.context.processor_uuid(),
            "OrchestrationCore started successfully with TAS-40 command pattern and TAS-49 background services"
        );

        Ok(())
    }

    /// TAS-49: Start background services for staleness detection
    /// TAS-75: Start health status evaluator
    async fn start_background_services(&mut self) -> TaskerResult<()> {
        // Get TAS-49 DLQ staleness detection configuration
        let staleness_config = self.context.tasker_config.staleness_detection_config();

        // Start staleness detector if enabled
        if staleness_config.enabled {
            // Get batch processing config (TAS-59 Phase 3)
            let batch_config = self
                .context
                .tasker_config
                .orchestration
                .as_ref()
                .map(|o| o.batch_processing.clone())
                .unwrap_or_default();

            let detector = StalenessDetector::new(
                self.context.database_pool().clone(),
                staleness_config.clone(),
                batch_config,
            );

            let handle = tokio::spawn(async move {
                info!("Spawning staleness detector background service");
                if let Err(e) = detector.run().await {
                    error!(error = %e, "Staleness detector failed");
                }
            });

            self.staleness_detector_handle = Some(handle);
            info!("Staleness detector background service started");
        } else {
            info!("Staleness detector disabled in configuration");
        }

        // TAS-75: Start health status evaluator
        self.start_status_evaluator().await;

        Ok(())
    }

    /// TAS-75: Start the health status evaluator background service
    async fn start_status_evaluator(&mut self) {
        use crate::health::HealthConfig;

        // Get queue names from configuration
        let queues_config = &self.context.tasker_config.common.queues;
        let queue_names = vec![
            queues_config.step_results_queue_name(),
            queues_config.task_requests_queue_name(),
            queues_config.task_finalizations_queue_name(),
        ];

        // Get health config (using defaults for now, can be made configurable later)
        let health_config = HealthConfig::default();

        // Create the status evaluator
        // TAS-78: Pass separate PGMQ pool for queue metrics (supports split-database deployments)
        let evaluator = StatusEvaluator::new(
            self.health_caches.clone(),
            self.context.database_pool().clone(),
            self.context.pgmq_pool().clone(),
            self.command_channel_monitor.clone(),
            self.command_sender.clone(),
            queue_names,
            self.web_circuit_breaker.clone(),
            health_config,
        );

        // Spawn the background task
        let handle = evaluator.spawn();
        self.status_evaluator_handle = Some(handle);

        info!("TAS-75: Health status evaluator background service started");
    }

    /// Stop the orchestration core
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!("Stopping OrchestrationCore");

        *self.status.write().await = OrchestrationCoreStatus::Stopping;

        // TAS-49: Stop background services first
        self.stop_background_services().await;

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

        *self.status.write().await = OrchestrationCoreStatus::Stopped;

        info!(
            processor_uuid = %self.context.processor_uuid(),
            "OrchestrationCore stopped successfully"
        );

        Ok(())
    }

    /// TAS-49: Stop background services gracefully
    /// TAS-75: Stop health status evaluator
    async fn stop_background_services(&mut self) {
        // Stop staleness detector
        if let Some(handle) = self.staleness_detector_handle.take() {
            info!("Stopping staleness detector background service");
            handle.abort();

            // Give it a moment to clean up
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(_) => info!("Staleness detector stopped gracefully"),
                Err(_) => warn!("Staleness detector stop timed out (already aborted)"),
            }
        }

        // TAS-75: Stop status evaluator
        if let Some(handle) = self.status_evaluator_handle.take() {
            info!("Stopping health status evaluator background service");
            handle.abort();

            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(_) => info!("Health status evaluator stopped gracefully"),
                Err(_) => warn!("Health status evaluator stop timed out (already aborted)"),
            }
        }
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

    /// Get current orchestration core status (shared reference for web layer)
    pub fn status(&self) -> Arc<RwLock<OrchestrationCoreStatus>> {
        Arc::clone(&self.status)
    }

    /// Get core ID
    pub fn processor_uuid(&self) -> Uuid {
        self.context.processor_uuid()
    }

    /// TAS-75: Get the backpressure checker for API operations
    ///
    /// This is the primary interface for web API handlers to check backpressure status.
    /// The checker reads from caches updated by the background StatusEvaluator.
    pub fn backpressure_checker(&self) -> &BackpressureChecker {
        &self.backpressure_checker
    }

    /// TAS-75: Get the health status caches
    ///
    /// Direct access to caches for advanced use cases (e.g., custom health endpoints).
    pub fn health_caches(&self) -> &HealthStatusCaches {
        &self.health_caches
    }

    /// TAS-75: Get the web database circuit breaker
    ///
    /// Shared circuit breaker used for both orchestration and web API operations.
    pub fn web_circuit_breaker(&self) -> Arc<WebDatabaseCircuitBreaker> {
        Arc::clone(&self.web_circuit_breaker)
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
