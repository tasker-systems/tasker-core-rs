//! # Worker Traits
//!
//! Common traits for worker functionality including domain event publishing.

use anyhow::Result;
use std::sync::Arc;

use tasker_shared::events::domain_events::{
    DomainEventPayload, DomainEventPublisher, EventMetadata,
};
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{debug, warn};

/// TAS-65: Trait providing convenient domain event publishing for workers
///
/// Automatically extracts metadata from `TaskSequenceStep` and handles errors gracefully.
/// Event publishing failures are logged but do NOT fail the step.
///
/// ## Deprecation Notice
///
/// This trait supports in-handler event publishing during the transition to
/// post-execution publisher callbacks. Once Phase 6 (TAS-65) is complete,
/// this trait will be removed in favor of YAML-declared events published
/// automatically after step completion.
///
/// ## Usage
///
/// ```rust,ignore
/// impl DomainEventPublishable for MyHandler {
///     fn event_publisher(&self) -> Option<&Arc<DomainEventPublisher>> {
///         self.publisher.as_ref()
///     }
///
///     fn handler_name(&self) -> &str {
///         "my_handler"
///     }
/// }
///
/// // In your handler (after building your execution result):
/// self.publish_domain_event(
///     step_data,
///     "order.processed",
///     execution_result,
///     json!({"order_id": 123})
/// ).await?;
/// ```
#[async_trait::async_trait]
pub trait DomainEventPublishable {
    /// Access to the event publisher (None if not configured)
    fn event_publisher(&self) -> Option<&Arc<DomainEventPublisher>>;

    /// The handler's name for metadata (used in `fired_by` field)
    fn handler_name(&self) -> &str;

    /// Publish a domain event with full execution context
    ///
    /// # Arguments
    /// - `step_data`: Current step execution context (provides task/step metadata)
    /// - `event_name`: Event name in dot notation (e.g., "payment.processed")
    /// - `execution_result`: The complete step execution result
    /// - `business_payload`: Business-specific event payload as JSON value
    ///
    /// # Error Handling
    /// - If publisher not configured: silently skip (no-op)
    /// - If publish fails: log warning, continue (don't fail step)
    ///
    /// # Example
    /// ```rust,ignore
    /// self.publish_domain_event(
    ///     step_data,
    ///     "order.fulfilled",
    ///     execution_result,
    ///     json!({"order_id": 123, "total": 99.99})
    /// ).await?;
    /// ```
    async fn publish_domain_event(
        &self,
        step_data: &TaskSequenceStep,
        event_name: &str,
        execution_result: StepExecutionResult,
        business_payload: serde_json::Value,
    ) -> Result<()> {
        // Skip if publisher not configured (e.g., in tests)
        let publisher = match self.event_publisher() {
            Some(p) => p,
            None => {
                debug!(
                    handler = self.handler_name(),
                    event_name = event_name,
                    "Skipping event publication - no publisher configured"
                );
                return Ok(());
            }
        };

        // Extract metadata from step_data
        let metadata = EventMetadata {
            task_uuid: step_data.task.task.task_uuid,
            step_uuid: Some(step_data.workflow_step.workflow_step_uuid),
            step_name: Some(step_data.workflow_step.name.clone()),
            namespace: step_data.task.namespace_name.clone(),
            correlation_id: step_data.task.task.correlation_id,
            fired_at: chrono::Utc::now(),
            fired_by: self.handler_name().to_string(),
        };

        // Construct domain event payload with full execution context
        let domain_payload = DomainEventPayload {
            task_sequence_step: step_data.clone(),
            execution_result,
            payload: business_payload,
        };

        // Publish event with graceful error handling
        if let Err(e) = publisher
            .publish_event(event_name, domain_payload, metadata)
            .await
        {
            warn!(
                handler = self.handler_name(),
                event_name = event_name,
                error = %e,
                "Failed to publish domain event - step will continue"
            );
        } else {
            debug!(
                handler = self.handler_name(),
                event_name = event_name,
                "Domain event published successfully"
            );
        }

        Ok(())
    }
}
