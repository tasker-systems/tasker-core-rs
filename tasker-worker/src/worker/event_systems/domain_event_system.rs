//! # TAS-65/TAS-69: Domain Event System
//!
//! Async system for processing domain event commands using fire-and-forget semantics.
//! Commands are received via an MPSC channel and processed in a dedicated task.
//!
//! ## Architecture
//!
//! ```text
//! command_processor.rs                  DomainEventSystem
//!       |                                     |
//!       | try_send(command)                   | spawn process_loop()
//!       v                                     v
//! mpsc::Sender<DomainEventCommand>  →  mpsc::Receiver
//!                                             |
//!                                             v
//!                                     StepEventPublisher.publish()
//!                                             |
//!                                             v
//!                                     EventRouter → PGMQ / InProcess
//! ```
//!
//! ## Key Design Decisions
//!
//! 1. **Fire-and-forget dispatch**: `dispatch_events()` uses `try_send()` which
//!    never blocks. If the channel is full, events are dropped with metrics.
//!
//! 2. **Async processing loop**: Commands are processed in a background task,
//!    decoupling event publishing from step execution flow.
//!
//! 3. **Per-step publishers**: Custom publishers can be registered for specific
//!    steps. Unregistered publishers fail loudly at task initialization.
//!
//! 4. **Graceful shutdown**: Fast events are drained up to a configurable timeout.
//!    Durable events are already persisted so require no special handling.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use tasker_shared::events::domain_events::DomainEventPayload;
use tasker_shared::metrics::worker::{
    DOMAIN_EVENTS_DISPATCHED_TOTAL, DOMAIN_EVENTS_DROPPED_TOTAL, DOMAIN_EVENTS_FAILED_TOTAL,
    DOMAIN_EVENTS_PUBLISHED_TOTAL, DOMAIN_EVENT_PUBLISH_DURATION,
};

use super::super::domain_event_commands::{
    DomainEventCommand, DomainEventSystemStats, DomainEventToPublish, ShutdownResult,
};
use super::super::event_router::EventRouter;

/// Configuration for the DomainEventSystem
#[derive(Debug, Clone)]
pub struct DomainEventSystemConfig {
    /// Buffer size for the command channel
    pub channel_buffer_size: usize,
    /// Timeout for draining fast events during shutdown
    pub shutdown_drain_timeout_ms: u64,
    /// Whether to log dropped events (useful for debugging)
    pub log_dropped_events: bool,
}

impl Default for DomainEventSystemConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 1000,
            shutdown_drain_timeout_ms: 5000,
            log_dropped_events: true,
        }
    }
}

/// Atomic statistics for thread-safe updates
struct AtomicStats {
    events_dispatched: AtomicU64,
    events_published: AtomicU64,
    events_failed: AtomicU64,
    events_dropped: AtomicU64,
    durable_events: AtomicU64,
    fast_events: AtomicU64,
    broadcast_events: AtomicU64,
}

impl Default for AtomicStats {
    fn default() -> Self {
        Self {
            events_dispatched: AtomicU64::new(0),
            events_published: AtomicU64::new(0),
            events_failed: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
            durable_events: AtomicU64::new(0),
            fast_events: AtomicU64::new(0),
            broadcast_events: AtomicU64::new(0),
        }
    }
}

/// Handle for sending commands to the DomainEventSystem
///
/// This handle is cloneable and can be shared across threads.
/// Uses `try_send()` for fire-and-forget dispatch.
#[derive(Clone)]
pub struct DomainEventSystemHandle {
    /// Command sender
    sender: mpsc::Sender<DomainEventCommand>,
    /// Statistics (shared with system)
    stats: Arc<AtomicStats>,
    /// Configuration
    config: DomainEventSystemConfig,
    /// Running flag
    is_running: Arc<AtomicBool>,
}

impl std::fmt::Debug for DomainEventSystemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainEventSystemHandle")
            .field("channel_capacity", &self.sender.capacity())
            .field("is_running", &self.is_running.load(Ordering::SeqCst))
            .finish()
    }
}

impl DomainEventSystemHandle {
    /// Dispatch domain events (fire-and-forget)
    ///
    /// This method uses `try_send()` which never blocks. If the channel is full,
    /// events are dropped and a metric is recorded.
    ///
    /// # Arguments
    ///
    /// * `events` - Events to publish
    /// * `publisher_name` - Name of the publisher to use
    /// * `correlation_id` - Correlation ID for tracing
    ///
    /// # Returns
    ///
    /// `true` if command was sent, `false` if dropped due to backpressure
    #[instrument(skip(self, events), fields(
        event_count = events.len(),
        publisher = %publisher_name,
        correlation_id = %correlation_id
    ))]
    pub fn dispatch_events(
        &self,
        events: Vec<DomainEventToPublish>,
        publisher_name: String,
        correlation_id: Uuid,
    ) -> bool {
        if events.is_empty() {
            debug!("No events to dispatch");
            return true;
        }

        let event_count = events.len() as u64;
        self.stats
            .events_dispatched
            .fetch_add(event_count, Ordering::SeqCst);

        // TAS-65: Emit dispatched metric using static counter
        if let Some(counter) = DOMAIN_EVENTS_DISPATCHED_TOTAL.get() {
            counter.add(
                event_count,
                &[
                    opentelemetry::KeyValue::new("namespace", "aggregate"),
                    opentelemetry::KeyValue::new("delivery_mode", "mixed"),
                ],
            );
        }

        let command = DomainEventCommand::DispatchEvents {
            events,
            publisher_name,
            correlation_id,
        };

        match self.sender.try_send(command) {
            Ok(()) => {
                debug!(
                    event_count = event_count,
                    "Domain events dispatched to processing queue"
                );
                true
            }
            Err(mpsc::error::TrySendError::Full(cmd)) => {
                // Channel is full - drop events and record metric
                let dropped_count = match cmd {
                    DomainEventCommand::DispatchEvents { events, .. } => events.len() as u64,
                    _ => 0,
                };
                self.stats
                    .events_dropped
                    .fetch_add(dropped_count, Ordering::SeqCst);

                if self.config.log_dropped_events {
                    warn!(
                        dropped_count = dropped_count,
                        correlation_id = %correlation_id,
                        "Domain events dropped due to channel backpressure"
                    );
                }

                // TAS-65: Emit OpenTelemetry metric using static counter
                if let Some(counter) = DOMAIN_EVENTS_DROPPED_TOTAL.get() {
                    counter.add(
                        dropped_count,
                        &[
                            opentelemetry::KeyValue::new("namespace", "unknown"),
                            opentelemetry::KeyValue::new("delivery_mode", "unknown"),
                        ],
                    );
                }

                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!("Domain event system channel closed");
                false
            }
        }
    }

    /// Get current statistics
    pub fn get_stats(&self) -> DomainEventSystemStats {
        DomainEventSystemStats {
            events_dispatched: self.stats.events_dispatched.load(Ordering::SeqCst),
            events_published: self.stats.events_published.load(Ordering::SeqCst),
            events_failed: self.stats.events_failed.load(Ordering::SeqCst),
            events_dropped: self.stats.events_dropped.load(Ordering::SeqCst),
            durable_events: self.stats.durable_events.load(Ordering::SeqCst),
            fast_events: self.stats.fast_events.load(Ordering::SeqCst),
            broadcast_events: self.stats.broadcast_events.load(Ordering::SeqCst),
            last_event_at: None, // Would need RwLock to track this
            channel_depth: self.sender.max_capacity() - self.sender.capacity(),
            channel_capacity: self.sender.max_capacity(),
        }
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    /// Request shutdown and wait for completion
    pub async fn shutdown(&self) -> ShutdownResult {
        let (tx, rx) = tokio::sync::oneshot::channel();

        if self
            .sender
            .send(DomainEventCommand::Shutdown { resp: tx })
            .await
            .is_err()
        {
            return ShutdownResult {
                success: false,
                events_drained: 0,
                duration_ms: 0,
            };
        }

        rx.await.unwrap_or(ShutdownResult {
            success: false,
            events_drained: 0,
            duration_ms: 0,
        })
    }
}

/// The domain event processing system
///
/// Runs as a background task, processing commands from the channel.
pub struct DomainEventSystem {
    /// Command receiver
    receiver: mpsc::Receiver<DomainEventCommand>,
    /// Event router for actual publishing
    event_router: Arc<EventRouter>,
    /// Statistics
    stats: Arc<AtomicStats>,
    /// Configuration
    config: DomainEventSystemConfig,
    /// Running flag
    is_running: Arc<AtomicBool>,
}

impl std::fmt::Debug for DomainEventSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainEventSystem")
            .field("event_router", &"<EventRouter>")
            .field("is_running", &self.is_running.load(Ordering::SeqCst))
            .finish()
    }
}

impl DomainEventSystem {
    /// Create a new domain event system with handle
    ///
    /// Returns both the system (to be spawned) and the handle (for sending commands).
    pub fn new(
        event_router: Arc<EventRouter>,
        config: DomainEventSystemConfig,
    ) -> (Self, DomainEventSystemHandle) {
        let (sender, receiver) = mpsc::channel(config.channel_buffer_size);
        let stats = Arc::new(AtomicStats::default());
        let is_running = Arc::new(AtomicBool::new(false));

        let handle = DomainEventSystemHandle {
            sender,
            stats: Arc::clone(&stats),
            config: config.clone(),
            is_running: Arc::clone(&is_running),
        };

        let system = Self {
            receiver,
            event_router,
            stats,
            config,
            is_running,
        };

        (system, handle)
    }

    /// Run the processing loop
    ///
    /// This should be spawned as a background task.
    #[instrument(skip(self), name = "domain_event_system_loop")]
    pub async fn run(mut self) {
        info!("Starting DomainEventSystem processing loop");
        self.is_running.store(true, Ordering::SeqCst);

        while let Some(command) = self.receiver.recv().await {
            match command {
                DomainEventCommand::DispatchEvents {
                    events,
                    publisher_name,
                    correlation_id,
                } => {
                    self.process_dispatch_events(events, &publisher_name, correlation_id)
                        .await;
                }
                DomainEventCommand::GetStats { resp } => {
                    let stats = DomainEventSystemStats {
                        events_dispatched: self.stats.events_dispatched.load(Ordering::SeqCst),
                        events_published: self.stats.events_published.load(Ordering::SeqCst),
                        events_failed: self.stats.events_failed.load(Ordering::SeqCst),
                        events_dropped: self.stats.events_dropped.load(Ordering::SeqCst),
                        durable_events: self.stats.durable_events.load(Ordering::SeqCst),
                        fast_events: self.stats.fast_events.load(Ordering::SeqCst),
                        broadcast_events: self.stats.broadcast_events.load(Ordering::SeqCst),
                        last_event_at: None,
                        channel_depth: 0,
                        channel_capacity: self.config.channel_buffer_size,
                    };
                    let _ = resp.send(stats);
                }
                DomainEventCommand::Shutdown { resp } => {
                    let result = self.process_shutdown().await;
                    let _ = resp.send(result);
                    break;
                }
            }
        }

        self.is_running.store(false, Ordering::SeqCst);
        info!("DomainEventSystem processing loop stopped");
    }

    /// Process a dispatch events command
    async fn process_dispatch_events(
        &self,
        events: Vec<DomainEventToPublish>,
        publisher_name: &str,
        correlation_id: Uuid,
    ) {
        debug!(
            event_count = events.len(),
            publisher = %publisher_name,
            correlation_id = %correlation_id,
            "Processing domain event dispatch"
        );

        for event in events {
            // TAS-65: Extract metric labels before moving event data
            let event_name = event.event_name.clone();
            let namespace = event.metadata.namespace.clone();
            // Use Display trait from EventDeliveryMode for accurate label (durable/fast/broadcast)
            let delivery_mode_label = event.delivery_mode.to_string();
            let delivery_mode = event.delivery_mode;

            // Create the full domain event payload
            let payload = DomainEventPayload {
                task_sequence_step: event.task_sequence_step,
                execution_result: event.execution_result,
                payload: event.business_payload,
            };

            // Route the event with duration tracking
            let route_start = Instant::now();
            let route_result = self
                .event_router
                .route_event(event.delivery_mode, &event_name, payload, event.metadata)
                .await;
            let duration_ms = route_start.elapsed().as_millis() as f64;

            match route_result {
                Ok(outcome) => {
                    use tasker_shared::models::core::task_template::EventDeliveryMode;

                    self.stats.events_published.fetch_add(1, Ordering::SeqCst);

                    // Track stats by delivery mode (durable/fast/broadcast)
                    match delivery_mode {
                        EventDeliveryMode::Durable => {
                            self.stats.durable_events.fetch_add(1, Ordering::SeqCst);
                        }
                        EventDeliveryMode::Fast => {
                            self.stats.fast_events.fetch_add(1, Ordering::SeqCst);
                        }
                        EventDeliveryMode::Broadcast => {
                            self.stats.broadcast_events.fetch_add(1, Ordering::SeqCst);
                        }
                    }

                    debug!(
                        event_id = %outcome.event_id(),
                        event_name = %event_name,
                        delivery_mode = %delivery_mode_label,
                        "Domain event published successfully"
                    );

                    // TAS-65: Emit success metric using static counter
                    if let Some(counter) = DOMAIN_EVENTS_PUBLISHED_TOTAL.get() {
                        counter.add(
                            1,
                            &[
                                opentelemetry::KeyValue::new("namespace", namespace.clone()),
                                opentelemetry::KeyValue::new("event_name", event_name.clone()),
                                opentelemetry::KeyValue::new(
                                    "delivery_mode",
                                    delivery_mode_label.clone(),
                                ),
                                opentelemetry::KeyValue::new("publisher", "default"),
                            ],
                        );
                    }

                    // TAS-65: Record publish duration
                    if let Some(histogram) = DOMAIN_EVENT_PUBLISH_DURATION.get() {
                        histogram.record(
                            duration_ms,
                            &[
                                opentelemetry::KeyValue::new("namespace", namespace.clone()),
                                opentelemetry::KeyValue::new(
                                    "delivery_mode",
                                    delivery_mode_label.clone(),
                                ),
                                opentelemetry::KeyValue::new("result", "success"),
                            ],
                        );
                    }
                }
                Err(e) => {
                    self.stats.events_failed.fetch_add(1, Ordering::SeqCst);

                    error!(
                        event_name = %event_name,
                        error = %e,
                        "Failed to publish domain event"
                    );

                    // TAS-65: Emit failure metric using static counter
                    if let Some(counter) = DOMAIN_EVENTS_FAILED_TOTAL.get() {
                        counter.add(
                            1,
                            &[
                                opentelemetry::KeyValue::new("namespace", namespace.clone()),
                                opentelemetry::KeyValue::new("event_name", event_name.clone()),
                                opentelemetry::KeyValue::new(
                                    "delivery_mode",
                                    delivery_mode_label.clone(),
                                ),
                                opentelemetry::KeyValue::new("error_type", "publish_failed"),
                            ],
                        );
                    }

                    // TAS-65: Record publish duration (even on failure)
                    if let Some(histogram) = DOMAIN_EVENT_PUBLISH_DURATION.get() {
                        histogram.record(
                            duration_ms,
                            &[
                                opentelemetry::KeyValue::new("namespace", namespace.clone()),
                                opentelemetry::KeyValue::new(
                                    "delivery_mode",
                                    delivery_mode_label.clone(),
                                ),
                                opentelemetry::KeyValue::new("result", "error"),
                            ],
                        );
                    }
                }
            }
        }
    }

    /// Process shutdown command
    async fn process_shutdown(&mut self) -> ShutdownResult {
        let start = Instant::now();
        info!(
            timeout_ms = self.config.shutdown_drain_timeout_ms,
            "Processing domain event system shutdown"
        );

        let mut events_drained = 0u64;
        let timeout = Duration::from_millis(self.config.shutdown_drain_timeout_ms);
        let deadline = Instant::now() + timeout;

        // Drain remaining commands until timeout
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                warn!(
                    events_drained = events_drained,
                    "Shutdown drain timeout reached"
                );
                break;
            }

            match tokio::time::timeout(remaining, self.receiver.recv()).await {
                Ok(Some(DomainEventCommand::DispatchEvents {
                    events,
                    publisher_name,
                    correlation_id,
                })) => {
                    let count = events.len() as u64;
                    self.process_dispatch_events(events, &publisher_name, correlation_id)
                        .await;
                    events_drained += count;
                }
                Ok(Some(DomainEventCommand::GetStats { resp })) => {
                    // Still respond to stats during drain
                    let stats = DomainEventSystemStats {
                        events_dispatched: self.stats.events_dispatched.load(Ordering::SeqCst),
                        events_published: self.stats.events_published.load(Ordering::SeqCst),
                        events_failed: self.stats.events_failed.load(Ordering::SeqCst),
                        events_dropped: self.stats.events_dropped.load(Ordering::SeqCst),
                        durable_events: self.stats.durable_events.load(Ordering::SeqCst),
                        fast_events: self.stats.fast_events.load(Ordering::SeqCst),
                        broadcast_events: self.stats.broadcast_events.load(Ordering::SeqCst),
                        last_event_at: None,
                        channel_depth: 0,
                        channel_capacity: self.config.channel_buffer_size,
                    };
                    let _ = resp.send(stats);
                }
                Ok(Some(DomainEventCommand::Shutdown { resp })) => {
                    // Already shutting down
                    let _ = resp.send(ShutdownResult {
                        success: true,
                        events_drained: 0,
                        duration_ms: 0,
                    });
                }
                Ok(None) => {
                    // Channel closed
                    break;
                }
                Err(_) => {
                    // Timeout
                    break;
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            events_drained = events_drained,
            duration_ms = duration_ms,
            "Domain event system shutdown complete"
        );

        ShutdownResult {
            success: true,
            events_drained,
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use chrono::Utc;
    use tasker_shared::events::domain_events::EventMetadata;
    use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        EventDeliveryMode, HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::types::base::TaskSequenceStep;

    fn create_test_event(delivery_mode: EventDeliveryMode) -> DomainEventToPublish {
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
                    context: Some(serde_json::json!({})),
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
            result: serde_json::json!({"test": true}),
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

        DomainEventToPublish {
            event_name: "test.event".to_string(),
            delivery_mode,
            business_payload: serde_json::json!({"key": "value"}),
            metadata: EventMetadata {
                task_uuid: Uuid::new_v4(),
                step_uuid: Some(Uuid::new_v4()),
                step_name: Some("test_step".to_string()),
                namespace: "test".to_string(),
                correlation_id: Uuid::new_v4(),
                fired_at: Utc::now(),
                fired_by: "test".to_string(),
            },
            task_sequence_step,
            execution_result,
        }
    }

    #[test]
    fn test_handle_stats() {
        let stats = Arc::new(AtomicStats::default());
        stats.events_dispatched.store(10, Ordering::SeqCst);
        stats.events_published.store(8, Ordering::SeqCst);
        stats.events_dropped.store(2, Ordering::SeqCst);

        let (sender, _) = mpsc::channel(100);
        let handle = DomainEventSystemHandle {
            sender,
            stats,
            config: DomainEventSystemConfig::default(),
            is_running: Arc::new(AtomicBool::new(true)),
        };

        let result = handle.get_stats();
        assert_eq!(result.events_dispatched, 10);
        assert_eq!(result.events_published, 8);
        assert_eq!(result.events_dropped, 2);
    }

    #[test]
    fn test_dispatch_empty_events() {
        let (sender, _) = mpsc::channel(100);
        let handle = DomainEventSystemHandle {
            sender,
            stats: Arc::new(AtomicStats::default()),
            config: DomainEventSystemConfig::default(),
            is_running: Arc::new(AtomicBool::new(true)),
        };

        // Empty events should return true without sending
        let result = handle.dispatch_events(vec![], "default".to_string(), Uuid::new_v4());
        assert!(result);
        assert_eq!(handle.get_stats().events_dispatched, 0);
    }

    #[tokio::test]
    async fn test_dispatch_with_backpressure() {
        // Create channel with small buffer
        let (sender, _receiver) = mpsc::channel(1);
        let stats = Arc::new(AtomicStats::default());
        let handle = DomainEventSystemHandle {
            sender,
            stats: Arc::clone(&stats),
            config: DomainEventSystemConfig {
                log_dropped_events: false,
                ..Default::default()
            },
            is_running: Arc::new(AtomicBool::new(true)),
        };

        // First dispatch should succeed
        let event1 = create_test_event(EventDeliveryMode::Fast);
        let result1 = handle.dispatch_events(vec![event1], "default".to_string(), Uuid::new_v4());
        assert!(result1);

        // Second dispatch should fail (channel full, no consumer)
        let event2 = create_test_event(EventDeliveryMode::Fast);
        let result2 = handle.dispatch_events(vec![event2], "default".to_string(), Uuid::new_v4());
        assert!(!result2);

        // Check dropped counter
        assert_eq!(stats.events_dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_config_default() {
        let config = DomainEventSystemConfig::default();
        assert_eq!(config.channel_buffer_size, 1000);
        assert_eq!(config.shutdown_drain_timeout_ms, 5000);
        assert!(config.log_dropped_events);
    }
}
