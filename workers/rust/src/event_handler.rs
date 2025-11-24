//! # Native Rust Event Handler
//!
//! Subscribes to the worker event system and executes Rust step handlers
//! when `StepExecutionEvents` are received.

use anyhow::Result;
use std::sync::Arc;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    types::{StepExecutionCompletionEvent, StepExecutionEvent},
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::step_handlers::RustStepHandlerRegistry;

/// Event handler that bridges the worker event system with native Rust handlers
pub struct RustEventHandler {
    registry: Arc<RustStepHandlerRegistry>,
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
}

impl std::fmt::Debug for RustEventHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustEventHandler")
            .field("worker_id", &self.worker_id)
            .field("has_registry", &true)
            .field("has_event_subscriber", &true)
            .finish()
    }
}

impl RustEventHandler {
    /// Create a new event handler with the given registry and event system
    #[must_use]
    pub fn new(
        registry: Arc<RustStepHandlerRegistry>,
        event_system: Arc<WorkerEventSystem>,
        worker_id: String,
    ) -> Self {
        // Clone the Arc to get the WorkerEventSystem value
        let event_system_cloned = (*event_system).clone();
        let event_subscriber = Arc::new(WorkerEventSubscriber::new(event_system_cloned));

        Self {
            registry,
            event_subscriber,
            worker_id,
        }
    }

    /// Start listening for step execution events and handle them with the registry
    pub async fn start(&self) -> Result<()> {
        info!(
            worker_id = %self.worker_id,
            "Starting Rust event handler - subscribing to step execution events"
        );

        // Subscribe to step execution events
        let mut receiver = self.event_subscriber.subscribe_to_step_executions();

        // Clone what we need for the async task
        let registry = self.registry.clone();
        let event_subscriber = self.event_subscriber.clone();
        let worker_id = self.worker_id.clone();

        // Spawn task to handle events
        tokio::spawn(async move {
            info!(
                worker_id = %worker_id,
                "Rust event handler task started - waiting for step execution events"
            );

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        debug!(
                            worker_id = %worker_id,
                            event_id = %event.event_id,
                            step_name = %event.payload.task_sequence_step.workflow_step.name,
                            handler_class = %event.payload.task_sequence_step.step_definition.handler.callable,
                            "Received step execution event"
                        );

                        // Handle the event
                        if let Err(e) = Self::handle_step_execution(
                            &registry,
                            &event_subscriber,
                            event,
                            &worker_id,
                        )
                        .await
                        {
                            error!(
                                worker_id = %worker_id,
                                error = %e,
                                "Failed to handle step execution event"
                            );
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(
                            worker_id = %worker_id,
                            lagged_count = count,
                            "Event handler lagged behind - some events may have been missed"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!(
                            worker_id = %worker_id,
                            "Event channel closed - stopping event handler"
                        );
                        break;
                    }
                }
            }

            info!(
                worker_id = %worker_id,
                "Rust event handler task stopped"
            );
        });

        Ok(())
    }

    /// Handle a single step execution event
    async fn handle_step_execution(
        registry: &Arc<RustStepHandlerRegistry>,
        event_subscriber: &Arc<WorkerEventSubscriber>,
        event: StepExecutionEvent,
        worker_id: &str,
    ) -> Result<()> {
        // Use template_step_name for handler lookup
        // For regular steps, template_step_name equals name
        // For dynamically created steps (batch workers, decision point steps),
        // template_step_name contains the template name from inputs->>'__template_step_name'
        let handler_name = &event
            .payload
            .task_sequence_step
            .workflow_step
            .template_step_name;

        debug!(
            worker_id = %worker_id,
            handler_name = %handler_name,
            workflow_step_name = %event.payload.task_sequence_step.workflow_step.name,
            "Looking up handler in registry"
        );

        // Look up the handler in the registry
        match registry.get_handler(handler_name) {
            Ok(handler) => {
                info!(
                    worker_id = %worker_id,
                    handler_name = %handler_name,
                    "Found handler - executing"
                );

                // Execute the handler
                let result = handler.call(&event.payload.task_sequence_step).await;

                // Create step completion event
                let completion_event = match result {
                    Ok(step_result) => StepExecutionCompletionEvent {
                        event_id: event.event_id, // Use same event_id for correlation
                        task_uuid: event.payload.task_uuid,
                        step_uuid: event.payload.step_uuid,
                        span_id: event.span_id,
                        trace_id: event.trace_id,
                        success: step_result.success,
                        result: step_result.result,
                        metadata: Some(
                            serde_json::to_value(step_result.metadata)
                                .unwrap_or(serde_json::Value::Null),
                        ),
                        error_message: step_result.error.map(|e| e.message),
                    },
                    Err(e) => StepExecutionCompletionEvent {
                        event_id: event.event_id, // Use same event_id for correlation
                        task_uuid: event.payload.task_uuid,
                        step_uuid: event.payload.step_uuid,
                        span_id: event.span_id,
                        trace_id: event.trace_id,
                        success: false,
                        result: serde_json::Value::Null,
                        metadata: None,
                        error_message: Some(e.to_string()),
                    },
                };

                // Publish the completion event
                debug!(
                    worker_id = %worker_id,
                    event_id = %event.event_id,
                    "Publishing step completion event"
                );

                event_subscriber
                    .publish_step_completion(completion_event)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to publish step completion: {}", e))?;

                info!(
                    worker_id = %worker_id,
                    handler_name = %handler_name,
                    event_id = %event.event_id,
                    "Successfully handled step execution event"
                );
            }
            Err(e) => {
                warn!(
                    worker_id = %worker_id,
                    handler_name = %handler_name,
                    error = %e,
                    "Handler not found in registry - this might be handled by a different worker"
                );

                // We don't publish a failure response here because this worker
                // might not be responsible for this handler type
            }
        }

        Ok(())
    }
}
