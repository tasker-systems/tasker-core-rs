//! # TAS-43 Event-Driven Orchestration Coordinator
//!
//! This module integrates pgmq-notify with tasker-orchestration to enable event-driven
//! orchestration operations, replacing polling-based approaches with PostgreSQL LISTEN/NOTIFY
//! for <10ms latency orchestration responses.
//!
//! ## Key Features
//!
//! - **Event-Driven Architecture**: Uses pgmq-notify for real-time step result notifications
//! - **Namespace-Aware**: Coordinates orchestration for specific namespaces
//! - **Hybrid Reliability**: Event-driven with fallback polling for resilience
//! - **Command Pattern Integration**: Works with existing OrchestrationCore command architecture
//! - **TAS-37 Compatible**: Integrates with atomic finalization claiming and race condition prevention

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use pgmq_notify::{
    listener::PgmqEventHandler, MessageReadyEvent, PgmqNotifyConfig, PgmqNotifyEvent,
    PgmqNotifyListener,
};
use tasker_shared::{
    messaging::PgmqClientTrait, system_context::SystemContext, TaskerError, TaskerResult,
};

use crate::orchestration::command_processor::OrchestrationCommand;
use crate::orchestration::OrchestrationCore;

/// Configuration for event-driven orchestration coordination
#[derive(Debug, Clone)]
pub struct EventDrivenCoordinatorConfig {
    /// Namespace for orchestration coordination (e.g., "orchestration")
    pub namespace: String,
    /// Enable event-driven coordination (vs polling-only)
    pub event_driven_enabled: bool,
    /// Fallback polling interval for resilience
    pub fallback_polling_interval: Duration,
    /// Message batch size for polling fallback
    pub batch_size: u32,
    /// Queue configuration for orchestration-owned queues
    pub queues: tasker_shared::config::QueueConfig,
    /// Visibility timeout for message processing
    pub visibility_timeout: Duration,
}

impl Default for EventDrivenCoordinatorConfig {
    fn default() -> Self {
        Self {
            namespace: "orchestration".to_string(),
            event_driven_enabled: true,
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            queues: tasker_shared::config::QueueConfig {
                orchestration_owned: tasker_shared::config::OrchestrationOwnedQueues::default(),
                worker_queues: std::collections::HashMap::new(),
                settings: tasker_shared::config::QueueSettings {
                    visibility_timeout_seconds: 30,
                    message_retention_seconds: 604800,
                    dead_letter_queue_enabled: true,
                    max_receive_count: 3,
                },
            },
            visibility_timeout: Duration::from_secs(30),
        }
    }
}

/// Event-driven orchestration coordinator that integrates pgmq-notify with OrchestrationCore
pub struct EventDrivenOrchestrationCoordinator {
    /// Configuration
    config: EventDrivenCoordinatorConfig,

    /// System context for dependency injection
    /// Holding this ensures we don't go out of scope and prevent premature shutdown
    #[allow(dead_code)]
    context: Arc<SystemContext>,

    /// PGMQ notification listener for event-driven coordination
    listener: Option<PgmqNotifyListener>,

    /// Orchestration core for command processing
    orchestration_core: Arc<OrchestrationCore>,

    /// Command sender for orchestration operations
    orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,

    /// Coordinator ID for logging
    coordinator_id: Uuid,

    /// Statistics counters
    operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
    events_received: Arc<std::sync::atomic::AtomicU64>,

    /// Running state
    is_running: bool,
}

impl EventDrivenOrchestrationCoordinator {
    /// Create new event-driven orchestration coordinator
    pub async fn new(
        config: EventDrivenCoordinatorConfig,
        context: Arc<SystemContext>,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
    ) -> TaskerResult<Self> {
        info!(
            namespace = %config.namespace,
            "Creating EventDrivenOrchestrationCoordinator for TAS-43"
        );

        // Create pgmq-notify listener for orchestration coordination
        let notify_config = PgmqNotifyConfig::new()
            .with_queue_naming_pattern(&format!(r"(?P<namespace>{})", config.namespace))
            .with_default_namespace(&config.namespace);

        let pool = context.database_pool();
        let listener = if config.event_driven_enabled {
            Some(
                PgmqNotifyListener::new(pool.clone(), notify_config)
                    .await
                    .map_err(|e| {
                        TaskerError::OrchestrationError(format!(
                            "Failed to create notify listener: {}",
                            e
                        ))
                    })?,
            )
        } else {
            None
        };

        let coordinator_id = Uuid::now_v7();

        Ok(Self {
            config,
            context,
            listener,
            orchestration_core,
            orchestration_command_sender,
            coordinator_id,
            operations_coordinated: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            events_received: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            is_running: false,
        })
    }

    /// Start event-driven orchestration coordination
    pub async fn start(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
            namespace = %self.config.namespace,
            event_driven = %self.config.event_driven_enabled,
            "Starting EventDrivenOrchestrationCoordinator"
        );

        self.is_running = true;

        if self.config.event_driven_enabled {
            self.start_event_driven_coordination().await?;
        }

        self.start_fallback_polling().await?;

        info!(
            coordinator_id = %self.coordinator_id,
            "EventDrivenOrchestrationCoordinator started successfully"
        );

        Ok(())
    }

    /// Stop event-driven orchestration coordination
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            coordinator_id = %self.coordinator_id,
            "Stopping EventDrivenOrchestrationCoordinator"
        );

        self.is_running = false;

        if let Some(ref mut listener) = self.listener {
            listener.disconnect().await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to disconnect listener: {}", e))
            })?;
        }

        Ok(())
    }

    /// Start event-driven coordination with pgmq-notify
    async fn start_event_driven_coordination(&mut self) -> TaskerResult<()> {
        if let Some(ref mut listener) = self.listener {
            // Connect to PostgreSQL
            listener.connect().await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to connect listener: {}", e))
            })?;

            // Listen to message ready events for orchestration namespace
            listener
                .listen_message_ready_for_namespace(&self.config.namespace)
                .await
                .map_err(|e| {
                    TaskerError::OrchestrationError(format!("Failed to listen to namespace: {}", e))
                })?;

            let handler = OrchestrationEventHandler::new(
                self.config.clone(),
                self.orchestration_core.clone(),
                self.orchestration_command_sender.clone(),
                self.coordinator_id,
                self.operations_coordinated.clone(),
                self.events_received.clone(),
            );

            // Start listening with the event handler
            listener.listen_with_handler(handler).await.map_err(|e| {
                TaskerError::OrchestrationError(format!("Failed to start event handler: {}", e))
            })?;

            info!(
                coordinator_id = %self.coordinator_id,
                namespace = %self.config.namespace,
                "Event-driven orchestration coordination started successfully"
            );
        }

        Ok(())
    }

    /// Start fallback polling for reliability
    async fn start_fallback_polling(&self) -> TaskerResult<()> {
        let config = self.config.clone();
        let _orchestration_core = self.orchestration_core.clone(); // For future use
        let coordinator_id = self.coordinator_id;
        let context = self.context.clone();
        let orchestration_command_sender = self.orchestration_command_sender.clone();

        tokio::spawn(async move {
            info!(
                coordinator_id = %coordinator_id,
                interval_ms = %config.fallback_polling_interval.as_millis(),
                "Starting fallback polling for orchestration coordination reliability"
            );

            let mut interval = tokio::time::interval(config.fallback_polling_interval);
            // Use config-driven queue classification instead of hardcoded patterns
            let classifier = tasker_shared::config::QueueClassifier::new(
                config.queues.orchestration_owned.clone(),
                config.namespace.clone(),
            );

            let step_results_queue = classifier
                .ensure_orchestration_prefix(&config.queues.orchestration_owned.step_results);
            let task_requests_queue = classifier
                .ensure_orchestration_prefix(&config.queues.orchestration_owned.task_requests);
            let task_finalizations_queue = classifier
                .ensure_orchestration_prefix(&config.queues.orchestration_owned.task_finalizations);

            loop {
                interval.tick().await;

                // Poll all three queue types using the new command pattern
                Self::poll_queue_for_messages(
                    &context,
                    &orchestration_command_sender,
                    &step_results_queue,
                    &config,
                    coordinator_id,
                    "step_results",
                )
                .await;

                Self::poll_queue_for_messages(
                    &context,
                    &orchestration_command_sender,
                    &task_requests_queue,
                    &config,
                    coordinator_id,
                    "task_requests",
                )
                .await;

                Self::poll_queue_for_messages(
                    &context,
                    &orchestration_command_sender,
                    &task_finalizations_queue,
                    &config,
                    coordinator_id,
                    "task_finalizations",
                )
                .await;
            }
        });

        Ok(())
    }

    async fn poll_queue_for_messages(
        context: &SystemContext,
        orchestration_command_sender: &mpsc::Sender<OrchestrationCommand>,
        queue_name: &str,
        config: &EventDrivenCoordinatorConfig,
        coordinator_id: Uuid,
        queue_type: &str,
    ) {
        debug!(
            coordinator_id = %coordinator_id,
            queue = %queue_name,
            queue_type = queue_type,
            "Performing fallback polling check"
        );

        match context
            .message_client
            .read_messages(queue_name, Some(30), Some(config.batch_size as i32))
            .await
        {
            Ok(messages) => {
                debug!(
                    coordinator_id = %coordinator_id,
                    queue = %queue_name,
                    count = messages.len(),
                    queue_type = queue_type,
                    "Read messages from fallback polling"
                );

                for message in messages {
                    // Extract namespace from queue name (remove the suffix)
                    let namespace = if let Some(pos) = queue_name.rfind('_') {
                        queue_name[..pos].to_string()
                    } else {
                        queue_name.to_string()
                    };

                    let message_event = pgmq_notify::MessageReadyEvent {
                        msg_id: message.msg_id,
                        queue_name: queue_name.to_string(),
                        namespace,
                        ready_at: chrono::Utc::now(),
                        metadata: std::collections::HashMap::new(),
                        visibility_timeout_seconds: Some(config.visibility_timeout.as_secs() as i32),
                    };

                    // Use config-driven queue classification instead of hardcoded queue_type string matching
                    // Create queue classifier from config for this static function
                    let queue_classifier = tasker_shared::config::QueueClassifier::new(
                        config.queues.orchestration_owned.clone(),
                        config.namespace.clone(),
                    );
                    let classified_message =
                        tasker_shared::config::ConfigDrivenMessageEvent::classify(
                            message_event.clone(),
                            &queue_name,
                            &queue_classifier,
                        );

                    let command_result = match classified_message {
                        tasker_shared::config::ConfigDrivenMessageEvent::StepResults(event) => {
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            orchestration_command_sender
                                .send(OrchestrationCommand::ProcessStepResultFromMessageEvent {
                                    message_event: event,
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::TaskRequests(event) => {
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            orchestration_command_sender
                                .send(OrchestrationCommand::InitializeTaskFromMessageEvent {
                                    message_event: event,
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::TaskFinalizations(
                            event,
                        ) => {
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            orchestration_command_sender
                                .send(OrchestrationCommand::FinalizeTaskFromMessageEvent {
                                    message_event: event,
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::WorkerNamespace {
                            namespace,
                            event: _,
                        } => {
                            debug!(
                                coordinator_id = %coordinator_id,
                                queue = %queue_name,
                                namespace = %namespace,
                                "Worker namespace message received in orchestration fallback polling"
                            );
                            continue;
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::Unknown(event) => {
                            warn!(
                                coordinator_id = %coordinator_id,
                                queue = %queue_name,
                                event_message_id = %event.msg_id,
                                event_namespace = %event.namespace,
                                "Unknown queue type in fallback polling using config-driven classification",
                            );
                            continue;
                        }
                    };

                    if let Err(e) = command_result {
                        warn!(
                            coordinator_id = %coordinator_id,
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            queue_type = queue_type,
                            error = %e,
                            "Failed to send command from fallback polling"
                        );
                    }
                }
            }
            Err(e) => {
                error!(
                    coordinator_id = %coordinator_id,
                    queue = %queue_name,
                    queue_type = queue_type,
                    error = %e,
                    "Failed to read messages from fallback polling"
                );
            }
        }
    }

    /// Check if coordinator is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Get coordinator statistics
    pub async fn get_stats(&self) -> EventDrivenCoordinatorStats {
        let listener_connected = if let Some(ref listener) = self.listener {
            listener.is_healthy().await
        } else {
            false
        };

        EventDrivenCoordinatorStats {
            coordinator_id: self.coordinator_id,
            namespace: self.config.namespace.clone(),
            event_driven_enabled: self.config.event_driven_enabled,
            listener_connected,
            operations_coordinated: self
                .operations_coordinated
                .load(std::sync::atomic::Ordering::Relaxed),
            events_received: self
                .events_received
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// Event handler for PGMQ notifications in orchestration coordination
struct OrchestrationEventHandler {
    config: EventDrivenCoordinatorConfig,
    #[allow(dead_code)] // Will be used for actual orchestration operations in the future
    orchestration_core: Arc<OrchestrationCore>,
    /// Command sender for orchestration operations
    orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
    coordinator_id: Uuid,
    /// Statistics counters (shared with coordinator)
    operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
    events_received: Arc<std::sync::atomic::AtomicU64>,
    /// Config-driven queue classifier for replacing hardcoded string matching
    queue_classifier: tasker_shared::config::QueueClassifier,
}

impl OrchestrationEventHandler {
    fn new(
        config: EventDrivenCoordinatorConfig,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_command_sender: mpsc::Sender<OrchestrationCommand>,
        coordinator_id: Uuid,
        operations_coordinated: Arc<std::sync::atomic::AtomicU64>,
        events_received: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        // Create queue classifier from config to replace hardcoded string matching
        let queue_classifier = tasker_shared::config::QueueClassifier::new(
            config.queues.orchestration_owned.clone(),
            config.namespace.clone(),
        );

        Self {
            config,
            orchestration_core,
            orchestration_command_sender,
            coordinator_id,
            operations_coordinated,
            events_received,
            queue_classifier,
        }
    }
}

// Legacy MessageReadyEventKind replaced by config-driven ConfigDrivenMessageEvent from tasker-shared
// This eliminates hardcoded string matching patterns in favor of configuration-grounded classification

#[async_trait::async_trait]
impl PgmqEventHandler for OrchestrationEventHandler {
    async fn handle_event(&self, event: PgmqNotifyEvent) -> pgmq_notify::Result<()> {
        match event {
            PgmqNotifyEvent::MessageReady(msg_event) => {
                // Only process messages for our orchestration namespace
                if msg_event.namespace != self.config.namespace {
                    return Ok(());
                }

                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %msg_event.queue_name,
                    msg_id = %msg_event.msg_id,
                    namespace = %msg_event.namespace,
                    "Received orchestration message ready event"
                );

                // Increment events received counter
                self.events_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Classify the message event using config-driven classification
                let queue_name = msg_event.queue_name.clone();
                let classified_event = tasker_shared::config::ConfigDrivenMessageEvent::classify(
                    msg_event,
                    &queue_name,
                    &self.queue_classifier,
                );

                debug!(
                    coordinator_id = %self.coordinator_id,
                    queue = %classified_event.inner().queue_name,
                    event_type = ?classified_event,
                    "Classified message event using config-driven enum-based dispatching"
                );

                match classified_event {
                    tasker_shared::config::ConfigDrivenMessageEvent::StepResults(event) => {
                        self.handle_step_results_message_event(event).await?;
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::TaskRequests(event) => {
                        self.handle_task_requests_message_event(event).await?;
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::TaskFinalizations(event) => {
                        self.handle_task_finalizations_message_event(event).await?;
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::WorkerNamespace {
                        namespace,
                        event,
                    } => {
                        // Log worker namespace messages for monitoring (these shouldn't normally be processed by orchestration)
                        debug!(
                            coordinator_id = %self.coordinator_id,
                            queue = %event.queue_name,
                            namespace = %namespace,
                            "Received worker namespace message in orchestration listener"
                        );
                    }
                    tasker_shared::config::ConfigDrivenMessageEvent::Unknown(event) => {
                        // Log unknown queue types for monitoring
                        debug!(
                            queue = %event.queue_name,
                            msg_id = %event.msg_id,
                            "Received message event for unknown queue type"
                        );
                        // Could potentially handle or forward to a generic handler
                    }
                }
            }
            PgmqNotifyEvent::QueueCreated(queue_event) => {
                info!(
                    coordinator_id = %self.coordinator_id,
                    queue = %queue_event.queue_name,
                    namespace = %queue_event.namespace,
                    "Queue created event received in orchestration coordinator"
                );
                // Could use this for dynamic namespace discovery or queue management
            }
        }

        Ok(())
    }

    async fn handle_parse_error(
        &self,
        channel: &str,
        payload: &str,
        error: pgmq_notify::PgmqNotifyError,
    ) {
        warn!(
            coordinator_id = %self.coordinator_id,
            channel = %channel,
            payload = %payload,
            error = %error,
            "Failed to parse PGMQ notification in orchestration coordinator"
        );
    }

    async fn handle_connection_error(&self, error: pgmq_notify::PgmqNotifyError) {
        error!(
            coordinator_id = %self.coordinator_id,
            error = %error,
            "PGMQ notification connection error in orchestration coordinator"
        );
    }
}

impl OrchestrationEventHandler {
    async fn handle_task_finalizations_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Delegating task finalization message processing to worker via command pattern"
        );

        // Delegate the full message lifecycle to the worker via command pattern
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        match self
            .orchestration_command_sender
            .send(OrchestrationCommand::FinalizeTaskFromMessageEvent {
                message_event: msg_event.clone(),
                resp: resp_tx,
            })
            .await
        {
            Err(e) => {
                warn!(
                    coordinator_id = %self.coordinator_id,
                    error = %e,
                    "Failed to send FinalizeTaskFromMessageEvent command"
                );
                Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to send command: {e}"
                )))
            }
            Ok(_) => {
                // Successfully delegated task finalization operation to worker
                self.operations_coordinated
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                debug!(
                    coordinator_id = %self.coordinator_id,
                    msg_id = %msg_event.msg_id,
                    "Successfully delegated task finalization message processing to worker"
                );
                Ok(())
            }
        }
    }

    async fn handle_task_requests_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Delegating task request message processing to worker via command pattern"
        );

        // Delegate the full message lifecycle to the worker via command pattern
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        match self
            .orchestration_command_sender
            .send(OrchestrationCommand::InitializeTaskFromMessageEvent {
                message_event: msg_event.clone(),
                resp: resp_tx,
            })
            .await
        {
            Err(e) => {
                warn!(
                    coordinator_id = %self.coordinator_id,
                    error = %e,
                    "Failed to send InitializeTaskFromMessageEvent command"
                );
                Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to send command: {e}"
                )))
            }
            Ok(_) => {
                // Successfully delegated task request operation to worker
                self.operations_coordinated
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                debug!(
                    coordinator_id = %self.coordinator_id,
                    msg_id = %msg_event.msg_id,
                    "Successfully delegated task request message processing to worker"
                );
                Ok(())
            }
        }
    }
    async fn handle_step_results_message_event(
        &self,
        msg_event: MessageReadyEvent,
    ) -> pgmq_notify::Result<()> {
        debug!(
            coordinator_id = %self.coordinator_id,
            queue = %msg_event.queue_name,
            msg_id = %msg_event.msg_id,
            "Delegating step result message processing to worker via command pattern"
        );

        // Delegate the full message lifecycle to the worker via command pattern
        let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
        match self
            .orchestration_command_sender
            .send(OrchestrationCommand::ProcessStepResultFromMessageEvent {
                message_event: msg_event.clone(),
                resp: resp_tx,
            })
            .await
        {
            Err(e) => {
                warn!(
                    coordinator_id = %self.coordinator_id,
                    error = %e,
                    "Failed to send ProcessStepResultFromMessageEvent command"
                );
                Err(pgmq_notify::PgmqNotifyError::Generic(anyhow::anyhow!(
                    "Failed to send command: {e}"
                )))
            }
            Ok(_) => {
                // Successfully delegated step result operation to worker
                self.operations_coordinated
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                debug!(
                    coordinator_id = %self.coordinator_id,
                    msg_id = %msg_event.msg_id,
                    "Successfully delegated step result message processing to worker"
                );
                Ok(())
            }
        }
    }
}

/// Statistics for event-driven orchestration coordination
#[derive(Debug, Clone)]
pub struct EventDrivenCoordinatorStats {
    pub coordinator_id: Uuid,
    pub namespace: String,
    pub event_driven_enabled: bool,
    pub listener_connected: bool,
    pub operations_coordinated: u64,
    pub events_received: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::config::QueueConfig;
    use tokio::time::sleep;

    /// Test creating an EventDrivenOrchestrationCoordinator
    #[tokio::test]
    async fn test_create_coordinator() {
        let config = EventDrivenCoordinatorConfig::default();
        assert_eq!(config.namespace, "orchestration");
        assert!(config.event_driven_enabled);
        assert_eq!(config.fallback_polling_interval, Duration::from_millis(500));
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.visibility_timeout, Duration::from_secs(30));
    }

    /// Test coordinator with event-driven disabled (polling only)
    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_coordinator_polling_only_mode(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create system context
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Create orchestration core
        let orchestration_core = Arc::new(OrchestrationCore::new(context.clone()).await?);
        let command_sender = orchestration_core.command_sender();

        // Create coordinator config with event-driven disabled
        let config = EventDrivenCoordinatorConfig {
            namespace: "test_orchestration".to_string(),
            event_driven_enabled: false, // Polling only mode
            fallback_polling_interval: Duration::from_millis(100),
            batch_size: 5,
            visibility_timeout: Duration::from_secs(10),
            queues: QueueConfig::default(),
        };

        // Create coordinator
        let mut coordinator = EventDrivenOrchestrationCoordinator::new(
            config.clone(),
            context.clone(),
            orchestration_core.clone(),
            command_sender,
        )
        .await?;

        // Verify initial state
        assert!(!coordinator.is_running());
        let stats = coordinator.get_stats().await;
        assert_eq!(stats.namespace, "test_orchestration");
        assert!(!stats.event_driven_enabled);
        assert!(!stats.listener_connected);
        assert_eq!(stats.operations_coordinated, 0);
        assert_eq!(stats.events_received, 0);

        // Start coordinator
        coordinator.start().await?;
        assert!(coordinator.is_running());

        // Let polling run for a bit
        sleep(Duration::from_millis(300)).await;

        // Stop coordinator
        coordinator.stop().await?;
        assert!(!coordinator.is_running());

        Ok(())
    }

    /// Test coordinator with event-driven enabled
    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_coordinator_event_driven_mode(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create system context
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Create orchestration core
        let orchestration_core = Arc::new(OrchestrationCore::new(context.clone()).await?);
        let command_sender = orchestration_core.command_sender();

        // Create coordinator config with event-driven enabled
        let config = EventDrivenCoordinatorConfig {
            namespace: "test_event_driven".to_string(),
            event_driven_enabled: true,
            fallback_polling_interval: Duration::from_millis(500),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            queues: QueueConfig::default(),
        };

        // Create coordinator
        let mut coordinator = EventDrivenOrchestrationCoordinator::new(
            config.clone(),
            context.clone(),
            orchestration_core.clone(),
            command_sender,
        )
        .await?;

        // Start coordinator
        coordinator.start().await?;
        assert!(coordinator.is_running());

        // Get stats
        let stats = coordinator.get_stats().await;
        assert_eq!(stats.namespace, "test_event_driven");
        assert!(stats.event_driven_enabled);
        // Note: listener_connected might be false if DB doesn't support LISTEN/NOTIFY in test
        assert_eq!(stats.operations_coordinated, 0);
        assert_eq!(stats.events_received, 0);

        // Stop coordinator
        coordinator.stop().await?;
        assert!(!coordinator.is_running());

        Ok(())
    }

    /// Test coordinator processes step results from queue
    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_coordinator_processes_step_results(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tasker_shared::messaging::StepExecutionResult;
        use tasker_shared::config::QueueConfig;

        // Create system context
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Use the default queue name from config
        let config = QueueConfig::default();
        let queue_name = &config.orchestration_owned.step_results;
        context.message_client.create_queue(queue_name).await?;

        // Create test step result using the constructor
        let step_result = StepExecutionResult::success(
            Uuid::new_v4(),
            serde_json::json!({"test": "data"}),
            100,
            None,
        );

        // Send message to queue
        context
            .message_client
            .send_json_message(queue_name, &step_result)
            .await?;

        // Create orchestration core
        let orchestration_core = Arc::new(OrchestrationCore::new(context.clone()).await?);
        let command_sender = orchestration_core.command_sender();

        // Create coordinator with short polling interval
        let config = EventDrivenCoordinatorConfig {
            namespace: "orchestration".to_string(), // Use default orchestration namespace
            event_driven_enabled: false, // Use polling for predictable test
            fallback_polling_interval: Duration::from_millis(50),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            queues: QueueConfig::default(),
        };

        let mut coordinator = EventDrivenOrchestrationCoordinator::new(
            config,
            context.clone(),
            orchestration_core,
            command_sender,
        )
        .await?;

        // Start coordinator
        coordinator.start().await?;

        // Wait for polling to process the message
        sleep(Duration::from_millis(200)).await;

        // Check that message was processed (it should be deleted from queue)
        let remaining = context
            .message_client
            .read_messages(queue_name, Some(1), Some(10))
            .await?;
        assert_eq!(
            remaining.len(),
            0,
            "Message should have been processed and deleted"
        );

        // Stop coordinator
        coordinator.stop().await?;

        Ok(())
    }

    /// Test coordinator handles multiple message types
    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_coordinator_handles_multiple_message_types(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};

        // Create system context
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Use the default queue names from config
        let queue_config = QueueConfig::default();
        let step_results_queue = &queue_config.orchestration_owned.step_results;
        let task_requests_queue = &queue_config.orchestration_owned.task_requests;
        let task_finalizations_queue = &queue_config.orchestration_owned.task_finalizations;
        
        // Create queues with default names
        context
            .message_client
            .create_queue(step_results_queue)
            .await?;
        context
            .message_client
            .create_queue(task_requests_queue)
            .await?;
        context
            .message_client
            .create_queue(task_finalizations_queue)
            .await?;

        // Create orchestration core
        let orchestration_core = Arc::new(OrchestrationCore::new(context.clone()).await?);
        let command_sender = orchestration_core.command_sender();

        // Create coordinator
        let config = EventDrivenCoordinatorConfig {
            namespace: "orchestration".to_string(), // Use default orchestration namespace
            event_driven_enabled: false,
            fallback_polling_interval: Duration::from_millis(50),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            queues: QueueConfig::default(),
        };

        let mut coordinator = EventDrivenOrchestrationCoordinator::new(
            config,
            context.clone(),
            orchestration_core,
            command_sender,
        )
        .await?;

        coordinator.start().await?;

        // Send different message types to the default queues
        let step_result = StepExecutionResult::success(
            Uuid::new_v4(),
            serde_json::json!({"test": "step_result"}),
            50,
            None,
        );
        context
            .message_client
            .send_json_message(step_results_queue, &step_result)
            .await?;

        use tasker_shared::messaging::TaskPriority;
        use tasker_shared::models::core::task_request::TaskRequest;

        let task_request_domain =
            TaskRequest::new("test_task".to_string(), "test_multi".to_string())
                .with_version("1.0.0".to_string())
                .with_context(serde_json::json!({"test": "context"}))
                .with_initiator("test_suite".to_string())
                .with_source_system("test".to_string())
                .with_reason("Test message handling".to_string());

        let task_request = TaskRequestMessage::new(task_request_domain, "test_suite".to_string())
            .with_priority(TaskPriority::Normal);
        context
            .message_client
            .send_json_message(task_requests_queue, &task_request)
            .await?;

        let task_uuid = Uuid::new_v4();
        context
            .message_client
            .send_json_message(task_finalizations_queue, &task_uuid)
            .await?;

        // Wait for processing - increased timeout for command pattern async processing
        sleep(Duration::from_millis(1000)).await;

        // Verify all messages were processed from the default queues
        assert_eq!(
            context
                .message_client
                .read_messages(step_results_queue, Some(1), Some(10))
                .await?
                .len(),
            0
        );
        assert_eq!(
            context
                .message_client
                .read_messages(task_requests_queue, Some(1), Some(10))
                .await?
                .len(),
            0
        );
        assert_eq!(
            context
                .message_client
                .read_messages(task_finalizations_queue, Some(1), Some(10))
                .await?
                .len(),
            0
        );

        coordinator.stop().await?;

        Ok(())
    }

    /// Test coordinator statistics tracking
    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_coordinator_statistics_tracking(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let orchestration_core = Arc::new(OrchestrationCore::new(context.clone()).await?);
        let command_sender = orchestration_core.command_sender();

        let config = EventDrivenCoordinatorConfig {
            namespace: "test_stats".to_string(),
            event_driven_enabled: false,
            fallback_polling_interval: Duration::from_millis(100),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            queues: QueueConfig::default(),
        };

        let mut coordinator = EventDrivenOrchestrationCoordinator::new(
            config,
            context.clone(),
            orchestration_core,
            command_sender,
        )
        .await?;

        // Get initial stats
        let initial_stats = coordinator.get_stats().await;
        assert_eq!(initial_stats.operations_coordinated, 0);
        assert_eq!(initial_stats.events_received, 0);
        assert!(!initial_stats.listener_connected);

        coordinator.start().await?;
        sleep(Duration::from_millis(100)).await;
        coordinator.stop().await?;

        // Stats should be accessible after stop
        let final_stats = coordinator.get_stats().await;
        assert_eq!(final_stats.namespace, "test_stats");

        Ok(())
    }
}
