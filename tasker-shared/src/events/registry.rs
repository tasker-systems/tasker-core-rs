//! # Event Registry (TAS-65 Phase 2.3a)
//!
//! Pattern-matching event subscription and dispatch system for domain events.
//! Supports exact matches, wildcard patterns, and global subscriptions.
//!
//! ## Features
//!
//! - **Pattern Matching**: Subscribe with exact names or wildcards (`payment.*`, `*`)
//! - **Concurrent Dispatch**: Handlers execute in parallel via `futures::join_all`
//! - **Error Collection**: All handlers execute even if some fail
//! - **Type Safety**: Strong typing for handlers and errors
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_shared::events::registry::{EventRegistry, EventHandler};
//! use tasker_shared::events::domain_events::{DomainEvent, DomainEventPayload, EventMetadata};
//! use tasker_shared::types::base::TaskSequenceStep;
//! use tasker_shared::messaging::execution_types::StepExecutionResult;
//! use std::sync::Arc;
//! use serde_json::json;
//!
//! # async fn example(
//! #     tss: TaskSequenceStep,
//! #     result: StepExecutionResult,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let mut registry = EventRegistry::new();
//!
//! // Subscribe to specific event
//! let handler = Arc::new(|event: DomainEvent| {
//!     Box::pin(async move {
//!         println!("Handling: {}", event.event_name);
//!         Ok(())
//!     }) as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), _>> + Send>>
//! });
//! registry.subscribe("payment.charged", handler)?;
//!
//! // Subscribe with wildcard
//! let wildcard_handler = Arc::new(|event: DomainEvent| {
//!     Box::pin(async move {
//!         println!("Wildcard: {}", event.event_name);
//!         Ok(())
//!     }) as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), _>> + Send>>
//! });
//! registry.subscribe("payment.*", wildcard_handler)?;
//!
//! // Dispatch event (both handlers will execute)
//! # let event = DomainEvent {
//! #     event_id: uuid::Uuid::new_v4(),
//! #     event_name: "payment.charged".to_string(),
//! #     event_version: "1.0".to_string(),
//! #     payload: DomainEventPayload {
//! #         task_sequence_step: tss,
//! #         execution_result: result,
//! #         payload: json!({}),
//! #     },
//! #     metadata: EventMetadata {
//! #         task_uuid: uuid::Uuid::new_v4(),
//! #         step_uuid: None,
//! #         step_name: None,
//! #         namespace: "test".to_string(),
//! #         correlation_id: uuid::Uuid::new_v4(),
//! #         fired_at: chrono::Utc::now(),
//! #         fired_by: "test".to_string(),
//! #     },
//! # };
//! let errors = registry.dispatch(&event).await;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::future::join_all;
use thiserror::Error;

use super::domain_events::DomainEvent;

/// Event handler function type
///
/// Handlers are async functions that process domain events. They must be:
/// - `Send + Sync` for thread-safe sharing
/// - Return a pinned future for async execution
/// - Return `Result<(), EventHandlerError>` for error handling
pub type EventHandler = Arc<
    dyn Fn(DomainEvent) -> Pin<Box<dyn Future<Output = Result<(), EventHandlerError>> + Send>>
        + Send
        + Sync,
>;

/// Errors that can occur during event registry operations
#[derive(Debug, Clone, Error)]
pub enum RegistryError {
    /// Invalid pattern syntax
    #[error("Invalid event pattern '{pattern}': {reason}")]
    InvalidPattern { pattern: String, reason: String },

    /// Duplicate subscription (currently not enforced, reserved for future use)
    #[error("Duplicate subscription for pattern '{pattern}'")]
    DuplicateSubscription { pattern: String },
}

/// Errors that can occur during event handler execution
#[derive(Debug, Clone, Error)]
pub enum EventHandlerError {
    /// Handler execution failed
    #[error("Handler for pattern '{pattern}' failed on event '{event_name}': {reason}")]
    ExecutionFailed {
        event_name: String,
        pattern: String,
        reason: String,
    },

    /// Handler panicked (caught via panic handling)
    #[error("Handler for pattern '{pattern}' panicked on event '{event_name}'")]
    HandlerPanicked { event_name: String, pattern: String },

    /// Generic error for handler failures
    #[error("Handler error: {0}")]
    Generic(String),
}

/// Event registry with pattern-matching subscription system
///
/// Manages event handler subscriptions and dispatches events to matching handlers.
/// Supports exact matches, wildcard patterns, and concurrent handler execution.
///
/// ## Pattern Matching Rules
///
/// - **Exact match**: `"payment.charged"` matches only `"payment.charged"`
/// - **Wildcard**: `"payment.*"` matches `"payment.charged"`, `"payment.failed"`, etc.
/// - **Global**: `"*"` matches all events
///
/// ## Concurrency
///
/// All matching handlers execute concurrently using `futures::join_all`.
/// Errors from individual handlers are collected but don't stop other handlers.
#[derive(Default)]
pub struct EventRegistry {
    /// Map of patterns to handlers
    subscriptions: HashMap<String, Vec<EventHandler>>,
}

impl EventRegistry {
    /// Create a new event registry
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    /// Subscribe a handler to an event pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - Event name pattern (exact, wildcard, or global)
    /// * `handler` - Event handler function
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, `Err(RegistryError)` if pattern is invalid
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::events::registry::EventRegistry;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut registry = EventRegistry::new();
    ///
    /// // Exact match
    /// let handler = Arc::new(|event| Box::pin(async move { Ok(()) }) as _);
    /// registry.subscribe("payment.charged", handler)?;
    ///
    /// // Wildcard match
    /// let handler2 = Arc::new(|event| Box::pin(async move { Ok(()) }) as _);
    /// registry.subscribe("payment.*", handler2)?;
    ///
    /// // Global match
    /// let handler3 = Arc::new(|event| Box::pin(async move { Ok(()) }) as _);
    /// registry.subscribe("*", handler3)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn subscribe(&mut self, pattern: &str, handler: EventHandler) -> Result<(), RegistryError> {
        // Validate pattern (basic validation)
        if pattern.is_empty() {
            return Err(RegistryError::InvalidPattern {
                pattern: pattern.to_string(),
                reason: "Pattern cannot be empty".to_string(),
            });
        }

        // Add handler to pattern's handler list
        self.subscriptions
            .entry(pattern.to_string())
            .or_default()
            .push(handler);

        Ok(())
    }

    /// Dispatch an event to all matching handlers
    ///
    /// Finds all handlers whose patterns match the event name, then executes them
    /// concurrently. Returns a vector of errors from failed handlers.
    ///
    /// # Arguments
    ///
    /// * `event` - The domain event to dispatch
    ///
    /// # Returns
    ///
    /// Vector of errors from failed handlers (empty if all succeeded)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tasker_shared::events::registry::EventRegistry;
    /// # use tasker_shared::events::domain_events::DomainEvent;
    /// # async fn example(registry: &EventRegistry, event: &DomainEvent) {
    /// let errors = registry.dispatch(event).await;
    /// if !errors.is_empty() {
    ///     eprintln!("Some handlers failed: {:?}", errors);
    /// }
    /// # }
    /// ```
    pub async fn dispatch(&self, event: &DomainEvent) -> Vec<EventHandlerError> {
        // Find all matching handlers
        let mut handlers_to_execute = Vec::new();

        for (pattern, handlers) in &self.subscriptions {
            if self.pattern_matches(pattern, &event.event_name) {
                for handler in handlers {
                    handlers_to_execute.push((pattern.clone(), handler.clone()));
                }
            }
        }

        // Execute all handlers concurrently
        let futures: Vec<_> = handlers_to_execute
            .into_iter()
            .map(|(_pattern, handler)| {
                let event_clone = event.clone();
                async move {
                    // Execute handler and convert errors
                    (handler(event_clone.clone()).await).err()
                }
            })
            .collect();

        // Wait for all handlers to complete
        let results = join_all(futures).await;

        // Collect errors
        results.into_iter().flatten().collect()
    }

    /// Check if a pattern matches an event name
    ///
    /// # Pattern Matching Logic
    ///
    /// - If pattern is `"*"`: matches all events
    /// - If pattern contains `"*"`: split on `.`, compare parts, `*` matches any part
    /// - Otherwise: exact string match
    ///
    /// # Arguments
    ///
    /// * `pattern` - The subscription pattern
    /// * `event_name` - The event name to test
    ///
    /// # Returns
    ///
    /// `true` if pattern matches event name, `false` otherwise
    fn pattern_matches(&self, pattern: &str, event_name: &str) -> bool {
        // Global wildcard matches everything
        if pattern == "*" {
            return true;
        }

        // If pattern contains wildcard, do wildcard matching
        if pattern.contains('*') {
            let pattern_parts: Vec<&str> = pattern.split('.').collect();
            let event_parts: Vec<&str> = event_name.split('.').collect();

            // Must have same number of parts
            if pattern_parts.len() != event_parts.len() {
                return false;
            }

            // Compare each part
            pattern_parts
                .iter()
                .zip(event_parts.iter())
                .all(|(p, e)| p == &"*" || p == e)
        } else {
            // Exact match
            pattern == event_name
        }
    }

    /// Get the number of registered patterns
    pub fn pattern_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Get the total number of handlers across all patterns
    pub fn handler_count(&self) -> usize {
        self.subscriptions.values().map(Vec::len).sum()
    }

    /// Clear all subscriptions
    pub fn clear(&mut self) {
        self.subscriptions.clear();
    }
}

// Manual Debug implementation for EventRegistry
impl std::fmt::Debug for EventRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRegistry")
            .field("pattern_count", &self.pattern_count())
            .field("handler_count", &self.handler_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    use crate::events::domain_events::{DomainEventPayload, EventMetadata};
    use crate::messaging::execution_types::{StepExecutionMetadata, StepExecutionResult};
    use crate::models::core::task::{Task, TaskForOrchestration};
    use crate::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use crate::models::core::workflow_step::WorkflowStepWithName;
    use crate::types::base::TaskSequenceStep;
    // HashMap imported via super::*

    /// Helper to create a minimal test payload for registry testing
    fn create_minimal_test_payload() -> DomainEventPayload {
        // Minimal TaskSequenceStep for testing
        let task_sequence_step = TaskSequenceStep {
            task: TaskForOrchestration {
                task: Task {
                    task_uuid: Uuid::new_v4(),
                    named_task_uuid: Uuid::new_v4(),
                    complete: false,
                    requested_at: chrono::Utc::now().naive_utc(),
                    initiator: Some("test".to_string()),
                    source_system: None,
                    reason: None,
                    bypass_steps: None,
                    tags: None,
                    context: Some(json!({})),
                    identity_hash: "test_hash".to_string(),
                    priority: 5,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
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
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
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

        // Minimal StepExecutionResult for testing
        let execution_result = StepExecutionResult {
            step_uuid: Uuid::new_v4(),
            success: true,
            result: json!({}),
            metadata: StepExecutionMetadata {
                execution_time_ms: 0,
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
            payload: json!({}),
        }
    }

    /// Helper to create a test event
    fn create_test_event(event_name: &str) -> DomainEvent {
        DomainEvent {
            event_id: Uuid::new_v4(),
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(),
            payload: create_minimal_test_payload(),
            metadata: EventMetadata {
                task_uuid: Uuid::new_v4(),
                step_uuid: None,
                step_name: None,
                namespace: "test".to_string(),
                correlation_id: Uuid::new_v4(),
                fired_at: Utc::now(),
                fired_by: "test_handler".to_string(),
            },
        }
    }

    /// Helper to create a success handler with counter
    fn create_counting_handler(counter: Arc<AtomicUsize>) -> EventHandler {
        Arc::new(move |_event| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        })
    }

    /// Helper to create a failing handler
    fn create_failing_handler(error_msg: &str) -> EventHandler {
        let msg = error_msg.to_string();
        Arc::new(move |event| {
            let msg = msg.clone();
            Box::pin(async move {
                Err(EventHandlerError::ExecutionFailed {
                    event_name: event.event_name.clone(),
                    pattern: "test_pattern".to_string(),
                    reason: msg,
                })
            })
        })
    }

    #[test]
    fn test_new_registry() {
        let registry = EventRegistry::new();
        assert_eq!(registry.pattern_count(), 0);
        assert_eq!(registry.handler_count(), 0);
    }

    #[test]
    fn test_subscribe_exact_pattern() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        let result = registry.subscribe("payment.charged", handler);
        assert!(result.is_ok());
        assert_eq!(registry.pattern_count(), 1);
        assert_eq!(registry.handler_count(), 1);
    }

    #[test]
    fn test_subscribe_wildcard_pattern() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        let result = registry.subscribe("payment.*", handler);
        assert!(result.is_ok());
        assert_eq!(registry.pattern_count(), 1);
    }

    #[test]
    fn test_subscribe_global_pattern() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        let result = registry.subscribe("*", handler);
        assert!(result.is_ok());
        assert_eq!(registry.pattern_count(), 1);
    }

    #[test]
    fn test_subscribe_empty_pattern() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        let result = registry.subscribe("", handler);
        assert!(result.is_err());

        match result {
            Err(RegistryError::InvalidPattern { pattern, reason }) => {
                assert_eq!(pattern, "");
                assert!(reason.contains("empty"));
            }
            _ => panic!("Expected InvalidPattern error"),
        }
    }

    #[test]
    fn test_multiple_handlers_same_pattern() {
        let mut registry = EventRegistry::new();
        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        let handler1 = create_counting_handler(counter1);
        let handler2 = create_counting_handler(counter2);

        registry.subscribe("payment.charged", handler1).unwrap();
        registry.subscribe("payment.charged", handler2).unwrap();

        assert_eq!(registry.pattern_count(), 1);
        assert_eq!(registry.handler_count(), 2);
    }

    #[tokio::test]
    async fn test_dispatch_exact_match() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter.clone());

        registry.subscribe("payment.charged", handler).unwrap();

        let event = create_test_event("payment.charged");
        let errors = registry.dispatch(&event).await;

        assert!(errors.is_empty());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dispatch_no_match() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter.clone());

        registry.subscribe("payment.charged", handler).unwrap();

        let event = create_test_event("order.created");
        let errors = registry.dispatch(&event).await;

        assert!(errors.is_empty());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_dispatch_wildcard_match() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter.clone());

        registry.subscribe("payment.*", handler).unwrap();

        // Should match
        let event1 = create_test_event("payment.charged");
        let errors1 = registry.dispatch(&event1).await;
        assert!(errors1.is_empty());

        let event2 = create_test_event("payment.failed");
        let errors2 = registry.dispatch(&event2).await;
        assert!(errors2.is_empty());

        assert_eq!(counter.load(Ordering::SeqCst), 2);

        // Should not match
        let event3 = create_test_event("order.created");
        let errors3 = registry.dispatch(&event3).await;
        assert!(errors3.is_empty());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_dispatch_global_wildcard() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter.clone());

        registry.subscribe("*", handler).unwrap();

        // Should match everything
        let event1 = create_test_event("payment.charged");
        registry.dispatch(&event1).await;

        let event2 = create_test_event("order.created");
        registry.dispatch(&event2).await;

        let event3 = create_test_event("user.registered");
        registry.dispatch(&event3).await;

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_dispatch_multiple_matching_patterns() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Three patterns that will all match "payment.charged"
        let handler1 = create_counting_handler(counter.clone());
        let handler2 = create_counting_handler(counter.clone());
        let handler3 = create_counting_handler(counter.clone());

        registry.subscribe("payment.charged", handler1).unwrap(); // Exact
        registry.subscribe("payment.*", handler2).unwrap(); // Wildcard
        registry.subscribe("*", handler3).unwrap(); // Global

        let event = create_test_event("payment.charged");
        let errors = registry.dispatch(&event).await;

        assert!(errors.is_empty());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_dispatch_handler_failure() {
        let mut registry = EventRegistry::new();
        let handler = create_failing_handler("intentional failure");

        registry.subscribe("payment.charged", handler).unwrap();

        let event = create_test_event("payment.charged");
        let errors = registry.dispatch(&event).await;

        assert_eq!(errors.len(), 1);
        match &errors[0] {
            EventHandlerError::ExecutionFailed { reason, .. } => {
                assert!(reason.contains("intentional failure"));
            }
            _ => panic!("Expected ExecutionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_dispatch_partial_failure() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Mix of successful and failing handlers
        let success_handler = create_counting_handler(counter.clone());
        let failing_handler = create_failing_handler("test failure");

        registry
            .subscribe("payment.charged", success_handler)
            .unwrap();
        registry
            .subscribe("payment.charged", failing_handler)
            .unwrap();

        let event = create_test_event("payment.charged");
        let errors = registry.dispatch(&event).await;

        // One handler should succeed, one should fail
        assert_eq!(errors.len(), 1);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dispatch_concurrent_execution() {
        use std::time::Duration;
        use tokio::time::sleep;

        let mut registry = EventRegistry::new();

        // Create handlers that sleep for different durations
        let handler1 = Arc::new(|_event: DomainEvent| {
            Box::pin(async move {
                sleep(Duration::from_millis(50)).await;
                Ok(())
            }) as Pin<Box<dyn Future<Output = Result<(), EventHandlerError>> + Send>>
        });

        let handler2 = Arc::new(|_event: DomainEvent| {
            Box::pin(async move {
                sleep(Duration::from_millis(50)).await;
                Ok(())
            }) as Pin<Box<dyn Future<Output = Result<(), EventHandlerError>> + Send>>
        });

        registry.subscribe("test.event", handler1).unwrap();
        registry.subscribe("test.event", handler2).unwrap();

        let event = create_test_event("test.event");

        // If concurrent, should take ~50ms. If sequential, would take ~100ms.
        let start = std::time::Instant::now();
        let errors = registry.dispatch(&event).await;
        let elapsed = start.elapsed();

        assert!(errors.is_empty());
        assert!(
            elapsed < Duration::from_millis(80),
            "Handlers should execute concurrently, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_pattern_matching_edge_cases() {
        let registry = EventRegistry::new();

        // Test exact matches
        assert!(registry.pattern_matches("payment.charged", "payment.charged"));
        assert!(!registry.pattern_matches("payment.charged", "payment.failed"));

        // Test wildcard matches
        assert!(registry.pattern_matches("payment.*", "payment.charged"));
        assert!(registry.pattern_matches("payment.*", "payment.failed"));
        assert!(!registry.pattern_matches("payment.*", "order.created"));

        // Test global wildcard
        assert!(registry.pattern_matches("*", "anything"));
        assert!(registry.pattern_matches("*", "payment.charged"));

        // Test multi-part wildcards
        assert!(registry.pattern_matches("*.charged", "payment.charged"));
        assert!(!registry.pattern_matches("*.charged", "payment.failed"));

        // Test different depths
        assert!(!registry.pattern_matches("payment.*", "payment"));
        assert!(!registry.pattern_matches("payment.*", "payment.charged.now"));
    }

    #[test]
    fn test_clear_registry() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        registry.subscribe("payment.charged", handler).unwrap();
        assert_eq!(registry.pattern_count(), 1);

        registry.clear();
        assert_eq!(registry.pattern_count(), 0);
        assert_eq!(registry.handler_count(), 0);
    }

    #[test]
    fn test_debug_impl() {
        let mut registry = EventRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let handler = create_counting_handler(counter);

        registry.subscribe("payment.*", handler).unwrap();

        let debug_str = format!("{:?}", registry);
        assert!(debug_str.contains("EventRegistry"));
        assert!(debug_str.contains("pattern_count"));
        assert!(debug_str.contains("handler_count"));
    }
}
