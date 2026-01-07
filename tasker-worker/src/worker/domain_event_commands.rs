//! # TAS-65/TAS-69: Domain Event Commands
//!
//! Command pattern for fire-and-forget domain event publishing.
//! Uses `try_send()` to ensure workflow steps never block on event publishing.
//!
//! ## Architecture
//!
//! ```text
//! command_processor.rs
//!       |
//!       | try_send(DispatchEvents)  <-- fire-and-forget, never blocks
//!       v
//! DomainEventSystem
//!       |
//!       v
//! StepEventPublisher.publish(ctx)
//!       |
//!       v
//! EventRouter → Durable (PGMQ) / Fast (InProcess)
//! ```
//!
//! ## Key Design Decisions
//!
//! 1. **Fire-and-forget with `try_send()`**: Commands are dispatched without waiting
//!    for acknowledgment. If the channel is full, events are dropped with metrics.
//!
//! 2. **No oneshot response channels**: Unlike other commands that need responses,
//!    domain event dispatch is purely fire-and-forget. Stats queries use oneshot.
//!
//! 3. **Step-level publishing**: Events are collected during step execution and
//!    dispatched as a batch after completion.

use serde_json::Value;
use tokio::sync::oneshot;
use uuid::Uuid;

use tasker_shared::events::domain_events::EventMetadata;
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::models::core::task_template::EventDeliveryMode;
use tasker_shared::types::base::TaskSequenceStep;

// Re-export from canonical location in tasker-shared
pub use tasker_shared::metrics::worker::{
    DomainEventShutdownResult as ShutdownResult, DomainEventSystemStats,
};

/// A domain event ready to be published
#[derive(Debug, Clone)]
pub struct DomainEventToPublish {
    /// Event name in dot notation (e.g., "order.processed")
    pub event_name: String,
    /// How to deliver the event (Durable or Fast)
    pub delivery_mode: EventDeliveryMode,
    /// Business-specific event payload
    pub business_payload: Value,
    /// Event metadata for correlation and tracing
    pub metadata: EventMetadata,
    /// The complete task sequence step context
    pub task_sequence_step: TaskSequenceStep,
    /// The execution result from the step
    pub execution_result: StepExecutionResult,
}

/// Commands for the DomainEventSystem
///
/// These commands are processed asynchronously by the DomainEventSystem.
/// The `DispatchEvents` command uses fire-and-forget semantics (no response).
#[derive(Debug)]
pub enum DomainEventCommand {
    /// Dispatch domain events after step completion (fire-and-forget)
    ///
    /// This command has no response channel - it's purely fire-and-forget.
    /// If the command cannot be sent (channel full), events are dropped
    /// and a metric is recorded.
    DispatchEvents {
        /// Events to publish
        events: Vec<DomainEventToPublish>,
        /// Name of the publisher to use (for custom publisher support)
        publisher_name: String,
        /// Correlation ID for tracing
        correlation_id: Uuid,
    },

    /// Get system statistics
    GetStats {
        /// Response channel for stats
        resp: oneshot::Sender<DomainEventSystemStats>,
    },

    /// Shutdown the domain event system
    ///
    /// Drains any pending fast events up to the configured timeout,
    /// then stops processing. Durable events are already persisted.
    Shutdown {
        /// Response when shutdown is complete
        resp: oneshot::Sender<ShutdownResult>,
    },
}

// DomainEventSystemStats and ShutdownResult are re-exported from tasker_shared::metrics::worker

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    use tasker_shared::messaging::execution_types::StepExecutionMetadata;
    use tasker_shared::models::core::task::{Task, TaskForOrchestration};
    use tasker_shared::models::core::task_template::{
        HandlerDefinition, RetryConfiguration, StepDefinition,
    };
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;

    fn create_test_event() -> DomainEventToPublish {
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
            delivery_mode: EventDeliveryMode::Fast,
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
    fn test_domain_event_to_publish() {
        let event = create_test_event();
        assert_eq!(event.event_name, "test.event");
        assert!(matches!(event.delivery_mode, EventDeliveryMode::Fast));
    }

    #[test]
    fn test_stats_default() {
        let stats = DomainEventSystemStats::default();
        assert_eq!(stats.events_dispatched, 0);
        assert_eq!(stats.events_published, 0);
        assert_eq!(stats.events_dropped, 0);
        assert!(stats.last_event_at.is_none());
    }

    #[test]
    fn test_shutdown_result() {
        let result = ShutdownResult {
            success: true,
            events_drained: 5,
            duration_ms: 100,
        };
        assert!(result.success);
        assert_eq!(result.events_drained, 5);
    }
}
