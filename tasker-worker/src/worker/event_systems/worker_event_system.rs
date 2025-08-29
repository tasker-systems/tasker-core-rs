//! # WorkerEventSystem - Unified Event-Driven Architecture for Workers
//!
//! This module implements the EventDrivenSystem trait for worker namespace queue processing,
//! following the same modular patterns established in orchestration. It provides consistent
//! architecture across the entire tasker-core ecosystem.
//!
//! ## Key Features
//!
//! - **EventDrivenSystem Implementation**: Follows the unified trait from tasker-shared
//! - **Deployment Mode Support**: PollingOnly, Hybrid, EventDrivenOnly configurations
//! - **Modular Components**: Separate listener and fallback poller for maintainability
//! - **Command Pattern Integration**: Seamless integration with existing WorkerCommand pattern
//! - **Health Monitoring**: Built-in health checks and statistics tracking

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use async_trait::async_trait;
use tasker_shared::{
    event_system::{
        deployment::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus},
        event_driven::{EventDrivenSystem, EventSystemStatistics},
    },
    messaging::UnifiedPgmqClient,
    system_context::SystemContext,
    TaskerResult,
};

use super::super::command_processor::WorkerCommand;
use super::super::worker_queues::{
    events::{WorkerNotification, WorkerQueueEvent},
    fallback_poller::{WorkerFallbackPoller, WorkerPollerConfig},
    listener::{WorkerListenerConfig, WorkerQueueListener},
};

/// Configuration for the WorkerEventSystem
#[derive(Debug, Clone)]
pub struct WorkerEventSystemConfig {
    /// System identifier
    pub system_id: String,
    /// Current deployment mode
    pub deployment_mode: DeploymentMode,
    /// Supported namespaces for this worker
    pub supported_namespaces: Vec<String>,
    /// Health monitoring enabled
    pub health_monitoring_enabled: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum concurrent processors
    pub max_concurrent_processors: usize,
    /// Processing timeout
    pub processing_timeout: Duration,
    /// Listener configuration
    pub listener: WorkerListenerConfig,
    /// Fallback poller configuration
    pub fallback_poller: WorkerPollerConfig,
}

impl Default for WorkerEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "worker-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            supported_namespaces: vec!["linear_workflow".to_string()],
            health_monitoring_enabled: true,
            health_check_interval: Duration::from_secs(30),
            max_concurrent_processors: 10,
            processing_timeout: Duration::from_millis(100),
            listener: WorkerListenerConfig::default(),
            fallback_poller: WorkerPollerConfig::default(),
        }
    }
}

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
/// following the same patterns established in orchestration components.
pub struct WorkerEventSystem {
    /// System identifier
    system_id: String,
    /// Current deployment mode
    deployment_mode: DeploymentMode,
    /// Worker namespace queue listener
    queue_listener: Option<WorkerQueueListener>,
    /// Fallback poller for reliability
    fallback_poller: Option<WorkerFallbackPoller>,
    /// Worker command sender
    command_sender: mpsc::Sender<WorkerCommand>,
    /// System configuration
    config: WorkerEventSystemConfig,
    /// Runtime statistics
    statistics: Arc<AtomicStatistics>,
    /// Running state
    is_running: AtomicBool,
    /// System context
    context: Arc<SystemContext>,
    /// PGMQ client for queue operations
    pgmq_client: Arc<UnifiedPgmqClient>,
    /// Event receiver for processing
    event_receiver: Option<mpsc::Receiver<WorkerNotification>>,
}

/// Thread-safe statistics wrapper
#[derive(Debug, Default)]
struct AtomicStatistics {
    events_processed: AtomicU64,
    events_failed: AtomicU64,
    step_messages_processed: AtomicU64,
    health_checks_processed: AtomicU64,
    config_updates_processed: AtomicU64,
    last_event_at: parking_lot::Mutex<Option<SystemTime>>,
}

impl WorkerEventSystem {
    /// Create new WorkerEventSystem
    pub async fn new(
        config: WorkerEventSystemConfig,
        context: Arc<SystemContext>,
        command_sender: mpsc::Sender<WorkerCommand>,
        pgmq_client: Arc<UnifiedPgmqClient>,
    ) -> TaskerResult<Self> {
        info!(
            system_id = %config.system_id,
            deployment_mode = %config.deployment_mode,
            supported_namespaces = ?config.supported_namespaces,
            "Creating WorkerEventSystem"
        );

        Ok(Self {
            system_id: config.system_id.clone(),
            deployment_mode: config.deployment_mode.clone(),
            queue_listener: None,
            fallback_poller: None,
            command_sender,
            config,
            statistics: Arc::new(AtomicStatistics::default()),
            is_running: AtomicBool::new(false),
            context,
            pgmq_client,
            event_receiver: None,
        })
    }

    /// Start event-driven processing
    async fn start_event_driven_processing(&mut self) -> Result<(), DeploymentModeError> {
        info!(
            system_id = %self.system_id,
            "Starting event-driven processing for WorkerEventSystem"
        );

        // Create event channel for notifications
        let (event_sender, event_receiver) = mpsc::channel(1000);

        // Create and start listener
        let mut listener = WorkerQueueListener::new(
            self.config.listener.clone(),
            self.context.clone(),
            event_sender,
        )
        .await
        .map_err(|e| DeploymentModeError::ConfigurationError {
            message: format!("Failed to create worker queue listener: {}", e),
        })?;

        listener
            .start()
            .await
            .map_err(|e| DeploymentModeError::ConfigurationError {
                message: format!("Failed to start worker queue listener: {}", e),
            })?;

        self.queue_listener = Some(listener);
        self.event_receiver = Some(event_receiver);

        Ok(())
    }

    /// Start fallback polling
    async fn start_fallback_polling(&mut self) -> Result<(), DeploymentModeError> {
        info!(
            system_id = %self.system_id,
            "Starting fallback polling for WorkerEventSystem"
        );

        let poller = WorkerFallbackPoller::new(
            self.config.fallback_poller.clone(),
            self.context.clone(),
            self.command_sender.clone(),
            self.pgmq_client.clone(),
        )
        .await
        .map_err(|e| DeploymentModeError::ConfigurationError {
            message: format!("Failed to create worker fallback poller: {}", e),
        })?;

        poller
            .start()
            .await
            .map_err(|e| DeploymentModeError::ConfigurationError {
                message: format!("Failed to start worker fallback poller: {}", e),
            })?;

        self.fallback_poller = Some(poller);

        Ok(())
    }

    /// Handle step message event
    async fn handle_step_message_event(
        &self,
        message_event: pgmq_notify::MessageReadyEvent,
    ) -> Result<(), DeploymentModeError> {
        debug!(
            system_id = %self.system_id,
            queue_name = %message_event.queue_name,
            namespace = %message_event.namespace,
            "Handling step message event"
        );

        // Update statistics
        self.statistics
            .step_messages_processed
            .fetch_add(1, Ordering::Relaxed);
        *self.statistics.last_event_at.lock() = Some(SystemTime::now());

        // Convert to WorkerCommand and send through command pattern
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

        self.command_sender
            .send(WorkerCommand::ExecuteStepFromEvent {
                message_event,
                resp: resp_tx,
            })
            .await
            .map_err(|e| DeploymentModeError::ConfigurationError {
                message: format!("Failed to send worker command: {}", e),
            })?;

        self.statistics
            .events_processed
            .fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Process event notifications from the listener
    async fn process_event_notifications(&mut self) -> Result<(), DeploymentModeError> {
        while let Some(receiver) = &mut self.event_receiver {
            if let Some(notification) = receiver.recv().await {
                match notification {
                    WorkerNotification::Event(event) => {
                        if let Err(e) = self.process_event(event).await {
                            error!(
                                system_id = %self.system_id,
                                error = %e,
                                "Failed to process worker queue event"
                            );
                            self.statistics
                                .events_failed
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    WorkerNotification::Health(_health_update) => {
                        // Handle health updates if needed
                        self.statistics
                            .health_checks_processed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    WorkerNotification::Configuration(_config_update) => {
                        // Handle configuration updates if needed
                        self.statistics
                            .config_updates_processed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            } else {
                // Channel closed, break out of loop
                break;
            }
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
        self.is_running.load(Ordering::SeqCst)
    }

    async fn start(&mut self) -> Result<(), DeploymentModeError> {
        info!(
            system_id = %self.system_id,
            deployment_mode = %self.deployment_mode,
            "Starting WorkerEventSystem"
        );

        match self.deployment_mode {
            DeploymentMode::EventDrivenOnly => {
                self.start_event_driven_processing().await?;
            }
            DeploymentMode::Hybrid => {
                self.start_event_driven_processing().await?;
                self.start_fallback_polling().await?;
            }
            DeploymentMode::PollingOnly => {
                self.start_fallback_polling().await?;
            }
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Start event processing loop only if we have event-driven or hybrid mode
        if matches!(
            self.deployment_mode,
            DeploymentMode::EventDrivenOnly | DeploymentMode::Hybrid
        ) {
            let system_id = self.system_id.clone();
            let config = self.config.clone();
            let context = self.context.clone();
            let command_sender = self.command_sender.clone();
            let pgmq_client = self.pgmq_client.clone();

            tokio::spawn(async move {
                let mut event_system =
                    match Self::new(config, context, command_sender, pgmq_client).await {
                        Ok(system) => system,
                        Err(e) => {
                            error!(
                                system_id = %system_id,
                                error = %e,
                                "Failed to create event processing system"
                            );
                            return;
                        }
                    };

                if let Err(e) = event_system.process_event_notifications().await {
                    error!(
                        system_id = %system_id,
                        error = %e,
                        "Event processing loop failed"
                    );
                }
            });
        }

        info!(
            system_id = %self.system_id,
            deployment_mode = %self.deployment_mode,
            "WorkerEventSystem started successfully"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), DeploymentModeError> {
        info!(system_id = %self.system_id, "Stopping WorkerEventSystem");

        self.is_running.store(false, Ordering::SeqCst);

        // Stop fallback poller if running
        if let Some(poller) = &self.fallback_poller {
            poller.stop().await;
        }

        // Close event receiver
        if let Some(mut receiver) = self.event_receiver.take() {
            receiver.close();
        }

        info!(system_id = %self.system_id, "WorkerEventSystem stopped successfully");

        Ok(())
    }

    fn statistics(&self) -> Self::Statistics {
        WorkerEventSystemStatistics {
            events_processed: self.statistics.events_processed.load(Ordering::Relaxed),
            events_failed: self.statistics.events_failed.load(Ordering::Relaxed),
            processing_rate: 0.0,       // TODO: Calculate actual rate
            average_latency_ms: 0.0,    // TODO: Calculate actual latency
            deployment_mode_score: 0.9, // TODO: Calculate actual score
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
            last_event_at: *self.statistics.last_event_at.lock(),
        }
    }

    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError> {
        match event {
            WorkerQueueEvent::StepMessage(message_event) => {
                self.handle_step_message_event(message_event).await
            }
            WorkerQueueEvent::HealthCheck(_message_event) => {
                // Handle health check events
                self.statistics
                    .health_checks_processed
                    .fetch_add(1, Ordering::Relaxed);
                self.statistics
                    .events_processed
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            WorkerQueueEvent::ConfigurationUpdate(_message_event) => {
                // Handle configuration update events
                self.statistics
                    .config_updates_processed
                    .fetch_add(1, Ordering::Relaxed);
                self.statistics
                    .events_processed
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            WorkerQueueEvent::Unknown {
                queue_name,
                payload,
            } => {
                warn!(
                    system_id = %self.system_id,
                    queue_name = %queue_name,
                    payload = %payload,
                    "Received unknown worker queue event"
                );
                self.statistics
                    .events_failed
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
        // Basic health check based on recent activity and error rates
        let stats = self.statistics();
        let success_rate = stats.success_rate();

        if success_rate > 0.95 {
            Ok(DeploymentModeHealthStatus::Healthy)
        } else if success_rate > 0.90 {
            Ok(DeploymentModeHealthStatus::Degraded)
        } else if success_rate > 0.75 {
            Ok(DeploymentModeHealthStatus::Warning)
        } else {
            Ok(DeploymentModeHealthStatus::Critical)
        }
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tasker_shared::event_system::deployment::DeploymentMode;

    #[test]
    fn test_worker_event_system_config_creation() {
        let config = WorkerEventSystemConfig {
            system_id: "test-worker".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            supported_namespaces: vec![
                "linear_workflow".to_string(),
                "order_fulfillment".to_string(),
            ],
            health_monitoring_enabled: true,
            health_check_interval: Duration::from_secs(30),
            max_concurrent_processors: 10,
            processing_timeout: Duration::from_millis(500),
            listener: WorkerListenerConfig {
                supported_namespaces: vec!["linear_workflow".to_string()],
                retry_interval: Duration::from_secs(5),
                max_retry_attempts: 3,
                event_timeout: Duration::from_secs(30),
                health_check_interval: Duration::from_secs(60),
                batch_processing: true,
                connection_timeout: Duration::from_secs(10),
            },
            fallback_poller: WorkerPollerConfig {
                enabled: true,
                polling_interval: Duration::from_secs(5),
                batch_size: 10,
                age_threshold: Duration::from_millis(500),
                max_age: Duration::from_secs(300),
                supported_namespaces: vec!["order_fulfillment".to_string()],
                visibility_timeout: Duration::from_secs(30),
            },
        };

        assert_eq!(config.system_id, "test-worker");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        assert_eq!(config.supported_namespaces.len(), 2);
        assert!(config.health_monitoring_enabled);
        assert_eq!(config.max_concurrent_processors, 10);
        assert!(config.fallback_poller.enabled);
        assert!(config.listener.batch_processing);
    }

    #[test]
    fn test_worker_event_system_config_default() {
        let config = WorkerEventSystemConfig::default();

        assert_eq!(config.system_id, "worker-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        assert_eq!(config.supported_namespaces, vec!["linear_workflow"]);
        assert!(config.health_monitoring_enabled);
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.max_concurrent_processors, 10);
        assert_eq!(config.processing_timeout, Duration::from_millis(100));
    }

    #[test]
    fn test_worker_event_system_statistics_creation() {
        let stats = WorkerEventSystemStatistics::new();

        assert_eq!(stats.events_processed, 0);
        assert_eq!(stats.events_failed, 0);
        assert_eq!(stats.processing_rate, 0.0);
        assert_eq!(stats.average_latency_ms, 0.0);
        assert_eq!(stats.deployment_mode_score, 0.0);
        assert_eq!(stats.step_messages_processed, 0);
        assert_eq!(stats.health_checks_processed, 0);
        assert_eq!(stats.config_updates_processed, 0);
        assert!(stats.last_event_at.is_none());
    }
}
