//! # OrchestrationEventSystem - Queue-Level Event System Implementation
//!
//! This module provides the concrete implementation of the EventDrivenSystem trait
//! for orchestration coordination using queue-level events (step results, task requests).
//!
//! This system coordinates the orchestration queue components (listener, fallback poller)
//! with the unified EventDrivenSystem interface, enabling deployment mode management and
//! integration with the EventSystemManager.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tasker_shared::config::orchestration::event_systems::OrchestrationEventSystemConfig;
use tasker_shared::{system_context::SystemContext, TaskerResult};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::orchestration::{
    command_processor::OrchestrationCommand,
    orchestration_queues::{
        OrchestrationFallbackPoller, OrchestrationListenerConfig, OrchestrationListenerStats,
        OrchestrationNotification, OrchestrationPollerConfig, OrchestrationPollerStats,
        OrchestrationQueueEvent, OrchestrationQueueListener,
    },
    OrchestrationCore,
};
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus};

use crate::orchestration::command_processor::{
    StepProcessResult, TaskFinalizationResult, TaskInitializeResult,
};
use tasker_shared::{EventDrivenSystem, EventSystemStatistics, SystemStatistics};

/// Queue-level event system for orchestration coordination
///
/// This system handles step result notifications and task request events through
/// pgmq-notify and queue polling, coordinating orchestration operations through
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
    #[allow(dead_code)]
    orchestration_core: Arc<OrchestrationCore>,

    /// Command sender for orchestration operations
    command_sender: mpsc::Sender<OrchestrationCommand>,

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

/// Runtime statistics for orchestration event system
#[derive(Debug, Default)]
pub struct OrchestrationStatistics {
    /// Events processed counter (system-level)
    events_processed: AtomicU64,
    /// Events failed counter (system-level)
    events_failed: AtomicU64,
    /// Operations coordinated counter (system-level)
    operations_coordinated: AtomicU64,
    /// Last processing timestamp (system-level)
    last_processing_time: std::sync::Mutex<Option<Instant>>,
    /// Processing latencies for rate calculation (system-level)
    processing_latencies: std::sync::Mutex<Vec<Duration>>,
}

impl Clone for OrchestrationStatistics {
    fn clone(&self) -> Self {
        Self {
            events_processed: AtomicU64::new(self.events_processed.load(Ordering::Relaxed)),
            events_failed: AtomicU64::new(self.events_failed.load(Ordering::Relaxed)),
            operations_coordinated: AtomicU64::new(
                self.operations_coordinated.load(Ordering::Relaxed),
            ),
            last_processing_time: std::sync::Mutex::new(*self.last_processing_time.lock().unwrap()),
            processing_latencies: std::sync::Mutex::new(
                self.processing_latencies.lock().unwrap().clone(),
            ),
        }
    }
}

impl EventSystemStatistics for OrchestrationStatistics {
    fn events_processed(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    fn events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    fn processing_rate(&self) -> f64 {
        let latencies = self.processing_latencies.lock().unwrap();
        if latencies.is_empty() {
            return 0.0;
        }

        // Calculate events per second based on recent latencies
        let recent_latencies: Vec<_> = latencies.iter().rev().take(100).collect();
        if recent_latencies.is_empty() {
            return 0.0;
        }

        let total_time: Duration = recent_latencies.iter().copied().sum();
        if total_time.as_secs_f64() > 0.0 {
            recent_latencies.len() as f64 / total_time.as_secs_f64()
        } else {
            0.0
        }
    }

    fn average_latency_ms(&self) -> f64 {
        let latencies = self.processing_latencies.lock().unwrap();
        if latencies.is_empty() {
            return 0.0;
        }

        let sum: Duration = latencies.iter().sum();
        sum.as_millis() as f64 / latencies.len() as f64
    }

    fn deployment_mode_score(&self) -> f64 {
        // Score based on success rate and processing efficiency
        let total_events = self.events_processed() + self.events_failed();
        if total_events == 0 {
            return 1.0; // No events yet, assume perfect
        }

        let success_rate = self.events_processed() as f64 / total_events as f64;
        let latency = self.average_latency_ms();

        // High score for high success rate and low latency
        let latency_score = if latency > 0.0 { 100.0 / latency } else { 1.0 };
        (success_rate + latency_score.min(1.0)) / 2.0
    }
}

impl OrchestrationStatistics {
    /// Create statistics aggregated from component statistics
    pub async fn with_component_aggregation(
        &self,
        fallback_poller: Option<&OrchestrationFallbackPoller>,
        queue_listener: Option<&OrchestrationQueueListener>,
    ) -> OrchestrationStatistics {
        let aggregated = self.clone();

        // Aggregate fallback poller statistics
        if let Some(poller) = fallback_poller {
            let poller_stats = poller.stats().await;
            // Add poller-specific events to system total
            let poller_processed = poller_stats.messages_processed.load(Ordering::Relaxed);
            aggregated
                .events_processed
                .fetch_add(poller_processed, Ordering::Relaxed);

            // Add poller errors to system failed events
            let poller_errors = poller_stats.polling_errors.load(Ordering::Relaxed);
            aggregated
                .events_failed
                .fetch_add(poller_errors, Ordering::Relaxed);
        }

        // Aggregate queue listener statistics
        if let Some(listener) = queue_listener {
            let listener_stats = listener.stats().await;
            // Add listener-specific events to system total
            let listener_processed = listener_stats.events_received.load(Ordering::Relaxed);
            aggregated
                .events_processed
                .fetch_add(listener_processed, Ordering::Relaxed);

            // Add listener errors to system failed events
            let listener_errors = listener_stats.connection_errors.load(Ordering::Relaxed);
            aggregated
                .events_failed
                .fetch_add(listener_errors, Ordering::Relaxed);
        }

        aggregated
    }
}

/// Detailed component statistics for monitoring and debugging
#[derive(Debug)]
pub struct OrchestrationComponentStatistics {
    /// Fallback poller statistics (if active)
    pub fallback_poller_stats: Option<OrchestrationPollerStats>,
    /// Queue listener statistics (if active)
    pub queue_listener_stats: Option<OrchestrationListenerStats>,
    /// System uptime since start
    pub system_uptime: Option<Duration>,
    /// Current deployment mode
    pub deployment_mode: DeploymentMode,
}

impl OrchestrationEventSystem {
    /// Create new orchestration event system
    pub async fn new(
        config: OrchestrationEventSystemConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        command_sender: mpsc::Sender<OrchestrationCommand>,
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
        OrchestrationListenerConfig {
            namespace: "orchestration".to_string(),
            monitored_queues: vec![
                "orchestration_step_results_queue".to_string(),
                "orchestration_task_requests_queue".to_string(),
            ],
            retry_interval: Duration::from_secs(5),
            max_retry_attempts: 10,
            event_timeout: self.config.visibility_timeout(),
            health_check_interval: self.config.health_check_interval(),
        }
    }

    /// Convert to OrchestrationPollerConfig for fallback poller
    fn poller_config(&self) -> OrchestrationPollerConfig {
        OrchestrationPollerConfig {
            enabled: true, // Always enabled when instantiated
            polling_interval: self.config.fallback_polling_interval(),
            batch_size: self.config.message_batch_size() as u32,
            age_threshold: Duration::from_secs(5), // Only poll messages >5 seconds old
            max_age: Duration::from_secs(24 * 60 * 60), // Don't poll messages >24 hours old
            monitored_queues: vec![
                "orchestration_step_results_queue".to_string(),
                "orchestration_task_requests_queue".to_string(),
            ],
            namespace: "orchestration".to_string(),
            visibility_timeout: self.config.visibility_timeout(),
        }
    }

    /// Record event processing latency
    fn record_latency(&self, latency: Duration) {
        let mut latencies = self.statistics.processing_latencies.lock().unwrap();
        latencies.push(latency);

        // Keep only recent latencies (last 1000)
        if latencies.len() > 1000 {
            latencies.drain(0..500); // Remove oldest half
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

        info!(
            system_id = %self.system_id,
            deployment_mode = ?self.deployment_mode,
            "Starting OrchestrationEventSystem"
        );

        // Create components based on deployment mode using command pattern
        match self.deployment_mode {
            DeploymentMode::PollingOnly => {
                // Only create fallback poller - sends commands instead of events
                let poller_config = self.poller_config();
                let fallback_poller = OrchestrationFallbackPoller::new(
                    poller_config,
                    self.context.clone(),
                    self.command_sender.clone(),
                    self.context.message_client(),
                )
                .await?;

                fallback_poller.start().await?;
                self.fallback_poller = Some(fallback_poller);
            }

            DeploymentMode::Hybrid => {
                // Create both queue listener (primary) and fallback poller (backup) for hybrid reliability
                let listener_config = self.listener_config();
                // TAS-51: Use configured buffer size for event channel
                let buffer_size = self
                    .context
                    .tasker_config
                    .orchestration
                    .as_ref()
                    .map(|o| o.mpsc_channels.event_systems.event_channel_buffer_size)
                    .unwrap_or(10000);
                let (event_sender, mut event_receiver) = mpsc::channel(buffer_size as usize);

                // TAS-51: Initialize channel monitor for observability
                let channel_monitor =
                    ChannelMonitor::new("orchestration_hybrid_event_channel", buffer_size as usize);

                let mut queue_listener = OrchestrationQueueListener::new(
                    listener_config,
                    self.context.clone(),
                    event_sender,
                    channel_monitor.clone(),
                )
                .await?;

                queue_listener.start().await?;
                self.queue_listener = Some(queue_listener);

                // Create fallback poller for backup reliability
                let poller_config = self.poller_config();
                let fallback_poller = OrchestrationFallbackPoller::new(
                    poller_config,
                    self.context.clone(),
                    self.command_sender.clone(),
                    self.context.message_client(),
                )
                .await?;

                fallback_poller.start().await?;
                self.fallback_poller = Some(fallback_poller);

                // Start event processing loop for listener events
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
            }

            DeploymentMode::EventDrivenOnly => {
                // Pure event-driven mode - create queue listener for orchestration queue events
                let listener_config = self.listener_config();
                // TAS-51: Use configured buffer size for event channel
                let buffer_size = self
                    .context
                    .tasker_config
                    .orchestration
                    .as_ref()
                    .map(|o| o.mpsc_channels.event_systems.event_channel_buffer_size)
                    .unwrap_or(10000);
                let (event_sender, mut event_receiver) = mpsc::channel(buffer_size as usize);

                // TAS-51: Initialize channel monitor for observability
                let channel_monitor = ChannelMonitor::new(
                    "orchestration_event_driven_event_channel",
                    buffer_size as usize,
                );

                let mut queue_listener = OrchestrationQueueListener::new(
                    listener_config,
                    self.context.clone(),
                    event_sender,
                    channel_monitor.clone(),
                )
                .await?;

                queue_listener.start().await?;

                // Start event processing loop
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

                self.queue_listener = Some(queue_listener);
            }
            DeploymentMode::Disabled => {
                // Handle disabled mode
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
            OrchestrationQueueEvent::StepResult(message_ready_event) => {
                let msg_id = message_ready_event.msg_id;
                let (command_tx, command_rx) = tokio::sync::oneshot::channel();

                let command = OrchestrationCommand::ProcessStepResultFromMessageEvent {
                    message_event: message_ready_event,
                    resp: command_tx,
                };

                // Send command to orchestration processor with monitoring (TAS-51)
                match self.command_sender.send(command).await {
                    Ok(_) => {
                        // TAS-51: Record send success and check saturation
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
                            "Failed to send ProcessStepResult command"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Failed to send orchestration command: {}", e),
                        });
                    }
                }

                // Wait for command processing result
                match command_rx.await {
                    Ok(Ok(result)) => {
                        match result {
                            StepProcessResult::Success { message } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    message = %message,
                                    "ProcessStepResultFromMessageEvent command completed successfully"
                                );

                                // Track statistics
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            StepProcessResult::Failed { error } => {
                                warn!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    error = %error,
                                    "ProcessStepResultFromMessageEvent command failed"
                                );
                                self.statistics
                                    .events_failed
                                    .fetch_add(1, Ordering::Relaxed);
                                return Err(DeploymentModeError::ConfigurationError {
                                    message: format!("Step processing failed: {}", error),
                                });
                            }
                            StepProcessResult::Skipped { reason } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    reason = %reason,
                                    "ProcessStepResultFromMessageEvent command skipped"
                                );
                                // Still count as successful operation
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "ProcessStepResultFromMessageEvent command failed"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command processing failed: {}", e),
                        });
                    }
                    Err(e) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "Failed to receive ProcessStepResultFromMessageEvent command response"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command response failed: {}", e),
                        });
                    }
                }
            }

            OrchestrationQueueEvent::TaskRequest(message_ready_event) => {
                let msg_id = message_ready_event.msg_id;
                let (command_tx, command_rx) = tokio::sync::oneshot::channel();

                let command = OrchestrationCommand::InitializeTaskFromMessageEvent {
                    message_event: message_ready_event,
                    resp: command_tx,
                };

                // Send command to orchestration processor with monitoring (TAS-51)
                match self.command_sender.send(command).await {
                    Ok(_) => {
                        // TAS-51: Record send success and check saturation
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
                            "Failed to send InitializeTaskFromMessageEvent command"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Failed to send orchestration command: {}", e),
                        });
                    }
                }

                // Wait for command processing result
                match command_rx.await {
                    Ok(Ok(result)) => {
                        match result {
                            TaskInitializeResult::Success { task_uuid, message } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    task_uuid = %task_uuid,
                                    message = %message,
                                    "InitializeTaskFromMessageEvent command completed successfully"
                                );

                                // Track statistics
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            TaskInitializeResult::Failed { error } => {
                                warn!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    error = %error,
                                    "InitializeTaskFromMessageEvent command failed"
                                );
                                self.statistics
                                    .events_failed
                                    .fetch_add(1, Ordering::Relaxed);
                                return Err(DeploymentModeError::ConfigurationError {
                                    message: format!("Task Initialization failed: {}", error),
                                });
                            }
                            TaskInitializeResult::Skipped { reason } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    reason = %reason,
                                    "InitializeTaskFromMessageEvent command skipped"
                                );
                                // Still count as successful operation
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "InitializeTaskFromMessageEvent command failed"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command processing failed: {}", e),
                        });
                    }
                    Err(e) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "Failed to receive InitializeTaskFromMessageEvent command response"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command response failed: {}", e),
                        });
                    }
                }
            }

            OrchestrationQueueEvent::TaskFinalization(message_ready_event) => {
                let msg_id = message_ready_event.msg_id;
                let namespace = message_ready_event.namespace.clone();
                let (command_tx, command_rx) = tokio::sync::oneshot::channel();

                let command = OrchestrationCommand::FinalizeTaskFromMessageEvent {
                    message_event: message_ready_event,
                    resp: command_tx,
                };

                // Send command to orchestration processor with monitoring (TAS-51)
                match self.command_sender.send(command).await {
                    Ok(_) => {
                        // TAS-51: Record send success and check saturation
                        if self.command_channel_monitor.record_send_success() {
                            self.command_channel_monitor
                                .check_and_warn_saturation(self.command_sender.capacity());
                        }
                    }
                    Err(e) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            namespace = %namespace,
                            error = %e,
                            "Failed to send FinalizeTaskFromMessageEvent command"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Failed to send orchestration command: {}", e),
                        });
                    }
                }

                // Wait for command processing result
                match command_rx.await {
                    Ok(Ok(result)) => {
                        match result {
                            TaskFinalizationResult::Success {
                                task_uuid,
                                final_status,
                                completion_time,
                            } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    task_uuid_completed = %task_uuid,
                                    final_status = %final_status,
                                    completion_time = format!("{:?}", completion_time),
                                    "FinalizeTaskFromMessageEvent command completed successfully"
                                );

                                // Track statistics
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            TaskFinalizationResult::Failed { error } => {
                                warn!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    error = %error,
                                    "FinalizeTaskFromMessageEvent command failed"
                                );
                                self.statistics
                                    .events_failed
                                    .fetch_add(1, Ordering::Relaxed);
                                return Err(DeploymentModeError::ConfigurationError {
                                    message: format!("Task Finalization failed: {}", error),
                                });
                            }
                            TaskFinalizationResult::NotClaimed {
                                reason,
                                already_claimed_by,
                            } => {
                                debug!(
                                    system_id = %self.system_id,
                                    msg_id = %msg_id,
                                    reason = %reason,
                                    already_claimed_by = %already_claimed_by.unwrap_or(uuid::Uuid::nil()).to_string(),
                                    "FinalizeTaskFromMessageEvent command not claimed"
                                );
                                // Still count as successful operation
                                self.statistics
                                    .operations_coordinated
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "InitializeTaskFromMessageEvent command failed"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command processing failed: {}", e),
                        });
                    }
                    Err(e) => {
                        error!(
                            system_id = %self.system_id,
                            msg_id = %msg_id,
                            error = %e,
                            "Failed to receive InitializeTaskFromMessageEvent command response"
                        );
                        self.statistics
                            .events_failed
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command response failed: {}", e),
                        });
                    }
                }
            }

            OrchestrationQueueEvent::Unknown {
                queue_name,
                payload,
            } => {
                debug!(
                    system_id = %self.system_id,
                    queue_name = %queue_name,
                    payload = %payload,
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
    /// Process orchestration notifications from the queue listener
    async fn process_orchestration_notification(
        notification: OrchestrationNotification,
        _context: &Arc<SystemContext>,
        command_sender: &mpsc::Sender<OrchestrationCommand>,
        command_channel_monitor: &ChannelMonitor,
        statistics: &Arc<OrchestrationStatistics>,
    ) {
        match notification {
            OrchestrationNotification::Event(event) => match event {
                OrchestrationQueueEvent::StepResult(message_ready_event) => {
                    let msg_id = message_ready_event.msg_id;
                    let namespace = message_ready_event.namespace.clone();

                    debug!(
                        msg_id = %msg_id,
                        namespace = %namespace,
                        "Processing step result event from orchestration queue"
                    );

                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

                    match command_sender
                        .send(OrchestrationCommand::ProcessStepResultFromMessageEvent {
                            message_event: message_ready_event,
                            resp: resp_tx,
                        })
                        .await
                    {
                        Ok(_) => {
                            // TAS-51: Record send success and check saturation
                            if command_channel_monitor.record_send_success() {
                                command_channel_monitor
                                    .check_and_warn_saturation(command_sender.capacity());
                            }
                            statistics
                                .operations_coordinated
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            warn!(
                                msg_id = %msg_id,
                                error = %e,
                                "Failed to send ProcessStepResult command"
                            );
                            statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                OrchestrationQueueEvent::TaskRequest(message_ready_event) => {
                    let msg_id = message_ready_event.msg_id;
                    let namespace = message_ready_event.namespace.clone();
                    debug!(
                        msg_id = %msg_id,
                        namespace = %namespace,
                        "Processing task request event from orchestration queue"
                    );

                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

                    match command_sender
                        .send(OrchestrationCommand::InitializeTaskFromMessageEvent {
                            message_event: message_ready_event,
                            resp: resp_tx,
                        })
                        .await
                    {
                        Ok(_) => {
                            // TAS-51: Record send success and check saturation
                            if command_channel_monitor.record_send_success() {
                                command_channel_monitor
                                    .check_and_warn_saturation(command_sender.capacity());
                            }
                            statistics
                                .operations_coordinated
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            warn!(
                                msg_id = %msg_id,
                                namespace = %namespace,
                                error = %e,
                                "Failed to send InitializeTaskFromMessageEvent command"
                            );
                            statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                OrchestrationQueueEvent::TaskFinalization(message_ready_event) => {
                    let msg_id = message_ready_event.msg_id;
                    let namespace = message_ready_event.namespace.clone();
                    debug!(
                        msg_id = %msg_id,
                        namespace = %namespace,
                        "Processing task request event from orchestration queue"
                    );

                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

                    match command_sender
                        .send(OrchestrationCommand::FinalizeTaskFromMessageEvent {
                            message_event: message_ready_event,
                            resp: resp_tx,
                        })
                        .await
                    {
                        Ok(_) => {
                            // TAS-51: Record send success and check saturation
                            if command_channel_monitor.record_send_success() {
                                command_channel_monitor
                                    .check_and_warn_saturation(command_sender.capacity());
                            }
                            statistics
                                .operations_coordinated
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            warn!(
                                msg_id = %msg_id,
                                namespace = %namespace,
                                error = %e,
                                "Failed to send FinalizeTaskFromMessageEvent command"
                            );
                            statistics.events_failed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }

                OrchestrationQueueEvent::Unknown {
                    queue_name,
                    payload,
                } => {
                    warn!(
                        queue = %queue_name,
                        payload = %payload,
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
                // Could reset error counters or adjust health metrics here
            }
        }

        // Update processing timestamp
        *statistics.last_processing_time.lock().unwrap() = Some(Instant::now());
    }
}
