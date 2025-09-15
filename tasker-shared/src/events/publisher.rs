//! # Unified Event Publisher
//!
//! High-performance event publishing system that supports both simple generic events
//! and complex orchestration events with FFI bridge capabilities.
//!
//! ## Features
//!
//! - **Dual API**: Simple API for basic events, advanced API for orchestration
//! - **FFI Bridge**: Cross-language event publishing for Rails integration
//! - **High Performance**: Async processing with configurable buffering
//! - **Rails Compatibility**: Event constants and metadata matching Rails engine
//! - **Observability**: Correlation IDs, tracing, and comprehensive statistics
//!
//! ## Usage
//!
//! ### Simple Events
//! ```rust
//! use tasker_shared::events::EventPublisher;
//! use serde_json::json;
//!
//! # tokio_test::block_on(async {
//! let publisher = EventPublisher::new();
//!
//! // Publish a simple event
//! publisher.publish("user.created", json!({"user_id": 123})).await.unwrap();
//! # });
//! ```
//!
//! ### Orchestration Events
//! ```rust
//! use tasker_shared::events::{EventPublisher, Event, OrchestrationEvent};
//! use chrono::Utc;
//!
//! # tokio_test::block_on(async {
//! let publisher = EventPublisher::new();
//!
//! // Publish a structured orchestration event
//! let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationStarted {
//!     task_uuid: uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap(),
//!     framework: "rust_client".to_string(),
//!     started_at: Utc::now(),
//! });
//!
//! publisher.publish_event(event).await.unwrap();
//! # });
//! ```

use crate::events::types::{Event, OrchestrationEvent, StepResult, TaskResult, ViableStep};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// Event subscriber callback type
pub type EventCallback = Arc<
    dyn Fn(Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync,
>;

/// Event publisher configuration
#[derive(Debug, Clone)]
pub struct EventPublisherConfig {
    /// Maximum number of events to buffer
    pub buffer_size: usize,
    /// Timeout for event publishing
    pub publish_timeout: Duration,
    /// Enable FFI bridge for cross-language events
    pub ffi_enabled: bool,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Enable background event processing
    pub async_processing: bool,
}

impl Default for EventPublisherConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            publish_timeout: Duration::from_secs(5),
            ffi_enabled: false,
            correlation_id: None,
            async_processing: true,
        }
    }
}

/// Unified event publisher for all event types
pub struct EventPublisher {
    /// Configuration
    config: EventPublisherConfig,
    /// Internal event broadcast channel
    event_sender: broadcast::Sender<Event>,
    /// Subscribers for different event types
    subscribers: Arc<RwLock<HashMap<String, Vec<EventCallback>>>>,
    /// Event queue for async processing
    event_queue: Option<mpsc::UnboundedSender<Event>>,
    /// Correlation ID for this publisher instance
    correlation_id: String,
    /// FFI bridge for cross-language publishing
    ffi_bridge: Option<FfiBridge>,
}

impl EventPublisher {
    /// Create a new event publisher with default configuration
    pub fn new() -> Self {
        Self::with_config(EventPublisherConfig::default())
    }

    /// Create a new event publisher with the specified channel capacity (simple API)
    pub fn with_capacity(capacity: usize) -> Self {
        let config = EventPublisherConfig {
            buffer_size: capacity,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a new event publisher with custom configuration
    pub fn with_config(config: EventPublisherConfig) -> Self {
        let (event_sender, _) = broadcast::channel(config.buffer_size);

        let correlation_id = config
            .correlation_id
            .clone()
            .unwrap_or_else(|| format!("pub_{}", &Uuid::new_v4().to_string()[..8]));

        let subscribers = Arc::new(RwLock::new(HashMap::new()));

        // Set up async processing if enabled
        let event_queue = if config.async_processing {
            let (queue_sender, queue_receiver) = mpsc::unbounded_channel();

            // Start background processor
            Self::start_background_processor(
                queue_receiver,
                Arc::clone(&subscribers),
                correlation_id.clone(),
                config.clone(),
            );

            Some(queue_sender)
        } else {
            None
        };

        // Set up FFI bridge if enabled
        let ffi_bridge = if config.ffi_enabled {
            Some(FfiBridge::new(correlation_id.clone()))
        } else {
            None
        };

        let publisher = Self {
            config: config.clone(),
            event_sender,
            subscribers,
            event_queue,
            correlation_id: correlation_id.clone(),
            ffi_bridge,
        };

        info!(
            correlation_id = correlation_id,
            buffer_size = config.buffer_size,
            ffi_enabled = config.ffi_enabled,
            async_processing = config.async_processing,
            "EventPublisher initialized"
        );

        publisher
    }

    /// Publish a simple event with name and context (simple API)
    pub async fn publish(
        &self,
        event_name: impl Into<String>,
        context: Value,
    ) -> Result<(), PublishError> {
        let event = Event::generic(event_name, context);
        self.publish_event(event).await
    }

    /// Publish a unified event (advanced API)
    #[instrument(skip(self, event), fields(correlation_id = %self.correlation_id))]
    pub async fn publish_event(&self, event: Event) -> Result<(), PublishError> {
        let event_name = event.name();

        // Send to async queue for processing if enabled
        if let Some(queue) = &self.event_queue {
            queue
                .send(event.clone())
                .map_err(|e| PublishError::QueueError {
                    event_name: event_name.clone(),
                    reason: format!("Failed to queue event: {e}"),
                })?;
        }

        // Send to broadcast channel for immediate subscribers
        if let Err(e) = self.event_sender.send(event.clone()) {
            // Only log warning if there are supposed to be subscribers
            if self.event_sender.receiver_count() > 0 {
                warn!(
                    event_name = event_name,
                    error = %e,
                    "Failed to broadcast event to subscribers"
                );
            }
        }

        // Send to FFI bridge if enabled
        if let Some(bridge) = &self.ffi_bridge {
            if let Ok(payload) = event.to_json() {
                if let Err(e) = bridge.publish_to_framework(&event_name, payload).await {
                    warn!(
                        event_name = event_name,
                        error = %e,
                        "Failed to publish event via FFI bridge"
                    );
                }
            }
        }

        info!(
            event_name = event_name,
            correlation_id = self.correlation_id,
            "Event published successfully"
        );

        Ok(())
    }

    /// Publish task orchestration started event
    pub async fn publish_task_orchestration_started(
        &self,
        task_uuid: Uuid,
        framework: &str,
    ) -> Result<(), PublishError> {
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationStarted {
            task_uuid,
            framework: framework.to_string(),
            started_at: Utc::now(),
        });

        self.publish_event(event).await
    }

    /// Publish viable steps discovered event
    pub async fn publish_viable_steps_discovered(
        &self,
        task_uuid: Uuid,
        steps: &[ViableStep],
    ) -> Result<(), PublishError> {
        let event = Event::orchestration(OrchestrationEvent::ViableStepsDiscovered {
            task_uuid,
            step_count: steps.len(),
            steps: steps.to_vec(),
        });

        self.publish_event(event).await
    }

    /// Publish task orchestration completed event
    pub async fn publish_task_orchestration_completed(
        &self,
        task_uuid: Uuid,
        result: TaskResult,
    ) -> Result<(), PublishError> {
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result,
            completed_at: Utc::now(),
        });

        self.publish_event(event).await
    }

    /// Publish step execution started event
    pub async fn publish_step_execution_started(
        &self,
        step_uuid: Uuid,
        task_uuid: Uuid,
        step_name: &str,
    ) -> Result<(), PublishError> {
        let event = Event::orchestration(OrchestrationEvent::StepExecutionStarted {
            step_uuid,
            task_uuid,
            step_name: step_name.to_string(),
            started_at: Utc::now(),
        });

        self.publish_event(event).await
    }

    /// Publish step execution completed event
    pub async fn publish_step_execution_completed(
        &self,
        step_uuid: Uuid,
        task_uuid: Uuid,
        result: StepResult,
    ) -> Result<(), PublishError> {
        let event = Event::orchestration(OrchestrationEvent::StepExecutionCompleted {
            step_uuid,
            task_uuid,
            result,
            completed_at: Utc::now(),
        });

        self.publish_event(event).await
    }

    /// Subscribe to events of a specific type
    pub async fn subscribe<F>(&self, event_type: &str, callback: F) -> Result<(), PublishError>
    where
        F: Fn(Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
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

    /// Get a receiver for the broadcast channel (simple API)
    pub fn subscribe_to_all(&self) -> broadcast::Receiver<Event> {
        self.event_sender.subscribe()
    }

    /// Get the number of active subscribers (simple API compatibility)
    pub fn subscriber_count(&self) -> usize {
        self.event_sender.receiver_count()
    }

    /// Get event publisher statistics
    pub fn stats(&self) -> EventPublisherStats {
        EventPublisherStats {
            buffer_size: self.config.buffer_size,
            subscriber_count: self.event_sender.receiver_count(),
            correlation_id: self.correlation_id.clone(),
            ffi_enabled: self.config.ffi_enabled,
            async_processing: self.config.async_processing,
        }
    }

    /// Start background event processor
    fn start_background_processor(
        mut event_queue_rx: mpsc::UnboundedReceiver<Event>,
        subscribers: Arc<RwLock<HashMap<String, Vec<EventCallback>>>>,
        correlation_id: String,
        config: EventPublisherConfig,
    ) {
        tokio::spawn(async move {
            while let Some(event) = event_queue_rx.recv().await {
                let event_name = event.name();

                // Process subscribers for this event type
                let subscribers_guard = subscribers.read().await;

                // Process specific event type subscribers
                if let Some(event_subscribers) = subscribers_guard.get(&event_name) {
                    for callback in event_subscribers {
                        let callback = Arc::clone(callback);
                        let event = event.clone();

                        // Execute callback with timeout
                        let future = callback(event);
                        if let Err(e) = timeout(config.publish_timeout, future).await {
                            error!(
                                event_type = event_name,
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
                                event_type = event_name,
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
}

impl Clone for EventPublisher {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_sender: self.event_sender.clone(),
            subscribers: Arc::clone(&self.subscribers),
            event_queue: self.event_queue.clone(),
            correlation_id: self.correlation_id.clone(),
            ffi_bridge: self.ffi_bridge.clone(),
        }
    }
}

impl Default for EventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventPublisher")
            .field("config", &self.config)
            .field("correlation_id", &self.correlation_id)
            .field("subscriber_count", &self.event_sender.receiver_count())
            .field("ffi_bridge_enabled", &self.ffi_bridge.is_some())
            .finish()
    }
}

/// Event publisher statistics
#[derive(Debug, Clone)]
pub struct EventPublisherStats {
    pub buffer_size: usize,
    pub subscriber_count: usize,
    pub correlation_id: String,
    pub ffi_enabled: bool,
    pub async_processing: bool,
}

/// Error types for event publishing
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("Event channel is closed")]
    ChannelClosed,
    #[error("Event queue error for {event_name}: {reason}")]
    QueueError { event_name: String, reason: String },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("FFI bridge error: {0}")]
    FfiBridge(String),
}

/// FFI bridge for cross-language event publishing
#[derive(Debug, Clone)]
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
        payload: Value,
    ) -> Result<(), PublishError> {
        if !self.enabled {
            return Ok(());
        }

        // Forward event to external callbacks if registered
        if let Err(e) = call_external_event_callbacks(event_name, &payload).await {
            warn!(
                event_name = event_name,
                error = %e,
                "Failed to forward event to external callbacks"
            );
        }

        // Log the event for observability
        info!(
            event_name = event_name,
            correlation_id = self.correlation_id,
            payload = ?payload,
            "FFI bridge event published"
        );

        Ok(())
    }
}

/// External Event Callback System
///
/// This allows external systems (like Ruby bindings) to register callbacks
/// for receiving events published by the Rust orchestration system.
///
/// Type definition for external event callbacks
pub type ExternalEventCallback =
    Box<dyn Fn(&str, &serde_json::Value) -> Result<(), String> + Send + Sync>;

/// Global registry of external event callbacks
static EXTERNAL_CALLBACKS: OnceLock<tokio::sync::Mutex<Vec<ExternalEventCallback>>> =
    OnceLock::new();

/// Register an external event callback
///
/// This allows external systems to receive events published by the Rust orchestration.
/// Callbacks are called for every event published through the EventPublisher.
pub async fn register_external_event_callback<F>(callback: F)
where
    F: Fn(&str, &serde_json::Value) -> Result<(), String> + Send + Sync + 'static,
{
    let callbacks = EXTERNAL_CALLBACKS.get_or_init(|| tokio::sync::Mutex::new(Vec::new()));
    let mut callbacks = callbacks.lock().await;
    callbacks.push(Box::new(callback));
}

/// Call all registered external event callbacks
///
/// This is used internally by the EventPublisher to forward events to external systems.
async fn call_external_event_callbacks(
    event_name: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let callbacks = EXTERNAL_CALLBACKS.get();
    if callbacks.is_none() {
        return Ok(());
    }

    let callbacks = callbacks.unwrap();
    let callbacks = callbacks.lock().await;

    for callback in callbacks.iter() {
        if let Err(e) = callback(event_name, payload) {
            warn!("External event callback failed: {}", e);
            // Continue with other callbacks even if one fails
        }
    }

    Ok(())
}

/// Clear all external event callbacks (useful for testing)
pub async fn clear_external_event_callbacks() {
    let callbacks = EXTERNAL_CALLBACKS.get_or_init(|| tokio::sync::Mutex::new(Vec::new()));
    let mut callbacks = callbacks.lock().await;
    callbacks.clear();
}

#[cfg(test)]
mod tests {

    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_simple_event_publishing() {
        let publisher = EventPublisher::new();

        let result = publisher
            .publish("user.created", json!({"user_id": 123}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_orchestration_event_publishing() {
        let publisher = EventPublisher::new();
        let task_uuid = Uuid::now_v7();

        let result = publisher
            .publish_task_orchestration_started(task_uuid, "test_framework")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let publisher = EventPublisher::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        // Subscribe to specific event type
        publisher
            .subscribe("user.created", move |_event| {
                let counter = Arc::clone(&counter_clone);
                Box::pin(async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                })
            })
            .await
            .unwrap();

        // Publish an event
        publisher
            .publish("user.created", json!({"user_id": 123}))
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
            .publish("test.event", json!({"data": "value"}))
            .await
            .unwrap();

        // Receive the event
        let event = receiver.recv().await.unwrap();
        assert_eq!(event.name(), "test.event");
    }

    #[tokio::test]
    async fn test_publisher_stats() {
        let publisher = EventPublisher::new();
        let _receiver = publisher.subscribe_to_all();

        let stats = publisher.stats();
        assert_eq!(stats.buffer_size, 1000);
        assert_eq!(stats.subscriber_count, 1);
        assert!(!stats.correlation_id.is_empty());
        assert!(!stats.ffi_enabled);
        assert!(stats.async_processing);
    }

    #[tokio::test]
    async fn test_custom_config() {
        let config = EventPublisherConfig {
            buffer_size: 500,
            publish_timeout: Duration::from_secs(10),
            ffi_enabled: true,
            correlation_id: Some("test_id".to_string()),
            async_processing: false,
        };

        let publisher = EventPublisher::with_config(config);
        let stats = publisher.stats();

        assert_eq!(stats.buffer_size, 500);
        assert_eq!(stats.correlation_id, "test_id");
        assert!(stats.ffi_enabled);
        assert!(!stats.async_processing);
    }

    #[tokio::test]
    async fn test_backward_compatibility() {
        // Test that the simple API still works like the old implementation
        let publisher = EventPublisher::with_capacity(500);

        assert_eq!(publisher.stats().buffer_size, 500);
        assert_eq!(publisher.subscriber_count(), 0);

        // Test default constructor
        let default_publisher = EventPublisher::default();
        assert_eq!(default_publisher.stats().buffer_size, 1000);
    }

    #[tokio::test]
    async fn test_viable_steps_event() {
        let publisher = EventPublisher::new();
        let task_uuid = Uuid::now_v7();

        let steps = vec![ViableStep {
            step_uuid: Uuid::now_v7(),
            task_uuid,
            name: "test_step".to_string(),
            named_step_uuid: Uuid::now_v7(),
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: true,
            attempts: 0,
            retry_limit: 3,
            last_failure_at: None,
            next_retry_at: None,
        }];

        let result = publisher
            .publish_viable_steps_discovered(task_uuid, &steps)
            .await;
        assert!(result.is_ok());
    }
}
