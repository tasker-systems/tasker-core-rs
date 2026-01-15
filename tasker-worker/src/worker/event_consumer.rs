//! # TAS-65 Phase 2.3b: Event Consumer Service
//!
//! Polling-based consumer for domain events with retry logic and DLQ support.
//!
//! ## Architecture
//!
//! - **Polling Loop**: Uses `tokio::time::interval` for periodic queue polling
//! - **Backpressure Control**: Semaphore-based concurrency limiting
//! - **Event Dispatch**: Routes events to EventRegistry handlers
//! - **Error Handling**: Success → delete, failure → DLQ
//! - **Observability**: Atomic counters + OpenTelemetry metrics
//!
//! ## Provider-Agnostic Design (TAS-133)
//!
//! This consumer uses `MessageClient` for all messaging operations, making it
//! compatible with any messaging provider (PGMQ, RabbitMQ, InMemory).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//! use tasker_worker::worker::event_consumer::{EventConsumer, EventConsumerConfig};
//! use tasker_shared::system_context::SystemContext;
//! use tasker_shared::events::registry::EventRegistry;
//!
//! # async fn example(context: Arc<SystemContext>) -> Result<(), Box<dyn std::error::Error>> {
//! let config = EventConsumerConfig {
//!     namespace: "payments".to_string(),
//!     poll_interval: std::time::Duration::from_secs(1),
//!     batch_size: 10,
//!     visibility_timeout: std::time::Duration::from_secs(30),
//!     max_concurrent_handlers: 10,
//!     handler_timeout: std::time::Duration::from_secs(30),
//! };
//!
//! let event_registry = Arc::new(RwLock::new(EventRegistry::new()));
//!
//! let consumer = Arc::new(EventConsumer::new(
//!     context,
//!     event_registry,
//!     config,
//! )?);
//!
//! // Start background polling
//! consumer.clone().start().await?;
//! # Ok(())
//! # }
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, error, info, instrument, warn};

use tasker_shared::{
    events::domain_events::{DomainEvent, DomainEventError},
    events::registry::EventRegistry,
    messaging::service::ReceiptHandle,
    system_context::SystemContext,
    TaskerError, TaskerResult,
};

/// Configuration for the event consumer
#[derive(Debug, Clone)]
pub struct EventConsumerConfig {
    /// Namespace for the event consumer (determines queue name)
    pub namespace: String,
    /// Polling interval for checking the queue
    pub poll_interval: Duration,
    /// Number of messages to fetch per batch
    pub batch_size: i32,
    /// Visibility timeout for messages being processed
    pub visibility_timeout: Duration,
    /// Maximum number of concurrent handlers
    pub max_concurrent_handlers: usize,
    /// Timeout for individual handler execution
    pub handler_timeout: Duration,
}

impl Default for EventConsumerConfig {
    fn default() -> Self {
        Self {
            namespace: "default".to_string(),
            poll_interval: Duration::from_secs(1),
            batch_size: 10,
            visibility_timeout: Duration::from_secs(30),
            max_concurrent_handlers: 10,
            handler_timeout: Duration::from_secs(30),
        }
    }
}

impl EventConsumerConfig {
    /// Validate configuration parameters
    pub fn validate(&self) -> TaskerResult<()> {
        if self.namespace.is_empty() {
            return Err(TaskerError::ConfigurationError(
                "namespace cannot be empty".to_string(),
            ));
        }

        if self.batch_size < 1 {
            return Err(TaskerError::ConfigurationError(
                "batch_size must be at least 1".to_string(),
            ));
        }

        if self.max_concurrent_handlers < 1 {
            return Err(TaskerError::ConfigurationError(
                "max_concurrent_handlers must be at least 1".to_string(),
            ));
        }

        Ok(())
    }

    /// Get the domain events queue name for this namespace
    pub fn domain_events_queue(&self) -> String {
        format!("{}_domain_events", self.namespace)
    }

    /// Get the DLQ name for this namespace
    pub fn dlq_name(&self) -> String {
        format!("{}_domain_events_dlq", self.namespace)
    }
}

/// Statistics for event consumer observability
#[derive(Debug, Default)]
pub struct EventConsumerStats {
    /// Total number of polling cycles executed
    pub polling_cycles: AtomicU64,
    /// Total number of events successfully processed
    pub events_processed: AtomicU64,
    /// Total number of events that failed processing
    pub events_failed: AtomicU64,
    /// Total number of events sent to DLQ
    pub events_dlq: AtomicU64,
}

impl EventConsumerStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_polling_cycles(&self) -> u64 {
        self.polling_cycles.load(Ordering::Relaxed)
    }

    pub fn get_events_processed(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    pub fn get_events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    pub fn get_events_dlq(&self) -> u64 {
        self.events_dlq.load(Ordering::Relaxed)
    }
}

/// Event consumer that polls domain event queues and dispatches to handlers
///
/// TAS-133e: Updated to use SystemContext for messaging provider access.
pub struct EventConsumer {
    /// System context for messaging operations (TAS-133e)
    context: Arc<SystemContext>,
    /// Event registry for handler dispatch
    event_registry: Arc<RwLock<EventRegistry>>,
    /// Consumer configuration
    config: EventConsumerConfig,
    /// Running state flag
    running: Arc<AtomicBool>,
    /// Statistics for observability
    stats: Arc<EventConsumerStats>,
}

impl EventConsumer {
    /// Create a new event consumer
    ///
    /// TAS-133: Provider-agnostic - works with any messaging provider via MessageClient.
    pub fn new(
        context: Arc<SystemContext>,
        event_registry: Arc<RwLock<EventRegistry>>,
        config: EventConsumerConfig,
    ) -> TaskerResult<Self> {
        // Validate configuration
        config.validate()?;

        // TAS-133: Provider-agnostic - no provider-specific validation needed.
        // All messaging operations go through MessageClient.

        Ok(Self {
            context,
            event_registry,
            config,
            running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(EventConsumerStats::new()),
        })
    }

    /// Start the event consumer polling loop
    #[instrument(skip(self), fields(namespace = %self.config.namespace))]
    pub async fn start(self: Arc<Self>) -> TaskerResult<()> {
        info!(
            namespace = %self.config.namespace,
            poll_interval = ?self.config.poll_interval,
            batch_size = self.config.batch_size,
            "Starting event consumer"
        );

        self.running.store(true, Ordering::SeqCst);

        // Spawn the polling loop
        let consumer = self.clone();
        tokio::spawn(async move {
            if let Err(e) = consumer.polling_loop().await {
                error!("Event consumer polling loop failed: {}", e);
            }
        });

        Ok(())
    }

    /// Stop the event consumer
    #[instrument(skip(self), fields(namespace = %self.config.namespace))]
    pub async fn stop(&self) -> TaskerResult<()> {
        info!(
            namespace = %self.config.namespace,
            "Stopping event consumer"
        );

        self.running.store(false, Ordering::SeqCst);

        // Wait a bit for the polling loop to finish current iteration
        tokio::time::sleep(self.config.poll_interval).await;

        info!(
            namespace = %self.config.namespace,
            "Event consumer stopped"
        );

        Ok(())
    }

    /// Check if the consumer is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get consumer statistics
    pub fn get_stats(&self) -> Arc<EventConsumerStats> {
        self.stats.clone()
    }

    /// Main polling loop
    async fn polling_loop(self: Arc<Self>) -> TaskerResult<()> {
        let mut interval = tokio::time::interval(self.config.poll_interval);

        while self.running.load(Ordering::SeqCst) {
            interval.tick().await;

            // Update polling cycle counter (atomic, not metric - hot path)
            self.stats.polling_cycles.fetch_add(1, Ordering::Relaxed);

            if let Err(e) = self.poll_once().await {
                warn!(
                    namespace = %self.config.namespace,
                    error = %e,
                    "Poll iteration failed"
                );
            }
        }

        info!(
            namespace = %self.config.namespace,
            "Polling loop exited"
        );

        Ok(())
    }

    /// Execute a single poll iteration
    #[instrument(skip(self), fields(namespace = %self.config.namespace))]
    async fn poll_once(&self) -> TaskerResult<()> {
        let queue_name = self.config.domain_events_queue();

        debug!(
            queue = %queue_name,
            batch_size = self.config.batch_size,
            "Polling domain events queue"
        );

        // Fetch batch of messages from queue
        let messages = self.fetch_batch(&queue_name).await?;

        if messages.is_empty() {
            debug!(queue = %queue_name, "No messages in queue");
            return Ok(());
        }

        info!(
            queue = %queue_name,
            count = messages.len(),
            "Processing domain events batch"
        );

        // Process messages with backpressure control
        self.process_batch(messages).await?;

        Ok(())
    }

    /// Fetch a batch of messages from the queue
    ///
    /// TAS-133: Provider-agnostic - uses MessageClient.receive_messages().
    async fn fetch_batch(
        &self,
        queue_name: &str,
    ) -> TaskerResult<Vec<(ReceiptHandle, DomainEvent)>> {
        // TAS-133: Use provider-agnostic MessageClient
        // We receive as serde_json::Value to handle deserialization errors gracefully
        let raw_messages = self
            .context
            .message_client()
            .receive_messages::<serde_json::Value>(
                queue_name,
                self.config.batch_size as usize,
                self.config.visibility_timeout,
            )
            .await
            .map_err(|e| TaskerError::MessagingError(format!("Failed to read messages: {}", e)))?;

        // Parse messages into DomainEvent structs
        let mut events = Vec::new();
        for msg in raw_messages {
            let receipt_handle = msg.receipt_handle.clone();
            let receipt_handle_display = receipt_handle.to_string();

            match serde_json::from_value::<DomainEvent>(msg.message) {
                Ok(event) => {
                    events.push((receipt_handle, event));
                }
                Err(e) => {
                    warn!(
                        receipt_handle = %receipt_handle_display,
                        error = %e,
                        "Failed to deserialize domain event, sending to DLQ"
                    );

                    // Send malformed message to DLQ with serialization error
                    let dlq_error = DomainEventError::SerializationFailed {
                        event_name: "unknown".to_string(),
                        reason: e.to_string(),
                    };
                    if let Err(dlq_err) = self
                        .send_to_dlq(&receipt_handle_display, &dlq_error)
                        .await
                    {
                        error!(
                            receipt_handle = %receipt_handle_display,
                            error = %dlq_err,
                            "Failed to send malformed message to DLQ"
                        );
                    }

                    // Acknowledge (delete) the malformed message from the queue
                    if let Err(del_err) = self
                        .context
                        .message_client()
                        .ack_message(queue_name, &receipt_handle)
                        .await
                    {
                        error!(
                            receipt_handle = %receipt_handle_display,
                            error = %del_err,
                            "Failed to delete malformed message"
                        );
                    }
                }
            }
        }

        Ok(events)
    }

    /// Process a batch of messages with concurrent handlers
    #[instrument(skip(self, messages), fields(count = messages.len()))]
    async fn process_batch(
        &self,
        messages: Vec<(ReceiptHandle, DomainEvent)>,
    ) -> TaskerResult<()> {
        // Create semaphore for backpressure control
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_handlers));
        let mut handles = Vec::new();

        for (receipt_handle, event) in messages {
            let permit = semaphore.clone().acquire_owned().await.map_err(|e| {
                TaskerError::Internal(format!("Failed to acquire semaphore permit: {}", e))
            })?;

            let consumer = Arc::new(self.clone());
            let handle = tokio::spawn(async move {
                let result = consumer.process_message(receipt_handle, event).await;
                drop(permit); // Release semaphore permit
                result
            });

            handles.push(handle);
        }

        // Wait for all handlers to complete
        let results = join_all(handles).await;

        // Check for task panics
        for (idx, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                error!(
                    task_index = idx,
                    error = %e,
                    "Event processing task panicked"
                );
            }
        }

        Ok(())
    }

    /// Process a single message
    ///
    /// TAS-133: Uses ReceiptHandle for provider-agnostic message acknowledgment.
    #[instrument(skip(self, receipt_handle, event), fields(
        event_id = %event.event_id,
        event_name = %event.event_name
    ))]
    async fn process_message(
        self: Arc<Self>,
        receipt_handle: ReceiptHandle,
        event: DomainEvent,
    ) -> TaskerResult<()> {
        let queue_name = self.config.domain_events_queue();
        let receipt_handle_display = receipt_handle.to_string();

        debug!(
            receipt_handle = %receipt_handle_display,
            event_name = %event.event_name,
            "Processing domain event"
        );

        // Execute handler with timeout
        let handler_result = tokio::time::timeout(
            self.config.handler_timeout,
            self.dispatch_to_registry(event.clone()),
        )
        .await;

        match handler_result {
            Ok(Ok(())) => {
                // Success: acknowledge (delete) message from queue
                debug!(
                    receipt_handle = %receipt_handle_display,
                    event_name = %event.event_name,
                    "Event processed successfully, acknowledging message"
                );

                if let Err(e) = self.ack_message(&queue_name, &receipt_handle).await {
                    error!(
                        receipt_handle = %receipt_handle_display,
                        error = %e,
                        "Failed to acknowledge processed message"
                    );
                    return Err(e);
                }

                // Update success counter
                self.stats.events_processed.fetch_add(1, Ordering::Relaxed);

                // Emit OpenTelemetry metric for handler duration
                metrics::counter!(
                    "tasker.event_consumer.events_processed.total",
                    "namespace" => self.config.namespace.clone(),
                    "event_name" => event.event_name.clone()
                )
                .increment(1);

                Ok(())
            }
            Ok(Err(handler_error)) => {
                // Handler execution failed: send to DLQ
                error!(
                    receipt_handle = %receipt_handle_display,
                    event_name = %event.event_name,
                    error = %handler_error,
                    "Handler execution failed, sending to DLQ"
                );

                self.handle_failure(&receipt_handle, &receipt_handle_display, &event, handler_error)
                    .await
            }
            Err(_timeout) => {
                // Handler timed out: treat as failure
                error!(
                    receipt_handle = %receipt_handle_display,
                    event_name = %event.event_name,
                    timeout = ?self.config.handler_timeout,
                    "Handler execution timed out, sending to DLQ"
                );

                let timeout_error = TaskerError::EventError(format!(
                    "Handler timed out after {:?}",
                    self.config.handler_timeout
                ));

                self.handle_failure(&receipt_handle, &receipt_handle_display, &event, timeout_error)
                    .await
            }
        }
    }

    /// Dispatch event to the event registry
    async fn dispatch_to_registry(&self, event: DomainEvent) -> TaskerResult<()> {
        let registry = self.event_registry.read().await;
        let errors = registry.dispatch(&event).await;

        if errors.is_empty() {
            Ok(())
        } else {
            Err(TaskerError::EventError(format!(
                "Handler execution failed: {} errors: {}",
                errors.len(),
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    /// Handle message processing failure
    ///
    /// TAS-133: Uses ReceiptHandle for provider-agnostic message operations.
    async fn handle_failure(
        &self,
        receipt_handle: &ReceiptHandle,
        receipt_handle_display: &str,
        event: &DomainEvent,
        error: TaskerError,
    ) -> TaskerResult<()> {
        let queue_name = self.config.domain_events_queue();

        // Send to DLQ with publish failed error
        let dlq_error = DomainEventError::PublishFailed {
            event_name: event.event_name.clone(),
            queue_name: self.config.domain_events_queue(),
            reason: error.to_string(),
        };
        if let Err(dlq_error) = self.send_to_dlq(receipt_handle_display, &dlq_error).await {
            error!(
                receipt_handle = %receipt_handle_display,
                error = %dlq_error,
                "Failed to send failed message to DLQ"
            );
        }

        // Acknowledge (delete) from main queue
        if let Err(del_error) = self.ack_message(&queue_name, receipt_handle).await {
            error!(
                receipt_handle = %receipt_handle_display,
                error = %del_error,
                "Failed to acknowledge failed message from main queue"
            );
        }

        // Update failure counters
        self.stats.events_failed.fetch_add(1, Ordering::Relaxed);
        self.stats.events_dlq.fetch_add(1, Ordering::Relaxed);

        // Emit OpenTelemetry metrics
        metrics::counter!(
            "tasker.event_consumer.events_failed.total",
            "namespace" => self.config.namespace.clone(),
            "event_name" => event.event_name.clone()
        )
        .increment(1);

        Ok(())
    }

    /// Send a message to the DLQ
    ///
    /// TAS-133: Provider-agnostic - uses MessageClient.send_message().
    async fn send_to_dlq(
        &self,
        original_handle: &str,
        error: &DomainEventError,
    ) -> TaskerResult<()> {
        let dlq_name = self.config.dlq_name();

        // Create DLQ message with error context
        let dlq_message = serde_json::json!({
            "original_receipt_handle": original_handle,
            "error": error.to_string(),
            "timestamp": chrono::Utc::now(),
            "namespace": self.config.namespace,
        });

        // TAS-133: Use provider-agnostic MessageClient
        self.context
            .message_client()
            .send_message(&dlq_name, &dlq_message)
            .await
            .map_err(|e| TaskerError::MessagingError(format!("Failed to send to DLQ: {}", e)))?;

        debug!(
            original_handle = %original_handle,
            dlq = %dlq_name,
            "Message sent to DLQ"
        );

        Ok(())
    }

    /// Acknowledge (delete) a message from a queue
    ///
    /// TAS-133: Provider-agnostic - uses MessageClient.ack_message().
    async fn ack_message(
        &self,
        queue_name: &str,
        receipt_handle: &ReceiptHandle,
    ) -> TaskerResult<()> {
        // TAS-133: Use provider-agnostic MessageClient
        self.context
            .message_client()
            .ack_message(queue_name, receipt_handle)
            .await
            .map_err(|e| TaskerError::MessagingError(format!("Failed to acknowledge message: {}", e)))?;

        Ok(())
    }
}

impl std::fmt::Debug for EventConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventConsumer")
            .field("config", &self.config)
            .field("running", &self.running.load(Ordering::Relaxed))
            .field("stats", &self.stats)
            .finish()
    }
}

impl Clone for EventConsumer {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            event_registry: self.event_registry.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            stats: self.stats.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        // Valid config
        let config = EventConsumerConfig::default();
        assert!(config.validate().is_ok());

        // Empty namespace
        let config = EventConsumerConfig {
            namespace: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid batch size
        let config = EventConsumerConfig {
            batch_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Invalid max concurrent handlers
        let config = EventConsumerConfig {
            max_concurrent_handlers: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_queue_names() {
        let config = EventConsumerConfig {
            namespace: "payments".to_string(),
            ..Default::default()
        };

        assert_eq!(config.domain_events_queue(), "payments_domain_events");
        assert_eq!(config.dlq_name(), "payments_domain_events_dlq");
    }

    // TAS-133: test_consumer_lifecycle requires a SystemContext with a messaging provider.
    // This test is skipped as it requires database/messaging infrastructure.
    // Integration tests cover the full EventConsumer lifecycle.

    #[test]
    fn test_stats_counters() {
        let stats = EventConsumerStats::new();

        assert_eq!(stats.get_polling_cycles(), 0);
        assert_eq!(stats.get_events_processed(), 0);
        assert_eq!(stats.get_events_failed(), 0);
        assert_eq!(stats.get_events_dlq(), 0);

        stats.polling_cycles.fetch_add(5, Ordering::Relaxed);
        stats.events_processed.fetch_add(3, Ordering::Relaxed);
        stats.events_failed.fetch_add(2, Ordering::Relaxed);
        stats.events_dlq.fetch_add(1, Ordering::Relaxed);

        assert_eq!(stats.get_polling_cycles(), 5);
        assert_eq!(stats.get_events_processed(), 3);
        assert_eq!(stats.get_events_failed(), 2);
        assert_eq!(stats.get_events_dlq(), 1);
    }
}
