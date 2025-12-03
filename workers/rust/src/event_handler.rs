//! # Native Rust Event Handler
//!
//! Subscribes to the worker event system and executes Rust step handlers
//! when `StepExecutionEvents` are received.
//!
//! ## TAS-65: Post-Execution Domain Event Publishing
//!
//! After step execution completes, this handler invokes the appropriate
//! `StepEventPublisher` to publish domain events based on YAML configuration.
//! This implements the post-execution publisher callback pattern.

use anyhow::Result;
use std::sync::Arc;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    messaging::StepExecutionResult,
    types::{StepExecutionCompletionEvent, StepExecutionEvent, TaskSequenceStep},
};
use tasker_worker::worker::{StepEventContext, StepEventPublisherRegistry};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::step_handlers::RustStepHandlerRegistry;

/// Event handler that bridges the worker event system with native Rust handlers
///
/// ## TAS-65: Domain Event Publishing
///
/// After step execution completes, the handler invokes the `StepEventPublisherRegistry`
/// to publish domain events based on YAML configuration. This implements the
/// post-execution publisher callback pattern where:
///
/// 1. Handler executes business logic → Returns StepExecutionResult
/// 2. Result is persisted and enqueued for orchestration
/// 3. StepEventPublisher is invoked with full execution context
/// 4. Domain events are published (failures logged but don't affect step)
pub struct RustEventHandler {
    registry: Arc<RustStepHandlerRegistry>,
    event_subscriber: Arc<WorkerEventSubscriber>,
    /// TAS-65: Registry of step event publishers for post-execution publishing
    step_event_publisher_registry: Arc<RwLock<StepEventPublisherRegistry>>,
    worker_id: String,
}

impl std::fmt::Debug for RustEventHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustEventHandler")
            .field("worker_id", &self.worker_id)
            .field("has_registry", &true)
            .field("has_event_subscriber", &true)
            .field("has_step_event_publisher_registry", &true)
            .finish()
    }
}

impl RustEventHandler {
    /// Create a new event handler with the given registry, event system, and publisher registry
    ///
    /// # Arguments
    ///
    /// * `registry` - Step handler registry for looking up handlers
    /// * `event_system` - Worker event system for subscribing to events
    /// * `step_event_publisher_registry` - Registry for post-execution domain event publishers
    /// * `worker_id` - Unique identifier for this worker
    #[must_use]
    pub fn new(
        registry: Arc<RustStepHandlerRegistry>,
        event_system: Arc<WorkerEventSystem>,
        step_event_publisher_registry: Arc<RwLock<StepEventPublisherRegistry>>,
        worker_id: String,
    ) -> Self {
        // Clone the Arc to get the WorkerEventSystem value
        let event_system_cloned = (*event_system).clone();
        let event_subscriber = Arc::new(WorkerEventSubscriber::new(event_system_cloned));

        Self {
            registry,
            event_subscriber,
            step_event_publisher_registry,
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
        let step_event_publisher_registry = self.step_event_publisher_registry.clone();
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
                            &step_event_publisher_registry,
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
    ///
    /// ## TAS-65: Post-Execution Event Publishing
    ///
    /// After the handler completes and the completion event is published,
    /// this method invokes the appropriate `StepEventPublisher` to publish
    /// domain events based on YAML configuration. Event publishing failures
    /// are logged but do not affect step execution.
    async fn handle_step_execution(
        registry: &Arc<RustStepHandlerRegistry>,
        event_subscriber: &Arc<WorkerEventSubscriber>,
        step_event_publisher_registry: &Arc<RwLock<StepEventPublisherRegistry>>,
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

                // Create step completion event and execution result for domain event publishing
                let (completion_event, execution_result) = match result {
                    Ok(step_result) => {
                        let completion = StepExecutionCompletionEvent {
                            event_id: event.event_id,
                            task_uuid: event.payload.task_uuid,
                            step_uuid: event.payload.step_uuid,
                            span_id: event.span_id,
                            trace_id: event.trace_id,
                            success: step_result.success,
                            result: step_result.result.clone(),
                            metadata: Some(
                                serde_json::to_value(&step_result.metadata)
                                    .unwrap_or(serde_json::Value::Null),
                            ),
                            error_message: step_result.error.as_ref().map(|e| e.message.clone()),
                        };
                        (completion, step_result)
                    }
                    Err(e) => {
                        let error_result = StepExecutionResult::failure(
                            event.payload.step_uuid,
                            e.to_string(),
                            Some("HANDLER_ERROR".to_string()),
                            Some("HandlerExecutionError".to_string()),
                            true, // Retryable by default
                            0,
                            None,
                        );
                        let completion = StepExecutionCompletionEvent {
                            event_id: event.event_id,
                            task_uuid: event.payload.task_uuid,
                            step_uuid: event.payload.step_uuid,
                            span_id: event.span_id,
                            trace_id: event.trace_id,
                            success: false,
                            result: serde_json::Value::Null,
                            metadata: None,
                            error_message: Some(e.to_string()),
                        };
                        (completion, error_result)
                    }
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

                // TAS-65: Post-execution domain event publishing
                // This happens AFTER step completion is published to orchestration
                Self::publish_step_events(
                    step_event_publisher_registry,
                    &event.payload.task_sequence_step,
                    execution_result,
                    worker_id,
                )
                .await;

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

    /// TAS-65: Publish domain events after step execution completes
    ///
    /// This method invokes the appropriate `StepEventPublisher` based on
    /// YAML configuration. Failures are logged but do not affect step execution.
    ///
    /// ## Publisher Selection
    ///
    /// 1. Check if step has `publishes_events` in YAML configuration
    /// 2. Look up custom publisher if specified, otherwise use DefaultDomainEventPublisher
    /// 3. Invoke publisher with full execution context
    async fn publish_step_events(
        registry: &Arc<RwLock<StepEventPublisherRegistry>>,
        task_sequence_step: &TaskSequenceStep,
        execution_result: StepExecutionResult,
        worker_id: &str,
    ) {
        // Check if step declares any events to publish
        let publishes_events = &task_sequence_step.step_definition.publishes_events;
        if publishes_events.is_empty() {
            debug!(
                worker_id = %worker_id,
                step_name = %task_sequence_step.workflow_step.name,
                "Step has no declared events to publish"
            );
            return;
        }

        debug!(
            worker_id = %worker_id,
            step_name = %task_sequence_step.workflow_step.name,
            event_count = publishes_events.len(),
            "Publishing domain events for step"
        );

        // Get the first event's publisher name (all events in a step use the same publisher)
        let publisher_name = publishes_events
            .first()
            .and_then(|e| e.publisher.as_deref());

        // Get the appropriate publisher from the registry
        let registry_guard = registry.read().await;
        let publisher = registry_guard.get_or_default(publisher_name);

        // Create the execution context
        let ctx = StepEventContext::new(task_sequence_step.clone(), execution_result);

        // Check if publisher should handle this step
        if !publisher.should_handle(&task_sequence_step.workflow_step.name) {
            debug!(
                worker_id = %worker_id,
                step_name = %task_sequence_step.workflow_step.name,
                publisher = publisher.name(),
                "Publisher declined to handle step"
            );
            return;
        }

        // Invoke the publisher
        let result = publisher.publish(&ctx).await;

        // Log results
        if !result.published.is_empty() {
            info!(
                worker_id = %worker_id,
                step_name = %task_sequence_step.workflow_step.name,
                published_count = result.published.len(),
                published_events = ?result.published,
                "Domain events published successfully"
            );
        }

        if !result.skipped.is_empty() {
            debug!(
                worker_id = %worker_id,
                step_name = %task_sequence_step.workflow_step.name,
                skipped_count = result.skipped.len(),
                skipped_events = ?result.skipped,
                "Some events were skipped"
            );
        }

        if !result.errors.is_empty() {
            // Event publishing failures are logged but don't fail the step
            error!(
                worker_id = %worker_id,
                step_name = %task_sequence_step.workflow_step.name,
                error_count = result.errors.len(),
                errors = ?result.errors,
                "Some events failed to publish"
            );
        }
    }
}
