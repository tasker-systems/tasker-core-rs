//! # TAS-40 Worker Event Publisher
//!
//! Specialized event publisher for workers to fire step execution events
//! to FFI handlers using the in-process event system.
//!
//! ## Key Features
//!
//! - **Database-Hydrated Events**: Uses database-as-API pattern to create fully hydrated events
//! - **Ruby-Compatible Context**: Creates execution context matching Ruby expectations
//! - **Command Integration**: Seamlessly works with WorkerProcessor command pattern
//! - **FFI Bridge**: Publishes events that FFI handlers can subscribe to
//! - **Traceability**: Provides correlation IDs and comprehensive logging
//!
//! ## Architecture Integration
//!
//! ```text
//! WorkerProcessor.handle_execute_step()
//!     ↓ (after database hydration)
//! WorkerEventPublisher.fire_step_execution_event()
//!     ↓ (via WorkerEventSystem)
//! FFI Handler (Ruby/Python/WASM) receives StepExecutionEvent
//! ```
//!
//! ## Usage in WorkerProcessor
//!
//! ```rust
//! use tasker_worker::worker::event_publisher::WorkerEventPublisher;
//! use tasker_shared::types::TaskSequenceStep;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Inside WorkerProcessor
//!     let worker_id = "worker-001".to_string();
//!     let event_publisher = WorkerEventPublisher::new(worker_id.clone());
//!
//!     // Example TaskSequenceStep would come from database hydration
//!     // For doctest purposes, we'll create a minimal example
//!     // In practice, this would come from WorkerProcessor's step processing
//!
//!     // Note: TaskSequenceStep creation would typically happen in the processor
//!     // This is just to demonstrate the fire_step_execution_event API
//!
//!     // Uncomment for real usage with TaskSequenceStep:
//!     // event_publisher.fire_step_execution_event(&task_sequence_step).await?;
//!
//!     println!("WorkerEventPublisher created successfully");
//!     Ok(())
//! }
//! ```

use std::sync::Arc;
use tasker_shared::events::{WorkerEventPublisher as SharedEventPublisher, WorkerEventSystem};
use tasker_shared::types::TaskSequenceStep;

use tasker_shared::types::{StepEventPayload, StepExecutionEvent};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Worker-specific event publisher that creates Ruby-compatible events
#[derive(Debug, Clone)]
pub struct WorkerEventPublisher {
    /// Worker identifier for traceability
    worker_id: String,
    /// Shared event publisher for cross-language communication
    shared_publisher: SharedEventPublisher,
    /// Event system for direct publishing if needed
    event_system: Arc<WorkerEventSystem>,
}

impl WorkerEventPublisher {
    /// Create a new worker event publisher
    pub fn new(worker_id: String) -> Self {
        let event_system = Arc::new(WorkerEventSystem::new());
        let shared_publisher = event_system.create_publisher();

        info!(
            worker_id = %worker_id,
            "Creating WorkerEventPublisher for FFI communication"
        );

        Self {
            worker_id,
            shared_publisher,
            event_system,
        }
    }

    /// Create a worker event publisher with custom event system
    pub fn with_event_system(worker_id: String, event_system: Arc<WorkerEventSystem>) -> Self {
        let shared_publisher = event_system.create_publisher();

        Self {
            worker_id,
            shared_publisher,
            event_system,
        }
    }

    /// Fire a step execution event to FFI handlers after database hydration
    ///
    /// This is the main integration point called by WorkerProcessor after it has
    /// hydrated all step context from the database using SimpleStepMessage UUIDs.
    pub async fn fire_step_execution_event(
        &self,
        task_sequence_step: &TaskSequenceStep,
    ) -> Result<StepExecutionEvent, WorkerEventError> {
        let task_uuid = task_sequence_step.task.task.task_uuid;
        let step_uuid = task_sequence_step.workflow_step.workflow_step_uuid;

        // Create step event payload with all hydrated context
        let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step.clone());

        // Create the event
        let event = StepExecutionEvent::new(payload);
        let event_id = event.event_id;

        // Publish event to FFI handlers
        match self
            .shared_publisher
            .publish_step_execution(event.payload.clone())
            .await
        {
            Ok(()) => {
                info!(
                    worker_id = %self.worker_id,
                    event_id = %event_id,
                    step_name = %task_sequence_step.step_definition.name,
                    callable = %task_sequence_step.step_definition.handler.callable,
                    "Step execution event fired successfully to FFI handlers"
                );
                Ok(event)
            }
            Err(e) => {
                error!(
                    worker_id = %self.worker_id,
                    event_id = %event_id,
                    step_name = %task_sequence_step.step_definition.name,
                    error = %e,
                    "Failed to fire step execution event to FFI handlers"
                );
                Err(WorkerEventError::PublishError(format!(
                    "Failed to publish step execution event: {e}"
                )))
            }
        }
    }

    /// Fire a step execution event with specific correlation ID
    ///
    /// This allows for event correlation between worker and FFI completion events.
    pub async fn fire_step_execution_event_with_correlation(
        &self,
        correlation_id: Uuid,
        task_sequence_step: &TaskSequenceStep,
    ) -> Result<StepExecutionEvent, WorkerEventError> {
        let task_uuid = task_sequence_step.task.task.task_uuid;
        let step_uuid = task_sequence_step.workflow_step.workflow_step_uuid;

        debug!(
            worker_id = %self.worker_id,
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            step_name = %task_sequence_step.step_definition.name,
            "Firing step execution event with correlation ID"
        );

        // Create step event payload
        let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step.clone());

        // Publish with correlation ID
        match self
            .shared_publisher
            .publish_step_execution_with_correlation(correlation_id, payload.clone())
            .await
        {
            Ok(()) => {
                info!(
                    worker_id = %self.worker_id,
                    correlation_id = %correlation_id,
                    step_name = %task_sequence_step.step_definition.name,
                    "Step execution event fired with correlation ID"
                );
                Ok(StepExecutionEvent::with_event_id(correlation_id, payload))
            }
            Err(e) => {
                error!(
                    worker_id = %self.worker_id,
                    correlation_id = %correlation_id,
                    error = %e,
                    "Failed to fire step execution event with correlation ID"
                );
                Err(WorkerEventError::PublishError(format!(
                    "Failed to publish correlated step execution event: {e}"
                )))
            }
        }
    }

    /// TAS-65 Phase 1.5b: Fire a step execution event with trace context for distributed tracing
    ///
    /// This propagates trace_id and span_id to FFI handlers for distributed tracing.
    pub async fn fire_step_execution_event_with_trace(
        &self,
        task_sequence_step: &TaskSequenceStep,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> Result<StepExecutionEvent, WorkerEventError> {
        let task_uuid = task_sequence_step.task.task.task_uuid;
        let step_uuid = task_sequence_step.workflow_step.workflow_step_uuid;

        debug!(
            worker_id = %self.worker_id,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            step_name = %task_sequence_step.step_definition.name,
            trace_id = ?trace_id,
            span_id = ?span_id,
            "Firing step execution event with trace context"
        );

        // Create step event payload
        let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step.clone());

        // Create event with trace context
        let event = StepExecutionEvent::with_trace_context(
            payload.clone(),
            trace_id.clone(),
            span_id.clone(),
        );
        let event_id = event.event_id;

        // Publish event to FFI handlers
        match self
            .shared_publisher
            .publish_step_execution_event(event.clone())
            .await
        {
            Ok(()) => {
                info!(
                    worker_id = %self.worker_id,
                    event_id = %event_id,
                    step_name = %task_sequence_step.step_definition.name,
                    trace_id = ?trace_id,
                    span_id = ?span_id,
                    "Step execution event with trace context fired successfully to FFI handlers"
                );
                Ok(event)
            }
            Err(e) => {
                error!(
                    worker_id = %self.worker_id,
                    event_id = %event_id,
                    step_name = %task_sequence_step.step_definition.name,
                    error = %e,
                    "Failed to fire step execution event with trace context to FFI handlers"
                );
                Err(WorkerEventError::PublishError(format!(
                    "Failed to publish step execution event with trace context: {e}"
                )))
            }
        }
    }

    /// Get event system statistics for monitoring
    pub fn get_statistics(&self) -> WorkerEventPublisherStats {
        let system_stats = self.event_system.get_statistics();

        WorkerEventPublisherStats {
            worker_id: self.worker_id.clone(),
            events_published: 0, // Would need to track this separately
            ffi_handlers_subscribed: system_stats.step_execution_subscribers,
            completion_subscribers: system_stats.step_completion_subscribers,
            total_events_tracked: system_stats.total_events_tracked,
        }
    }
}

/// Statistics for worker event publisher monitoring
#[derive(Debug, Clone)]
pub struct WorkerEventPublisherStats {
    pub worker_id: String,
    pub events_published: u64,
    pub ffi_handlers_subscribed: usize,
    pub completion_subscribers: usize,
    pub total_events_tracked: usize,
}

/// Errors specific to worker event publishing
#[derive(Debug, thiserror::Error)]
pub enum WorkerEventError {
    #[error("Failed to publish event: {0}")]
    PublishError(String),

    #[error("Invalid event data: {0}")]
    InvalidEventData(String),

    #[error("Context creation error: {0}")]
    ContextError(String),

    #[error("Event system error: {0}")]
    EventSystemError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tasker_shared::models::orchestration::step_transitive_dependencies::StepDependencyResultMap;
    use tasker_shared::models::task::{Task, TaskForOrchestration};
    use tasker_shared::models::task_template::{
        BackoffStrategy, HandlerDefinition, RetryConfiguration, StepDefinition, StepType,
    };
    use tasker_shared::models::workflow_step::WorkflowStepWithName;
    use tasker_shared::StepExecutionResult;

    /// Helper function to create a test TaskSequenceStep for testing
    fn create_test_task_sequence_step(
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        handler_class: &str,
        step_payload: serde_json::Value,
        dependency_results: StepDependencyResultMap,
        task_context: serde_json::Value,
    ) -> TaskSequenceStep {
        TaskSequenceStep {
            task: TaskForOrchestration {
                task: Task {
                    task_uuid,
                    named_task_uuid: uuid::Uuid::new_v4(),
                    complete: false,
                    requested_at: chrono::Utc::now().naive_utc(),
                    initiator: Some("test".to_string()),
                    source_system: Some("test_system".to_string()),
                    reason: Some("test_reason".to_string()),
                    bypass_steps: None,
                    tags: None,
                    context: Some(task_context),
                    identity_hash: "test_hash".to_string(),
                    priority: 5,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                    correlation_id: Uuid::now_v7(),
                    parent_correlation_id: None,
                },
                task_name: "test_task".to_string(),
                task_version: "1.0.0".to_string(),
                namespace_name: "test_namespace".to_string(),
            },
            workflow_step: WorkflowStepWithName {
                workflow_step_uuid: step_uuid,
                task_uuid,
                named_step_uuid: uuid::Uuid::new_v4(),
                name: step_name.to_string(),
                retryable: false,
                max_attempts: None,
                in_process: false,
                processed: false,
                processed_at: None,
                attempts: None,
                last_attempted_at: None,
                backoff_request_seconds: None,
                inputs: Some(step_payload),
                results: None,
                skippable: false,
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
                template_step_name: step_name.to_string(),
                checkpoint: None,
            },
            dependency_results,
            step_definition: StepDefinition {
                name: step_name.to_string(),
                step_type: StepType::Standard,
                description: Some(format!("Test step: {}", step_name)),
                handler: HandlerDefinition {
                    callable: handler_class.to_string(),
                    initialization: HashMap::new(),
                },
                system_dependency: None,
                dependencies: vec![],
                timeout_seconds: Some(30),
                retry: RetryConfiguration {
                    retryable: false,
                    max_attempts: 1,
                    backoff: BackoffStrategy::None,
                    backoff_base_ms: None,
                    max_backoff_ms: None,
                },
                publishes_events: vec![],
                batch_config: None,
            },
        }
    }

    #[tokio::test]
    async fn test_worker_event_publisher_creation() {
        let publisher = WorkerEventPublisher::new("test_worker_123".to_string());

        assert_eq!(publisher.worker_id, "test_worker_123");
    }

    #[tokio::test]
    async fn test_fire_step_execution_event() {
        let publisher = WorkerEventPublisher::new("test_worker_123".to_string());

        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "process_payment";
        let step_payload = serde_json::json!({"amount": 100.0, "currency": "USD"});
        let mut dependency_results = StepDependencyResultMap::default();
        dependency_results.insert(
            "process_payment".into(),
            StepExecutionResult::success(
                step_uuid,
                serde_json::json!({"amount": 100.0}),
                120,
                None,
            ),
        );
        let task_context = serde_json::json!({"order_id": 12345});

        let task_sequence_step = create_test_task_sequence_step(
            task_uuid,
            step_uuid,
            step_name,
            "OrderProcessing::PaymentHandler",
            step_payload,
            dependency_results,
            task_context,
        );

        let result = publisher
            .fire_step_execution_event(&task_sequence_step)
            .await;

        // Should succeed even with no subscribers
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(
            event.payload.task_sequence_step.step_definition.name,
            step_name
        );
        assert_eq!(
            event
                .payload
                .task_sequence_step
                .step_definition
                .handler
                .callable,
            "OrderProcessing::PaymentHandler"
        );
        assert_eq!(event.payload.task_uuid, task_uuid);
        assert_eq!(event.payload.step_uuid, step_uuid);
    }

    #[tokio::test]
    async fn test_step_event_payload_structure() {
        let publisher = WorkerEventPublisher::new("test_worker_123".to_string());

        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "process_payment";
        let task_context = serde_json::json!({"order_id": 12345});
        let mut dependency_results = StepDependencyResultMap::default();
        dependency_results.insert(
            "process_payment".into(),
            StepExecutionResult::success(
                step_uuid,
                serde_json::json!({"amount": 100.0}),
                120,
                None,
            ),
        );
        dependency_results.insert(
            "validate_order".into(),
            StepExecutionResult::success(
                Uuid::new_v4(),
                serde_json::json!({ "order_valid": true }),
                50,
                None,
            ),
        );
        dependency_results.insert(
            "check_inventory".into(),
            StepExecutionResult::success(
                Uuid::new_v4(),
                serde_json::json!({ "inventory_checked": true }),
                70,
                None,
            ),
        );

        let step_payload = serde_json::json!({"amount": 100.0, "currency": "USD"});

        let task_sequence_step = create_test_task_sequence_step(
            task_uuid,
            step_uuid,
            step_name,
            "OrderProcessing::PaymentHandler",
            step_payload,
            dependency_results,
            task_context.clone(),
        );

        let result = publisher
            .fire_step_execution_event(&task_sequence_step)
            .await;

        assert!(result.is_ok());
        let event = result.unwrap();

        // Verify the event payload has the correct structure for FFI
        assert_eq!(event.payload.task_uuid, task_uuid);
        assert_eq!(event.payload.step_uuid, step_uuid);
        assert_eq!(
            event.payload.task_sequence_step.step_definition.name,
            step_name
        );

        // Verify that the TaskSequenceStep has all the FFI-needed data
        assert_eq!(
            event.payload.task_sequence_step.task.task.task_uuid,
            task_uuid
        );
        assert_eq!(
            event
                .payload
                .task_sequence_step
                .workflow_step
                .workflow_step_uuid,
            step_uuid
        );
        assert_eq!(
            event
                .payload
                .task_sequence_step
                .step_definition
                .handler
                .callable,
            "OrderProcessing::PaymentHandler"
        );

        // Verify dependency results are preserved
        assert_eq!(event.payload.task_sequence_step.dependency_results.len(), 3);
        assert!(event
            .payload
            .task_sequence_step
            .dependency_results
            .contains_key("validate_order"));
        assert!(event
            .payload
            .task_sequence_step
            .dependency_results
            .contains_key("process_payment"));
        assert!(event
            .payload
            .task_sequence_step
            .dependency_results
            .contains_key("check_inventory"));
    }

    #[tokio::test]
    async fn test_event_correlation() {
        let publisher = WorkerEventPublisher::new("test_worker_123".to_string());

        let correlation_id = Uuid::new_v4();
        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "test_step";

        let task_sequence_step = create_test_task_sequence_step(
            task_uuid,
            step_uuid,
            step_name,
            "TestHandler",
            serde_json::json!({"test": true}),
            HashMap::new(),
            serde_json::json!({"context": "test"}),
        );

        let result = publisher
            .fire_step_execution_event_with_correlation(correlation_id, &task_sequence_step)
            .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_id, correlation_id);
        assert_eq!(
            event.payload.task_sequence_step.step_definition.name,
            step_name
        );
        assert_eq!(event.payload.task_uuid, task_uuid);
        assert_eq!(event.payload.step_uuid, step_uuid);
    }

    #[tokio::test]
    async fn test_publisher_statistics() {
        let publisher = WorkerEventPublisher::new("test_worker_123".to_string());

        let stats = publisher.get_statistics();
        assert_eq!(stats.worker_id, "test_worker_123");
        assert_eq!(stats.events_published, 0);
    }
}
