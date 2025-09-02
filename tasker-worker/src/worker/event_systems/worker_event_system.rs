//! # WorkerEventSystem - Unified Event-Driven Architecture for Workers
//!
//! This module implements the EventDrivenSystem trait for worker namespace queue processing,
//! following the same modular patterns established in orchestration. It provides consistent
//! architecture across the entire tasker-core ecosystem with unified event systems configuration.
//!
//! ## Key Features
//!
//! - **Unified Configuration**: Uses `WorkerEventSystemConfig` from the centralized event systems
//! - **EventDrivenSystem Implementation**: Follows the unified trait from tasker-shared
//! - **Deployment Mode Support**: PollingOnly, Hybrid, EventDrivenOnly configurations
//! - **FFI Event Integration**: Event-driven all the way down to FFI boundary
//! - **Command Pattern Integration**: Seamless integration with existing WorkerCommand pattern
//! - **Health Monitoring**: Built-in health checks and statistics tracking

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::SystemTime;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use async_trait::async_trait;
use tasker_shared::{
    event_system::{
        deployment::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus},
        event_driven::{EventDrivenSystem, EventSystemStatistics},
    },
    system_context::SystemContext,
};

// Re-export for consumers of this module
pub use tasker_shared::config::event_systems::WorkerEventSystemConfig;

use super::super::command_processor::WorkerCommand;
use super::super::worker_queues::{
    events::{WorkerNotification, WorkerQueueEvent},
    fallback_poller::{WorkerFallbackPoller, WorkerPollerConfig},
    listener::{WorkerListenerConfig, WorkerQueueListener},
};

/// Statistics for the WorkerEventSystem
#[derive(Debug, Clone, Default)]
pub struct WorkerEventSystemStatistics {
    /// Total events processed successfully
    pub events_processed: u64,
    /// Total events that failed processing
    pub events_failed: u64,
    /// Current event processing rate (events/second)
    pub processing_rate: f64,
    /// Average event processing latency in milliseconds
    pub average_latency_ms: f64,
    /// Current deployment mode effectiveness score (0.0-1.0)
    pub deployment_mode_score: f64,
    /// Step message events processed
    pub step_messages_processed: u64,
    /// Health check events processed
    pub health_checks_processed: u64,
    /// Configuration update events processed
    pub config_updates_processed: u64,
    /// Last event processed timestamp
    pub last_event_at: Option<SystemTime>,
}

impl WorkerEventSystemStatistics {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventSystemStatistics for WorkerEventSystemStatistics {
    fn events_processed(&self) -> u64 {
        self.events_processed
    }

    fn events_failed(&self) -> u64 {
        self.events_failed
    }

    fn processing_rate(&self) -> f64 {
        self.processing_rate
    }

    fn average_latency_ms(&self) -> f64 {
        self.average_latency_ms
    }

    fn deployment_mode_score(&self) -> f64 {
        self.deployment_mode_score
    }
}

/// Unified WorkerEventSystem implementing EventDrivenSystem
///
/// Provides consistent event-driven architecture for worker namespace queue processing,
/// following the unified configuration patterns and event-driven FFI integration.
///
/// ## FFI Event Architecture
///
/// This system implements event-driven processing all the way to the FFI boundary:
/// 1. Listens for `worker_{namespace}_queue` messages via pg_notify or polling
/// 2. Claims steps via `tasker-worker/src/worker/command_processor.rs`
/// 3. Fires step execution events that forward to FFI event systems (dry-events, pypubsub)
/// 4. FFI-side handlers process and fire completion events back to Rust
/// 5. Rust receives completion via `tasker-shared/src/events/worker_events.rs:168`
///
/// This avoids complex FFI memory management and dynamic method invocation.
pub struct WorkerEventSystem {
    /// System identifier
    system_id: String,
    /// Current deployment mode
    deployment_mode: DeploymentMode,
    /// Worker namespace queue listener
    queue_listener: Option<WorkerQueueListener>,
    /// Fallback poller for reliability
    fallback_poller: Option<WorkerFallbackPoller>,
    /// Worker command sender (for FFI event forwarding)
    command_sender: mpsc::Sender<WorkerCommand>,
    /// Unified system configuration
    config: WorkerEventSystemConfig,
    /// Runtime statistics
    statistics: Arc<AtomicStatistics>,
    /// System context
    context: Arc<SystemContext>,
    /// Running flag
    is_running: Arc<AtomicBool>,
}

/// Thread-safe statistics container
#[derive(Debug)]
struct AtomicStatistics {
    events_processed: AtomicU64,
    events_failed: AtomicU64,
    step_messages_processed: AtomicU64,
    health_checks_processed: AtomicU64,
    config_updates_processed: AtomicU64,
}

impl Default for AtomicStatistics {
    fn default() -> Self {
        Self {
            events_processed: AtomicU64::new(0),
            events_failed: AtomicU64::new(0),
            step_messages_processed: AtomicU64::new(0),
            health_checks_processed: AtomicU64::new(0),
            config_updates_processed: AtomicU64::new(0),
        }
    }
}

impl WorkerEventSystem {
    /// Create new WorkerEventSystem with unified configuration
    pub fn new(
        config: WorkerEventSystemConfig,
        command_sender: mpsc::Sender<WorkerCommand>,
        context: Arc<SystemContext>,
    ) -> Self {
        let system_id = config.system_id.clone();
        let deployment_mode = config.deployment_mode.clone();

        info!(
            system_id = %system_id,
            deployment_mode = ?deployment_mode,
            supported_namespaces = ?config.namespaces,
            "🔄 WORKER_EVENT_SYSTEM: Initializing with unified configuration"
        );

        Self {
            system_id,
            deployment_mode,
            queue_listener: None,
            fallback_poller: None,
            command_sender,
            config,
            statistics: Arc::new(AtomicStatistics::default()),
            context,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initialize components based on deployment mode
    async fn initialize_components(&mut self) -> Result<(), DeploymentModeError> {
        match self.deployment_mode {
            DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid => {
                // Convert unified config to listener config format
                let listener_config = WorkerListenerConfig {
                    supported_namespaces: self.config.namespaces.clone(),
                    retry_interval: std::time::Duration::from_secs(self.config.metadata.listener.retry_interval_seconds),
                    max_retry_attempts: self.config.metadata.listener.max_retry_attempts,
                    event_timeout: std::time::Duration::from_secs(self.config.metadata.listener.event_timeout_seconds),
                    batch_processing: self.config.metadata.listener.batch_processing,
                    connection_timeout: std::time::Duration::from_secs(self.config.metadata.listener.connection_timeout_seconds),
                    health_check_interval: std::time::Duration::from_secs(60), // Default from WorkerListenerConfig
                };
                
                // Create a notification sender - we'll convert notifications to commands
                let (notification_sender, mut notification_receiver) = mpsc::channel::<WorkerNotification>(1000);
                
                // Initialize queue listener for event-driven processing
                self.queue_listener = Some(
                    WorkerQueueListener::new(
                        listener_config,
                        self.context.clone(),
                        notification_sender,
                    )
                    .await.map_err(|e| DeploymentModeError::ConfigurationError { 
                        message: format!("Failed to initialize queue listener: {}", e)
                    })?,
                );
                
                // Spawn a task to convert notifications to commands
                let command_sender = self.command_sender.clone();
                tokio::spawn(async move {
                    while let Some(notification) = notification_receiver.recv().await {
                        // Convert WorkerNotification to WorkerCommand
                        // This is where the FFI event forwarding will happen
                        match notification {
                            WorkerNotification::Event(WorkerQueueEvent::StepMessage(msg_event)) => {
                                // Convert to command that will process the step
                                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                if let Err(e) = command_sender.send(WorkerCommand::ExecuteStepFromEvent { 
                                    message_event: msg_event,
                                    resp: resp_tx,
                                }).await {
                                    error!("Failed to forward step notification to command processor: {}", e);
                                }
                            }
                            WorkerNotification::Health(_) => {
                                // Forward health check
                                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                if let Err(e) = command_sender.send(WorkerCommand::HealthCheck {
                                    resp: resp_tx,
                                }).await {
                                    error!("Failed to forward health check notification: {}", e);
                                }
                            }
                            _ => {
                                // Other notification types - log but don't process
                            }
                        }
                    }
                });

                info!("🎧 WORKER_EVENT_SYSTEM: Queue listener initialized for event-driven processing");
            }
            _ => {}
        }

        match self.deployment_mode {
            DeploymentMode::PollingOnly | DeploymentMode::Hybrid => {
                // Convert unified config to poller config format
                let poller_config = WorkerPollerConfig {
                    enabled: self.config.metadata.fallback_poller.enabled,
                    polling_interval: std::time::Duration::from_millis(self.config.metadata.fallback_poller.polling_interval_ms),
                    batch_size: self.config.metadata.fallback_poller.batch_size,
                    age_threshold: std::time::Duration::from_secs(self.config.metadata.fallback_poller.age_threshold_seconds),
                    max_age: std::time::Duration::from_secs(self.config.metadata.fallback_poller.max_age_hours * 3600),
                    supported_namespaces: self.config.namespaces.clone(),
                    visibility_timeout: std::time::Duration::from_secs(self.config.metadata.fallback_poller.visibility_timeout_seconds),
                };
                
                // Initialize fallback poller for reliability
                self.fallback_poller = Some(
                    WorkerFallbackPoller::new(
                        poller_config,
                        self.context.clone(),
                        self.command_sender.clone(),
                    )
                    .await.map_err(|e| DeploymentModeError::ConfigurationError { 
                        message: format!("Failed to initialize fallback poller: {}", e)
                    })?,
                );

                info!("📊 WORKER_EVENT_SYSTEM: Fallback poller initialized for reliable processing");
            }
            _ => {}
        }

        Ok(())
    }
}

#[async_trait]
impl EventDrivenSystem for WorkerEventSystem {
    type SystemId = String;
    type Event = WorkerQueueEvent;
    type Config = WorkerEventSystemConfig;
    type Statistics = WorkerEventSystemStatistics;

    fn system_id(&self) -> Self::SystemId {
        self.system_id.clone()
    }

    fn deployment_mode(&self) -> DeploymentMode {
        self.deployment_mode.clone()
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    fn statistics(&self) -> Self::Statistics {
        WorkerEventSystemStatistics {
            events_processed: self.statistics.events_processed.load(Ordering::Relaxed),
            events_failed: self.statistics.events_failed.load(Ordering::Relaxed),
            processing_rate: 0.0, // TODO: Calculate actual processing rate
            average_latency_ms: 0.0, // TODO: Calculate actual latency
            deployment_mode_score: 1.0, // TODO: Calculate effectiveness score
            step_messages_processed: self.statistics.step_messages_processed.load(Ordering::Relaxed),
            health_checks_processed: self.statistics.health_checks_processed.load(Ordering::Relaxed),
            config_updates_processed: self.statistics.config_updates_processed.load(Ordering::Relaxed),
            last_event_at: Some(SystemTime::now()),
        }
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }

    async fn process_event(&self, _event: Self::Event) -> Result<(), DeploymentModeError> {
        // TODO: Implement actual event processing
        // This would forward events to the FFI layer via the command sender
        Ok(())
    }

    async fn start(&mut self) -> Result<(), DeploymentModeError> {
        if self.is_running.load(Ordering::Acquire) {
            warn!("Worker event system already running");
            return Ok(());
        }

        info!(
            system_id = %self.system_id,
            deployment_mode = ?self.deployment_mode,
            "🚀 WORKER_EVENT_SYSTEM: Starting unified worker event system"
        );

        // Initialize components based on deployment mode
        self.initialize_components().await?;

        // Start queue listener if configured
        if let Some(ref mut listener) = self.queue_listener {
            listener.start().await.map_err(|e| DeploymentModeError::ConfigurationError {
                message: format!("Failed to start queue listener: {}", e)
            })?;
            info!("🎧 WORKER_EVENT_SYSTEM: Queue listener started");
        }

        // Start fallback poller if configured
        if let Some(ref mut _poller) = self.fallback_poller {
            // Note: The poller doesn't have a start method, it starts automatically in new()
            info!("📊 WORKER_EVENT_SYSTEM: Fallback poller started");
        }

        self.is_running.store(true, Ordering::Release);

        info!(
            system_id = %self.system_id,
            "✅ WORKER_EVENT_SYSTEM: Started successfully with unified configuration"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), DeploymentModeError> {
        if !self.is_running.load(Ordering::Acquire) {
            warn!("Worker event system not running");
            return Ok(());
        }

        info!(
            system_id = %self.system_id,
            "🛑 WORKER_EVENT_SYSTEM: Stopping unified worker event system"
        );

        // Stop fallback poller (has explicit stop method)
        if let Some(ref mut poller) = self.fallback_poller {
            poller.stop().await;
            info!("📊 WORKER_EVENT_SYSTEM: Fallback poller stopped");
        }

        // Queue listener stops automatically when dropped - no explicit stop method needed
        if self.queue_listener.is_some() {
            info!("🎧 WORKER_EVENT_SYSTEM: Queue listener will stop when dropped");
        }

        self.is_running.store(false, Ordering::Release);

        info!(
            system_id = %self.system_id,
            "✅ WORKER_EVENT_SYSTEM: Stopped successfully"
        );

        Ok(())
    }

    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
        // Update health check statistics
        self.statistics
            .health_checks_processed
            .fetch_add(1, Ordering::Relaxed);

        // For now, return healthy if the system is running
        // TODO: Implement actual component health checks when the components support it
        if self.is_running.load(Ordering::Acquire) {
            Ok(DeploymentModeHealthStatus::Healthy)
        } else {
            Ok(DeploymentModeHealthStatus::Critical)
        }
    }

}
