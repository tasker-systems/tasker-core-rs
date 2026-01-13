//! # Fast Event Test Helper
//!
//! TAS-65: Test helper for capturing fast/in-process domain events.
//!
//! ## Important Limitation
//!
//! Fast events are dispatched in-memory via the `InProcessEventBus`.
//! They are ONLY observable in integration tests where the test runs
//! in the same memory space as the worker.
//!
//! Fast events are NOT observable in E2E tests where workers run in
//! Docker containers (different processes/memory spaces).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tests::common::fast_event_test_helper::FastEventCapture;
//!
//! // Create capture with a bus instance
//! let capture = FastEventCapture::new();
//!
//! // Register capture handler with the worker's in-process bus
//! bus.subscribe("*", capture.create_handler()).unwrap();
//!
//! // Run your workflow...
//!
//! // Check captured events
//! assert!(capture.event_count() > 0);
//! let events = capture.get_captured_events().await;
//! assert!(events.iter().any(|e| e.event_name == "order.created"));
//! ```

#![expect(
    dead_code,
    reason = "Test module for capturing fast/in-process domain events"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tasker_shared::events::domain_events::DomainEvent;
use tasker_shared::events::registry::EventHandler;
use tokio::sync::RwLock;

/// Captures fast/in-process domain events for test verification
///
/// ## Key Features
///
/// - Thread-safe event collection using Arc<RwLock<...>>
/// - Atomic counter for quick event count checks
/// - Event filtering by name pattern
/// - Works only in integration tests (same memory space)
pub struct FastEventCapture {
    /// Captured events storage
    captured_events: Arc<RwLock<Vec<DomainEvent>>>,
    /// Quick event counter
    event_count: Arc<AtomicUsize>,
}

impl Default for FastEventCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl FastEventCapture {
    /// Create a new fast event capture
    pub fn new() -> Self {
        Self {
            captured_events: Arc::new(RwLock::new(Vec::new())),
            event_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create an event handler that captures events to this instance
    ///
    /// Register this handler with the `InProcessEventBus`:
    /// ```rust,ignore
    /// bus.subscribe("*", capture.create_handler()).unwrap();
    /// ```
    pub fn create_handler(&self) -> EventHandler {
        let captured = Arc::clone(&self.captured_events);
        let counter = Arc::clone(&self.event_count);

        Arc::new(move |event| {
            let captured = Arc::clone(&captured);
            let counter = Arc::clone(&counter);

            Box::pin(async move {
                // Store the event
                captured.write().await.push(event);

                // Increment counter
                counter.fetch_add(1, Ordering::SeqCst);

                Ok(())
            })
        })
    }

    /// Get the number of captured events
    pub fn event_count(&self) -> usize {
        self.event_count.load(Ordering::SeqCst)
    }

    /// Get all captured events
    pub async fn get_captured_events(&self) -> Vec<DomainEvent> {
        self.captured_events.read().await.clone()
    }

    /// Get events matching a name pattern
    pub async fn get_events_matching(&self, pattern: &str) -> Vec<DomainEvent> {
        self.captured_events
            .read()
            .await
            .iter()
            .filter(|e| e.event_name.contains(pattern))
            .cloned()
            .collect()
    }

    /// Check if an event with the given name was captured
    pub async fn has_event(&self, event_name: &str) -> bool {
        self.captured_events
            .read()
            .await
            .iter()
            .any(|e| e.event_name == event_name)
    }

    /// Assert that an event matching a pattern was captured
    pub async fn assert_event_captured(&self, pattern: &str) -> Option<DomainEvent> {
        let events = self.get_events_matching(pattern).await;
        if events.is_empty() {
            panic!("Expected event matching '{}' but none found", pattern);
        }
        events.into_iter().next()
    }

    /// Wait for a specific number of events with timeout
    pub async fn wait_for_events(&self, count: usize, timeout_ms: u64) -> bool {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if self.event_count() >= count {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        false
    }

    /// Clear all captured events
    pub async fn clear(&self) {
        self.captured_events.write().await.clear();
        self.event_count.store(0, Ordering::SeqCst);
    }

    /// Get event names captured
    pub async fn event_names(&self) -> Vec<String> {
        self.captured_events
            .read()
            .await
            .iter()
            .map(|e| e.event_name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::collections::HashMap;
    use tasker_shared::events::domain_events::{DomainEventPayload, EventMetadata};
    use tasker_shared::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::types::base::TaskSequenceStep;
    use uuid::Uuid;

    fn create_test_event(name: &str) -> DomainEvent {
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
                        namespace_name: "test".to_string(),
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
                        checkpoint: None,
                        created_at: Utc::now().naive_utc(),
                        updated_at: Utc::now().naive_utc(),
                    },
                    dependency_results: HashMap::new(),
                    step_definition: StepDefinition {
                        name: "test_step".to_string(),
                        description: None,
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
                },
                execution_result: StepExecutionResult {
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
                },
                payload: json!({"order_id": "test-001"}),
            },
            metadata: EventMetadata {
                task_uuid,
                step_uuid: Some(Uuid::new_v4()),
                step_name: Some("test_step".to_string()),
                namespace: "test".to_string(),
                correlation_id: Uuid::new_v4(),
                fired_at: Utc::now(),
                fired_by: "test".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_fast_event_capture_handler() {
        let capture = FastEventCapture::new();
        let handler = capture.create_handler();

        // Simulate event capture
        let event = create_test_event("payment.processed");
        handler(event).await.unwrap();

        assert_eq!(capture.event_count(), 1);
        let events = capture.get_captured_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "payment.processed");
    }

    #[tokio::test]
    async fn test_fast_event_capture_pattern_matching() {
        let capture = FastEventCapture::new();
        let handler = capture.create_handler();

        // Capture multiple events
        handler(create_test_event("payment.processed"))
            .await
            .unwrap();
        handler(create_test_event("payment.failed")).await.unwrap();
        handler(create_test_event("order.created")).await.unwrap();

        assert_eq!(capture.event_count(), 3);

        // Filter by pattern
        let payment_events = capture.get_events_matching("payment").await;
        assert_eq!(payment_events.len(), 2);

        let order_events = capture.get_events_matching("order").await;
        assert_eq!(order_events.len(), 1);
    }

    #[tokio::test]
    async fn test_fast_event_capture_has_event() {
        let capture = FastEventCapture::new();
        let handler = capture.create_handler();

        handler(create_test_event("payment.processed"))
            .await
            .unwrap();

        assert!(capture.has_event("payment.processed").await);
        assert!(!capture.has_event("order.created").await);
    }

    #[tokio::test]
    async fn test_fast_event_capture_clear() {
        let capture = FastEventCapture::new();
        let handler = capture.create_handler();

        handler(create_test_event("event1")).await.unwrap();
        handler(create_test_event("event2")).await.unwrap();

        assert_eq!(capture.event_count(), 2);

        capture.clear().await;

        assert_eq!(capture.event_count(), 0);
        assert!(capture.get_captured_events().await.is_empty());
    }
}
