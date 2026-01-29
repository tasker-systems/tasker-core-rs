//! Fallback poller for worker namespace queue reliability in TAS-43
//!
//! This module provides a safety net polling mechanism that directly queries
//! worker namespace queues to catch any messages missed by event-driven systems.
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

use tasker_shared::{
    messaging::service::MessagingProvider, system_context::SystemContext, TaskerResult,
};

// Import command pattern types for direct command sending
use crate::worker::channels::WorkerCommandSender;
use crate::worker::command_processor::WorkerCommand;

/// Configuration for worker namespace queue fallback polling
///
/// Controls the behavior of the fallback safety net, including polling intervals,
/// message age thresholds, and batch sizes for processing missed namespace queue messages.
#[derive(Debug, Clone)]
pub struct WorkerPollerConfig {
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
    /// Visibility timeout for polled messages
    pub visibility_timeout: Duration,
    /// Supported namespaces for fallback polling
    pub supported_namespaces: Vec<String>,
}

impl Default for WorkerPollerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            polling_interval: Duration::from_secs(5), // Faster than orchestration as workers need quick response
            batch_size: 10,
            age_threshold: Duration::from_secs(2), // Only poll for messages >2 seconds old
            max_age: Duration::from_secs(12 * 60 * 60), // Don't poll for messages >12 hours old
            visibility_timeout: Duration::from_secs(30),
            supported_namespaces: vec![],
        }
    }
}

/// Fallback poller for worker namespace queue messages
///
/// Provides queue polling safety net to catch messages missed by tasker-pgmq events.
/// Uses direct queue queries to find older messages that may have been missed
/// by the event-driven coordination system.
pub(crate) struct WorkerFallbackPoller {
    /// Poller identifier
    poller_id: Uuid,
    /// Configuration
    config: WorkerPollerConfig,
    /// System context
    context: Arc<SystemContext>,
    /// Command sender for worker operations (TAS-133: NewType wrapper)
    command_sender: WorkerCommandSender,
    /// Running state
    is_running: AtomicBool,
    /// Statistics
    stats: WorkerPollerStats,
}

impl std::fmt::Debug for WorkerFallbackPoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerFallbackPoller")
            .field("poller_id", &self.poller_id)
            .field("config", &self.config)
            .field("has_command_sender", &true)
            .field(
                "is_running",
                &self.is_running.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

/// Runtime statistics for worker fallback poller
#[derive(Debug, Default)]
pub struct WorkerPollerStats {
    /// Total polling cycles completed
    pub polling_cycles: AtomicU64,
    /// Total messages found and processed
    pub messages_processed: AtomicU64,
    /// Step messages processed
    pub step_messages_processed: AtomicU64,
    /// Messages skipped (too new or too old)
    pub messages_skipped: AtomicU64,
    /// Polling errors encountered
    pub polling_errors: AtomicU64,
    /// Last polling cycle timestamp
    pub last_poll_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
    /// Poller startup timestamp
    pub started_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl WorkerFallbackPoller {
    /// Create new worker fallback poller
    ///
    /// The fallback poller is provider-agnostic - it uses the `MessagingProvider`
    /// abstraction for polling. The decision of WHETHER to use fallback polling
    /// (based on provider capabilities) should be made by the event system before
    /// creating this poller.
    pub async fn new(
        config: WorkerPollerConfig,
        context: Arc<SystemContext>,
        command_sender: WorkerCommandSender,
    ) -> TaskerResult<Self> {
        let poller_id = Uuid::new_v4();

        info!(
            poller_id = %poller_id,
            polling_interval = ?config.polling_interval,
            provider = %context.messaging_provider().provider_name(),
            "Creating WorkerFallbackPoller"
        );

        Ok(Self {
            poller_id,
            config,
            context,
            command_sender,
            is_running: AtomicBool::new(false),
            stats: WorkerPollerStats::default(),
        })
    }

    /// Start the fallback poller
    pub async fn start(&self) -> TaskerResult<()> {
        if !self.config.enabled {
            info!(
                poller_id = %self.poller_id,
                "WorkerFallbackPoller disabled by configuration"
            );
            return Ok(());
        }

        info!(
            poller_id = %self.poller_id,
            polling_interval = ?self.config.polling_interval,
            "Starting WorkerFallbackPoller"
        );

        self.is_running.store(true, Ordering::SeqCst);
        *self.stats.started_at.lock().await = Some(Instant::now());

        // Start polling loop in background task
        let poller_id = self.poller_id;
        let config = self.config.clone();
        let command_sender = self.command_sender.clone();
        // Use provider-agnostic MessagingProvider for polling
        let messaging_provider = self.context.messaging_provider().clone();
        // Use message router for queue name abstraction
        let message_client = self.context.message_client.clone();
        let stats = Arc::new(WorkerPollerStats::default());
        let is_running = Arc::new(AtomicBool::new(true));

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.polling_interval);

            while is_running.load(Ordering::SeqCst) {
                interval.tick().await;

                debug!(poller_id = %poller_id, "Starting worker fallback polling cycle");

                let cycle_start = Instant::now();
                stats.polling_cycles.fetch_add(1, Ordering::Relaxed);
                *stats.last_poll_at.lock().await = Some(cycle_start);

                // Poll all supported namespace queues
                for namespace in &config.supported_namespaces {
                    // Use router abstraction for queue naming
                    let queue_name = message_client.router().step_queue(namespace);

                    if let Err(e) = Self::poll_namespace_queue(
                        &queue_name,
                        namespace,
                        &config,
                        &messaging_provider,
                        &command_sender,
                        &stats,
                        poller_id,
                    )
                    .await
                    {
                        error!(
                            poller_id = %poller_id,
                            namespace = %namespace,
                            error = %e,
                            "Failed to poll namespace queue"
                        );
                        stats.polling_errors.fetch_add(1, Ordering::Relaxed);
                    }
                }

                let cycle_duration = cycle_start.elapsed();
                debug!(
                    poller_id = %poller_id,
                    duration_ms = cycle_duration.as_millis(),
                    "Completed worker fallback polling cycle"
                );
            }

            info!(poller_id = %poller_id, "WorkerFallbackPoller stopped");
        });

        Ok(())
    }

    /// Poll a specific namespace queue for missed messages
    ///
    /// Uses the provider-agnostic `MessagingProvider.receive_messages()` abstraction.
    /// The returned `QueuedMessage` objects already have proper handles set by the provider.
    async fn poll_namespace_queue(
        queue_name: &str,
        namespace: &str,
        config: &WorkerPollerConfig,
        messaging_provider: &MessagingProvider,
        command_sender: &WorkerCommandSender,
        stats: &WorkerPollerStats,
        poller_id: Uuid,
    ) -> TaskerResult<()> {
        debug!(
            poller_id = %poller_id,
            queue = %queue_name,
            namespace = %namespace,
            batch_size = config.batch_size,
            "Polling namespace queue for missed messages"
        );

        // Use provider-agnostic receive_messages() - returns QueuedMessage with proper handles
        let messages: Vec<_> = messaging_provider
            .receive_messages::<serde_json::Value>(
                queue_name,
                config.batch_size as usize,
                config.visibility_timeout,
            )
            .await
            .map_err(|e| {
                tasker_shared::TaskerError::MessagingError(format!(
                    "Failed to read messages: {}",
                    e
                ))
            })?;

        if messages.is_empty() {
            // Only log at debug level for empty queues to avoid noise
            debug!(
                poller_id = %poller_id,
                queue = %queue_name,
                namespace = %namespace,
                "No messages found in fallback polling"
            );
            return Ok(());
        }

        debug!(
            poller_id = %poller_id,
            queue = %queue_name,
            namespace = %namespace,
            count = messages.len(),
            "Found messages in worker fallback polling - sending to command processor"
        );

        for queued_message in messages {
            // Messages from receive_messages() already have proper handles - send directly
            let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();

            let command_result = command_sender
                .send(WorkerCommand::ExecuteStepFromMessage {
                    message: queued_message,
                    resp: resp_tx,
                })
                .await;

            if let Err(e) = command_result {
                warn!(
                    poller_id = %poller_id,
                    queue = %queue_name,
                    namespace = %namespace,
                    error = %e,
                    "Failed to send worker command from fallback polling"
                );
                stats.polling_errors.fetch_add(1, Ordering::Relaxed);
            } else {
                stats.messages_processed.fetch_add(1, Ordering::Relaxed);
                stats
                    .step_messages_processed
                    .fetch_add(1, Ordering::Relaxed);

                debug!(
                    poller_id = %poller_id,
                    queue = %queue_name,
                    namespace = %namespace,
                    "Successfully sent worker command from fallback polling"
                );
            }
        }

        Ok(())
    }

    /// Stop the fallback poller
    pub async fn stop(&self) {
        info!(poller_id = %self.poller_id, "Stopping WorkerFallbackPoller");
        self.is_running.store(false, Ordering::SeqCst);
    }

    /// Get poller statistics
    #[expect(dead_code, reason = "Public API for monitoring poller statistics")]
    pub fn stats(&self) -> &WorkerPollerStats {
        &self.stats
    }

    /// Check if poller is running
    #[expect(dead_code, reason = "Public API for checking poller running state")]
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Get poller ID
    #[expect(dead_code, reason = "Public API for getting poller identifier")]
    pub fn poller_id(&self) -> Uuid {
        self.poller_id
    }
}
