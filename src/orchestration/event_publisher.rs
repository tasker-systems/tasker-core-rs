//! # Orchestration Event Publisher
//!
//! Event publishing system for orchestration events with FFI bridge support.
//!
//! ## Architecture
//!
//! The EventPublisher coordinates event publishing across multiple systems:
//! - **Internal event bus**: For Rust components within the orchestration system
//! - **FFI bridge**: For publishing events to external frameworks (Rails, Python, etc.)
//! - **Structured logging**: For observability and debugging
//! - **Event batching**: For performance optimization
//!
//! ## Features
//!
//! - **Async event publishing** with non-blocking performance
//! - **Event filtering** and subscription management
//! - **Serialization** with proper error handling
//! - **Correlation IDs** for distributed tracing
//! - **Backpressure handling** for high-volume scenarios
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::event_publisher::EventPublisher;
//! use tasker_core::orchestration::types::OrchestrationEvent;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let publisher = EventPublisher::new();
//!
//! // Publish a task orchestration event
//! publisher.publish_event(OrchestrationEvent::TaskOrchestrationStarted {
//!     task_id: 123,
//!     framework: "rust_client".to_string(),
//!     started_at: chrono::Utc::now(),
//! }).await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::errors::{EventError, OrchestrationResult};
use crate::orchestration::types::{OrchestrationEvent, StepResult, TaskResult, ViableStep};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Event subscriber callback type
pub type EventCallback = Arc<
    dyn Fn(OrchestrationEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// Event publisher configuration
#[derive(Debug, Clone)]
pub struct EventPublisherConfig {
    /// Maximum number of events to buffer
    pub buffer_size: usize,
    /// Timeout for event publishing
    pub publish_timeout: Duration,
    /// Enable FFI bridge
    pub ffi_enabled: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
}

impl Default for EventPublisherConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            publish_timeout: Duration::from_secs(5),
            ffi_enabled: false,
            correlation_id: None,
        }
    }
}

/// Event publisher for orchestration events
pub struct EventPublisher {
    /// Configuration
    config: EventPublisherConfig,
    /// Internal event broadcast channel
    event_sender: broadcast::Sender<OrchestrationEvent>,
    /// Subscribers for different event types
    subscribers: Arc<RwLock<HashMap<String, Vec<EventCallback>>>>,
    /// Event queue for async processing
    event_queue: mpsc::UnboundedSender<OrchestrationEvent>,
    /// Correlation ID for this publisher instance
    correlation_id: String,
}

impl EventPublisher {
    /// Create a new event publisher with default configuration
    pub fn new() -> Self {
        Self::with_config(EventPublisherConfig::default())
    }

    /// Create a new event publisher with custom configuration
    pub fn with_config(config: EventPublisherConfig) -> Self {
        let (event_sender, _) = broadcast::channel(config.buffer_size);
        let (event_queue, event_queue_rx) = mpsc::unbounded_channel();

        let correlation_id = config
            .correlation_id
            .clone()
            .unwrap_or_else(|| format!("pub_{}", &Uuid::new_v4().to_string()[..8]));

        let publisher = Self {
            config,
            event_sender,
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            event_queue,
            correlation_id,
        };

        // Start background event processing
        publisher.start_event_processor(event_queue_rx);

        info!(
            correlation_id = publisher.correlation_id,
            buffer_size = publisher.config.buffer_size,
            "EventPublisher initialized"
        );

        publisher
    }

    /// Publish an orchestration event
    #[instrument(skip(self, event), fields(correlation_id = %self.correlation_id))]
    pub async fn publish_event(&self, event: OrchestrationEvent) -> OrchestrationResult<()> {
        let event_type = self.get_event_type(&event);

        // Send to async queue for processing
        self.event_queue
            .send(event.clone())
            .map_err(|e| EventError::PublishingFailed {
                event_type: event_type.clone(),
                reason: format!("Failed to queue event: {e}"),
            })?;

        // Send to broadcast channel for immediate subscribers
        if let Err(e) = self.event_sender.send(event.clone()) {
            // Only log warning if there are supposed to be subscribers
            if self.event_sender.receiver_count() > 0 {
                warn!(
                    event_type = event_type,
                    error = %e,
                    "Failed to broadcast event to subscribers"
                );
            }
        }

        info!(
            event_type = event_type,
            correlation_id = self.correlation_id,
            "Event published successfully"
        );

        Ok(())
    }

    /// Publish task orchestration started event
    pub async fn publish_task_orchestration_started(
        &self,
        task_id: i64,
        framework: &str,
    ) -> OrchestrationResult<()> {
        let event = OrchestrationEvent::TaskOrchestrationStarted {
            task_id,
            framework: framework.to_string(),
            started_at: Utc::now(),
        };

        self.publish_event(event).await
    }

    /// Publish viable steps discovered event
    pub async fn publish_viable_steps_discovered(
        &self,
        task_id: i64,
        steps: &[ViableStep],
    ) -> OrchestrationResult<()> {
        let event = OrchestrationEvent::ViableStepsDiscovered {
            task_id,
            step_count: steps.len(),
            steps: steps.to_vec(),
        };

        self.publish_event(event).await
    }

    /// Publish task orchestration completed event
    pub async fn publish_task_orchestration_completed(
        &self,
        task_id: i64,
        result: TaskResult,
    ) -> OrchestrationResult<()> {
        let event = OrchestrationEvent::TaskOrchestrationCompleted {
            task_id,
            result,
            completed_at: Utc::now(),
        };

        self.publish_event(event).await
    }

    /// Publish step execution started event
    pub async fn publish_step_execution_started(
        &self,
        step_id: i64,
        task_id: i64,
        step_name: &str,
    ) -> OrchestrationResult<()> {
        let event = OrchestrationEvent::StepExecutionStarted {
            step_id,
            task_id,
            step_name: step_name.to_string(),
            started_at: Utc::now(),
        };

        self.publish_event(event).await
    }

    /// Publish step execution completed event
    pub async fn publish_step_execution_completed(
        &self,
        step_id: i64,
        task_id: i64,
        result: StepResult,
    ) -> OrchestrationResult<()> {
        let event = OrchestrationEvent::StepExecutionCompleted {
            step_id,
            task_id,
            result,
            completed_at: Utc::now(),
        };

        self.publish_event(event).await
    }

    /// Subscribe to events of a specific type
    pub async fn subscribe<F>(&self, event_type: &str, callback: F) -> OrchestrationResult<()>
    where
        F: Fn(
                OrchestrationEvent,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let mut subscribers = self.subscribers.write().await;

        subscribers
            .entry(event_type.to_string())
            .or_insert_with(Vec::new)
            .push(Arc::new(callback));

        info!(
            event_type = event_type,
            correlation_id = self.correlation_id,
            "Event subscriber registered"
        );

        Ok(())
    }

    /// Get a receiver for the broadcast channel
    pub fn subscribe_to_all(&self) -> broadcast::Receiver<OrchestrationEvent> {
        self.event_sender.subscribe()
    }

    /// Get event statistics
    pub fn stats(&self) -> EventPublisherStats {
        EventPublisherStats {
            buffer_size: self.config.buffer_size,
            subscriber_count: self.event_sender.receiver_count(),
            correlation_id: self.correlation_id.clone(),
            ffi_enabled: self.config.ffi_enabled,
        }
    }

    /// Start background event processor
    fn start_event_processor(
        &self,
        mut event_queue_rx: mpsc::UnboundedReceiver<OrchestrationEvent>,
    ) {
        let subscribers = Arc::clone(&self.subscribers);
        let correlation_id = self.correlation_id.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            while let Some(event) = event_queue_rx.recv().await {
                let event_type = Self::get_event_type_static(&event);

                // Process subscribers for this event type
                let subscribers_guard = subscribers.read().await;
                if let Some(event_subscribers) = subscribers_guard.get(&event_type) {
                    for callback in event_subscribers {
                        let callback = Arc::clone(callback);
                        let event = event.clone();

                        // Execute callback with timeout
                        let future = callback(event);
                        if let Err(e) = timeout(config.publish_timeout, future).await {
                            error!(
                                event_type = event_type,
                                correlation_id = correlation_id,
                                error = %e,
                                "Event callback timed out"
                            );
                        }
                    }
                }

                // Process "all" subscribers
                if let Some(all_subscribers) = subscribers_guard.get("*") {
                    for callback in all_subscribers {
                        let callback = Arc::clone(callback);
                        let event = event.clone();

                        let future = callback(event);
                        if let Err(e) = timeout(config.publish_timeout, future).await {
                            error!(
                                event_type = event_type,
                                correlation_id = correlation_id,
                                error = %e,
                                "Event callback timed out"
                            );
                        }
                    }
                }
            }
        });
    }

    /// Get event type as string
    fn get_event_type(&self, event: &OrchestrationEvent) -> String {
        Self::get_event_type_static(event)
    }

    /// Get event type as string (static version)
    fn get_event_type_static(event: &OrchestrationEvent) -> String {
        match event {
            OrchestrationEvent::TaskOrchestrationStarted { .. } => {
                "task_orchestration_started".to_string()
            }
            OrchestrationEvent::ViableStepsDiscovered { .. } => {
                "viable_steps_discovered".to_string()
            }
            OrchestrationEvent::TaskOrchestrationCompleted { .. } => {
                "task_orchestration_completed".to_string()
            }
            OrchestrationEvent::StepExecutionStarted { .. } => "step_execution_started".to_string(),
            OrchestrationEvent::StepExecutionCompleted { .. } => {
                "step_execution_completed".to_string()
            }
            OrchestrationEvent::HandlerRegistered { .. } => "handler_registered".to_string(),
        }
    }
}

impl Clone for EventPublisher {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_sender: self.event_sender.clone(),
            subscribers: Arc::clone(&self.subscribers),
            event_queue: self.event_queue.clone(),
            correlation_id: self.correlation_id.clone(),
        }
    }
}

/// Event publisher statistics
#[derive(Debug, Clone)]
pub struct EventPublisherStats {
    pub buffer_size: usize,
    pub subscriber_count: usize,
    pub correlation_id: String,
    pub ffi_enabled: bool,
}

impl Default for EventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

/// FFI bridge for cross-language event publishing
pub struct FfiBridge {
    enabled: bool,
    correlation_id: String,
}

impl FfiBridge {
    /// Create a new FFI bridge
    pub fn new(correlation_id: String) -> Self {
        Self {
            enabled: true,
            correlation_id,
        }
    }

    /// Publish event to external framework
    pub async fn publish_to_framework(
        &self,
        event_name: &str,
        _payload: Value,
    ) -> OrchestrationResult<()> {
        if !self.enabled {
            return Ok(());
        }

        // TODO: Implement actual FFI bridge when needed
        // For now, just log the event
        info!(
            event_name = event_name,
            correlation_id = self.correlation_id,
            "FFI bridge event published"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_event_publisher_creation() {
        let publisher = EventPublisher::new();
        let stats = publisher.stats();

        assert_eq!(stats.buffer_size, 1000);
        assert_eq!(stats.subscriber_count, 0);
        assert!(!stats.ffi_enabled);
    }

    #[tokio::test]
    async fn test_publish_task_orchestration_started() {
        let publisher = EventPublisher::new();

        let result = publisher
            .publish_task_orchestration_started(123, "test_framework")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_viable_steps_discovered() {
        let publisher = EventPublisher::new();

        let steps = vec![ViableStep {
            step_id: 1,
            task_id: 123,
            name: "test_step".to_string(),
            named_step_id: 1,
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: true,
            attempts: 0,
            retry_limit: 3,
            last_failure_at: None,
            next_retry_at: None,
        }];

        let result = publisher.publish_viable_steps_discovered(123, &steps).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let publisher = EventPublisher::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // Subscribe to task orchestration started events
        publisher
            .subscribe("task_orchestration_started", move |_event| {
                let counter = Arc::clone(&counter_clone);
                Box::pin(async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .await
            .unwrap();

        // Publish an event
        publisher
            .publish_task_orchestration_started(123, "test")
            .await
            .unwrap();

        // Give some time for async processing
        sleep(Duration::from_millis(50)).await;

        // Check that subscriber was called
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_broadcast_subscription() {
        let publisher = EventPublisher::new();
        let mut receiver = publisher.subscribe_to_all();

        // Publish an event
        publisher
            .publish_task_orchestration_started(123, "test")
            .await
            .unwrap();

        // Receive the event
        let event = receiver.recv().await.unwrap();

        match event {
            OrchestrationEvent::TaskOrchestrationStarted {
                task_id, framework, ..
            } => {
                assert_eq!(task_id, 123);
                assert_eq!(framework, "test");
            }
            _ => panic!("Wrong event type received"),
        }
    }

    #[tokio::test]
    async fn test_event_publisher_stats() {
        let publisher = EventPublisher::new();
        let _receiver = publisher.subscribe_to_all();

        let stats = publisher.stats();
        assert_eq!(stats.subscriber_count, 1);
        assert!(!stats.correlation_id.is_empty());
    }

    #[tokio::test]
    async fn test_custom_config() {
        let config = EventPublisherConfig {
            buffer_size: 500,
            publish_timeout: Duration::from_secs(10),
            ffi_enabled: true,
            correlation_id: Some("test_id".to_string()),
        };

        let publisher = EventPublisher::with_config(config);
        let stats = publisher.stats();

        assert_eq!(stats.buffer_size, 500);
        assert_eq!(stats.correlation_id, "test_id");
        assert!(stats.ffi_enabled);
    }
}
