//! # Logging Subscriber
//!
//! TAS-65: Example event subscriber that logs all domain events.
//!
//! ## Purpose
//!
//! Demonstrates how to create a fast/in-process event subscriber for logging.
//! This subscriber logs event details using the tracing framework, making
//! it useful for debugging and audit trails.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_worker_rust::event_subscribers::logging_subscriber::create_logging_subscriber;
//! use tasker_worker::worker::in_process_event_bus::InProcessEventBus;
//!
//! let mut bus = InProcessEventBus::new(config);
//!
//! // Subscribe to all events
//! bus.subscribe("*", create_logging_subscriber("[ALL]")).unwrap();
//!
//! // Or subscribe to specific event patterns
//! bus.subscribe("payment.*", create_logging_subscriber("[PAYMENT]")).unwrap();
//! bus.subscribe("order.*", create_logging_subscriber("[ORDER]")).unwrap();
//! ```
//!
//! ## Output
//!
//! Events are logged at INFO level with structured fields:
//! ```text
//! INFO [PAYMENT] Domain event received
//!   event_name: payment.processed
//!   event_id: 550e8400-e29b-41d4-a716-446655440000
//!   task_uuid: 6ba7b810-9dad-11d1-80b4-00c04fd430c8
//!   step_name: process_payment
//!   correlation_id: 7c9e6679-7425-40de-944b-e07fc1f90ae7
//! ```

use std::sync::Arc;
use tasker_shared::events::registry::EventHandler;
use tracing::info;

/// Create a logging subscriber that logs all events matching a pattern
///
/// # Arguments
///
/// * `prefix` - A prefix to identify this subscriber in logs (e.g., "\[PAYMENT\]")
///
/// # Returns
///
/// An `EventHandler` that can be registered with the `InProcessEventBus`
///
/// # Example
///
/// ```rust,ignore
/// let handler = create_logging_subscriber("[METRICS]");
/// bus.subscribe("*", handler).unwrap();
/// ```
pub fn create_logging_subscriber(prefix: &str) -> EventHandler {
    let prefix = prefix.to_string();

    Arc::new(move |event| {
        let prefix = prefix.clone();

        Box::pin(async move {
            // Extract step name from metadata if available
            let step_name = event.metadata.step_name.as_deref().unwrap_or("unknown");

            info!(
                prefix = %prefix,
                event_name = %event.event_name,
                event_id = %event.event_id,
                event_version = %event.event_version,
                task_uuid = %event.metadata.task_uuid,
                step_name = %step_name,
                namespace = %event.metadata.namespace,
                correlation_id = %event.metadata.correlation_id,
                fired_at = %event.metadata.fired_at,
                fired_by = %event.metadata.fired_by,
                "Domain event received"
            );

            Ok(())
        })
    })
}

/// Create a logging subscriber with custom log level (debug)
///
/// Same as `create_logging_subscriber` but logs at DEBUG level.
/// Useful for verbose environments where INFO is too noisy.
pub fn create_debug_logging_subscriber(prefix: &str) -> EventHandler {
    let prefix = prefix.to_string();

    Arc::new(move |event| {
        let prefix = prefix.clone();

        Box::pin(async move {
            let step_name = event.metadata.step_name.as_deref().unwrap_or("unknown");

            tracing::debug!(
                prefix = %prefix,
                event_name = %event.event_name,
                event_id = %event.event_id,
                task_uuid = %event.metadata.task_uuid,
                step_name = %step_name,
                namespace = %event.metadata.namespace,
                correlation_id = %event.metadata.correlation_id,
                "Domain event (debug)"
            );

            Ok(())
        })
    })
}

/// Create a logging subscriber that also logs the payload
///
/// More verbose logging that includes the business payload.
/// Use with caution in production as payloads may contain sensitive data.
pub fn create_verbose_logging_subscriber(prefix: &str) -> EventHandler {
    let prefix = prefix.to_string();

    Arc::new(move |event| {
        let prefix = prefix.clone();

        Box::pin(async move {
            let step_name = event.metadata.step_name.as_deref().unwrap_or("unknown");

            // Serialize payload for logging (truncate if too long)
            let payload_str = serde_json::to_string(&event.payload.payload)
                .unwrap_or_else(|_| "<serialization error>".to_string());
            let payload_preview = if payload_str.len() > 500 {
                format!("{}...(truncated)", &payload_str[..500])
            } else {
                payload_str
            };

            info!(
                prefix = %prefix,
                event_name = %event.event_name,
                event_id = %event.event_id,
                task_uuid = %event.metadata.task_uuid,
                step_name = %step_name,
                namespace = %event.metadata.namespace,
                correlation_id = %event.metadata.correlation_id,
                payload = %payload_preview,
                "Domain event with payload"
            );

            Ok(())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::collections::HashMap;
    use tasker_shared::events::domain_events::{DomainEvent, DomainEventPayload, EventMetadata};
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
                        skippable: false,
                        checkpoint: None,
                        created_at: chrono::Utc::now().naive_utc(),
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
    async fn test_logging_subscriber_executes_without_error() {
        let handler = create_logging_subscriber("[TEST]");
        let event = create_test_event("test.event");

        // Should complete without error
        let result = handler(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_debug_logging_subscriber_executes_without_error() {
        let handler = create_debug_logging_subscriber("[DEBUG-TEST]");
        let event = create_test_event("debug.test.event");

        let result = handler(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_verbose_logging_subscriber_executes_without_error() {
        let handler = create_verbose_logging_subscriber("[VERBOSE]");
        let event = create_test_event("verbose.test.event");

        let result = handler(event).await;
        assert!(result.is_ok());
    }
}
