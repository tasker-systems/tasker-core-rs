//! # TAS-65 Phase 3: Step Event Publisher Trait
//!
//! Defines the contract for custom domain event publishers that are invoked
//! after step execution completes. This enables complex event logic beyond
//! what the GenericEventPublisher handles automatically.
//!
//! ## Architecture
//!
//! ```text
//! Step Handler executes
//!     ↓
//! Result persisted to database
//!     ↓
//! Result enqueued for orchestration
//!     ↓
//! StepEventPublisher.publish() invoked  ← THIS TRAIT
//!     ↓
//! Domain events sent to PGMQ
//! ```
//!
//! ## Design Principles
//!
//! 1. **Pure DTO Context**: `StepEventContext` holds only data, no behavior
//! 2. **Trait-Based Publishing**: `publish_event()` is a default impl on the trait
//! 3. **Publisher Owns Arc**: Each publisher holds its `Arc<DomainEventPublisher>`,
//!    avoiding per-invocation Arc cloning
//! 4. **High-Throughput Friendly**: Minimal allocations per event
//!
//! ## Key Guarantees
//!
//! 1. **Post-Execution**: Publishers are called AFTER step results are persisted
//! 2. **Non-Blocking**: Event publishing failures never fail the workflow step
//! 3. **Full Context**: Publishers receive complete execution context
//! 4. **Flexible**: Custom logic for payload enrichment, conditional publishing, etc.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use std::sync::Arc;
//! use std::fmt::Debug;
//! use tasker_shared::events::domain_events::DomainEventPublisher;
//! use tasker_worker::worker::step_event_publisher::{
//!     StepEventPublisher, StepEventContext, PublishResult
//! };
//!
//! #[derive(Debug)]
//! pub struct PaymentEventPublisher {
//!     domain_publisher: Arc<DomainEventPublisher>,
//! }
//!
//! #[async_trait]
//! impl StepEventPublisher for PaymentEventPublisher {
//!     fn name(&self) -> &str {
//!         "PaymentEventPublisher"
//!     }
//!
//!     fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
//!         &self.domain_publisher
//!     }
//!
//!     async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
//!         let mut result = PublishResult::default();
//!
//!         if ctx.step_succeeded() {
//!             let payload = serde_json::json!({
//!                 "transaction_id": ctx.execution_result.result["transaction_id"],
//!                 "amount": ctx.execution_result.result["amount"],
//!             });
//!
//!             // Uses default impl from trait
//!             match self.publish_event(ctx, "payment.processed", payload).await {
//!                 Ok(event_id) => result.published.push(event_id),
//!                 Err(e) => result.errors.push(e.to_string()),
//!             }
//!         }
//!
//!         result
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tasker_shared::events::domain_events::{
    DomainEventPayload, DomainEventPublisher, EventMetadata,
};
use tasker_shared::messaging::execution_types::StepExecutionResult;
use tasker_shared::models::core::task_template::EventDeliveryMode;
use tasker_shared::types::TaskSequenceStep;
use tracing::{debug, warn};
use uuid::Uuid;

use super::event_router::EventRouter;

// ============================================================================
// Pure DTO: StepEventContext
// ============================================================================

/// Pure data transfer object for step event publishing
///
/// Contains all the information needed to publish domain events after step
/// execution. This is a lightweight struct with no behavior - all publishing
/// logic lives in the `StepEventPublisher` trait.
///
/// ## Design Notes
///
/// - **No Arc**: Publishers hold their own `Arc<DomainEventPublisher>`
/// - **No Methods**: Just data accessors, no side effects
/// - **Cheaply Created**: Only references task/step data, minimal overhead
#[derive(Debug, Clone)]
pub struct StepEventContext {
    /// The complete task sequence step that was executed
    pub task_sequence_step: TaskSequenceStep,

    /// The execution result from the step handler
    pub execution_result: StepExecutionResult,
}

impl StepEventContext {
    /// Create a new event context
    #[inline]
    pub fn new(
        task_sequence_step: TaskSequenceStep,
        execution_result: StepExecutionResult,
    ) -> Self {
        Self {
            task_sequence_step,
            execution_result,
        }
    }

    /// Get the step's declared events from the step definition
    #[inline]
    pub fn declared_events(
        &self,
    ) -> &[tasker_shared::models::core::task_template::EventDeclaration] {
        &self.task_sequence_step.step_definition.publishes_events
    }

    /// Check if an event is declared in the step's YAML configuration
    #[inline]
    pub fn is_event_declared(&self, event_name: &str) -> bool {
        self.task_sequence_step
            .step_definition
            .publishes_events
            .iter()
            .any(|e| e.name == event_name)
    }

    /// Get the task UUID
    #[inline]
    pub fn task_uuid(&self) -> Uuid {
        self.task_sequence_step.task.task.task_uuid
    }

    /// Get the step UUID
    #[inline]
    pub fn step_uuid(&self) -> Uuid {
        self.task_sequence_step.workflow_step.workflow_step_uuid
    }

    /// Get the step name
    #[inline]
    pub fn step_name(&self) -> &str {
        &self.task_sequence_step.workflow_step.name
    }

    /// Get the namespace
    #[inline]
    pub fn namespace(&self) -> &str {
        &self.task_sequence_step.task.namespace_name
    }

    /// Get the correlation ID
    #[inline]
    pub fn correlation_id(&self) -> Uuid {
        self.task_sequence_step.task.task.correlation_id
    }

    /// Check if the step execution succeeded
    #[inline]
    pub fn step_succeeded(&self) -> bool {
        self.execution_result.success
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of publishing events from a step event publisher
#[derive(Debug, Clone, Default)]
pub struct PublishResult {
    /// Successfully published event IDs
    pub published: Vec<Uuid>,

    /// Skipped events (condition not met, not declared, etc.)
    pub skipped: Vec<String>,

    /// Error messages for failed publications
    pub errors: Vec<String>,
}

impl PublishResult {
    /// Create a new empty result
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get count of successfully published events
    #[inline]
    pub fn published_count(&self) -> usize {
        self.published.len()
    }

    /// Get count of skipped events
    #[inline]
    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    /// Get count of errors
    #[inline]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Check if all publishing succeeded (no errors)
    #[inline]
    pub fn all_succeeded(&self) -> bool {
        self.errors.is_empty()
    }

    /// Check if there were any errors
    #[inline]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: PublishResult) {
        self.published.extend(other.published);
        self.skipped.extend(other.skipped);
        self.errors.extend(other.errors);
    }
}

/// Errors specific to step event publishing
#[derive(Debug, thiserror::Error)]
pub enum StepEventPublisherError {
    #[error("Failed to publish event '{event_name}': {reason}")]
    PublishFailed { event_name: String, reason: String },

    #[error("Event '{event_name}' is not declared in step definition")]
    UndeclaredEvent { event_name: String },

    #[error("Invalid payload for event '{event_name}': {reason}")]
    InvalidPayload { event_name: String, reason: String },

    #[error("Publisher error: {0}")]
    Other(String),
}

// ============================================================================
// StepEventPublisher Trait
// ============================================================================

/// Trait for custom domain event publishers
///
/// Implement this trait to create custom event publishing logic that goes
/// beyond what GenericEventPublisher handles automatically from YAML.
///
/// ## Design
///
/// - **Publisher Owns Arc**: Implementors hold `Arc<DomainEventPublisher>`
/// - **Default Impls**: `publish_event()`, `build_metadata()`, etc. are provided
/// - **Override `publish()`**: Custom logic goes in the required `publish()` method
///
/// ## Lifecycle
///
/// Publishers are invoked AFTER:
/// 1. Step handler completes execution
/// 2. Result is persisted to database
/// 3. Result is enqueued for orchestration
///
/// Therefore, event publishing failures CANNOT affect step execution.
///
/// ## Error Handling
///
/// Implementations should handle errors gracefully. The `PublishResult`
/// captures successes, skipped events, and errors. The worker logs errors
/// but continues processing.
///
/// ## Example
///
/// ```rust,no_run
/// use async_trait::async_trait;
/// use std::sync::Arc;
/// use std::fmt::Debug;
/// use tasker_shared::events::domain_events::DomainEventPublisher;
/// use tasker_worker::worker::step_event_publisher::{
///     StepEventPublisher, StepEventContext, PublishResult
/// };
///
/// #[derive(Debug)]
/// pub struct InventoryEventPublisher {
///     domain_publisher: Arc<DomainEventPublisher>,
/// }
///
/// impl InventoryEventPublisher {
///     pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
///         Self { domain_publisher }
///     }
/// }
///
/// #[async_trait]
/// impl StepEventPublisher for InventoryEventPublisher {
///     fn name(&self) -> &str {
///         "InventoryEventPublisher"
///     }
///
///     fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
///         &self.domain_publisher
///     }
///
///     async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
///         let mut result = PublishResult::new();
///
///         if !ctx.step_succeeded() {
///             result.skipped.push("inventory.updated".to_string());
///             return result;
///         }
///
///         let payload = serde_json::json!({
///             "sku": ctx.execution_result.result["sku"],
///             "quantity_changed": ctx.execution_result.result["quantity"],
///         });
///
///         match self.publish_event(ctx, "inventory.updated", payload).await {
///             Ok(event_id) => result.published.push(event_id),
///             Err(e) => result.errors.push(e.to_string()),
///         }
///
///         result
///     }
/// }
/// ```
#[async_trait]
pub trait StepEventPublisher: Send + Sync + std::fmt::Debug {
    /// Publisher name for logging and registry lookup
    fn name(&self) -> &str;

    /// Access to the domain event publisher
    ///
    /// Implementors should store `Arc<DomainEventPublisher>` as a field
    /// and return a reference here. This avoids per-invocation Arc cloning.
    fn domain_publisher(&self) -> &Arc<DomainEventPublisher>;

    /// Optional: Access to the event router for dual-path delivery
    ///
    /// Returns `Some` if the publisher supports dual-path routing (Durable/Fast).
    /// Default implementation returns None, meaning all events go to durable path.
    ///
    /// Note: `EventRouter` handles its own synchronization internally via mutex,
    /// so no external locking is required.
    ///
    /// Implementors that want dual-path support should:
    /// 1. Store `Arc<EventRouter>` as a field
    /// 2. Override this method to return a clone of the Arc
    fn event_router(&self) -> Option<Arc<EventRouter>> {
        None
    }

    /// Publish domain events based on step execution result
    ///
    /// Called after step execution completes. Receives the event context
    /// and can publish any number of events (or none).
    ///
    /// Use the default `publish_event()` method to actually publish events.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Pure DTO with execution data
    ///
    /// # Returns
    ///
    /// Result containing published event IDs, skipped events, and any errors
    async fn publish(&self, ctx: &StepEventContext) -> PublishResult;

    /// Optional: Check if this publisher should handle the given step
    ///
    /// Default implementation returns true (always handle).
    /// Override to add custom filtering logic.
    fn should_handle(&self, step_name: &str) -> bool {
        let _ = step_name;
        true
    }

    /// Publish a single domain event (always uses durable/PGMQ path)
    ///
    /// Default implementation that handles metadata construction, payload
    /// building, and actual publishing. Custom publishers call this from
    /// their `publish()` implementation.
    ///
    /// For dual-path routing based on delivery mode, use `publish_event_with_delivery_mode()`.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The event context (pure DTO)
    /// * `event_name` - Event name in dot notation (e.g., "payment.processed")
    /// * `business_payload` - Business-specific event data
    ///
    /// # Returns
    ///
    /// The generated event ID (UUID v7) on success
    async fn publish_event(
        &self,
        ctx: &StepEventContext,
        event_name: &str,
        business_payload: Value,
    ) -> Result<Uuid, StepEventPublisherError> {
        let metadata = self.build_metadata(ctx);
        let domain_payload = self.build_domain_payload(ctx, business_payload);

        debug!(
            publisher = %self.name(),
            event_name = %event_name,
            task_uuid = %metadata.task_uuid,
            step_uuid = ?metadata.step_uuid,
            "Publishing domain event (durable path)"
        );

        self.domain_publisher()
            .publish_event(event_name, domain_payload, metadata)
            .await
            .map_err(|e| StepEventPublisherError::PublishFailed {
                event_name: event_name.to_string(),
                reason: e.to_string(),
            })
    }

    /// Publish a single domain event with explicit delivery mode routing
    ///
    /// Routes events based on `EventDeliveryMode`:
    /// - `Durable`: Publishes to PGMQ (external consumers, audit trails)
    /// - `Fast`: Dispatches to in-process bus (metrics, telemetry, notifications)
    ///
    /// If no EventRouter is configured, falls back to durable path.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The event context (pure DTO)
    /// * `event_name` - Event name in dot notation
    /// * `business_payload` - Business-specific event data
    /// * `delivery_mode` - How to deliver the event
    ///
    /// # Returns
    ///
    /// The generated event ID (UUID v7) on success
    async fn publish_event_with_delivery_mode(
        &self,
        ctx: &StepEventContext,
        event_name: &str,
        business_payload: Value,
        delivery_mode: EventDeliveryMode,
    ) -> Result<Uuid, StepEventPublisherError> {
        let metadata = self.build_metadata(ctx);
        let domain_payload = self.build_domain_payload(ctx, business_payload);

        debug!(
            publisher = %self.name(),
            event_name = %event_name,
            delivery_mode = ?delivery_mode,
            task_uuid = %metadata.task_uuid,
            step_uuid = ?metadata.step_uuid,
            "Publishing domain event with delivery mode routing"
        );

        // If event router is available, use it for routing
        // Note: EventRouter handles its own synchronization internally
        if let Some(router) = self.event_router() {
            let outcome = router
                .route_event(delivery_mode, event_name, domain_payload, metadata)
                .await
                .map_err(|e| StepEventPublisherError::PublishFailed {
                    event_name: event_name.to_string(),
                    reason: e.to_string(),
                })?;
            return Ok(outcome.event_id());
        }

        // Fallback: no router, use durable path directly
        debug!(
            publisher = %self.name(),
            event_name = %event_name,
            "No EventRouter configured - falling back to durable path"
        );

        self.domain_publisher()
            .publish_event(event_name, domain_payload, metadata)
            .await
            .map_err(|e| StepEventPublisherError::PublishFailed {
                event_name: event_name.to_string(),
                reason: e.to_string(),
            })
    }

    /// Build event metadata from the context
    ///
    /// Default implementation extracts standard metadata. Override if you
    /// need custom metadata fields.
    fn build_metadata(&self, ctx: &StepEventContext) -> EventMetadata {
        EventMetadata {
            task_uuid: ctx.task_uuid(),
            step_uuid: Some(ctx.step_uuid()),
            step_name: Some(ctx.step_name().to_string()),
            namespace: ctx.namespace().to_string(),
            correlation_id: ctx.correlation_id(),
            fired_at: chrono::Utc::now(),
            fired_by: self.name().to_string(),
        }
    }

    /// Build the full domain event payload
    ///
    /// Default implementation includes full execution context. Override if
    /// you need to modify the payload structure.
    fn build_domain_payload(
        &self,
        ctx: &StepEventContext,
        business_payload: Value,
    ) -> DomainEventPayload {
        DomainEventPayload {
            task_sequence_step: ctx.task_sequence_step.clone(),
            execution_result: ctx.execution_result.clone(),
            payload: business_payload,
        }
    }
}

// ============================================================================
// DefaultDomainEventPublisher
// ============================================================================

/// Generic implementation that uses YAML-declared events with dual-path routing
///
/// This is the default publisher used when no custom publisher is specified.
/// It publishes events based on the `publishes_events` YAML configuration,
/// respecting conditions (success/failure/always), delivery modes (durable/fast),
/// and validating schemas.
///
/// ## Dual-Path Delivery
///
/// When an `EventRouter` is configured, events are routed based on their
/// `delivery_mode` from YAML:
/// - `durable`: Published to PGMQ for external consumers
/// - `fast`: Dispatched to in-process bus for internal callbacks
///
/// Without an `EventRouter`, all events use the durable (PGMQ) path.
///
/// Note: `EventRouter` handles its own synchronization internally via mutex,
/// so no external RwLock wrapper is needed.
pub struct DefaultDomainEventPublisher {
    /// The domain event publisher (owned Arc, not cloned per-call)
    domain_publisher: Arc<DomainEventPublisher>,
    /// Optional event router for dual-path delivery
    event_router: Option<Arc<EventRouter>>,
}

// Manual Debug implementation because EventRouter contains closures indirectly
impl std::fmt::Debug for DefaultDomainEventPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultDomainEventPublisher")
            .field("has_event_router", &self.event_router.is_some())
            .finish()
    }
}

impl Clone for DefaultDomainEventPublisher {
    fn clone(&self) -> Self {
        Self {
            domain_publisher: self.domain_publisher.clone(),
            event_router: self.event_router.clone(),
        }
    }
}

impl DefaultDomainEventPublisher {
    /// Create a new generic publisher (durable-only path)
    ///
    /// All events will be published to PGMQ regardless of delivery_mode.
    pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
        Self {
            domain_publisher,
            event_router: None,
        }
    }

    /// Create a new generic publisher with dual-path routing
    ///
    /// Events will be routed based on their `delivery_mode`:
    /// - `durable`: Published to PGMQ
    /// - `fast`: Dispatched to in-process bus
    ///
    /// Note: `EventRouter` handles its own synchronization internally,
    /// so no external locking wrapper is needed.
    pub fn with_event_router(
        domain_publisher: Arc<DomainEventPublisher>,
        event_router: Arc<EventRouter>,
    ) -> Self {
        Self {
            domain_publisher,
            event_router: Some(event_router),
        }
    }
}

#[async_trait]
impl StepEventPublisher for DefaultDomainEventPublisher {
    fn name(&self) -> &str {
        "DefaultDomainEventPublisher"
    }

    fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
        &self.domain_publisher
    }

    fn event_router(&self) -> Option<Arc<EventRouter>> {
        self.event_router.clone()
    }

    async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
        let mut result = PublishResult::new();

        let declared_events = ctx.declared_events();
        if declared_events.is_empty() {
            debug!(
                step_name = %ctx.step_name(),
                "No events declared for step - skipping"
            );
            return result;
        }

        let step_succeeded = ctx.step_succeeded();

        for event_decl in declared_events {
            // Check publication condition
            if !event_decl.should_publish(step_succeeded) {
                debug!(
                    event_name = %event_decl.name,
                    condition = ?event_decl.condition,
                    step_succeeded = step_succeeded,
                    "Skipping event - condition not met"
                );
                result.skipped.push(event_decl.name.clone());
                continue;
            }

            // For generic publisher, use the execution result as payload
            let business_payload = ctx.execution_result.result.clone();

            // Validate payload against schema
            if let Err(validation_error) = event_decl.validate_payload(&business_payload) {
                warn!(
                    event_name = %event_decl.name,
                    error = %validation_error,
                    "Payload validation failed - skipping event"
                );
                result.errors.push(format!(
                    "Schema validation failed for '{}': {}",
                    event_decl.name, validation_error
                ));
                continue;
            }

            // Publish the event using delivery mode routing
            // Uses publish_event_with_delivery_mode to respect YAML delivery_mode
            let publish_result = self
                .publish_event_with_delivery_mode(
                    ctx,
                    &event_decl.name,
                    business_payload,
                    event_decl.delivery_mode,
                )
                .await;

            match publish_result {
                Ok(event_id) => {
                    debug!(
                        event_name = %event_decl.name,
                        event_id = %event_id,
                        delivery_mode = ?event_decl.delivery_mode,
                        "Event published successfully"
                    );
                    result.published.push(event_id);
                }
                Err(e) => {
                    warn!(
                        event_name = %event_decl.name,
                        error = %e,
                        "Failed to publish event"
                    );
                    result.errors.push(e.to_string());
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_result_default() {
        let result = PublishResult::default();
        assert!(result.published.is_empty());
        assert!(result.skipped.is_empty());
        assert!(result.errors.is_empty());
        assert!(result.all_succeeded());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_publish_result_counts() {
        let mut result = PublishResult::new();
        result.published.push(Uuid::new_v4());
        result.published.push(Uuid::new_v4());
        result.skipped.push("skipped.event".to_string());
        result.errors.push("Error 1".to_string());

        assert_eq!(result.published_count(), 2);
        assert_eq!(result.skipped_count(), 1);
        assert_eq!(result.error_count(), 1);
        assert!(!result.all_succeeded());
        assert!(result.has_errors());
    }

    #[test]
    fn test_publish_result_merge() {
        let mut result1 = PublishResult::new();
        result1.published.push(Uuid::new_v4());
        result1.skipped.push("event1".to_string());

        let mut result2 = PublishResult::new();
        result2.published.push(Uuid::new_v4());
        result2.errors.push("Error".to_string());

        result1.merge(result2);

        assert_eq!(result1.published_count(), 2);
        assert_eq!(result1.skipped_count(), 1);
        assert_eq!(result1.error_count(), 1);
    }
}
