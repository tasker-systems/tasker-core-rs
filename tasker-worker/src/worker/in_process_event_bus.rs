//! # TAS-65: In-Process Event Bus
//!
//! Fast, in-memory event bus for internal domain event subscribers.
//! Provides fire-and-forget delivery to Rust handlers and a broadcast channel
//! for cross-language FFI subscribers (Ruby, Python, etc.).
//!
//! ## Architecture
//!
//! ```text
//! DomainEvent (Fast mode)
//!     ↓
//! InProcessEventBus
//!     |
//!     +--→ EventRegistry (Rust subscribers)
//!     |         ↓
//!     |    Pattern matching dispatch
//!     |    (exact, wildcard, global)
//!     |
//!     +--→ Broadcast Channel (FFI bridge)
//!               ↓
//!          Ruby/Python subscribers
//!          (via poll_in_process_events FFI)
//! ```
//!
//! ## Fire-and-Forget Semantics
//!
//! - Subscriber failures are logged but never propagate
//! - Events are dispatched to all matching handlers concurrently
//! - Failed handlers don't prevent other handlers from executing
//! - No retries - events lost on failure
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_worker::worker::in_process_event_bus::{InProcessEventBus, InProcessEventBusConfig};
//! use tasker_shared::events::domain_events::DomainEvent;
//! use tasker_shared::events::registry::EventHandler;
//! use std::sync::Arc;
//!
//! async fn example(event: DomainEvent) -> Result<(), Box<dyn std::error::Error>> {
//!     // Create bus
//!     let config = InProcessEventBusConfig::default();
//!     let mut bus = InProcessEventBus::new(config);
//!
//!     // Register Rust subscriber (handler wrapped in Arc)
//!     let handler: EventHandler = Arc::new(|event: DomainEvent| {
//!         Box::pin(async move {
//!             println!("Payment event: {}", event.event_name);
//!             Ok(())
//!         })
//!     });
//!     bus.subscribe("payment.*", handler)?;
//!
//!     // Publish event (fire-and-forget)
//!     bus.publish(event).await;
//!     Ok(())
//! }
//! ```

use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{debug, info, instrument, warn};

use tasker_shared::events::domain_events::DomainEvent;
use tasker_shared::events::registry::{EventHandler, EventRegistry, RegistryError};
use tasker_shared::metrics::worker::InProcessEventBusStats;
use tasker_shared::monitoring::ChannelMonitor;

/// Configuration for the in-process event bus
#[derive(Debug, Clone)]
pub struct InProcessEventBusConfig {
    /// Buffer size for the FFI broadcast channel
    ///
    /// This determines how many events can be buffered for FFI subscribers
    /// before older events are dropped (lagging subscribers).
    pub ffi_channel_buffer_size: usize,

    /// Whether to log individual subscriber errors at warn level
    ///
    /// When true, logs each subscriber failure. When false, only logs
    /// aggregated error counts for performance.
    pub log_subscriber_errors: bool,

    /// Maximum time to wait for dispatch to complete (for metrics)
    ///
    /// Dispatch is fire-and-forget, but this helps identify slow subscribers.
    pub dispatch_timeout_ms: u64,
}

impl Default for InProcessEventBusConfig {
    fn default() -> Self {
        Self {
            ffi_channel_buffer_size: 1000,
            log_subscriber_errors: true,
            dispatch_timeout_ms: 5000,
        }
    }
}

// InProcessEventBusStats is imported from tasker_shared::metrics::worker (canonical location)

/// In-process event bus for fast domain event delivery
///
/// Provides two dispatch paths:
/// 1. **Rust Path**: Pattern-matching dispatch to async Rust handlers
/// 2. **FFI Path**: Broadcast channel for Ruby/Python subscribers
///
/// Both paths are fire-and-forget - subscriber failures don't affect the caller.
pub struct InProcessEventBus {
    /// Rust subscriber registry with pattern matching
    registry: EventRegistry,
    /// Broadcast channel for FFI subscribers
    ffi_sender: broadcast::Sender<DomainEvent>,
    /// Channel monitor for observability (TAS-51)
    channel_monitor: ChannelMonitor,
    /// Configuration
    config: InProcessEventBusConfig,
    /// Statistics (wrapped for interior mutability)
    stats: Arc<std::sync::Mutex<InProcessEventBusStats>>,
}

// Manual Debug implementation because EventRegistry contains closures
impl std::fmt::Debug for InProcessEventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessEventBus")
            .field("registry_patterns", &self.registry.pattern_count())
            .field("registry_handlers", &self.registry.handler_count())
            .field("ffi_subscriber_count", &self.ffi_sender.receiver_count())
            .field("config", &self.config)
            .finish()
    }
}

impl InProcessEventBus {
    /// Create a new in-process event bus
    pub fn new(config: InProcessEventBusConfig) -> Self {
        let (ffi_sender, _) = broadcast::channel(config.ffi_channel_buffer_size);
        let channel_monitor = ChannelMonitor::new(
            "in_process_event_bus_ffi_channel",
            config.ffi_channel_buffer_size,
        );

        info!(
            ffi_buffer_size = config.ffi_channel_buffer_size,
            "Creating InProcessEventBus for fast domain event delivery"
        );

        Self {
            registry: EventRegistry::new(),
            ffi_sender,
            channel_monitor,
            config,
            stats: Arc::new(std::sync::Mutex::new(InProcessEventBusStats::default())),
        }
    }

    /// Subscribe a Rust handler to an event pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - Event pattern (exact, wildcard `payment.*`, or global `*`)
    /// * `handler` - Async handler function
    ///
    /// # Errors
    ///
    /// Returns error if the pattern is invalid (empty)
    pub fn subscribe(&mut self, pattern: &str, handler: EventHandler) -> Result<(), RegistryError> {
        debug!(pattern = %pattern, "Registering Rust subscriber for in-process events");
        self.registry.subscribe(pattern, handler)
    }

    /// Subscribe to FFI broadcast channel
    ///
    /// Returns a receiver for Ruby/Python FFI integration.
    /// The receiver will receive cloned `DomainEvent` values.
    pub fn subscribe_ffi(&self) -> broadcast::Receiver<DomainEvent> {
        debug!("Creating FFI subscriber for in-process events");
        self.ffi_sender.subscribe()
    }

    /// Publish a domain event (fire-and-forget)
    ///
    /// Dispatches the event to all matching Rust handlers concurrently,
    /// then broadcasts to FFI channel. Errors are logged but never propagated.
    ///
    /// This method is intentionally `async` even though it doesn't `await`
    /// internally because the EventRegistry dispatch is async.
    #[instrument(skip(self, event), fields(
        event_name = %event.event_name,
        event_id = %event.event_id,
        namespace = %event.metadata.namespace
    ))]
    pub async fn publish(&self, event: DomainEvent) {
        debug!(
            event_name = %event.event_name,
            event_id = %event.event_id,
            task_uuid = %event.metadata.task_uuid,
            "Publishing in-process domain event"
        );

        // Update total events stat
        {
            let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
            stats.total_events_dispatched += 1;
        }

        // Dispatch to Rust handlers
        self.dispatch_to_rust_handlers(&event).await;

        // Broadcast to FFI channel
        self.dispatch_to_ffi_channel(&event);
    }

    /// Dispatch event to Rust handlers via EventRegistry
    async fn dispatch_to_rust_handlers(&self, event: &DomainEvent) {
        let handler_count = self.registry.handler_count();
        if handler_count == 0 {
            debug!(
                event_name = %event.event_name,
                "No Rust handlers registered - skipping Rust dispatch"
            );
            return;
        }

        debug!(
            event_name = %event.event_name,
            handler_count = handler_count,
            "Dispatching to Rust handlers"
        );

        // Dispatch returns errors from failed handlers
        let errors = self.registry.dispatch(event).await;

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
            stats.rust_handler_dispatches += 1;
            stats.rust_handler_errors += errors.len() as u64;
        }

        // Log errors (fire-and-forget - never fail the workflow)
        if !errors.is_empty() {
            if self.config.log_subscriber_errors {
                for error in &errors {
                    warn!(
                        event_name = %event.event_name,
                        event_id = %event.event_id,
                        error = %error,
                        "In-process event handler failed (fire-and-forget)"
                    );
                }
            } else {
                warn!(
                    event_name = %event.event_name,
                    event_id = %event.event_id,
                    error_count = errors.len(),
                    "In-process event handlers failed (fire-and-forget)"
                );
            }
        }
    }

    /// Dispatch event to FFI broadcast channel
    fn dispatch_to_ffi_channel(&self, event: &DomainEvent) {
        let subscriber_count = self.ffi_sender.receiver_count();

        if subscriber_count == 0 {
            debug!(
                event_name = %event.event_name,
                "No FFI subscribers - skipping FFI broadcast"
            );

            // Update stats
            let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
            stats.ffi_channel_drops += 1;
            return;
        }

        debug!(
            event_name = %event.event_name,
            subscriber_count = subscriber_count,
            "Broadcasting to FFI channel"
        );

        match self.ffi_sender.send(event.clone()) {
            Ok(sent_count) => {
                debug!(
                    event_name = %event.event_name,
                    sent_count = sent_count,
                    "Event broadcast to FFI subscribers"
                );

                // Update stats
                let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
                stats.ffi_channel_dispatches += 1;

                // TAS-51: Record send for channel monitoring
                if self.channel_monitor.record_send_success() {
                    // Periodic saturation check (approximation since broadcast doesn't track this)
                    let _saturation =
                        subscriber_count as f64 / self.config.ffi_channel_buffer_size as f64;
                }
            }
            Err(broadcast::error::SendError(_)) => {
                // This only happens if there are no receivers, which we checked above
                // Could occur in race condition if receivers dropped between check and send
                warn!(
                    event_name = %event.event_name,
                    "FFI broadcast failed - all subscribers dropped"
                );

                let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner());
                stats.ffi_channel_drops += 1;
            }
        }
    }

    /// Get bus statistics for monitoring
    pub fn get_statistics(&self) -> InProcessEventBusStats {
        let mut stats = self.stats.lock().unwrap_or_else(|p| p.into_inner()).clone();

        // Add current counts
        stats.rust_subscriber_patterns = self.registry.pattern_count();
        stats.rust_handler_count = self.registry.handler_count();
        stats.ffi_subscriber_count = self.ffi_sender.receiver_count();

        stats
    }

    /// Clear all Rust subscriptions
    ///
    /// Useful for testing or reconfiguration. FFI subscribers are not affected
    /// (they maintain their broadcast receiver).
    pub fn clear_rust_subscriptions(&mut self) {
        info!("Clearing all Rust subscriptions from in-process event bus");
        self.registry.clear();
    }

    /// Get configuration reference
    pub fn config(&self) -> &InProcessEventBusConfig {
        &self.config
    }
}

/// Builder for InProcessEventBus for convenient setup
#[derive(Debug, Default)]
pub struct InProcessEventBusBuilder {
    config: InProcessEventBusConfig,
}

impl InProcessEventBusBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set FFI channel buffer size
    pub fn ffi_channel_buffer_size(mut self, size: usize) -> Self {
        self.config.ffi_channel_buffer_size = size;
        self
    }

    /// Enable/disable individual subscriber error logging
    pub fn log_subscriber_errors(mut self, enabled: bool) -> Self {
        self.config.log_subscriber_errors = enabled;
        self
    }

    /// Set dispatch timeout for metrics
    pub fn dispatch_timeout_ms(mut self, timeout: u64) -> Self {
        self.config.dispatch_timeout_ms = timeout;
        self
    }

    /// Build the event bus
    pub fn build(self) -> InProcessEventBus {
        InProcessEventBus::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use tasker_shared::events::domain_events::{DomainEventPayload, EventMetadata};
    use tasker_shared::events::registry::EventHandlerError;
    use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::types::base::TaskSequenceStep;

    /// Create a test domain event
    fn create_test_event(event_name: &str) -> DomainEvent {
        let task_sequence_step = TaskSequenceStep {
            task: TaskForOrchestration {
                task: Task {
                    task_uuid: Uuid::new_v4(),
                    named_task_uuid: Uuid::new_v4(),
                    complete: false,
                    requested_at: Utc::now().naive_utc(),
                    initiator: Some("test".to_string()),
                    source_system: None,
                    reason: None,
                    bypass_steps: None,
                    tags: None,
                    context: Some(json!({})),
                    identity_hash: "test_hash".to_string(),
                    priority: 5,
                    created_at: Utc::now().naive_utc(),
                    updated_at: Utc::now().naive_utc(),
                    correlation_id: Uuid::new_v4(),
                    parent_correlation_id: None,
                },
                task_name: "test_task".to_string(),
                task_version: "1.0".to_string(),
                namespace_name: "test".to_string(),
            },
            workflow_step: WorkflowStepWithName {
                workflow_step_uuid: Uuid::new_v4(),
                task_uuid: Uuid::new_v4(),
                named_step_uuid: Uuid::new_v4(),
                name: "test_step".to_string(),
                template_step_name: "test_step".to_string(),
                retryable: true,
                max_attempts: Some(3),
                in_process: false,
                processed: false,
                processed_at: None,
                attempts: Some(0),
                last_attempted_at: None,
                backoff_request_seconds: None,
                inputs: None,
                results: None,
                skippable: false,
                created_at: Utc::now().naive_utc(),
                updated_at: Utc::now().naive_utc(),
                checkpoint: None,
            },
            dependency_results: HashMap::new(),
            step_definition: StepDefinition {
                name: "test_step".to_string(),
                description: Some("Test step".to_string()),
                handler: HandlerDefinition {
                    callable: "TestHandler".to_string(),
                    initialization: HashMap::new(),
                },
                step_type: Default::default(),
                system_dependency: None,
                dependencies: vec![],
                retry: RetryConfiguration::default(),
                timeout_seconds: None,
                publishes_events: vec![],
                batch_config: None,
            },
        };

        let execution_result = StepExecutionResult {
            step_uuid: Uuid::new_v4(),
            success: true,
            result: json!({"test": true}),
            metadata: StepExecutionMetadata {
                execution_time_ms: 100,
                handler_version: None,
                retryable: true,
                completed_at: Utc::now(),
                worker_id: None,
                worker_hostname: None,
                started_at: None,
                custom: HashMap::new(),
                error_code: None,
                error_type: None,
                context: HashMap::new(),
            },
            status: "completed".to_string(),
            error: None,
            orchestration_metadata: None,
        };

        DomainEvent {
            event_id: Uuid::now_v7(),
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(),
            payload: DomainEventPayload {
                task_sequence_step,
                execution_result,
                payload: json!({}),
            },
            metadata: EventMetadata {
                task_uuid: Uuid::new_v4(),
                step_uuid: Some(Uuid::new_v4()),
                step_name: Some("test_step".to_string()),
                namespace: "test".to_string(),
                correlation_id: Uuid::new_v4(),
                fired_at: Utc::now(),
                fired_by: "test".to_string(),
            },
        }
    }

    /// Create a counting handler
    fn create_counting_handler(counter: Arc<AtomicUsize>) -> EventHandler {
        Arc::new(move |_event| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        })
    }

    /// Create a failing handler
    fn create_failing_handler() -> EventHandler {
        Arc::new(move |event| {
            Box::pin(async move {
                Err(EventHandlerError::ExecutionFailed {
                    event_name: event.event_name.clone(),
                    pattern: "test".to_string(),
                    reason: "intentional test failure".to_string(),
                })
            })
        })
    }

    #[test]
    fn test_bus_creation() {
        let bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let stats = bus.get_statistics();

        assert_eq!(stats.rust_subscriber_patterns, 0);
        assert_eq!(stats.rust_handler_count, 0);
        assert_eq!(stats.ffi_subscriber_count, 0);
        assert_eq!(stats.total_events_dispatched, 0);
    }

    #[test]
    fn test_rust_subscription() {
        let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let counter = Arc::new(AtomicUsize::new(0));

        bus.subscribe("payment.*", create_counting_handler(counter.clone()))
            .unwrap();
        bus.subscribe("*", create_counting_handler(counter.clone()))
            .unwrap();

        let stats = bus.get_statistics();
        assert_eq!(stats.rust_subscriber_patterns, 2);
        assert_eq!(stats.rust_handler_count, 2);
    }

    #[tokio::test]
    async fn test_publish_to_rust_handlers() {
        let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let counter = Arc::new(AtomicUsize::new(0));

        // Subscribe to payment events
        bus.subscribe("payment.*", create_counting_handler(counter.clone()))
            .unwrap();

        // Publish matching event
        let event = create_test_event("payment.processed");
        bus.publish(event).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let stats = bus.get_statistics();
        assert_eq!(stats.total_events_dispatched, 1);
        assert_eq!(stats.rust_handler_dispatches, 1);
    }

    #[tokio::test]
    async fn test_publish_no_match() {
        let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let counter = Arc::new(AtomicUsize::new(0));

        // Subscribe to payment events
        bus.subscribe("payment.*", create_counting_handler(counter.clone()))
            .unwrap();

        // Publish non-matching event
        let event = create_test_event("order.created");
        bus.publish(event).await;

        // Handler should not be called
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_publish_multiple_handlers() {
        let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let counter = Arc::new(AtomicUsize::new(0));

        // Subscribe with multiple patterns that all match
        bus.subscribe(
            "payment.processed",
            create_counting_handler(counter.clone()),
        )
        .unwrap();
        bus.subscribe("payment.*", create_counting_handler(counter.clone()))
            .unwrap();
        bus.subscribe("*", create_counting_handler(counter.clone()))
            .unwrap();

        // Publish event
        let event = create_test_event("payment.processed");
        bus.publish(event).await;

        // All three handlers should be called
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_fire_and_forget_error_handling() {
        let config = InProcessEventBusConfig {
            log_subscriber_errors: true,
            ..Default::default()
        };
        let mut bus = InProcessEventBus::new(config);

        let success_counter = Arc::new(AtomicUsize::new(0));

        // Subscribe both failing and succeeding handlers
        bus.subscribe("payment.*", create_failing_handler())
            .unwrap();
        bus.subscribe(
            "payment.*",
            create_counting_handler(success_counter.clone()),
        )
        .unwrap();

        // Publish event - should not panic despite handler failure
        let event = create_test_event("payment.processed");
        bus.publish(event).await;

        // Successful handler should still be called
        assert_eq!(success_counter.load(Ordering::SeqCst), 1);

        let stats = bus.get_statistics();
        assert_eq!(stats.rust_handler_errors, 1);
    }

    #[tokio::test]
    async fn test_ffi_broadcast() {
        let bus = InProcessEventBus::new(InProcessEventBusConfig::default());

        // Subscribe to FFI channel
        let mut ffi_receiver = bus.subscribe_ffi();

        // Publish event
        let event = create_test_event("payment.processed");
        let event_id = event.event_id;
        bus.publish(event).await;

        // FFI subscriber should receive event
        let received = ffi_receiver.recv().await.unwrap();
        assert_eq!(received.event_id, event_id);
        assert_eq!(received.event_name, "payment.processed");
    }

    #[tokio::test]
    async fn test_ffi_no_subscribers() {
        let bus = InProcessEventBus::new(InProcessEventBusConfig::default());

        // Publish without FFI subscribers - should not panic
        let event = create_test_event("payment.processed");
        bus.publish(event).await;

        let stats = bus.get_statistics();
        assert_eq!(stats.ffi_channel_drops, 1);
    }

    #[test]
    fn test_builder() {
        let bus = InProcessEventBusBuilder::new()
            .ffi_channel_buffer_size(500)
            .log_subscriber_errors(false)
            .dispatch_timeout_ms(1000)
            .build();

        assert_eq!(bus.config().ffi_channel_buffer_size, 500);
        assert!(!bus.config().log_subscriber_errors);
        assert_eq!(bus.config().dispatch_timeout_ms, 1000);
    }

    #[test]
    fn test_clear_subscriptions() {
        let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
        let counter = Arc::new(AtomicUsize::new(0));

        bus.subscribe("payment.*", create_counting_handler(counter))
            .unwrap();

        assert_eq!(bus.get_statistics().rust_handler_count, 1);

        bus.clear_rust_subscriptions();

        assert_eq!(bus.get_statistics().rust_handler_count, 0);
    }
}
