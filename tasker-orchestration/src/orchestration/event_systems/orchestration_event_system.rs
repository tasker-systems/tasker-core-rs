//! # OrchestrationEventSystem - Queue-Level Event System Implementation
//!
//! This module provides the concrete implementation of the EventDrivenSystem trait
//! for orchestration coordination using queue-level events (step results, task requests).
//!
//! This system coordinates the orchestration queue components (listener, fallback poller)
//! with the unified EventDrivenSystem interface, enabling deployment mode management and
//! integration with the EventSystemManager.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tasker_shared::config::orchestration::event_systems::OrchestrationEventSystemConfig;
use tasker_shared::{system_context::SystemContext, TaskerResult};
use tracing::{debug, error, info, warn};

use crate::orchestration::{
    channels::{ChannelFactory, OrchestrationCommandSender},
    commands::OrchestrationCommand,
    orchestration_queues::{
        OrchestrationFallbackPoller, OrchestrationListenerConfig, OrchestrationNotification,
        OrchestrationPollerConfig, OrchestrationQueueEvent, OrchestrationQueueListener,
    },
    OrchestrationCore,
};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus};

use tasker_shared::{EventDrivenSystem, EventSystemStatistics, SystemStatistics};

use super::command_outcome::CommandOutcome;
use super::orchestration_statistics::{OrchestrationComponentStatistics, OrchestrationStatistics};

/// Queue-level event system for orchestration coordination
///
/// This system handles step result notifications and task request events through
/// tasker-pgmq and queue polling, coordinating orchestration operations through
/// the command pattern. It integrates the orchestration queue components with
/// the unified EventDrivenSystem interface.
#[derive(Debug)]
pub struct OrchestrationEventSystem {
    /// System identifier
    system_id: String,

    /// Current deployment mode
    deployment_mode: DeploymentMode,

    /// Queue listener for event-driven coordination
    queue_listener: Option<OrchestrationQueueListener>,

    /// Fallback poller for hybrid/polling modes
    fallback_poller: Option<OrchestrationFallbackPoller>,

    /// System context
    context: Arc<SystemContext>,

    /// Orchestration core - future proofing
    #[expect(
        dead_code,
        reason = "Reserved for future reaper/sweeper processes for missed messages"
    )]
    orchestration_core: Arc<OrchestrationCore>,

    /// Command sender for orchestration operations (TAS-133: NewType wrapper)
    command_sender: OrchestrationCommandSender,

    /// System configuration
    config: OrchestrationEventSystemConfig,

    /// Runtime statistics
    statistics: Arc<OrchestrationStatistics>,

    /// System running state
    is_running: AtomicBool,

    /// Startup timestamp
    started_at: Option<Instant>,

    /// Channel monitor for command send observability (TAS-51)
    command_channel_monitor: ChannelMonitor,
}

impl OrchestrationEventSystem {
    /// Create new orchestration event system
    pub async fn new(
        config: OrchestrationEventSystemConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        command_sender: OrchestrationCommandSender,
        command_channel_monitor: ChannelMonitor,
    ) -> TaskerResult<Self> {
        info!(
            system_id = %config.system_id,
            deployment_mode = ?config.deployment_mode,
            command_channel = %command_channel_monitor.channel_name(),
            "Creating OrchestrationEventSystem with command channel monitoring"
        );

        Ok(Self {
            system_id: config.system_id.clone(),
            deployment_mode: config.deployment_mode,
            queue_listener: None,
            fallback_poller: None,
            context,
            orchestration_core,
            command_sender,
            config,
            statistics: Arc::new(OrchestrationStatistics::default()),
            is_running: AtomicBool::new(false),
            started_at: None,
            command_channel_monitor,
        })
    }

    /// Convert to OrchestrationListenerConfig for queue listener
    fn listener_config(&self) -> OrchestrationListenerConfig {
        // Use configured queue names from system context (no hardcoding)
        let queues = &self.context.tasker_config.common.queues;
        OrchestrationListenerConfig {
            namespace: "orchestration".to_string(),
            monitored_queues: vec![
                queues.orchestration_queues.step_results.clone(),
                queues.orchestration_queues.task_requests.clone(),
            ],
            retry_interval: Duration::from_secs(5),
            max_retry_attempts: 10,
            event_timeout: self.config.visibility_timeout(),
            health_check_interval: self.config.health_check_interval(),
        }
    }

    /// Convert to OrchestrationPollerConfig for fallback poller
    fn poller_config(&self) -> OrchestrationPollerConfig {
        // Use configured queue names from system context (no hardcoding)
        let queues = &self.context.tasker_config.common.queues;
        OrchestrationPollerConfig {
            enabled: true, // Always enabled when instantiated
            polling_interval: self.config.fallback_polling_interval(),
            batch_size: self.config.message_batch_size() as u32,
            age_threshold: Duration::from_secs(5), // Only poll messages >5 seconds old
            max_age: Duration::from_secs(24 * 60 * 60), // Don't poll messages >24 hours old
            monitored_queues: vec![
                queues.orchestration_queues.step_results.clone(),
                queues.orchestration_queues.task_requests.clone(),
            ],
            namespace: "orchestration".to_string(),
            visibility_timeout: self.config.visibility_timeout(),
        }
    }

    /// Record event processing latency
    fn record_latency(&self, latency: Duration) {
        let mut latencies = self
            .statistics
            .processing_latencies
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        latencies.push_back(latency);

        // Keep only recent latencies (last 1000)
        if latencies.len() > 1000 {
            latencies.drain(..500); // Remove oldest half (O(1) with VecDeque)
        }
    }

    /// Get detailed component statistics for monitoring and debugging
    pub async fn component_statistics(&self) -> OrchestrationComponentStatistics {
        let fallback_poller_stats = match &self.fallback_poller {
            Some(p) => Some(p.stats().await),
            None => None,
        };

        let queue_listener_stats = match &self.queue_listener {
            Some(l) => Some(l.stats().await),
            None => None,
        };

        OrchestrationComponentStatistics {
            fallback_poller_stats,
            queue_listener_stats,
            system_uptime: self.started_at.map(|start| start.elapsed()),
            deployment_mode: self.deployment_mode,
        }
    }

    /// Get aggregated system uptime for lifecycle tracking
    pub fn uptime(&self) -> Option<Duration> {
        self.started_at.map(|start| start.elapsed())
    }
}

#[async_trait]
impl EventDrivenSystem for OrchestrationEventSystem {
    type SystemId = String;
    type Event = OrchestrationQueueEvent;
    type Config = OrchestrationEventSystemConfig;
    type Statistics = SystemStatistics;

    fn system_id(&self) -> Self::SystemId {
        self.system_id.clone()
    }

    fn deployment_mode(&self) -> DeploymentMode {
        self.deployment_mode
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    async fn start(&mut self) -> Result<(), DeploymentModeError> {
        if self.is_running() {
            return Err(DeploymentModeError::ConfigurationError {
                message: "OrchestrationEventSystem is already running".to_string(),
            });
        }

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

        info!(
            system_id = %self.system_id,
            configured_mode = ?self.deployment_mode,
            effective_mode = ?effective_mode,
            provider = %provider_name,
            "Starting OrchestrationEventSystem"
        );

        // Create components based on effective deployment mode using command pattern
        match effective_mode {
            DeploymentMode::PollingOnly => {
                self.setup_fallback_poller().await?;
            }

            DeploymentMode::Hybrid => {
                self.setup_listener_and_spawn_loop("orchestration_hybrid_event_channel")
                    .await?;
                self.setup_fallback_poller().await?;
            }

            DeploymentMode::EventDrivenOnly => {
                self.setup_listener_and_spawn_loop("orchestration_event_driven_event_channel")
                    .await?;
            }
            DeploymentMode::Disabled => {
                warn!("OrchestrationEventSystem is disabled");
            }
        }

        self.is_running.store(true, Ordering::Relaxed);
        self.started_at = Some(Instant::now());

        info!(
            system_id = %self.system_id,
            deployment_mode = ?self.deployment_mode,
            "OrchestrationEventSystem started successfully"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), DeploymentModeError> {
        if !self.is_running() {
            return Ok(());
        }

        info!(
            system_id = %self.system_id,
            "Stopping OrchestrationEventSystem"
        );

        // Stop components based on what was created
        if let Some(mut queue_listener) = self.queue_listener.take() {
            queue_listener.stop().await?;
        }

        if let Some(fallback_poller) = self.fallback_poller.take() {
            fallback_poller.stop().await?;
        }

        self.is_running.store(false, Ordering::Relaxed);

        info!(
            system_id = %self.system_id,
            "OrchestrationEventSystem stopped successfully"
        );

        Ok(())
    }

    fn statistics(&self) -> Self::Statistics {
        // Aggregate statistics from child components for comprehensive reporting
        // Use block_in_place to call async method from sync context
        let aggregated_stats = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.statistics
                    .with_component_aggregation(
                        self.fallback_poller.as_ref(),
                        self.queue_listener.as_ref(),
                    )
                    .await
            })
        });

        SystemStatistics {
            events_processed: aggregated_stats.events_processed(),
            events_failed: aggregated_stats.events_failed(),
            processing_rate: aggregated_stats.processing_rate(),
            average_latency_ms: aggregated_stats.average_latency_ms(),
            deployment_mode_score: aggregated_stats.deployment_mode_score(),
        }
    }

    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError> {
        let start_time = Instant::now();

        debug!(
            system_id = %self.system_id,
            event_type = ?event,
            "Processing orchestration queue event"
        );

        match event {
            OrchestrationQueueEvent::StepResult(message_event) => {
                let msg_id = message_event.message_id.clone();
                self.send_command_and_await(
                    |resp| OrchestrationCommand::ProcessStepResultFromMessageEvent {
                        message_event,
                        resp,
                    },
                    &msg_id,
                    "ProcessStepResultFromMessageEvent",
                    CommandOutcome::from_step_result,
                )
                .await?;
            }

            OrchestrationQueueEvent::TaskRequest(message_event) => {
                let msg_id = message_event.message_id.clone();
                self.send_command_and_await(
                    |resp| OrchestrationCommand::InitializeTaskFromMessageEvent {
                        message_event,
                        resp,
                    },
                    &msg_id,
                    "InitializeTaskFromMessageEvent",
                    CommandOutcome::from_task_initialize_result,
                )
                .await?;
            }

            OrchestrationQueueEvent::TaskFinalization(message_event) => {
                let msg_id = message_event.message_id.clone();
                self.send_command_and_await(
                    |resp| OrchestrationCommand::FinalizeTaskFromMessageEvent {
                        message_event,
                        resp,
                    },
                    &msg_id,
                    "FinalizeTaskFromMessageEvent",
                    CommandOutcome::from_task_finalization_result,
                )
                .await?;
            }

            OrchestrationQueueEvent::Unknown { queue_name, .. } => {
                debug!(
                    system_id = %self.system_id,
                    queue_name = %queue_name,
                    "Unknown orchestration queue event received"
                );
            }
        }

        let latency = start_time.elapsed();
        self.record_latency(latency);
        self.statistics
            .events_processed
            .fetch_add(1, Ordering::Relaxed);

        debug!(
            system_id = %self.system_id,
            latency_ms = %latency.as_millis(),
            "Orchestration queue event processed successfully"
        );

        Ok(())
    }

    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
        if !self.is_running() {
            return Err(DeploymentModeError::HealthCheckFailed {
                details: "OrchestrationEventSystem is not running".to_string(),
            });
        }

        // Check component health based on deployment mode
        match self.deployment_mode {
            DeploymentMode::PollingOnly => {
                if let Some(fallback_poller) = &self.fallback_poller {
                    if !fallback_poller.is_healthy() {
                        return Err(DeploymentModeError::HealthCheckFailed {
                            details: "Orchestration fallback poller is not healthy".to_string(),
                        });
                    }
                }
            }

            DeploymentMode::Hybrid => {
                // Check command channel health
                if self.command_sender.is_closed() {
                    return Err(DeploymentModeError::HealthCheckFailed {
                        details: "Command channel is closed - cannot send orchestration commands"
                            .to_string(),
                    });
                }

                // Check fallback poller if available
                if let Some(fallback_poller) = &self.fallback_poller {
                    if !fallback_poller.is_healthy() {
                        warn!(
                            system_id = %self.system_id,
                            "Orchestration fallback poller is not healthy in hybrid mode"
                        );
                    }
                }
            }

            DeploymentMode::EventDrivenOnly => {
                // Check command channel health
                if self.command_sender.is_closed() {
                    return Err(DeploymentModeError::HealthCheckFailed {
                        details: "Command channel is closed - cannot send orchestration commands"
                            .to_string(),
                    });
                }

                // Check queue listener if available
                if let Some(queue_listener) = &self.queue_listener {
                    if !queue_listener.is_healthy() {
                        return Err(DeploymentModeError::HealthCheckFailed {
                            details: "Orchestration queue listener is not healthy".to_string(),
                        });
                    }
                }
            }
            DeploymentMode::Disabled => {
                info!("OrchestrationEventSystem is disabled");
            }
        }

        // Check processing health based on aggregated statistics
        let stats = self.statistics();
        if stats.deployment_mode_score < 0.5 {
            warn!(
                system_id = %self.system_id,
                score = %stats.deployment_mode_score,
                "Low deployment mode effectiveness score"
            );
        }

        // Get detailed component statistics for enhanced health monitoring
        let component_stats = self.component_statistics().await;

        // Check individual component health metrics
        if let Some(poller_stats) = &component_stats.fallback_poller_stats {
            let poller_errors = poller_stats.polling_errors.load(Ordering::Relaxed);
            let poller_cycles = poller_stats.polling_cycles.load(Ordering::Relaxed);

            // Calculate error rate for fallback poller
            if poller_cycles > 0 && poller_errors as f64 / poller_cycles as f64 > 0.1 {
                warn!(
                    system_id = %self.system_id,
                    poller_errors = %poller_errors,
                    poller_cycles = %poller_cycles,
                    "High error rate in fallback poller"
                );
            }
        }

        if let Some(listener_stats) = &component_stats.queue_listener_stats {
            let connection_errors = listener_stats.connection_errors.load(Ordering::Relaxed);
            let total_events = listener_stats.events_received.load(Ordering::Relaxed);

            // Check listener connection health
            if total_events > 0 && connection_errors as f64 / total_events as f64 > 0.05 {
                warn!(
                    system_id = %self.system_id,
                    connection_errors = %connection_errors,
                    total_events = %total_events,
                    "High connection error rate in queue listener"
                );
            }
        }

        debug!(
            system_id = %self.system_id,
            events_processed = %stats.events_processed,
            events_failed = %stats.events_failed,
            processing_rate = %stats.processing_rate,
            uptime = ?component_stats.system_uptime,
            "OrchestrationEventSystem health check passed"
        );

        Ok(DeploymentModeHealthStatus::Healthy)
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }
}

impl OrchestrationEventSystem {
    /// Create a queue listener with event processing loop.
    ///
    /// Shared setup for Hybrid and EventDrivenOnly deployment modes: creates the
    /// notification channel, queue listener, and spawns the event processing loop.
    /// Only the `channel_name` differs between modes.
    async fn setup_listener_and_spawn_loop(
        &mut self,
        channel_name: &str,
    ) -> Result<(), DeploymentModeError> {
        let listener_config = self.listener_config();
        // TAS-51: Use configured buffer size for event channel
        let buffer_size = self
            .context
            .tasker_config
            .orchestration
            .as_ref()
            .map(|o| o.mpsc_channels.event_systems.event_channel_buffer_size)
            .unwrap_or(10000);
        // TAS-133: Use ChannelFactory for type-safe channel creation
        let (event_sender, mut event_receiver) =
            ChannelFactory::orchestration_notification_channel(buffer_size as usize);

        // TAS-51: Initialize channel monitor for observability
        let channel_monitor = ChannelMonitor::new(channel_name, buffer_size as usize);

        let mut queue_listener = OrchestrationQueueListener::new(
            listener_config,
            self.context.clone(),
            event_sender,
            channel_monitor.clone(),
        )
        .await?;

        queue_listener.start().await?;
        self.queue_listener = Some(queue_listener);

        // Spawn event processing loop
        let context = self.context.clone();
        let command_sender = self.command_sender.clone();
        let command_channel_monitor = self.command_channel_monitor.clone();
        let statistics = self.statistics.clone();
        let monitor = channel_monitor;

        tokio::spawn(async move {
            while let Some(notification) = event_receiver.recv().await {
                // TAS-51: Record message receive for channel monitoring
                monitor.record_receive();

                Self::process_orchestration_notification(
                    notification,
                    &context,
                    &command_sender,
                    &command_channel_monitor,
                    &statistics,
                )
                .await;
            }
        });

        Ok(())
    }

    /// Create and start a fallback poller for polling/hybrid modes.
    async fn setup_fallback_poller(&mut self) -> Result<(), DeploymentModeError> {
        let poller_config = self.poller_config();
        // TAS-133e: Removed pgmq_client param - now accessed via MessagingProvider
        let fallback_poller = OrchestrationFallbackPoller::new(
            poller_config,
            self.context.clone(),
            self.command_sender.clone(),
        )
        .await?;

        fallback_poller.start().await?;
        self.fallback_poller = Some(fallback_poller);
        Ok(())
    }

    /// Send a command through the command channel and await its result.
    ///
    /// This helper eliminates the triplicated send-and-await pattern in `process_event`.
    /// It handles: channel send with monitoring, response waiting, result classification
    /// via `CommandOutcome`, stats tracking, and error propagation.
    async fn send_command_and_await<T>(
        &self,
        build_command: impl FnOnce(
            crate::orchestration::commands::CommandResponder<T>,
        ) -> OrchestrationCommand,
        msg_id: &(impl std::fmt::Display + ?Sized),
        event_label: &str,
        classify: impl FnOnce(&T) -> CommandOutcome,
    ) -> Result<(), DeploymentModeError>
    where
        T: std::fmt::Debug,
    {
        let (command_tx, command_rx) = tokio::sync::oneshot::channel();
        let command = build_command(command_tx);

        // Send command with channel monitoring (TAS-51)
        match self.command_sender.send(command).await {
            Ok(_) => {
                if self.command_channel_monitor.record_send_success() {
                    self.command_channel_monitor
                        .check_and_warn_saturation(self.command_sender.capacity());
                }
            }
            Err(e) => {
                error!(
                    system_id = %self.system_id,
                    msg_id = %msg_id,
                    error = %e,
                    "Failed to send {} command", event_label
                );
                self.statistics
                    .events_failed
                    .fetch_add(1, Ordering::Relaxed);
                return Err(DeploymentModeError::ConfigurationError {
                    message: format!("Failed to send orchestration command: {e}"),
                });
            }
        }

        // Wait for command processing result
        match command_rx.await {
            Ok(Ok(result)) => {
                let outcome = classify(&result);
                match &outcome {
                    CommandOutcome::Success => {
                        debug!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            "{} completed successfully", event_label
                        );
                        self.statistics
                            .operations_coordinated
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    CommandOutcome::Failed(error) => {
                        warn!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %error,
                            "{} failed", event_label
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("{event_label} failed: {error}"),
                        });
                    }
                    CommandOutcome::Skipped(reason) => {
                        debug!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            reason = %reason,
                            "{} skipped", event_label
                        );
                        self.statistics
                            .operations_coordinated
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(())
            }
            Ok(Err(e)) => {
                error!(
                    system_id = %self.system_id,
                    msg_id = %msg_id,
                    error = %e,
                    "{} command processing failed", event_label
                );
                self.statistics
                    .events_failed
                    .fetch_add(1, Ordering::Relaxed);
                Err(DeploymentModeError::ConfigurationError {
                    message: format!("Command processing failed: {e}"),
                })
            }
            Err(e) => {
                error!(
                    system_id = %self.system_id,
                    msg_id = %msg_id,
                    error = %e,
                    "Failed to receive {} command response", event_label
                );
                self.statistics
                    .events_failed
                    .fetch_add(1, Ordering::Relaxed);
                Err(DeploymentModeError::ConfigurationError {
                    message: format!("Command response failed: {e}"),
                })
            }
        }
    }

    /// Send a fire-and-forget command through the command channel.
    ///
    /// Creates a oneshot channel (receiver immediately dropped), sends the command,
    /// and updates statistics. Used by `process_orchestration_notification` for
    /// notification-driven command dispatch where no response is awaited.
    async fn fire_and_forget_command<T>(
        build_command: impl FnOnce(
            crate::orchestration::commands::CommandResponder<T>,
        ) -> OrchestrationCommand,
        command_sender: &OrchestrationCommandSender,
        command_channel_monitor: &ChannelMonitor,
        statistics: &Arc<OrchestrationStatistics>,
        label: &str,
    ) {
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        match command_sender.send(build_command(resp_tx)).await {
            Ok(_) => {
                if command_channel_monitor.record_send_success() {
                    command_channel_monitor.check_and_warn_saturation(command_sender.capacity());
                }
                statistics
                    .operations_coordinated
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                warn!(error = %e, "Failed to send {} command", label);
                statistics.events_failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Process orchestration notifications from the queue listener
    async fn process_orchestration_notification(
        notification: OrchestrationNotification,
        _context: &Arc<SystemContext>,
        command_sender: &OrchestrationCommandSender,
        command_channel_monitor: &ChannelMonitor,
        statistics: &Arc<OrchestrationStatistics>,
    ) {
        match notification {
            OrchestrationNotification::StepResultWithPayload(queued_msg) => {
                // TAS-133: RabbitMQ delivers full messages via push-based basic_consume()
                let queue_name = queued_msg.queue_name().to_string();

                let json_result: Result<serde_json::Value, _> =
                    serde_json::from_slice(&queued_msg.message);

                match json_result {
                    Ok(json_value) => {
                        let json_message = queued_msg.map(|_| json_value);

                        info!(
                            queue = %queue_name,
                            provider = %json_message.provider_name(),
                            "Processing RabbitMQ push message via ProcessStepResultFromMessage"
                        );

                        Self::fire_and_forget_command(
                            |resp| OrchestrationCommand::ProcessStepResultFromMessage {
                                message: json_message,
                                resp,
                            },
                            command_sender,
                            command_channel_monitor,
                            statistics,
                            "ProcessStepResultFromMessage",
                        )
                        .await;
                    }
                    Err(e) => {
                        error!(
                            queue = %queue_name,
                            error = %e,
                            "Failed to parse RabbitMQ step result message payload as JSON"
                        );
                        statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            OrchestrationNotification::Event(event) => match event {
                OrchestrationQueueEvent::StepResult(message_event) => {
                    debug!(
                        msg_id = %message_event.message_id,
                        namespace = %message_event.namespace,
                        "Processing step result event from orchestration queue (signal-only)"
                    );

                    Self::fire_and_forget_command(
                        |resp| OrchestrationCommand::ProcessStepResultFromMessageEvent {
                            message_event,
                            resp,
                        },
                        command_sender,
                        command_channel_monitor,
                        statistics,
                        "ProcessStepResultFromMessageEvent",
                    )
                    .await;
                }

                OrchestrationQueueEvent::TaskRequest(message_event) => {
                    debug!(
                        msg_id = %message_event.message_id,
                        namespace = %message_event.namespace,
                        "Processing task request event from orchestration queue"
                    );

                    Self::fire_and_forget_command(
                        |resp| OrchestrationCommand::InitializeTaskFromMessageEvent {
                            message_event,
                            resp,
                        },
                        command_sender,
                        command_channel_monitor,
                        statistics,
                        "InitializeTaskFromMessageEvent",
                    )
                    .await;
                }

                OrchestrationQueueEvent::TaskFinalization(message_event) => {
                    debug!(
                        msg_id = %message_event.message_id,
                        namespace = %message_event.namespace,
                        "Processing task finalization event from orchestration queue"
                    );

                    Self::fire_and_forget_command(
                        |resp| OrchestrationCommand::FinalizeTaskFromMessageEvent {
                            message_event,
                            resp,
                        },
                        command_sender,
                        command_channel_monitor,
                        statistics,
                        "FinalizeTaskFromMessageEvent",
                    )
                    .await;
                }

                OrchestrationQueueEvent::Unknown { queue_name, .. } => {
                    warn!(
                        queue = %queue_name,
                        "Received unknown orchestration queue event"
                    );
                    statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                }
            },

            OrchestrationNotification::ConnectionError(error_msg) => {
                error!(
                    error = %error_msg,
                    "Orchestration queue listener connection error"
                );
                statistics.events_failed.fetch_add(1, Ordering::Relaxed);
            }

            OrchestrationNotification::Reconnected => {
                info!("Orchestration queue listener reconnected successfully");
            }
        }

        // Update processing timestamp (lock-free)
        statistics.last_processing_time_epoch_nanos.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos() as u64,
            Ordering::Relaxed,
        );
    }
}
