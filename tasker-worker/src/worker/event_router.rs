//! # TAS-65: Event Router
//!
//! Routes domain events to appropriate delivery paths based on `EventDeliveryMode`.
//!
//! ## Architecture
//!
//! ```text
//! StepEventPublisher.publish(ctx)
//!         |
//!         v
//!   EventRouter.route_event(...)
//!         |
//!    +----+----+
//!    |         |
//!    v         v
//! Durable    Fast
//!    |         |
//!    v         v
//!  PGMQ    InProcessEventBus
//! ```
//!
//! ## Delivery Modes
//!
//! - **Durable**: Events persisted to PGMQ with at-least-once delivery.
//!   Tasker's responsibility ends at queue publication.
//!   Use for: External consumers, audit trails, cross-service events.
//!
//! - **Fast**: Events dispatched in-memory via `InProcessEventBus`.
//!   Fire-and-forget with no persistence.
//!   Use for: Metrics, telemetry, Sentry, DataDog, Slack notifications.
//!
//! - **Broadcast**: Events sent to BOTH paths - fast first, then durable.
//!   Fast delivery happens immediately (fire-and-forget), then durable
//!   ensures persistence. Fast errors don't block durable delivery.
//!   Use for: Events needing both real-time metrics AND guaranteed delivery.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_worker::worker::event_router::EventRouter;
//! use tasker_worker::worker::in_process_event_bus::InProcessEventBus;
//! use tasker_shared::events::domain_events::{DomainEventPublisher, DomainEventPayload, EventMetadata};
//! use tasker_shared::models::core::task_template::EventDeliveryMode;
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! async fn example(
//!     domain_publisher: Arc<DomainEventPublisher>,
//!     in_process_bus: Arc<RwLock<InProcessEventBus>>,
//!     payload: DomainEventPayload,
//!     metadata: EventMetadata,
//! ) {
//!     // Create router during worker bootstrap
//!     let router = EventRouter::new(domain_publisher, in_process_bus);
//!
//!     // Route based on delivery mode (determined by YAML config)
//!     let _ = router.route_event(
//!         EventDeliveryMode::Durable,
//!         "payment.processed",
//!         payload,
//!         metadata
//!     ).await;
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use tasker_shared::events::domain_events::{
    DomainEvent, DomainEventError, DomainEventPayload, DomainEventPublisher, EventMetadata,
};
use tasker_shared::metrics::worker::EventRouterStats;
use tasker_shared::models::core::task_template::EventDeliveryMode;

use super::in_process_event_bus::InProcessEventBus;

/// Lock-free atomic counters for EventRouter statistics.
///
/// Replaces `Arc<Mutex<EventRouterStats>>` to eliminate lock contention
/// on the hot path (every event route operation).
#[derive(Debug)]
struct AtomicEventRouterStats {
    total_routed: AtomicU64,
    durable_routed: AtomicU64,
    fast_routed: AtomicU64,
    broadcast_routed: AtomicU64,
    fast_delivery_errors: AtomicU64,
    routing_errors: AtomicU64,
}

impl Default for AtomicEventRouterStats {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicEventRouterStats {
    fn new() -> Self {
        Self {
            total_routed: AtomicU64::new(0),
            durable_routed: AtomicU64::new(0),
            fast_routed: AtomicU64::new(0),
            broadcast_routed: AtomicU64::new(0),
            fast_delivery_errors: AtomicU64::new(0),
            routing_errors: AtomicU64::new(0),
        }
    }

    #[inline]
    fn record_route(&self) {
        self.total_routed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_durable(&self) {
        self.durable_routed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_fast(&self) {
        self.fast_routed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_broadcast(&self) {
        self.broadcast_routed.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_fast_error(&self) {
        self.fast_delivery_errors.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_routing_error(&self) {
        self.routing_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> EventRouterStats {
        EventRouterStats {
            total_routed: self.total_routed.load(Ordering::Relaxed),
            durable_routed: self.durable_routed.load(Ordering::Relaxed),
            fast_routed: self.fast_routed.load(Ordering::Relaxed),
            broadcast_routed: self.broadcast_routed.load(Ordering::Relaxed),
            fast_delivery_errors: self.fast_delivery_errors.load(Ordering::Relaxed),
            routing_errors: self.routing_errors.load(Ordering::Relaxed),
        }
    }
}

/// Result type for event routing operations
pub type RouteResult = Result<EventRouteOutcome, EventRouterError>;

/// Outcome of routing an event
#[derive(Debug, Clone)]
pub enum EventRouteOutcome {
    /// Event was published to PGMQ (durable)
    PublishedDurable { event_id: Uuid, queue_name: String },
    /// Event was dispatched to in-process bus (fast)
    DispatchedFast { event_id: Uuid },
    /// Event was broadcast to both paths (fast first, then durable)
    Broadcast {
        /// Event ID (same for both deliveries)
        event_id: Uuid,
        /// Queue name for durable path
        queue_name: String,
        /// Whether fast delivery succeeded (fire-and-forget, always true)
        fast_dispatched: bool,
        /// Whether durable delivery succeeded
        durable_published: bool,
        /// Error message if fast delivery had issues (non-fatal)
        fast_error: Option<String>,
    },
}

impl EventRouteOutcome {
    /// Get the event ID regardless of delivery mode
    pub fn event_id(&self) -> Uuid {
        match self {
            Self::PublishedDurable { event_id, .. }
            | Self::DispatchedFast { event_id }
            | Self::Broadcast { event_id, .. } => *event_id,
        }
    }

    /// Check if event was delivered via durable path
    pub fn is_durable(&self) -> bool {
        matches!(self, Self::PublishedDurable { .. })
    }

    /// Check if event was delivered via fast path
    pub fn is_fast(&self) -> bool {
        matches!(self, Self::DispatchedFast { .. })
    }

    /// Check if event was broadcast to both paths
    pub fn is_broadcast(&self) -> bool {
        matches!(self, Self::Broadcast { .. })
    }

    /// Check if event used durable delivery (durable-only or broadcast)
    pub fn includes_durable(&self) -> bool {
        matches!(
            self,
            Self::PublishedDurable { .. }
                | Self::Broadcast {
                    durable_published: true,
                    ..
                }
        )
    }

    /// Check if event used fast delivery (fast-only or broadcast)
    pub fn includes_fast(&self) -> bool {
        matches!(
            self,
            Self::DispatchedFast { .. }
                | Self::Broadcast {
                    fast_dispatched: true,
                    ..
                }
        )
    }
}

/// Errors from event routing
#[derive(Debug, thiserror::Error)]
pub enum EventRouterError {
    #[error("Failed to publish durable event '{event_name}': {reason}")]
    DurablePublishFailed { event_name: String, reason: String },

    #[error("Event router not properly initialized: {0}")]
    NotInitialized(String),

    #[error("Domain event error: {0}")]
    DomainEvent(#[from] DomainEventError),
}

// EventRouterStats is imported from tasker_shared::metrics::worker (canonical location)

/// Routes domain events based on delivery mode
///
/// Thread-safe router that dispatches events to either:
/// - `DomainEventPublisher` for durable PGMQ delivery
/// - `InProcessEventBus` for fast in-memory delivery
pub struct EventRouter {
    /// Publisher for durable events (PGMQ)
    domain_publisher: Arc<DomainEventPublisher>,
    /// Bus for fast events (in-memory)
    in_process_bus: Arc<RwLock<InProcessEventBus>>,
    /// Lock-free atomic statistics
    stats: AtomicEventRouterStats,
}

// Manual Debug since DomainEventPublisher doesn't implement Debug
impl std::fmt::Debug for EventRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRouter")
            .field("domain_publisher", &"<DomainEventPublisher>")
            .field("in_process_bus", &"<InProcessEventBus>")
            .finish()
    }
}

impl EventRouter {
    /// Create a new event router
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - Publisher for durable events (PGMQ)
    /// * `in_process_bus` - Bus for fast events (in-memory)
    pub fn new(
        domain_publisher: Arc<DomainEventPublisher>,
        in_process_bus: Arc<RwLock<InProcessEventBus>>,
    ) -> Self {
        info!("Creating EventRouter for dual-path event delivery");

        Self {
            domain_publisher,
            in_process_bus,
            stats: AtomicEventRouterStats::new(),
        }
    }

    /// Route an event based on delivery mode
    ///
    /// # Arguments
    ///
    /// * `delivery_mode` - How to deliver the event (Durable or Fast)
    /// * `event_name` - Event name in dot notation
    /// * `payload` - Full domain event payload
    /// * `metadata` - Event metadata (correlation, namespace, etc.)
    ///
    /// # Returns
    ///
    /// `EventRouteOutcome` with event ID on success
    #[instrument(skip(self, payload, metadata), fields(
        delivery_mode = ?delivery_mode,
        event_name = %event_name,
        namespace = %metadata.namespace
    ))]
    pub async fn route_event(
        &self,
        delivery_mode: EventDeliveryMode,
        event_name: &str,
        payload: DomainEventPayload,
        metadata: EventMetadata,
    ) -> RouteResult {
        debug!(
            delivery_mode = ?delivery_mode,
            event_name = %event_name,
            task_uuid = %metadata.task_uuid,
            correlation_id = %metadata.correlation_id,
            "Routing domain event"
        );

        self.stats.record_route();

        match delivery_mode {
            EventDeliveryMode::Durable => self.route_durable(event_name, payload, metadata).await,
            EventDeliveryMode::Fast => self.route_fast(event_name, payload, metadata).await,
            EventDeliveryMode::Broadcast => {
                self.route_broadcast(event_name, payload, metadata).await
            }
        }
    }

    /// Route event via durable path (PGMQ)
    async fn route_durable(
        &self,
        event_name: &str,
        payload: DomainEventPayload,
        metadata: EventMetadata,
    ) -> RouteResult {
        debug!(
            event_name = %event_name,
            namespace = %metadata.namespace,
            "Routing to durable path (PGMQ)"
        );

        let queue_name = format!("{}_domain_events", metadata.namespace);

        match self
            .domain_publisher
            .publish_event(event_name, payload, metadata)
            .await
        {
            Ok(event_id) => {
                debug!(
                    event_id = %event_id,
                    event_name = %event_name,
                    queue_name = %queue_name,
                    "Event published to durable path"
                );

                self.stats.record_durable();

                Ok(EventRouteOutcome::PublishedDurable {
                    event_id,
                    queue_name,
                })
            }
            Err(e) => {
                error!(
                    event_name = %event_name,
                    error = %e,
                    "Failed to publish to durable path"
                );

                self.stats.record_routing_error();

                Err(EventRouterError::DurablePublishFailed {
                    event_name: event_name.to_string(),
                    reason: e.to_string(),
                })
            }
        }
    }

    /// Route event via fast path (in-process bus)
    async fn route_fast(
        &self,
        event_name: &str,
        payload: DomainEventPayload,
        metadata: EventMetadata,
    ) -> RouteResult {
        debug!(
            event_name = %event_name,
            namespace = %metadata.namespace,
            "Routing to fast path (in-process)"
        );

        // Create the domain event
        let event_id = Uuid::now_v7();
        let event = DomainEvent {
            event_id,
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(),
            payload,
            metadata,
        };

        // Dispatch to in-process bus (fire-and-forget)
        // The bus handles errors internally - never fails the workflow
        {
            let bus = self.in_process_bus.read().await;
            bus.publish(event).await;
        }

        debug!(
            event_id = %event_id,
            event_name = %event_name,
            "Event dispatched to fast path"
        );

        self.stats.record_fast();

        Ok(EventRouteOutcome::DispatchedFast { event_id })
    }

    /// Route event via broadcast (both fast AND durable)
    ///
    /// Fast delivery happens first (fire-and-forget), then durable.
    /// Fast errors are logged but don't prevent durable delivery.
    async fn route_broadcast(
        &self,
        event_name: &str,
        payload: DomainEventPayload,
        metadata: EventMetadata,
    ) -> RouteResult {
        debug!(
            event_name = %event_name,
            namespace = %metadata.namespace,
            "Routing to broadcast (fast + durable)"
        );

        let event_id = Uuid::now_v7();
        let queue_name = format!("{}_domain_events", metadata.namespace);
        // Fast delivery is fire-and-forget - errors captured for monitoring only
        let fast_error: Option<String> = None;

        // Step 1: Fast delivery first (fire-and-forget)
        // Create event for fast path
        let fast_event = DomainEvent {
            event_id,
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(),
            payload: payload.clone(),
            metadata: metadata.clone(),
        };

        // Dispatch to in-process bus - fire-and-forget, never blocks durable
        {
            let bus = self.in_process_bus.read().await;
            bus.publish(fast_event).await;
        }

        debug!(
            event_id = %event_id,
            event_name = %event_name,
            "Broadcast: fast delivery complete"
        );

        // Step 2: Durable delivery (required for success)
        match self
            .domain_publisher
            .publish_event(event_name, payload, metadata)
            .await
        {
            Ok(published_event_id) => {
                debug!(
                    event_id = %published_event_id,
                    event_name = %event_name,
                    queue_name = %queue_name,
                    "Broadcast: durable delivery complete"
                );

                self.stats.record_broadcast();
                if fast_error.is_some() {
                    self.stats.record_fast_error();
                }

                Ok(EventRouteOutcome::Broadcast {
                    event_id: published_event_id,
                    queue_name,
                    fast_dispatched: true,
                    durable_published: true,
                    fast_error,
                })
            }
            Err(e) => {
                error!(
                    event_name = %event_name,
                    error = %e,
                    "Broadcast: durable delivery failed"
                );

                self.stats.record_routing_error();
                if fast_error.is_some() {
                    self.stats.record_fast_error();
                }

                // Durable failure is a routing error
                Err(EventRouterError::DurablePublishFailed {
                    event_name: event_name.to_string(),
                    reason: e.to_string(),
                })
            }
        }
    }

    /// Get router statistics
    pub fn get_statistics(&self) -> EventRouterStats {
        self.stats.snapshot()
    }

    /// Get reference to the domain publisher
    pub fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
        &self.domain_publisher
    }

    /// Get reference to the in-process bus
    pub fn in_process_bus(&self) -> &Arc<RwLock<InProcessEventBus>> {
        &self.in_process_bus
    }
}

/// Builder for EventRouter
pub struct EventRouterBuilder {
    domain_publisher: Option<Arc<DomainEventPublisher>>,
    in_process_bus: Option<Arc<RwLock<InProcessEventBus>>>,
}

// Manual Debug implementation because DomainEventPublisher doesn't implement Debug
impl std::fmt::Debug for EventRouterBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRouterBuilder")
            .field(
                "domain_publisher",
                &self
                    .domain_publisher
                    .as_ref()
                    .map(|_| "<DomainEventPublisher>"),
            )
            .field(
                "in_process_bus",
                &self.in_process_bus.as_ref().map(|_| "<InProcessEventBus>"),
            )
            .finish()
    }
}

impl Default for EventRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EventRouterBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            domain_publisher: None,
            in_process_bus: None,
        }
    }

    /// Set the domain publisher for durable events
    pub fn domain_publisher(mut self, publisher: Arc<DomainEventPublisher>) -> Self {
        self.domain_publisher = Some(publisher);
        self
    }

    /// Set the in-process bus for fast events
    pub fn in_process_bus(mut self, bus: Arc<RwLock<InProcessEventBus>>) -> Self {
        self.in_process_bus = Some(bus);
        self
    }

    /// Build the router
    ///
    /// # Panics
    ///
    /// Panics if domain_publisher or in_process_bus is not set
    pub fn build(self) -> EventRouter {
        EventRouter::new(
            self.domain_publisher.expect("domain_publisher is required"),
            self.in_process_bus.expect("in_process_bus is required"),
        )
    }

    /// Try to build the router, returning an error if not properly configured
    pub fn try_build(self) -> Result<EventRouter, EventRouterError> {
        let domain_publisher = self.domain_publisher.ok_or_else(|| {
            EventRouterError::NotInitialized("domain_publisher not set".to_string())
        })?;
        let in_process_bus = self.in_process_bus.ok_or_else(|| {
            EventRouterError::NotInitialized("in_process_bus not set".to_string())
        })?;

        Ok(EventRouter::new(domain_publisher, in_process_bus))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::Utc;
    use serde_json::json;

    use tasker_shared::events::registry::EventHandler;
    use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::types::base::TaskSequenceStep;

    use crate::worker::in_process_event_bus::InProcessEventBusConfig;

    /// Create test metadata
    fn create_test_metadata(namespace: &str) -> EventMetadata {
        EventMetadata {
            task_uuid: Uuid::new_v4(),
            step_uuid: Some(Uuid::new_v4()),
            step_name: Some("test_step".to_string()),
            namespace: namespace.to_string(),
            correlation_id: Uuid::new_v4(),
            fired_at: Utc::now(),
            fired_by: "test".to_string(),
        }
    }

    /// Create test payload
    fn create_test_payload() -> DomainEventPayload {
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
                    method: None,
                    resolver: None,
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

        DomainEventPayload {
            task_sequence_step,
            execution_result,
            payload: json!({"business": "data"}),
        }
    }

    /// Create counting handler for testing
    fn create_counting_handler(counter: Arc<AtomicUsize>) -> EventHandler {
        Arc::new(move |_event| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        })
    }

    // Note: Testing the durable path requires a real DomainEventPublisher with
    // database connection. These tests focus on the fast path and routing logic.

    #[tokio::test]
    async fn test_route_fast_event() {
        // Create in-process bus with subscriber
        let counter = Arc::new(AtomicUsize::new(0));
        let bus = {
            let mut bus = InProcessEventBus::new(InProcessEventBusConfig::default());
            bus.subscribe("*", create_counting_handler(counter.clone()))
                .unwrap();
            Arc::new(RwLock::new(bus))
        };

        // We need a mock DomainEventPublisher - for this test we'll just verify
        // the fast path works. Full integration tests will test durable path.
        // For now, create a router that will panic if durable is used.

        // Create a minimal test to verify fast routing works
        let metadata = create_test_metadata("test");
        let payload = create_test_payload();

        // Dispatch directly to bus to verify handler works
        let event = DomainEvent {
            event_id: Uuid::now_v7(),
            event_name: "test.event".to_string(),
            event_version: "1.0".to_string(),
            payload,
            metadata,
        };

        {
            let bus_read = bus.read().await;
            bus_read.publish(event).await;
        }

        // Handler should have been called
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_event_route_outcome() {
        let event_id = Uuid::new_v4();

        let durable = EventRouteOutcome::PublishedDurable {
            event_id,
            queue_name: "test_queue".to_string(),
        };
        assert!(durable.is_durable());
        assert!(!durable.is_fast());
        assert_eq!(durable.event_id(), event_id);

        let fast = EventRouteOutcome::DispatchedFast { event_id };
        assert!(!fast.is_durable());
        assert!(fast.is_fast());
        assert_eq!(fast.event_id(), event_id);
    }

    #[test]
    fn test_builder_missing_fields() {
        let builder = EventRouterBuilder::new();
        let result = builder.try_build();
        assert!(result.is_err());

        match result.unwrap_err() {
            EventRouterError::NotInitialized(msg) => {
                assert!(msg.contains("domain_publisher"));
            }
            _ => panic!("Expected NotInitialized error"),
        }
    }

    #[test]
    fn test_broadcast_outcome() {
        let event_id = Uuid::new_v4();

        let broadcast = EventRouteOutcome::Broadcast {
            event_id,
            queue_name: "test_domain_events".to_string(),
            fast_dispatched: true,
            durable_published: true,
            fast_error: None,
        };

        // Check specific type checks
        assert!(broadcast.is_broadcast());
        assert!(!broadcast.is_durable());
        assert!(!broadcast.is_fast());
        assert_eq!(broadcast.event_id(), event_id);

        // Check includes_* helpers
        assert!(broadcast.includes_durable());
        assert!(broadcast.includes_fast());
    }

    #[test]
    fn test_broadcast_outcome_with_fast_error() {
        let event_id = Uuid::new_v4();

        let broadcast = EventRouteOutcome::Broadcast {
            event_id,
            queue_name: "test_domain_events".to_string(),
            fast_dispatched: true,
            durable_published: true,
            fast_error: Some("Fast handler error (non-fatal)".to_string()),
        };

        // Even with fast error, broadcast should still report success
        // because durable delivery succeeded
        assert!(broadcast.is_broadcast());
        assert!(broadcast.includes_durable());
        assert!(broadcast.includes_fast());
    }

    #[test]
    fn test_outcome_includes_helpers() {
        let event_id = Uuid::new_v4();

        // Durable-only
        let durable = EventRouteOutcome::PublishedDurable {
            event_id,
            queue_name: "test".to_string(),
        };
        assert!(durable.includes_durable());
        assert!(!durable.includes_fast());

        // Fast-only
        let fast = EventRouteOutcome::DispatchedFast { event_id };
        assert!(!fast.includes_durable());
        assert!(fast.includes_fast());

        // Broadcast with both successful
        let broadcast = EventRouteOutcome::Broadcast {
            event_id,
            queue_name: "test".to_string(),
            fast_dispatched: true,
            durable_published: true,
            fast_error: None,
        };
        assert!(broadcast.includes_durable());
        assert!(broadcast.includes_fast());

        // Broadcast with durable failed (shouldn't happen in real code,
        // but test the pattern matching)
        let broadcast_partial = EventRouteOutcome::Broadcast {
            event_id,
            queue_name: "test".to_string(),
            fast_dispatched: true,
            durable_published: false,
            fast_error: None,
        };
        assert!(!broadcast_partial.includes_durable());
        assert!(broadcast_partial.includes_fast());
    }

    #[test]
    fn test_router_stats_default() {
        let stats = EventRouterStats::default();
        assert_eq!(stats.total_routed, 0);
        assert_eq!(stats.durable_routed, 0);
        assert_eq!(stats.fast_routed, 0);
        assert_eq!(stats.broadcast_routed, 0);
        assert_eq!(stats.fast_delivery_errors, 0);
        assert_eq!(stats.routing_errors, 0);
    }
}
