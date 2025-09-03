//! # TaskReadinessEventSystem - Database-Level Event System Implementation
//!
//! This module provides the concrete implementation of the EventDrivenSystem trait
//! for task readiness coordination using database-level events (PostgreSQL LISTEN/NOTIFY).
//!
//! This system coordinates the various task readiness components (listener, coordinator,
//! fallback poller) with the unified EventDrivenSystem interface, enabling deployment
//! mode management and integration with the EventSystemManager.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{system_context::SystemContext, TaskerError, TaskerResult};

use crate::orchestration::{
    command_processor::{OrchestrationCommand, TaskReadinessResult}, // TAS-43: Command pattern integration
    task_readiness::{
        // coordinator and enhanced_coordinator imports removed - TAS-43 uses command pattern
        events::{ReadinessTrigger, TaskReadinessEvent}, // Added ReadinessTrigger import
        fallback_poller::{ReadinessFallbackConfig, ReadinessFallbackPoller},
        listener::{TaskReadinessListener, TaskReadinessNotification},
    },
    OrchestrationCore,
};
use tasker_shared::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus};

use tasker_shared::{EventDrivenSystem, EventSystemStatistics, SystemStatistics};

use tasker_shared::config::event_systems::TaskReadinessEventSystemConfig;

/// Database-level event system for task readiness coordination
///
/// This system handles task readiness notifications through PostgreSQL LISTEN/NOTIFY,
/// coordinating task claiming and step enqueueing based on database triggers.
/// It integrates the various task readiness components with the unified EventDrivenSystem interface.
pub struct TaskReadinessEventSystem {
    /// System identifier
    system_id: String,

    /// Current deployment mode
    deployment_mode: DeploymentMode,

    // Coordinators removed - TAS-43 uses command pattern instead of direct coordination
    /// Fallback poller for hybrid mode
    fallback_poller: Option<ReadinessFallbackPoller>,

    /// System context
    context: Arc<SystemContext>,

    /// Orchestration core
    orchestration_core: Arc<OrchestrationCore>,

    /// System configuration
    config: TaskReadinessEventSystemConfig,

    /// Runtime statistics
    statistics: Arc<TaskReadinessStatistics>,

    /// System running state
    is_running: AtomicBool,

    /// Startup timestamp
    started_at: Option<Instant>,

    /// Command sender for orchestration commands (TAS-43)
    command_sender: mpsc::Sender<OrchestrationCommand>,
}

/// Runtime statistics for task readiness event system
#[derive(Debug, Default)]
pub struct TaskReadinessStatistics {
    /// Events processed counter
    events_processed: AtomicU64,
    /// Events failed counter
    events_failed: AtomicU64,
    /// Tasks claimed counter
    tasks_claimed: AtomicU64,
    /// Tasks enqueued counter
    tasks_enqueued: AtomicU64,
    /// Last processing timestamp
    last_processing_time: std::sync::Mutex<Option<Instant>>,
    /// Processing latencies for rate calculation
    processing_latencies: std::sync::Mutex<Vec<Duration>>,
}

impl Clone for TaskReadinessStatistics {
    fn clone(&self) -> Self {
        Self {
            events_processed: AtomicU64::new(self.events_processed.load(Ordering::Relaxed)),
            events_failed: AtomicU64::new(self.events_failed.load(Ordering::Relaxed)),
            tasks_claimed: AtomicU64::new(self.tasks_claimed.load(Ordering::Relaxed)),
            tasks_enqueued: AtomicU64::new(self.tasks_enqueued.load(Ordering::Relaxed)),
            last_processing_time: std::sync::Mutex::new(
                self.last_processing_time.lock().unwrap().clone(),
            ),
            processing_latencies: std::sync::Mutex::new(
                self.processing_latencies.lock().unwrap().clone(),
            ),
        }
    }
}

impl EventSystemStatistics for TaskReadinessStatistics {
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
        // Score based on success rate and task processing efficiency
        let total_events = self.events_processed() + self.events_failed();
        if total_events == 0 {
            return 1.0; // No events yet, assume perfect
        }

        let success_rate = self.events_processed() as f64 / total_events as f64;
        let tasks_claimed = self.tasks_claimed.load(Ordering::Relaxed);
        let tasks_enqueued = self.tasks_enqueued.load(Ordering::Relaxed);

        // Factor in task processing efficiency
        let task_efficiency = if tasks_claimed > 0 {
            tasks_enqueued as f64 / tasks_claimed as f64
        } else {
            1.0
        };

        let latency = self.average_latency_ms();
        let latency_score = if latency > 0.0 { 10.0 / latency } else { 1.0 };

        // Combine success rate, task efficiency, and latency
        (success_rate + task_efficiency.min(1.0) + latency_score.min(1.0)) / 3.0
    }
}

impl TaskReadinessEventSystem {
    /// Create new task readiness event system
    pub async fn new(
        config: TaskReadinessEventSystemConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        command_sender: mpsc::Sender<OrchestrationCommand>, // TAS-43: Command pattern integration
    ) -> TaskerResult<Self> {
        info!(
            system_id = %config.system_id,
            deployment_mode = ?config.deployment_mode,
            "Creating TaskReadinessEventSystem"
        );

        Ok(Self {
            system_id: config.system_id.clone(),
            deployment_mode: config.deployment_mode.clone(),
            // coordinator and enhanced_coordinator fields removed - TAS-43 command pattern
            fallback_poller: None,
            context,
            orchestration_core,
            config,
            statistics: Arc::new(TaskReadinessStatistics::default()),
            is_running: AtomicBool::new(false),
            started_at: None,
            command_sender, // TAS-43: Command pattern integration
        })
    }

    // coordinator_config method removed - TAS-43 uses command pattern instead of coordinators

    // enhanced_coordinator_config method removed - TAS-43 uses command pattern instead of coordinators

    /// Convert to ReadinessFallbackConfig for fallback poller
    fn fallback_config(&self) -> ReadinessFallbackConfig {
        ReadinessFallbackConfig {
            enabled: true, // Always enabled for production reliability
            polling_interval: self.config.timing.fallback_polling_interval(),
            batch_size: self.config.processing.batch_size,
            age_threshold: std::time::Duration::from_secs(5), // Default age threshold
            max_age: std::time::Duration::from_secs(3600),    // 1 hour max age
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
}

#[async_trait]
impl EventDrivenSystem for TaskReadinessEventSystem {
    type SystemId = String;
    type Event = TaskReadinessEvent;
    type Config = TaskReadinessEventSystemConfig;
    type Statistics = SystemStatistics;

    fn system_id(&self) -> Self::SystemId {
        self.system_id.clone()
    }

    fn deployment_mode(&self) -> DeploymentMode {
        self.deployment_mode.clone()
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    async fn start(&mut self) -> Result<(), DeploymentModeError> {
        if self.is_running() {
            return Err(DeploymentModeError::ConfigurationError {
                message: "TaskReadinessEventSystem is already running".to_string(),
            });
        }

        info!(
            system_id = %self.system_id,
            deployment_mode = ?self.deployment_mode,
            "Starting TaskReadinessEventSystem"
        );

        // TAS-43: Create components based on deployment mode using command pattern
        match self.deployment_mode {
            DeploymentMode::PollingOnly => {
                // Only create fallback poller - sends commands instead of events
                let fallback_config = self.fallback_config();
                let mut fallback_poller = ReadinessFallbackPoller::new(
                    fallback_config,
                    self.context.clone(),
                    self.command_sender.clone(),
                );

                fallback_poller.start().await?;
                self.fallback_poller = Some(fallback_poller);
            }

            DeploymentMode::Hybrid => {
                // Create fallback poller for hybrid reliability - sends commands
                // Event-driven coordination is handled by UnifiedEventCoordinator at bootstrap level
                let fallback_config = self.fallback_config();
                let mut fallback_poller = ReadinessFallbackPoller::new(
                    fallback_config,
                    self.context.clone(),
                    self.command_sender.clone(),
                );

                fallback_poller.start().await?;
                self.fallback_poller = Some(fallback_poller);
            }

            DeploymentMode::EventDrivenOnly => {
                // Pure event-driven mode
                // Event coordination handled by UnifiedEventCoordinator at bootstrap level
                // No fallback poller needed
            }
        }

        self.is_running.store(true, Ordering::Relaxed);
        self.started_at = Some(Instant::now());

        info!(
            system_id = %self.system_id,
            deployment_mode = ?self.deployment_mode,
            "TaskReadinessEventSystem started successfully"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), DeploymentModeError> {
        if !self.is_running() {
            return Ok(());
        }

        info!(
            system_id = %self.system_id,
            "Stopping TaskReadinessEventSystem"
        );

        // Stop components (TAS-43: coordinators removed, only fallback poller remains)
        if let Some(mut fallback_poller) = self.fallback_poller.take() {
            fallback_poller.stop().await?;
        }

        self.is_running.store(false, Ordering::Relaxed);

        info!(
            system_id = %self.system_id,
            "TaskReadinessEventSystem stopped successfully"
        );

        Ok(())
    }

    fn statistics(&self) -> Self::Statistics {
        SystemStatistics {
            events_processed: self.statistics.events_processed(),
            events_failed: self.statistics.events_failed(),
            processing_rate: self.statistics.processing_rate(),
            average_latency_ms: self.statistics.average_latency_ms(),
            deployment_mode_score: self.statistics.deployment_mode_score(),
        }
    }

    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError> {
        let start_time = Instant::now();

        debug!(
            system_id = %self.system_id,
            event_type = ?event,
            "Processing task readiness event"
        );

        match event {
            TaskReadinessEvent::TaskReady(task_ready_event) => {
                // TAS-43: Convert TaskReadyEvent to ProcessTaskReadiness command
                // This implements the command pattern architecture where events are converted to commands

                let triggered_by_str = match task_ready_event.triggered_by {
                    ReadinessTrigger::StepTransition => "step_transition",
                    ReadinessTrigger::TaskStart => "task_start",
                    ReadinessTrigger::FallbackPolling => "fallback_polling",
                    ReadinessTrigger::Manual => "manual",
                }
                .to_string();

                let (command_tx, command_rx) = tokio::sync::oneshot::channel();

                let command = OrchestrationCommand::ProcessTaskReadiness {
                    task_uuid: task_ready_event.task_uuid,
                    namespace: task_ready_event.namespace.clone(),
                    priority: task_ready_event.priority,
                    ready_steps: task_ready_event.ready_steps,
                    triggered_by: triggered_by_str.clone(),
                    step_uuid: task_ready_event.step_uuid,
                    step_state: task_ready_event.step_state.clone(),
                    task_state: task_ready_event.task_state.clone(),
                    resp: command_tx,
                };

                debug!(
                    system_id = %self.system_id,
                    task_uuid = %task_ready_event.task_uuid,
                    namespace = %task_ready_event.namespace,
                    triggered_by = %triggered_by_str,
                    "Converting TaskReady event to ProcessTaskReadiness command"
                );

                // Send command to orchestration processor
                if let Err(e) = self.command_sender.send(command).await {
                    error!(
                        system_id = %self.system_id,
                        task_uuid = %task_ready_event.task_uuid,
                        error = %e,
                        "Failed to send ProcessTaskReadiness command"
                    );
                    return Err(DeploymentModeError::ConfigurationError {
                        message: format!("Command send failed: {}", e),
                    });
                }

                // Wait for command processing result
                match command_rx.await {
                    Ok(Ok(result)) => {
                        debug!(
                            system_id = %self.system_id,
                            task_uuid = %result.task_uuid,
                            steps_discovered = result.steps_discovered,
                            steps_enqueued = result.steps_enqueued,
                            processing_time_ms = result.processing_time_ms,
                            "ProcessTaskReadiness command completed successfully"
                        );

                        // Track statistics
                        self.statistics
                            .tasks_claimed
                            .fetch_add(1, Ordering::Relaxed);
                        self.statistics
                            .tasks_enqueued
                            .fetch_add(result.steps_enqueued as u64, Ordering::Relaxed);
                    }
                    Ok(Err(e)) => {
                        error!(
                            system_id = %self.system_id,
                            task_uuid = %task_ready_event.task_uuid,
                            error = %e,
                            "ProcessTaskReadiness command failed"
                        );
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command processing failed: {}", e),
                        });
                    }
                    Err(e) => {
                        error!(
                            system_id = %self.system_id,
                            task_uuid = %task_ready_event.task_uuid,
                            error = %e,
                            "Failed to receive ProcessTaskReadiness command response"
                        );
                        return Err(DeploymentModeError::ConfigurationError {
                            message: format!("Command response failed: {}", e),
                        });
                    }
                }
            }

            TaskReadinessEvent::TaskStateChange(task_state_change_event) => {
                debug!(
                    system_id = %self.system_id,
                    task_uuid = %task_state_change_event.task_uuid,
                    namespace = %task_state_change_event.namespace,
                    task_state = %task_state_change_event.task_state,
                    "Task state change event received"
                );
            }

            TaskReadinessEvent::NamespaceCreated(namespace_created_event) => {
                debug!(
                    system_id = %self.system_id,
                    namespace_uuid = %namespace_created_event.namespace_uuid,
                    namespace_name = %namespace_created_event.namespace_name,
                    "Namespace created event received"
                );
            }

            TaskReadinessEvent::Unknown { channel, payload } => {
                debug!(
                    system_id = %self.system_id,
                    channel = %channel,
                    payload = %payload,
                    "Unknown task readiness event received"
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
            "Task readiness event processed successfully"
        );

        Ok(())
    }

    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
        if !self.is_running() {
            return Err(DeploymentModeError::HealthCheckFailed {
                details: "TaskReadinessEventSystem is not running".to_string(),
            });
        }

        // Check component health based on deployment mode
        match self.deployment_mode {
            DeploymentMode::PollingOnly => {
                if let Some(fallback_poller) = &self.fallback_poller {
                    // Check if poller is healthy
                    if !fallback_poller.is_running() {
                        return Err(DeploymentModeError::HealthCheckFailed {
                            details: "Fallback poller is not running".to_string(),
                        });
                    }
                }
            }

            DeploymentMode::Hybrid | DeploymentMode::EventDrivenOnly => {
                // TAS-43: Check command channel health instead of coordinator
                if self.command_sender.is_closed() {
                    return Err(DeploymentModeError::HealthCheckFailed {
                        details: "Command channel is closed - cannot send orchestration commands"
                            .to_string(),
                    });
                }

                // For hybrid mode, also check fallback poller
                if self.deployment_mode == DeploymentMode::Hybrid {
                    if let Some(fallback_poller) = &self.fallback_poller {
                        if !fallback_poller.is_running() {
                            warn!(
                                system_id = %self.system_id,
                                "Fallback poller is not running in hybrid mode"
                            );
                        }
                    }
                }
            }
        }

        // Check processing health based on statistics
        let stats = self.statistics();
        if stats.deployment_mode_score < 0.5 {
            warn!(
                system_id = %self.system_id,
                score = %stats.deployment_mode_score,
                "Low deployment mode effectiveness score"
            );
        }

        debug!(
            system_id = %self.system_id,
            events_processed = %stats.events_processed,
            events_failed = %stats.events_failed,
            processing_rate = %stats.processing_rate,
            "TaskReadinessEventSystem health check passed"
        );

        Ok(DeploymentModeHealthStatus::Healthy)
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }
}
