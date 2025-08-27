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
//! use tasker_worker::event_publisher::WorkerEventPublisher;
//!
//! // Inside WorkerProcessor
//! let event_publisher = WorkerEventPublisher::new(worker_id.clone());
//!
//! // After database hydration, before or during step execution
//! event_publisher.fire_step_execution_event(
//!     task_uuid,
//!     step_uuid,
//!     step_name,
//!     handler_class,
//!     step_payload,
//!     dependency_results,
//!     execution_context,
//! ).await?;
//! ```

use crate::step_claim::TaskSequenceStep;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::events::{WorkerEventPublisher as SharedEventPublisher, WorkerEventSystem};

use tasker_shared::types::{StepEventPayload, StepExecutionEvent};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Worker-specific event publisher that creates Ruby-compatible events
#[derive(Debug, Clone)]
pub struct WorkerEventPublisher {
    /// Worker identifier for traceability
    worker_id: String,
    /// Namespace this worker is processing
    namespace: String,
    /// Shared event publisher for cross-language communication
    shared_publisher: SharedEventPublisher,
    /// Event system for direct publishing if needed
    event_system: Arc<WorkerEventSystem>,
}

impl WorkerEventPublisher {
    /// Create a new worker event publisher
    pub fn new(worker_id: String, namespace: String) -> Self {
        let event_system = Arc::new(WorkerEventSystem::new());
        let shared_publisher = event_system.create_publisher();

        info!(
            worker_id = %worker_id,
            namespace = %namespace,
            "Creating WorkerEventPublisher for FFI communication"
        );

        Self {
            worker_id,
            namespace,
            shared_publisher,
            event_system,
        }
    }

    /// Create a worker event publisher with custom event system
    pub fn with_event_system(
        worker_id: String,
        namespace: String,
        event_system: Arc<WorkerEventSystem>,
    ) -> Self {
        let shared_publisher = event_system.create_publisher();

        Self {
            worker_id,
            namespace,
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
        let step_name = task_sequence_step.workflow_step.name.clone();
        let step_definition = task_sequence_step.step_definition.clone();
        let workflow_step = task_sequence_step.workflow_step.clone();
        let task_context = task_sequence_step
            .task
            .task
            .context
            .clone()
            .unwrap_or(serde_json::json!({}));
        let dependency_results = task_sequence_step.dependency_results.clone();

        debug!(
            worker_id = %self.worker_id,
            namespace = %self.namespace,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            step_name = %step_name,
            callable = %step_definition.handler.callable,
            dependencies_count = dependency_results.len(),
            "Firing step execution event to FFI handlers"
        );

        // Create Ruby-compatible execution context
        let execution_context = self.create_ffi_compatible_context(
            task_uuid,
            step_uuid,
            &step_name,
            &task_context,
            &dependency_results,
        );

        // Create step event payload with all hydrated context
        let payload = StepEventPayload::new(
            task_uuid,
            step_uuid,
            step_name.clone(),
            step_definition.clone(),
            workflow_step,
            dependency_results.clone(),
            execution_context,
        );

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
                    step_name = %step_name,
                    callable = %step_definition.handler.callable,
                    "Step execution event fired successfully to FFI handlers"
                );
                Ok(event)
            }
            Err(e) => {
                error!(
                    worker_id = %self.worker_id,
                    event_id = %event_id,
                    step_name = %step_name,
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
        let step_name = task_sequence_step.workflow_step.name.clone();
        let step_definition = task_sequence_step.step_definition.clone();
        let workflow_step = task_sequence_step.workflow_step.clone();
        let task_context = task_sequence_step
            .task
            .task
            .context
            .clone()
            .unwrap_or(serde_json::json!({}));
        let dependency_results = task_sequence_step.dependency_results.clone();

        debug!(
            worker_id = %self.worker_id,
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            step_name = %step_name,
            "Firing step execution event with correlation ID"
        );

        // Create Ruby-compatible execution context
        let execution_context = self.create_ffi_compatible_context(
            task_uuid,
            step_uuid,
            &step_name,
            &task_context,
            &dependency_results,
        );

        // Create step event payload
        let payload = StepEventPayload::new(
            task_uuid,
            step_uuid,
            step_name.clone(),
            step_definition.clone(),
            workflow_step,
            dependency_results,
            execution_context,
        );

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
                    step_name = %step_name,
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

    /// Get event system statistics for monitoring
    pub fn get_statistics(&self) -> WorkerEventPublisherStats {
        let system_stats = self.event_system.get_statistics();

        WorkerEventPublisherStats {
            worker_id: self.worker_id.clone(),
            namespace: self.namespace.clone(),
            events_published: 0, // Would need to track this separately
            ffi_handlers_subscribed: system_stats.step_execution_subscribers,
            completion_subscribers: system_stats.step_completion_subscribers,
            total_events_tracked: system_stats.total_events_tracked,
        }
    }

    /// Create FFI-compatible execution context
    ///
    /// This creates the execution context structure that FFI step handlers expect,
    /// based on the existing FFI StepHandlerCallResult and sequence patterns.
    fn create_ffi_compatible_context(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        task_context: &serde_json::Value,
        dependency_results: &HashMap<String, serde_json::Value>,
    ) -> serde_json::Value {
        // Create sequence-like structure for dependency results
        let sequence_data: Vec<serde_json::Value> = dependency_results
            .iter()
            .map(|(step_name, result)| {
                serde_json::json!({
                    "step_name": step_name,
                    "result": result,
                    "success": true // Assuming successful since they're in dependency_results
                })
            })
            .collect();

        // Create FFI-compatible execution context
        serde_json::json!({
            "task": {
                "task_id": task_uuid,
                "task_uuid": task_uuid,
                "namespace": self.namespace,
                "context": task_context,
                "worker_id": self.worker_id
            },
            "sequence": sequence_data,
            "step": {
                "step_id": step_uuid,
                "step_uuid": step_uuid,
                "step_name": step_name,
                "namespace": self.namespace
            },
            "worker": {
                "worker_id": self.worker_id,
                "namespace": self.namespace
            },
            "metadata": {
                "execution_mode": "event_driven",
                "architecture": "TAS-40_command_pattern",
                "hydration_source": "database_api_layer"
            }
        })
    }
}

/// Statistics for worker event publisher monitoring
#[derive(Debug, Clone)]
pub struct WorkerEventPublisherStats {
    pub worker_id: String,
    pub namespace: String,
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
    use tasker_shared::models::task_template::StepDefinition;
    use tasker_shared::models::Task;

    #[tokio::test]
    async fn test_worker_event_publisher_creation() {
        let publisher =
            WorkerEventPublisher::new("test_worker_123".to_string(), "test_namespace".to_string());

        assert_eq!(publisher.worker_id, "test_worker_123");
        assert_eq!(publisher.namespace, "test_namespace");
    }

    #[tokio::test]
    async fn test_fire_step_execution_event() {
        let publisher = WorkerEventPublisher::new(
            "test_worker_123".to_string(),
            "order_processing".to_string(),
        );

        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "process_payment".to_string();
        let step_definition = StepDefinition {
            name: step_name.clone(),
            description: None,
            handler: tasker_shared::models::task_template::HandlerDefinition {
                callable: "OrderProcessing::PaymentHandler".to_string(),
                initialization: std::collections::HashMap::new(),
            },
            system_dependency: None,
            dependencies: vec![],
            timeout_seconds: None,
            retry: None,
            publishes_events: vec![],
        };
        let step_payload = serde_json::json!({"amount": 100.0, "currency": "USD"});
        let dependency_results = HashMap::from([
            (
                "validate_order".to_string(),
                serde_json::json!({"valid": true}),
            ),
            (
                "check_inventory".to_string(),
                serde_json::json!({"available": true}),
            ),
        ]);
        let task_context = serde_json::json!({"order_id": 12345});

        let result = publisher
            .fire_step_execution_event(
                task_uuid,
                step_uuid,
                step_name.clone(),
                step_definition.clone(),
                step_payload.clone(),
                dependency_results.clone(),
                task_context.clone(),
            )
            .await;

        // Should succeed even with no subscribers
        assert!(result.is_ok());

        let event = result.unwrap();
        assert_eq!(event.payload.step_name, step_name);
        assert_eq!(
            event.payload.step_definition.handler.callable,
            "OrderProcessing::PaymentHandler"
        );
        assert_eq!(event.payload.task_uuid, task_uuid);
        assert_eq!(event.payload.step_uuid, step_uuid);
    }

    #[tokio::test]
    async fn test_ruby_compatible_context_creation() {
        let publisher = WorkerEventPublisher::new(
            "test_worker_123".to_string(),
            "order_processing".to_string(),
        );

        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "process_payment";
        let task_context = serde_json::json!({"order_id": 12345});
        let dependency_results = HashMap::from([
            (
                "validate_order".to_string(),
                serde_json::json!({"valid": true}),
            ),
            (
                "check_inventory".to_string(),
                serde_json::json!({"available": true}),
            ),
        ]);

        let context = publisher.create_ffi_compatible_context(
            task_uuid,
            step_uuid,
            step_name,
            &task_context,
            &dependency_results,
        );

        // Verify Ruby-compatible structure
        assert!(context.get("task").is_some());
        assert!(context.get("sequence").is_some());
        assert!(context.get("step").is_some());
        assert!(context.get("worker").is_some());

        // Verify task structure
        let task = &context["task"];
        assert_eq!(
            task["task_uuid"],
            serde_json::Value::String(task_uuid.to_string())
        );
        assert_eq!(
            task["namespace"],
            serde_json::Value::String("order_processing".to_string())
        );
        assert_eq!(
            task["worker_id"],
            serde_json::Value::String("test_worker_123".to_string())
        );

        // Verify sequence structure (dependency results)
        let sequence = &context["sequence"];
        assert!(sequence.is_array());
        let sequence_array = sequence.as_array().unwrap();
        assert_eq!(sequence_array.len(), 2);

        // Verify step structure
        let step = &context["step"];
        assert_eq!(
            step["step_uuid"],
            serde_json::Value::String(step_uuid.to_string())
        );
        assert_eq!(
            step["step_name"],
            serde_json::Value::String(step_name.to_string())
        );
    }

    #[tokio::test]
    async fn test_event_correlation() {
        let publisher =
            WorkerEventPublisher::new("test_worker_123".to_string(), "test_namespace".to_string());

        let correlation_id = Uuid::new_v4();
        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();

        let step_definition = StepDefinition {
            name: "test_step".to_string(),
            description: None,
            handler: tasker_shared::models::task_template::HandlerDefinition {
                callable: "TestHandler".to_string(),
                initialization: std::collections::HashMap::new(),
            },
            system_dependency: None,
            dependencies: vec![],
            timeout_seconds: None,
            retry: None,
            publishes_events: vec![],
        };

        let result = publisher
            .fire_step_execution_event_with_correlation(
                correlation_id,
                task_uuid,
                step_uuid,
                "test_step".to_string(),
                step_definition,
                serde_json::json!({"test": true}),
                HashMap::new(),
                serde_json::json!({"context": "test"}),
            )
            .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_id, correlation_id);
    }

    #[tokio::test]
    async fn test_publisher_statistics() {
        let publisher =
            WorkerEventPublisher::new("test_worker_123".to_string(), "test_namespace".to_string());

        let stats = publisher.get_statistics();
        assert_eq!(stats.worker_id, "test_worker_123");
        assert_eq!(stats.namespace, "test_namespace");
        assert_eq!(stats.events_published, 0);
    }
}
