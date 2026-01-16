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
use std::time::{Instant, SystemTime};
use tracing::{debug, error, info, warn};

use crate::worker::channels::ChannelFactory;

use async_trait::async_trait;
use tasker_shared::{
    event_system::{
        deployment::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus},
        event_driven::{EventDrivenSystem, EventSystemStatistics},
    },
    monitoring::ChannelMonitor,
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
#[derive(Debug)]
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
    /// TAS-133: Updated to use NewType wrapper for type safety
    command_sender: crate::worker::channels::WorkerCommandSender,
    /// Unified system configuration
    config: WorkerEventSystemConfig,
    /// Runtime statistics
    statistics: Arc<AtomicStatistics>,
    /// System context
    context: Arc<SystemContext>,
    /// Running flag
    is_running: Arc<AtomicBool>,
    /// Supported namespaces
    supported_namespaces: Vec<String>,
    /// System start time for rate calculation
    start_time: Instant,
}

/// Thread-safe statistics container
#[derive(Debug)]
struct AtomicStatistics {
    events_processed: AtomicU64,
    events_failed: AtomicU64,
    step_messages_processed: AtomicU64,
    health_checks_processed: AtomicU64,
    config_updates_processed: AtomicU64,
    /// Total latency in microseconds (for average calculation)
    total_latency_us: AtomicU64,
    /// Count of latency measurements (for average calculation)
    latency_count: AtomicU64,
    /// Events processed via event-driven (pg_notify)
    event_driven_processed: AtomicU64,
    /// Events processed via polling
    polling_processed: AtomicU64,
}

impl Default for AtomicStatistics {
    fn default() -> Self {
        Self {
            events_processed: AtomicU64::new(0),
            events_failed: AtomicU64::new(0),
            step_messages_processed: AtomicU64::new(0),
            health_checks_processed: AtomicU64::new(0),
            config_updates_processed: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            latency_count: AtomicU64::new(0),
            event_driven_processed: AtomicU64::new(0),
            polling_processed: AtomicU64::new(0),
        }
    }
}

impl AtomicStatistics {
    /// Record an event processing latency
    fn record_latency(&self, latency: std::time::Duration) {
        self.total_latency_us
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get average latency in milliseconds
    fn average_latency_ms(&self) -> f64 {
        let total_us = self.total_latency_us.load(Ordering::Relaxed);
        let count = self.latency_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            (total_us as f64 / count as f64) / 1000.0
        }
    }

    /// Calculate deployment mode effectiveness score (0.0-1.0)
    /// Higher score means more event-driven processing (better/lower latency)
    fn deployment_mode_score(&self) -> f64 {
        let event_driven = self.event_driven_processed.load(Ordering::Relaxed);
        let polling = self.polling_processed.load(Ordering::Relaxed);
        let total = event_driven + polling;
        if total == 0 {
            1.0 // Default when no processing yet
        } else {
            event_driven as f64 / total as f64
        }
    }

    /// Record an event-driven processed event
    fn record_event_driven(&self) {
        self.event_driven_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a polling processed event
    fn record_polling(&self) {
        self.polling_processed.fetch_add(1, Ordering::Relaxed);
    }
}

impl WorkerEventSystem {
    /// Create new WorkerEventSystem with unified configuration
    pub fn new(
        config: WorkerEventSystemConfig,
        command_sender: crate::worker::channels::WorkerCommandSender,
        context: Arc<SystemContext>,
        supported_namespaces: Vec<String>,
    ) -> Self {
        let system_id = config.system_id.clone();
        let deployment_mode = config.deployment_mode;

        info!(
            system_id = %system_id,
            deployment_mode = ?deployment_mode,
            "Initializing with unified configuration"
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
            supported_namespaces,
            start_time: Instant::now(),
        }
    }

    /// Record a polling-based event processing (TAS-141)
    ///
    /// Call this when an event is processed via polling rather than pg_notify.
    pub fn record_polling_event(&self) {
        self.statistics.record_polling();
        self.statistics
            .events_processed
            .fetch_add(1, Ordering::Relaxed);
        self.statistics
            .step_messages_processed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record event processing latency (TAS-141)
    ///
    /// Call this when a step execution completes to track average latency.
    pub fn record_latency(&self, latency: std::time::Duration) {
        self.statistics.record_latency(latency);
    }

    /// Record a failed event processing (TAS-141)
    pub fn record_failure(&self) {
        self.statistics
            .events_failed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Initialize components based on deployment mode
    async fn initialize_components(&mut self) -> Result<(), DeploymentModeError> {
        // TAS-133: Determine effective deployment mode based on messaging provider
        // RabbitMQ uses push-based delivery with broker-managed redelivery, so fallback
        // polling is unnecessary. PGMQ uses fire-and-forget pg_notify where polling is
        // essential for reliability.
        let provider_name = self.context.messaging_provider().provider_name();
        let effective_mode = self.deployment_mode.effective_for_provider(provider_name);

        if effective_mode != self.deployment_mode {
            match self.deployment_mode {
                DeploymentMode::PollingOnly => {
                    // PollingOnly with RabbitMQ indicates a misunderstanding - RabbitMQ
                    // has no polling consumer, only push-based basic_consume()
                    error!(
                        system_id = %self.system_id,
                        configured_mode = ?self.deployment_mode,
                        effective_mode = ?effective_mode,
                        provider = %provider_name,
                        "PollingOnly deployment mode is not supported for {} - this provider \
                         uses push-based delivery only. Using EventDrivenOnly instead. \
                         Please update your configuration.",
                        provider_name
                    );
                }
                DeploymentMode::Hybrid => {
                    // Hybrid with RabbitMQ is a reasonable intent but unnecessary -
                    // broker handles message redelivery, no fallback polling needed
                    warn!(
                        system_id = %self.system_id,
                        configured_mode = ?self.deployment_mode,
                        effective_mode = ?effective_mode,
                        provider = %provider_name,
                        "Hybrid deployment mode adjusted to EventDrivenOnly for {} - this \
                         provider uses push-based delivery with broker-managed redelivery, \
                         fallback polling is unnecessary",
                        provider_name
                    );
                }
                _ => {}
            }
        }

        debug!(
            system_id = %self.system_id,
            configured_mode = ?self.deployment_mode,
            effective_mode = ?effective_mode,
            provider = %provider_name,
            "Initializing worker components"
        );

        match effective_mode {
            DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid => {
                // Convert unified config to listener config format
                let listener_config = WorkerListenerConfig {
                    retry_interval: std::time::Duration::from_secs(
                        self.config.metadata.listener.retry_interval_seconds.into(),
                    ),
                    max_retry_attempts: self.config.metadata.listener.max_retry_attempts,
                    event_timeout: std::time::Duration::from_secs(
                        self.config.metadata.listener.event_timeout_seconds.into(),
                    ),
                    batch_processing: self.config.metadata.listener.batch_processing,
                    connection_timeout: std::time::Duration::from_secs(
                        self.config
                            .metadata
                            .listener
                            .connection_timeout_seconds
                            .into(),
                    ),
                    health_check_interval: std::time::Duration::from_secs(60), // Default from WorkerListenerConfig
                    supported_namespaces: self.supported_namespaces.clone(),
                };

                // Create a notification sender - we'll convert notifications to commands
                // TAS-51: Use configured buffer size for notification channel
                // TAS-61 Phase 6D: Worker-specific mpsc channels are in worker.mpsc_channels
                // TAS-133: Use ChannelFactory for type-safe channel creation
                let buffer_size = self
                    .context
                    .tasker_config
                    .worker
                    .as_ref()
                    .map(|w| w.mpsc_channels.event_systems.event_channel_buffer_size as usize)
                    .expect("Worker configuration required for event channel buffer size");
                let (notification_sender, mut notification_receiver) =
                    ChannelFactory::worker_notification_channel(buffer_size);

                // TAS-51: Initialize channel monitor for observability
                let channel_monitor = ChannelMonitor::new("worker_event_channel", buffer_size);

                // Initialize queue listener for event-driven processing
                self.queue_listener = Some(
                    WorkerQueueListener::new(
                        listener_config,
                        self.context.clone(),
                        notification_sender,
                        channel_monitor.clone(),
                    )
                    .await
                    .map_err(|e| DeploymentModeError::ConfigurationError {
                        message: format!("Failed to initialize queue listener: {}", e),
                    })?,
                );

                // Spawn a task to convert notifications to commands
                let command_sender = self.command_sender.clone();
                let monitor = channel_monitor;
                let statistics = self.statistics.clone();
                tokio::spawn(async move {
                    while let Some(notification) = notification_receiver.recv().await {
                        // TAS-51: Record message receive for channel monitoring
                        monitor.record_receive();

                        // Convert WorkerNotification to WorkerCommand
                        // This is where the FFI event forwarding will happen
                        match notification {
                            WorkerNotification::Event(WorkerQueueEvent::StepMessage(msg_event)) => {
                                // TAS-141: Record event-driven processing
                                // PGMQ signal-only: requires fetch by message ID
                                statistics.record_event_driven();
                                statistics.events_processed.fetch_add(1, Ordering::Relaxed);
                                statistics
                                    .step_messages_processed
                                    .fetch_add(1, Ordering::Relaxed);

                                // Convert to command that will process the step
                                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                if let Err(e) = command_sender
                                    .send(WorkerCommand::ExecuteStepFromEvent {
                                        message_event: msg_event,
                                        resp: resp_tx,
                                    })
                                    .await
                                {
                                    statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                                    error!("Failed to forward step notification to command processor: {}", e);
                                }
                            }
                            WorkerNotification::StepMessageWithPayload(queued_msg) => {
                                // TAS-133: RabbitMQ delivers full messages via push-based basic_consume()
                                // No fetch needed - the message payload is already available
                                statistics.record_event_driven();
                                statistics.events_processed.fetch_add(1, Ordering::Relaxed);
                                statistics
                                    .step_messages_processed
                                    .fetch_add(1, Ordering::Relaxed);

                                // Parse Vec<u8> payload to serde_json::Value
                                let json_result: Result<serde_json::Value, _> =
                                    serde_json::from_slice(&queued_msg.message);

                                match json_result {
                                    Ok(json_value) => {
                                        // Use map to convert QueuedMessage<Vec<u8>> to QueuedMessage<serde_json::Value>
                                        let json_message = queued_msg.map(|_| json_value);

                                        info!(
                                            queue = %json_message.queue_name(),
                                            provider = %json_message.provider_name(),
                                            "Processing RabbitMQ push message via ExecuteStepFromMessage"
                                        );

                                        // Send to actor system via ExecuteStepFromMessage (has full payload)
                                        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                        if let Err(e) = command_sender
                                            .send(WorkerCommand::ExecuteStepFromMessage {
                                                message: json_message,
                                                resp: resp_tx,
                                            })
                                            .await
                                        {
                                            statistics
                                                .events_failed
                                                .fetch_add(1, Ordering::Relaxed);
                                            error!("Failed to forward step message to command processor: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                                        error!(
                                            queue = %queued_msg.queue_name(),
                                            error = %e,
                                            "Failed to parse RabbitMQ message payload as JSON"
                                        );
                                    }
                                }
                            }
                            WorkerNotification::Health(_) => {
                                // Forward health check
                                let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                                if let Err(e) = command_sender
                                    .send(WorkerCommand::HealthCheck { resp: resp_tx })
                                    .await
                                {
                                    error!("Failed to forward health check notification: {}", e);
                                }
                            }
                            _ => {
                                // Other notification types - log but don't process
                            }
                        }
                    }
                });

                info!("🎧 Queue listener initialized for event-driven processing");
            }
            _ => {}
        }

        match effective_mode {
            DeploymentMode::PollingOnly | DeploymentMode::Hybrid => {
                // Convert unified config to poller config format
                let poller_config = WorkerPollerConfig {
                    enabled: self.config.metadata.fallback_poller.enabled,
                    polling_interval: std::time::Duration::from_millis(
                        self.config
                            .metadata
                            .fallback_poller
                            .polling_interval_ms
                            .into(),
                    ),
                    batch_size: self.config.metadata.fallback_poller.batch_size,
                    age_threshold: std::time::Duration::from_secs(
                        self.config
                            .metadata
                            .fallback_poller
                            .age_threshold_seconds
                            .into(),
                    ),
                    max_age: std::time::Duration::from_secs(
                        (self.config.metadata.fallback_poller.max_age_hours * 3600).into(),
                    ),
                    visibility_timeout: std::time::Duration::from_secs(
                        self.config
                            .metadata
                            .fallback_poller
                            .visibility_timeout_seconds
                            .into(),
                    ),
                    supported_namespaces: self.supported_namespaces.clone(),
                };

                // Initialize fallback poller for reliability
                self.fallback_poller = Some(
                    WorkerFallbackPoller::new(
                        poller_config,
                        self.context.clone(),
                        self.command_sender.clone(),
                    )
                    .await
                    .map_err(|e| DeploymentModeError::ConfigurationError {
                        message: format!("Failed to initialize fallback poller: {}", e),
                    })?,
                );

                info!("Fallback poller initialized for reliable processing");
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
        self.deployment_mode
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Acquire)
    }

    fn statistics(&self) -> Self::Statistics {
        let events_processed = self.statistics.events_processed.load(Ordering::Relaxed);

        // Calculate processing rate (events per second)
        let elapsed_secs = self.start_time.elapsed().as_secs_f64();
        let processing_rate = if elapsed_secs > 0.0 {
            events_processed as f64 / elapsed_secs
        } else {
            0.0
        };

        WorkerEventSystemStatistics {
            events_processed,
            events_failed: self.statistics.events_failed.load(Ordering::Relaxed),
            processing_rate,
            average_latency_ms: self.statistics.average_latency_ms(),
            deployment_mode_score: self.statistics.deployment_mode_score(),
            step_messages_processed: self
                .statistics
                .step_messages_processed
                .load(Ordering::Relaxed),
            health_checks_processed: self
                .statistics
                .health_checks_processed
                .load(Ordering::Relaxed),
            config_updates_processed: self
                .statistics
                .config_updates_processed
                .load(Ordering::Relaxed),
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
            "Starting unified worker event system"
        );

        // Initialize components based on deployment mode
        self.initialize_components().await?;

        // Start queue listener if configured
        if let Some(ref mut listener) = self.queue_listener {
            listener
                .start()
                .await
                .map_err(|e| DeploymentModeError::ConfigurationError {
                    message: format!("Failed to start queue listener: {}", e),
                })?;
            info!("🎧 Queue listener started");
        }

        // Start fallback poller if configured
        if let Some(ref mut poller) = self.fallback_poller {
            poller
                .start()
                .await
                .map_err(|e| DeploymentModeError::ConfigurationError {
                    message: format!("Failed to start fallback poller: {}", e),
                })?;
            info!("Fallback poller started");
        }

        self.is_running.store(true, Ordering::Release);

        info!(
            system_id = %self.system_id,
            "Started successfully with unified configuration"
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
            "Stopping unified worker event system"
        );

        // Stop fallback poller (has explicit stop method)
        if let Some(ref mut poller) = self.fallback_poller {
            poller.stop().await;
            info!("Fallback poller stopped");
        }

        // TAS-133: Queue listener needs explicit stop to abort subscription task handles
        if let Some(ref mut listener) = self.queue_listener {
            if let Err(e) = listener.stop().await {
                warn!(error = %e, "Error stopping queue listener");
            } else {
                info!("Queue listener stopped");
            }
        }

        self.is_running.store(false, Ordering::Release);

        info!(
            system_id = %self.system_id,
            "Stopped successfully"
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
