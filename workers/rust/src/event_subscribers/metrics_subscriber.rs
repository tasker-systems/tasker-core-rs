//! # Metrics Subscriber
//!
//! TAS-65: Example event subscriber that collects metrics from domain events.
//!
//! ## Purpose
//!
//! Demonstrates how to create a fast/in-process event subscriber for metrics collection.
//! This subscriber maintains counters for various event types, useful for monitoring
//! and alerting.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_worker_rust::event_subscribers::metrics_subscriber::EventMetricsCollector;
//! use tasker_worker::worker::in_process_event_bus::InProcessEventBus;
//!
//! // Create a metrics collector
//! let metrics = EventMetricsCollector::new();
//!
//! // Subscribe to all events
//! let handler = metrics.create_handler();
//! bus.subscribe("*", handler).unwrap();
//!
//! // Later, read metrics
//! println!("Total events: {}", metrics.events_received());
//! println!("Success events: {}", metrics.success_events());
//! println!("Failure events: {}", metrics.failure_events());
//! ```
//!
//! ## Metrics Collected
//!
//! - `events_received` - Total domain events received
//! - `success_events` - Events from successful step executions
//! - `failure_events` - Events from failed step executions
//! - `events_by_namespace` - Event count per namespace
//! - `events_by_name` - Event count per event name
//! - `last_event_at` - Timestamp of most recent event

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use tasker_shared::events::registry::EventHandler;
use tracing::debug;

/// Collects metrics from domain events
///
/// Thread-safe metrics collector that can be shared across subscribers.
/// Uses atomic operations for counters and RwLock for collections.
#[derive(Debug)]
pub struct EventMetricsCollector {
    /// Total events received
    events_received: AtomicU64,
    /// Events from successful step executions
    success_events: AtomicU64,
    /// Events from failed step executions
    failure_events: AtomicU64,
    /// Event count by namespace
    by_namespace: RwLock<HashMap<String, u64>>,
    /// Event count by event name
    by_name: RwLock<HashMap<String, u64>>,
    /// Timestamp of last event
    last_event_at: RwLock<Option<DateTime<Utc>>>,
}

impl EventMetricsCollector {
    /// Create a new metrics collector wrapped in Arc
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events_received: AtomicU64::new(0),
            success_events: AtomicU64::new(0),
            failure_events: AtomicU64::new(0),
            by_namespace: RwLock::new(HashMap::new()),
            by_name: RwLock::new(HashMap::new()),
            last_event_at: RwLock::new(None),
        })
    }

    /// Create an event handler for this collector
    ///
    /// The returned handler can be registered with an InProcessEventBus.
    pub fn create_handler(self: &Arc<Self>) -> EventHandler {
        let metrics = Arc::clone(self);

        Arc::new(move |event| {
            let metrics = Arc::clone(&metrics);

            Box::pin(async move {
                // Increment total count
                metrics.events_received.fetch_add(1, Ordering::Relaxed);

                // Check if step succeeded or failed
                let succeeded = event.payload.execution_result.success;
                if succeeded {
                    metrics.success_events.fetch_add(1, Ordering::Relaxed);
                } else {
                    metrics.failure_events.fetch_add(1, Ordering::Relaxed);
                }

                // Update namespace counter
                {
                    let mut by_ns = metrics.by_namespace.write().unwrap();
                    *by_ns.entry(event.metadata.namespace.clone()).or_default() += 1;
                }

                // Update event name counter
                {
                    let mut by_name = metrics.by_name.write().unwrap();
                    *by_name.entry(event.event_name.clone()).or_default() += 1;
                }

                // Update last event timestamp
                {
                    let mut last = metrics.last_event_at.write().unwrap();
                    *last = Some(event.metadata.fired_at);
                }

                debug!(
                    event_name = %event.event_name,
                    total = metrics.events_received.load(Ordering::Relaxed),
                    "Metrics updated for domain event"
                );

                Ok(())
            })
        })
    }

    /// Get total events received
    pub fn events_received(&self) -> u64 {
        self.events_received.load(Ordering::Relaxed)
    }

    /// Get count of success events
    pub fn success_events(&self) -> u64 {
        self.success_events.load(Ordering::Relaxed)
    }

    /// Get count of failure events
    pub fn failure_events(&self) -> u64 {
        self.failure_events.load(Ordering::Relaxed)
    }

    /// Get event counts by namespace
    pub fn events_by_namespace(&self) -> HashMap<String, u64> {
        self.by_namespace.read().unwrap().clone()
    }

    /// Get event counts by event name
    pub fn events_by_name(&self) -> HashMap<String, u64> {
        self.by_name.read().unwrap().clone()
    }

    /// Get event count for a specific namespace
    pub fn namespace_count(&self, namespace: &str) -> u64 {
        self.by_namespace
            .read()
            .unwrap()
            .get(namespace)
            .copied()
            .unwrap_or(0)
    }

    /// Get event count for a specific event name
    pub fn event_name_count(&self, event_name: &str) -> u64 {
        self.by_name
            .read()
            .unwrap()
            .get(event_name)
            .copied()
            .unwrap_or(0)
    }

    /// Get the timestamp of the last event
    pub fn last_event_at(&self) -> Option<DateTime<Utc>> {
        *self.last_event_at.read().unwrap()
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.events_received.store(0, Ordering::Relaxed);
        self.success_events.store(0, Ordering::Relaxed);
        self.failure_events.store(0, Ordering::Relaxed);
        self.by_namespace.write().unwrap().clear();
        self.by_name.write().unwrap().clear();
        *self.last_event_at.write().unwrap() = None;
    }

    /// Generate a summary report
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            events_received: self.events_received(),
            success_events: self.success_events(),
            failure_events: self.failure_events(),
            by_namespace: self.events_by_namespace(),
            by_name: self.events_by_name(),
            last_event_at: self.last_event_at(),
        }
    }
}

/// Summary of collected metrics
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub events_received: u64,
    pub success_events: u64,
    pub failure_events: u64,
    pub by_namespace: HashMap<String, u64>,
    pub by_name: HashMap<String, u64>,
    pub last_event_at: Option<DateTime<Utc>>,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Event Metrics Summary")?;
        writeln!(f, "  Total received: {}", self.events_received)?;
        writeln!(f, "  Success events: {}", self.success_events)?;
        writeln!(f, "  Failure events: {}", self.failure_events)?;
        writeln!(f, "  By namespace:")?;
        for (ns, count) in &self.by_namespace {
            writeln!(f, "    {}: {}", ns, count)?;
        }
        writeln!(f, "  By event name:")?;
        for (name, count) in &self.by_name {
            writeln!(f, "    {}: {}", name, count)?;
        }
        if let Some(last) = self.last_event_at {
            writeln!(f, "  Last event at: {}", last)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tasker_shared::events::domain_events::{DomainEvent, DomainEventPayload, EventMetadata};
    use tasker_shared::messaging::execution_types::{
        StepExecutionError, StepExecutionMetadata, StepExecutionResult,
    };
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::types::base::TaskSequenceStep;
    use uuid::Uuid;

    fn create_test_event(name: &str, namespace: &str, success: bool) -> DomainEvent {
        let task_uuid = Uuid::new_v4();

        DomainEvent {
            event_id: Uuid::new_v4(),
            event_name: name.to_string(),
            event_version: "1.0".to_string(),
            payload: DomainEventPayload {
                task_sequence_step: TaskSequenceStep {
                    task: TaskForOrchestration {
                        task: Task {
                            task_uuid,
                            named_task_uuid: Uuid::new_v4(),
                            complete: false,
                            requested_at: Utc::now().naive_utc(),
                            initiator: Some("test".to_string()),
                            source_system: None,
                            reason: None,
                            bypass_steps: None,
                            tags: None,
                            context: Some(json!({})),
                            identity_hash: "test".to_string(),
                            priority: 5,
                            created_at: Utc::now().naive_utc(),
                            updated_at: Utc::now().naive_utc(),
                            correlation_id: Uuid::new_v4(),
                            parent_correlation_id: None,
                        },
                        task_name: "test_task".to_string(),
                        task_version: "1.0".to_string(),
                        namespace_name: namespace.to_string(),
                    },
                    workflow_step: WorkflowStepWithName {
                        workflow_step_uuid: Uuid::new_v4(),
                        task_uuid,
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
                    },
                    dependency_results: HashMap::new(),
                    step_definition: StepDefinition {
                        name: "test_step".to_string(),
                        description: None,
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
                },
                execution_result: StepExecutionResult {
                    step_uuid: Uuid::new_v4(),
                    success,
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
                    status: if success { "completed" } else { "error" }.to_string(),
                    error: if success {
                        None
                    } else {
                        Some(
                            StepExecutionError::new("Test error".to_string(), true)
                                .with_error_type("TestError".to_string()),
                        )
                    },
                    orchestration_metadata: None,
                },
                payload: json!({"order_id": "test-001"}),
            },
            metadata: EventMetadata {
                task_uuid,
                step_uuid: Some(Uuid::new_v4()),
                step_name: Some("test_step".to_string()),
                namespace: namespace.to_string(),
                correlation_id: Uuid::new_v4(),
                fired_at: Utc::now(),
                fired_by: "test".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_metrics_collector_counts_events() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        // Send a success event
        let event = create_test_event("payment.processed", "payments", true);
        handler(event).await.unwrap();

        assert_eq!(metrics.events_received(), 1);
        assert_eq!(metrics.success_events(), 1);
        assert_eq!(metrics.failure_events(), 0);
    }

    #[tokio::test]
    async fn test_metrics_collector_tracks_failures() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        // Send a failure event
        let event = create_test_event("payment.failed", "payments", false);
        handler(event).await.unwrap();

        assert_eq!(metrics.events_received(), 1);
        assert_eq!(metrics.success_events(), 0);
        assert_eq!(metrics.failure_events(), 1);
    }

    #[tokio::test]
    async fn test_metrics_collector_tracks_by_namespace() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        // Send events from different namespaces
        handler(create_test_event("e1", "payments", true))
            .await
            .unwrap();
        handler(create_test_event("e2", "payments", true))
            .await
            .unwrap();
        handler(create_test_event("e3", "inventory", true))
            .await
            .unwrap();

        assert_eq!(metrics.namespace_count("payments"), 2);
        assert_eq!(metrics.namespace_count("inventory"), 1);
        assert_eq!(metrics.namespace_count("unknown"), 0);
    }

    #[tokio::test]
    async fn test_metrics_collector_tracks_by_event_name() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        // Send events with different names
        handler(create_test_event("order.created", "orders", true))
            .await
            .unwrap();
        handler(create_test_event("order.created", "orders", true))
            .await
            .unwrap();
        handler(create_test_event("order.completed", "orders", true))
            .await
            .unwrap();

        assert_eq!(metrics.event_name_count("order.created"), 2);
        assert_eq!(metrics.event_name_count("order.completed"), 1);
    }

    #[tokio::test]
    async fn test_metrics_collector_reset() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        handler(create_test_event("test.event", "test", true))
            .await
            .unwrap();

        assert_eq!(metrics.events_received(), 1);

        metrics.reset();

        assert_eq!(metrics.events_received(), 0);
        assert_eq!(metrics.success_events(), 0);
        assert_eq!(metrics.failure_events(), 0);
        assert!(metrics.events_by_namespace().is_empty());
        assert!(metrics.events_by_name().is_empty());
    }

    #[tokio::test]
    async fn test_metrics_summary() {
        let metrics = EventMetricsCollector::new();
        let handler = metrics.create_handler();

        handler(create_test_event("payment.processed", "payments", true))
            .await
            .unwrap();
        handler(create_test_event("payment.failed", "payments", false))
            .await
            .unwrap();

        let summary = metrics.summary();

        assert_eq!(summary.events_received, 2);
        assert_eq!(summary.success_events, 1);
        assert_eq!(summary.failure_events, 1);
        assert!(summary.last_event_at.is_some());
    }
}
