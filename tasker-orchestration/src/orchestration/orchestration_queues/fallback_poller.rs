//! Fallback poller for orchestration queue reliability in TAS-43
//!
//! This module provides a safety net polling mechanism that directly queries
//! orchestration queues to catch any messages missed by event-driven systems.
//! It operates much slower than event-driven notifications but ensures zero
//! missed messages in production environments.
//!
//! ## Provider Agnostic Design
//!
//! The fallback poller uses the generic `MessagingProvider` abstraction rather than
//! provider-specific APIs. This means:
//! - Any provider that implements `receive_messages()` can use fallback polling
//! - The decision of WHETHER to poll lives in the event system (based on provider capabilities)
//! - The HOW to poll is provider-agnostic (this module)
//!
//! Currently, PGMQ needs fallback polling (pg_notify can miss signals), while RabbitMQ
//! does not (basic_consume with ack/nack is reliable). Future providers may or may not
//! need polling based on their delivery guarantees.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::service::MessagingProvider;
use tasker_shared::{system_context::SystemContext, TaskerResult};

use crate::orchestration::channels::OrchestrationCommandSender;
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
/// Provides queue polling safety net to catch messages missed by event-driven systems.
/// Uses the provider-agnostic `MessagingProvider` abstraction to poll queues.
///
/// The decision of WHETHER to use fallback polling (based on provider capabilities)
/// should be made by the event system before creating this poller.
pub struct OrchestrationFallbackPoller {
    /// Poller identifier
    poller_id: Uuid,
    /// Configuration
    config: OrchestrationPollerConfig,
    /// System context (provides access to MessagingProvider)
    context: Arc<SystemContext>,
    /// Command sender for orchestration operations
    /// TAS-133: Uses NewType wrapper for type-safe channel communication
    command_sender: OrchestrationCommandSender,
    /// Running state
    is_running: AtomicBool,
    /// Statistics
    stats: OrchestrationPollerStats,
}

impl std::fmt::Debug for OrchestrationFallbackPoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationFallbackPoller")
            .field("poller_id", &self.poller_id)
            .field("config", &self.config)
            .field(
                "provider",
                &self.context.messaging_provider().provider_name(),
            )
            .field(
                "is_running",
                &self.is_running.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
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
    pub last_poll_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
    /// Poller startup timestamp
    pub started_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl OrchestrationFallbackPoller {
    /// Create new orchestration fallback poller
    ///
    /// The fallback poller is provider-agnostic - it uses the `MessagingProvider`
    /// abstraction for polling. The decision of WHETHER to use fallback polling
    /// (based on provider capabilities) should be made by the event system before
    /// creating this poller.
    pub async fn new(
        config: OrchestrationPollerConfig,
        context: Arc<SystemContext>,
        command_sender: OrchestrationCommandSender,
    ) -> TaskerResult<Self> {
        let poller_id = Uuid::new_v4();

        info!(
            poller_id = %poller_id,
            namespace = %config.namespace,
            monitored_queues = ?config.monitored_queues,
            polling_interval = ?config.polling_interval,
            provider = %context.messaging_provider().provider_name(),
            "Creating OrchestrationFallbackPoller"
        );

        Ok(Self {
            poller_id,
            config,
            context,
            command_sender,
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
        *self.stats.started_at.lock().await = Some(Instant::now());

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
        self.is_running.load(Ordering::Relaxed)
    }

    /// Get poller statistics
    pub async fn stats(&self) -> OrchestrationPollerStats {
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
            last_poll_at: Arc::new(tokio::sync::Mutex::new(
                *self.stats.last_poll_at.lock().await,
            )),
            started_at: Arc::new(tokio::sync::Mutex::new(*self.stats.started_at.lock().await)),
        }
    }

    /// Start fallback polling loop
    async fn start_polling_loop(&self) -> TaskerResult<()> {
        let config = self.config.clone();
        let command_sender = self.command_sender.clone();
        let poller_id = self.poller_id;
        // Use provider-agnostic MessagingProvider for polling
        let messaging_provider = self.context.messaging_provider().clone();
        let stats = OrchestrationPollerStatsRef {
            polling_cycles: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            messages_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            step_results_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            task_requests_processed: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            polling_errors: Arc::new(AtomicU64::new(0)), // Shared counter for async task
            last_poll_at: self.stats.last_poll_at.clone(),
        };
        // TAS-61 V2: Access queues from common config
        let queue_config = self.context.tasker_config.common.queues.clone();
        let classifier = tasker_shared::config::QueueClassifier::from_queues_config(&queue_config);

        tokio::spawn(async move {
            info!(
                poller_id = %poller_id,
                interval_ms = %config.polling_interval.as_millis(),
                "Starting fallback polling for orchestration coordination reliability"
            );

            let mut interval = tokio::time::interval(config.polling_interval);

            // Use configured queue names from classifier (no hardcoding)
            let monitored_queues = vec![
                classifier.step_results_queue_name().to_string(),
                classifier.task_requests_queue_name().to_string(),
                classifier.task_finalizations_queue_name().to_string(),
            ];

            loop {
                interval.tick().await;
                stats.polling_cycles.fetch_add(1, Ordering::Relaxed);
                *stats.last_poll_at.lock().await = Some(Instant::now());

                // Poll all monitored queues
                for queue_name in &monitored_queues {
                    Self::poll_queue_for_messages(
                        &messaging_provider,
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

    /// Poll a single queue for messages
    ///
    /// Uses the provider-agnostic `MessagingProvider.receive_messages()` abstraction.
    /// Classifies messages based on queue name and dispatches to appropriate command handlers.
    async fn poll_queue_for_messages(
        messaging_provider: &MessagingProvider,
        command_sender: &OrchestrationCommandSender,
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

        // Use provider-agnostic receive_messages() - returns QueuedMessage with proper handles
        let messages = match messaging_provider
            .receive_messages::<serde_json::Value>(
                queue_name,
                config.batch_size as usize,
                config.visibility_timeout,
            )
            .await
        {
            Ok(msgs) => msgs,
            Err(e) => {
                error!(
                    poller_id = %poller_id,
                    queue = %queue_name,
                    error = %e,
                    "Failed to read messages from fallback polling"
                );
                stats.polling_errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        if messages.is_empty() {
            debug!(
                poller_id = %poller_id,
                queue = %queue_name,
                "No messages found in fallback polling"
            );
            return;
        }

        debug!(
            poller_id = %poller_id,
            queue = %queue_name,
            count = messages.len(),
            "Read messages from fallback polling"
        );

        // Classify queue once (all messages from same queue have same type)
        let queue_type = classifier.classify(queue_name);

        for queued_message in messages {
            // Messages from receive_messages() already have proper handles
            let command_result = match &queue_type {
                tasker_shared::config::QueueType::StepResults => {
                    stats.step_results_processed.fetch_add(1, Ordering::Relaxed);
                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                    command_sender
                        .send(OrchestrationCommand::ProcessStepResultFromMessage {
                            message: queued_message,
                            resp: resp_tx,
                        })
                        .await
                }
                tasker_shared::config::QueueType::TaskRequests => {
                    stats
                        .task_requests_processed
                        .fetch_add(1, Ordering::Relaxed);
                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                    command_sender
                        .send(OrchestrationCommand::InitializeTaskFromMessage {
                            message: queued_message,
                            resp: resp_tx,
                        })
                        .await
                }
                tasker_shared::config::QueueType::TaskFinalizations => {
                    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
                    command_sender
                        .send(OrchestrationCommand::FinalizeTaskFromMessage {
                            message: queued_message,
                            resp: resp_tx,
                        })
                        .await
                }
                tasker_shared::config::QueueType::WorkerNamespace(namespace) => {
                    debug!(
                        poller_id = %poller_id,
                        queue = %queue_name,
                        namespace = %namespace,
                        "Worker namespace message received in orchestration fallback polling"
                    );
                    continue;
                }
                tasker_shared::config::QueueType::Unknown => {
                    warn!(
                        poller_id = %poller_id,
                        queue = %queue_name,
                        "Unknown queue type in fallback polling",
                    );
                    continue;
                }
            };

            if let Err(e) = command_result {
                warn!(
                    poller_id = %poller_id,
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
}

/// Reference wrapper for atomic statistics (for sharing across async boundaries)
struct OrchestrationPollerStatsRef {
    polling_cycles: Arc<AtomicU64>,
    messages_processed: Arc<AtomicU64>,
    step_results_processed: Arc<AtomicU64>,
    task_requests_processed: Arc<AtomicU64>,
    polling_errors: Arc<AtomicU64>,
    last_poll_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}
