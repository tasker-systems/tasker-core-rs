//! Fallback poller for orchestration queue reliability in TAS-43
//!
//! This module provides a safety net polling mechanism that directly queries
//! orchestration queues to catch any messages missed by the pgmq-notify event system.
//! It operates much slower than event-driven notifications but ensures zero
//! missed messages in production environments.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{
    messaging::{PgmqClientTrait, UnifiedPgmqClient},
    system_context::SystemContext,
    TaskerResult,
};

// TAS-43: Import command pattern types for direct command sending
use crate::orchestration::command_processor::OrchestrationCommand;

/// Configuration for orchestration queue fallback polling
///
/// Controls the behavior of the fallback safety net, including polling intervals,
/// message age thresholds, and batch sizes for processing missed queue messages.
#[derive(Debug, Clone)]
pub struct OrchestrationPollerConfig {
    /// Enable fallback polling (should always be true for production reliability)
    pub enabled: bool,
    /// Polling interval (much slower than event-driven, e.g., 30 seconds)
    pub polling_interval: Duration,
    /// Batch size for reading messages from queues
    pub batch_size: u32,
    /// Message age threshold - only poll for messages older than this (avoids race with events)
    pub age_threshold: Duration,
    /// Maximum message age to poll for (prevents infinite old message processing)
    pub max_age: Duration,
    /// Queue names to poll for fallback
    pub monitored_queues: Vec<String>,
    /// Namespace for queue operations
    pub namespace: String,
    /// Visibility timeout for polled messages
    pub visibility_timeout: Duration,
}

impl Default for OrchestrationPollerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            polling_interval: Duration::from_secs(30), // Much slower than event-driven
            batch_size: 50,
            age_threshold: Duration::from_secs(5), // Only poll for messages >5 seconds old
            max_age: Duration::from_secs(24 * 60 * 60), // Don't poll for messages >24 hours old
            monitored_queues: vec![
                "orchestration_step_results".to_string(),
                "orchestration_task_requests".to_string(),
            ],
            namespace: "orchestration".to_string(),
            visibility_timeout: Duration::from_secs(30),
        }
    }
}

/// Fallback poller for orchestration queue messages
///
/// Provides queue polling safety net to catch messages missed by pgmq-notify events.
/// Uses direct queue queries to find older messages that may have been missed
/// by the event-driven coordination system.
pub struct OrchestrationFallbackPoller {
    /// Poller identifier
    poller_id: Uuid,
    /// Configuration
    config: OrchestrationPollerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Command sender for orchestration operations
    command_sender: mpsc::Sender<OrchestrationCommand>,
    /// PGMQ client for direct queue access
    pgmq_client: Option<Arc<UnifiedPgmqClient>>,
    /// Running state
    is_running: AtomicBool,
    /// Statistics
    stats: OrchestrationPollerStats,
}

/// Runtime statistics for orchestration fallback poller
#[derive(Debug, Default)]
pub struct OrchestrationPollerStats {
    /// Total polling cycles completed
    pub polling_cycles: AtomicU64,
    /// Total messages found and processed
    pub messages_processed: AtomicU64,
    /// Step result messages processed
    pub step_results_processed: AtomicU64,
    /// Task request messages processed
    pub task_requests_processed: AtomicU64,
    /// Messages skipped (too new or too old)
    pub messages_skipped: AtomicU64,
    /// Polling errors encountered
    pub polling_errors: AtomicU64,
    /// Last polling cycle timestamp
    pub last_poll_at: Arc<parking_lot::Mutex<Option<Instant>>>,
    /// Poller startup timestamp
    pub started_at: Arc<parking_lot::Mutex<Option<Instant>>>,
}

impl OrchestrationFallbackPoller {
    /// Create new orchestration fallback poller
    pub async fn new(
        config: OrchestrationPollerConfig,
        context: Arc<SystemContext>,
        command_sender: mpsc::Sender<OrchestrationCommand>,
        pgmq_client: Arc<UnifiedPgmqClient>,
    ) -> TaskerResult<Self> {
        let poller_id = Uuid::new_v4();

        info!(
            poller_id = %poller_id,
            namespace = %config.namespace,
            monitored_queues = ?config.monitored_queues,
            polling_interval = ?config.polling_interval,
            "Creating OrchestrationFallbackPoller"
        );

        Ok(Self {
            poller_id,
            config,
            context,
            command_sender,
            pgmq_client: Some(pgmq_client),
            is_running: AtomicBool::new(false),
            stats: OrchestrationPollerStats::default(),
        })
    }

    /// Start the fallback poller
    pub async fn start(&self) -> TaskerResult<()> {
        if !self.config.enabled {
            info!(
                poller_id = %self.poller_id,
                "OrchestrationFallbackPoller disabled by configuration"
            );
            return Ok(());
        }

        info!(
            poller_id = %self.poller_id,
            "Starting OrchestrationFallbackPoller"
        );

        self.is_running.store(true, Ordering::Relaxed);
        *self.stats.started_at.lock() = Some(Instant::now());

        // Start polling loop in background task
        self.start_polling_loop().await?;

        info!(
            poller_id = %self.poller_id,
            "OrchestrationFallbackPoller started successfully"
        );

        Ok(())
    }

    /// Stop the fallback poller
    pub async fn stop(&self) -> TaskerResult<()> {
        info!(
            poller_id = %self.poller_id,
            "Stopping OrchestrationFallbackPoller"
        );

        self.is_running.store(false, Ordering::Relaxed);

        info!(
            poller_id = %self.poller_id,
            "OrchestrationFallbackPoller stopped successfully"
        );

        Ok(())
    }

    /// Check if poller is running and healthy
    pub fn is_healthy(&self) -> bool {
        self.is_running.load(Ordering::Relaxed) && self.pgmq_client.is_some()
    }

    /// Get poller statistics
    pub fn stats(&self) -> OrchestrationPollerStats {
        OrchestrationPollerStats {
            polling_cycles: AtomicU64::new(self.stats.polling_cycles.load(Ordering::Relaxed)),
            messages_processed: AtomicU64::new(
                self.stats.messages_processed.load(Ordering::Relaxed),
            ),
            step_results_processed: AtomicU64::new(
                self.stats.step_results_processed.load(Ordering::Relaxed),
            ),
            task_requests_processed: AtomicU64::new(
                self.stats.task_requests_processed.load(Ordering::Relaxed),
            ),
            messages_skipped: AtomicU64::new(self.stats.messages_skipped.load(Ordering::Relaxed)),
            polling_errors: AtomicU64::new(self.stats.polling_errors.load(Ordering::Relaxed)),
            last_poll_at: Arc::new(parking_lot::Mutex::new(*self.stats.last_poll_at.lock())),
            started_at: Arc::new(parking_lot::Mutex::new(*self.stats.started_at.lock())),
        }
    }

    /// Start fallback polling loop (based on EventDrivenOrchestrationCoordinator implementation)
    async fn start_polling_loop(&self) -> TaskerResult<()> {
        let config = self.config.clone();
        let context = self.context.clone();
        let command_sender = self.command_sender.clone();
        let poller_id = self.poller_id;
        let stats = OrchestrationPollerStatsRef {
            polling_cycles: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            messages_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            step_results_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            task_requests_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            messages_skipped: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            polling_errors: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            last_poll_at: self.stats.last_poll_at.clone(),
        };
        let classifier = tasker_shared::config::QueueClassifier::new(
            self.context
                .config_manager
                .config()
                .orchestration
                .queues
                .orchestration_owned
                .clone(),
            config.namespace.clone(),
        );

        tokio::spawn(async move {
            info!(
                poller_id = %poller_id,
                interval_ms = %config.polling_interval.as_millis(),
                "Starting fallback polling for orchestration coordination reliability"
            );

            let mut interval = tokio::time::interval(config.polling_interval);

            let monitored_queues = vec![
                classifier.ensure_orchestration_prefix("orchestration_step_results"),
                classifier.ensure_orchestration_prefix("orchestration_task_requests"),
                classifier.ensure_orchestration_prefix("orchestration_task_finalizations"),
            ];

            loop {
                interval.tick().await;
                stats.polling_cycles.fetch_add(1, Ordering::Relaxed);
                *stats.last_poll_at.lock() = Some(Instant::now());

                // Poll all monitored queues
                for queue_name in &monitored_queues {
                    Self::poll_queue_for_messages(
                        &context,
                        &command_sender,
                        queue_name,
                        &config,
                        poller_id,
                        &classifier,
                        &stats,
                    )
                    .await;
                }
            }
        });

        Ok(())
    }

    /// Poll a single queue for messages (based on EventDrivenOrchestrationCoordinator)
    async fn poll_queue_for_messages(
        context: &SystemContext,
        command_sender: &mpsc::Sender<OrchestrationCommand>,
        queue_name: &str,
        config: &OrchestrationPollerConfig,
        poller_id: Uuid,
        classifier: &tasker_shared::config::QueueClassifier,
        stats: &OrchestrationPollerStatsRef,
    ) {
        debug!(
            poller_id = %poller_id,
            queue = %queue_name,
            "Performing fallback polling check"
        );

        match context
            .message_client
            .read_messages(
                queue_name,
                Some(config.visibility_timeout.as_secs() as i32),
                Some(config.batch_size as i32),
            )
            .await
        {
            Ok(messages) => {
                debug!(
                    poller_id = %poller_id,
                    queue = %queue_name,
                    count = messages.len(),
                    "Read messages from fallback polling"
                );

                for message in messages {
                    // Create message event for config-driven classification
                    let message_event = pgmq_notify::MessageReadyEvent {
                        msg_id: message.msg_id,
                        queue_name: queue_name.to_string(),
                        namespace: config.namespace.clone(),
                        ready_at: chrono::Utc::now(),
                        metadata: std::collections::HashMap::new(),
                        visibility_timeout_seconds: Some(config.visibility_timeout.as_secs() as i32),
                    };

                    // Use config-driven classification like EventDrivenOrchestrationCoordinator
                    let classified_message =
                        tasker_shared::config::ConfigDrivenMessageEvent::classify(
                            message_event.clone(),
                            queue_name,
                            classifier,
                        );

                    let command_result = match classified_message {
                        tasker_shared::config::ConfigDrivenMessageEvent::StepResults(_event) => {
                            stats.step_results_processed.fetch_add(1, Ordering::Relaxed);
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            command_sender
                                .send(OrchestrationCommand::ProcessStepResultFromMessage {
                                    queue_name: queue_name.to_string(),
                                    message: message.clone(),
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::TaskRequests(_event) => {
                            stats
                                .task_requests_processed
                                .fetch_add(1, Ordering::Relaxed);
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            command_sender
                                .send(OrchestrationCommand::InitializeTaskFromMessage {
                                    queue_name: queue_name.to_string(),
                                    message: message.clone(),
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::TaskFinalizations(
                            _event,
                        ) => {
                            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                            command_sender
                                .send(OrchestrationCommand::FinalizeTaskFromMessage {
                                    queue_name: queue_name.to_string(),
                                    message: message.clone(),
                                    resp: resp_tx,
                                })
                                .await
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::WorkerNamespace {
                            namespace,
                            event: _,
                        } => {
                            debug!(
                                poller_id = %poller_id,
                                queue = %queue_name,
                                namespace = %namespace,
                                "Worker namespace message received in orchestration fallback polling"
                            );
                            continue;
                        }
                        tasker_shared::config::ConfigDrivenMessageEvent::Unknown(event) => {
                            warn!(
                                poller_id = %poller_id,
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
                            poller_id = %poller_id,
                            msg_id = message.msg_id,
                            queue = %queue_name,
                            error = %e,
                            "Failed to send command from fallback polling"
                        );
                        stats.polling_errors.fetch_add(1, Ordering::Relaxed);
                    } else {
                        stats.messages_processed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            Err(e) => {
                error!(
                    poller_id = %poller_id,
                    queue = %queue_name,
                    error = %e,
                    "Failed to read messages from fallback polling"
                );
                stats.polling_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Reference wrapper for atomic statistics (for sharing across async boundaries)
struct OrchestrationPollerStatsRef {
    polling_cycles: Arc<AtomicU64>,
    messages_processed: Arc<AtomicU64>,
    step_results_processed: Arc<AtomicU64>,
    task_requests_processed: Arc<AtomicU64>,
    messages_skipped: Arc<AtomicU64>,
    polling_errors: Arc<AtomicU64>,
    last_poll_at: Arc<parking_lot::Mutex<Option<Instant>>>,
}
