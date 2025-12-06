//! # Domain Event Callback
//!
//! TAS-67: Unified `PostHandlerCallback` implementation for domain event publishing.
//!
//! This callback bridges the TAS-65 domain event publishing system with TAS-67's
//! dispatch architecture, ensuring domain events are published after handler
//! completion but before results are sent to the completion channel.
//!
//! Used by both native Rust workers and FFI workers (Ruby, Python).
//!
//! ## Architecture
//!
//! ```text
//! HandlerDispatchService / FfiDispatchChannel
//!     │
//!     ├─→ handler.call() → result
//!     │
//!     └─→ DomainEventCallback.on_handler_complete()
//!             │
//!             ├─→ Check publishes_events YAML config
//!             ├─→ Get publisher from registry
//!             └─→ publisher.publish(context)
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

use super::dispatch_service::PostHandlerCallback;
use crate::worker::step_event_publisher::StepEventContext;
use crate::worker::step_event_publisher_registry::StepEventPublisherRegistry;

/// Domain event callback for post-handler event publishing
///
/// Wraps `StepEventPublisherRegistry` and implements `PostHandlerCallback` trait
/// for integration with both `HandlerDispatchService` (Rust) and `FfiDispatchChannel` (Ruby/Python).
pub struct DomainEventCallback {
    /// Registry of step event publishers
    registry: Arc<RwLock<StepEventPublisherRegistry>>,
}

impl DomainEventCallback {
    /// Create new callback with event publisher registry
    #[must_use]
    pub fn new(registry: Arc<RwLock<StepEventPublisherRegistry>>) -> Self {
        Self { registry }
    }
}

impl std::fmt::Debug for DomainEventCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainEventCallback")
            .field("has_registry", &true)
            .finish()
    }
}

#[async_trait]
impl PostHandlerCallback for DomainEventCallback {
    async fn on_handler_complete(
        &self,
        step: &TaskSequenceStep,
        result: &StepExecutionResult,
        worker_id: &str,
    ) {
        // Check if step declares any events to publish
        let publishes_events = &step.step_definition.publishes_events;
        if publishes_events.is_empty() {
            debug!(
                worker_id = %worker_id,
                step_name = %step.workflow_step.name,
                "Step has no declared events to publish"
            );
            return;
        }

        debug!(
            worker_id = %worker_id,
            step_name = %step.workflow_step.name,
            event_count = publishes_events.len(),
            "Publishing domain events for step"
        );

        // Get the first event's publisher name (all events in a step use the same publisher)
        let publisher_name = publishes_events
            .first()
            .and_then(|e| e.publisher.as_deref());

        // Get the appropriate publisher from the registry
        let registry_guard = self.registry.read().await;
        let publisher = registry_guard.get_or_default(publisher_name);

        // Create the execution context
        let ctx = StepEventContext::new(step.clone(), result.clone());

        // Check if publisher should handle this step
        if !publisher.should_handle(&step.workflow_step.name) {
            debug!(
                worker_id = %worker_id,
                step_name = %step.workflow_step.name,
                publisher = publisher.name(),
                "Publisher declined to handle step"
            );
            return;
        }

        // Invoke the publisher
        let publish_result = publisher.publish(&ctx).await;

        // Log results
        if !publish_result.published.is_empty() {
            info!(
                worker_id = %worker_id,
                step_name = %step.workflow_step.name,
                published_count = publish_result.published.len(),
                published_events = ?publish_result.published,
                "Domain events published successfully"
            );
        }

        if !publish_result.skipped.is_empty() {
            debug!(
                worker_id = %worker_id,
                step_name = %step.workflow_step.name,
                skipped_count = publish_result.skipped.len(),
                skipped_events = ?publish_result.skipped,
                "Some events were skipped"
            );
        }

        if !publish_result.errors.is_empty() {
            // Event publishing failures are logged but don't fail the step
            error!(
                worker_id = %worker_id,
                step_name = %step.workflow_step.name,
                error_count = publish_result.errors.len(),
                errors = ?publish_result.errors,
                "Some events failed to publish"
            );
        }
    }

    fn name(&self) -> &str {
        "domain-event-callback"
    }
}
