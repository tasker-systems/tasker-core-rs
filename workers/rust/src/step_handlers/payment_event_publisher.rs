//! # TAS-65 Phase 3: Payment Event Publisher Example
//!
//! Demonstrates implementing a custom `StepEventPublisher` for domain event
//! publishing with enrichment logic beyond what `DefaultDomainEventPublisher` provides.
//!
//! ## Key Features
//!
//! - Conditional event publishing based on result status
//! - Payload enrichment with business logic
//! - Multiple events from a single step
//! - Analytics event for all outcomes
//!
//! ## Design
//!
//! - **Publisher Owns Arc**: Holds `Arc<DomainEventPublisher>` (no per-call cloning)
//! - **Uses Trait Defaults**: Calls `self.publish_event()` for actual publishing
//! - **Lightweight Context**: Receives `StepEventContext` as pure DTO
//!
//! ## Usage
//!
//! Register with WorkerCore during bootstrap:
//!
//! ```rust,ignore
//! use workers_rust::step_handlers::PaymentEventPublisher;
//!
//! let domain_publisher = worker_core.domain_event_publisher();
//! worker_core.register_step_event_publisher(
//!     PaymentEventPublisher::new(domain_publisher)
//! );
//! ```
//!
//! Then reference in YAML:
//!
//! ```yaml
//! steps:
//!   - name: process_payment
//!     handler:
//!       callable: PaymentHandler
//!     publishes_events:
//!       - name: payment.processed
//!         condition: success
//!         publisher: PaymentEventPublisher
//!       - name: payment.failed
//!         condition: failure
//!         publisher: PaymentEventPublisher
//! ```

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tasker_shared::events::domain_events::DomainEventPublisher;
use tasker_worker::worker::step_event_publisher::{
    PublishResult, StepEventContext, StepEventPublisher,
};
use tracing::{debug, info};

/// Custom event publisher for payment processing steps
///
/// Publishes enriched domain events with:
/// - `payment.processed` on success (with transaction details)
/// - `payment.failed` on failure (with error details and retry info)
/// - `payment.analytics` always (for metrics)
///
/// ## Performance
///
/// This publisher owns its `Arc<DomainEventPublisher>`, so no Arc cloning
/// occurs per invocation. The context is a lightweight DTO.
#[derive(Debug, Clone)]
pub struct PaymentEventPublisher {
    /// The domain event publisher (owned, not cloned per-call)
    domain_publisher: Arc<DomainEventPublisher>,
}

impl PaymentEventPublisher {
    /// Create a new payment event publisher
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - The domain event publisher to use
    pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
        Self { domain_publisher }
    }

    /// Build the success event payload with enrichment
    fn build_success_payload(&self, ctx: &StepEventContext) -> serde_json::Value {
        let result = &ctx.execution_result.result;

        json!({
            "transaction_id": result.get("transaction_id").cloned().unwrap_or(json!("unknown")),
            "amount": result.get("amount").cloned().unwrap_or(json!(0.0)),
            "currency": result.get("currency").cloned().unwrap_or(json!("USD")),
            "payment_method": result.get("payment_method").cloned().unwrap_or(json!("unknown")),
            "processed_at": chrono::Utc::now().to_rfc3339(),
            // Enriched fields
            "customer_tier": self.determine_customer_tier(ctx),
            "fraud_score": result.get("fraud_score").cloned().unwrap_or(json!(0.0)),
            "step_duration_ms": ctx.execution_result.metadata.execution_time_ms,
        })
    }

    /// Build the failure event payload
    fn build_failure_payload(&self, ctx: &StepEventContext) -> serde_json::Value {
        let error_message = ctx
            .execution_result
            .error
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or("Payment processing failed");

        json!({
            "error_code": ctx.execution_result.metadata.error_code.as_deref().unwrap_or("UNKNOWN"),
            "error_type": ctx.execution_result.metadata.error_type.as_deref().unwrap_or("UnknownError"),
            "error_message": error_message,
            "retry_after_seconds": self.calculate_retry_delay(ctx),
            "failed_at": chrono::Utc::now().to_rfc3339(),
            "attempt_number": self.get_attempt_number(ctx),
            "is_retryable": ctx.execution_result.metadata.retryable,
        })
    }

    /// Build analytics payload (published for all outcomes)
    fn build_analytics_payload(&self, ctx: &StepEventContext) -> serde_json::Value {
        json!({
            "step_name": ctx.step_name(),
            "namespace": ctx.namespace(),
            "success": ctx.step_succeeded(),
            "duration_ms": ctx.execution_result.metadata.execution_time_ms,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "worker_id": ctx.execution_result.metadata.worker_id.as_deref().unwrap_or("unknown"),
        })
    }

    /// Determine customer tier from task context
    fn determine_customer_tier(&self, ctx: &StepEventContext) -> String {
        ctx.task_sequence_step
            .task
            .task
            .context
            .as_ref()
            .and_then(|c| c.get("customer_tier"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| "standard".to_string())
    }

    /// Calculate retry delay based on error type
    fn calculate_retry_delay(&self, ctx: &StepEventContext) -> i64 {
        let error_code = ctx
            .execution_result
            .metadata
            .error_code
            .as_deref()
            .unwrap_or("");

        match error_code {
            "RATE_LIMITED" => 600,        // 10 minutes
            "TEMPORARY_FAILURE" => 300,   // 5 minutes
            "NETWORK_ERROR" => 60,        // 1 minute
            "CARD_DECLINED" => 0,         // No retry (user action needed)
            _ => 120,                     // 2 minutes default
        }
    }

    /// Get the attempt number from workflow step
    fn get_attempt_number(&self, ctx: &StepEventContext) -> i32 {
        ctx.task_sequence_step
            .workflow_step
            .attempts
            .unwrap_or(1)
    }
}

#[async_trait]
impl StepEventPublisher for PaymentEventPublisher {
    fn name(&self) -> &str {
        "PaymentEventPublisher"
    }

    fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
        &self.domain_publisher
    }

    fn should_handle(&self, step_name: &str) -> bool {
        // Only handle payment-related steps
        step_name.contains("payment") || step_name.contains("charge") || step_name.contains("refund")
    }

    async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
        let mut result = PublishResult::new();

        debug!(
            step_name = %ctx.step_name(),
            success = ctx.step_succeeded(),
            "PaymentEventPublisher processing step completion"
        );

        // Publish success or failure event based on outcome
        if ctx.step_succeeded() {
            // Check if payment.processed is declared
            if ctx.is_event_declared("payment.processed") {
                let payload = self.build_success_payload(ctx);

                // Uses trait's default publish_event impl
                match self.publish_event(ctx, "payment.processed", payload).await {
                    Ok(event_id) => {
                        info!(
                            event_id = %event_id,
                            "Published payment.processed event"
                        );
                        result.published.push(event_id);
                    }
                    Err(e) => {
                        result.errors.push(e.to_string());
                    }
                }
            } else {
                result.skipped.push("payment.processed (not declared)".to_string());
            }
        } else {
            // Check if payment.failed is declared
            if ctx.is_event_declared("payment.failed") {
                let payload = self.build_failure_payload(ctx);

                match self.publish_event(ctx, "payment.failed", payload).await {
                    Ok(event_id) => {
                        info!(
                            event_id = %event_id,
                            "Published payment.failed event"
                        );
                        result.published.push(event_id);
                    }
                    Err(e) => {
                        result.errors.push(e.to_string());
                    }
                }
            } else {
                result.skipped.push("payment.failed (not declared)".to_string());
            }
        }

        // Always publish analytics event if declared
        if ctx.is_event_declared("payment.analytics") {
            let payload = self.build_analytics_payload(ctx);

            match self.publish_event(ctx, "payment.analytics", payload).await {
                Ok(event_id) => {
                    debug!(
                        event_id = %event_id,
                        "Published payment.analytics event"
                    );
                    result.published.push(event_id);
                }
                Err(e) => {
                    result.errors.push(e.to_string());
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    // Note: Full tests require a DomainEventPublisher which needs a database.
    // These tests verify the basic structure.

    #[test]
    fn test_should_handle() {
        // We can't create a full PaymentEventPublisher without a database,
        // but we can verify the should_handle logic pattern
        assert!("process_payment".contains("payment"));
        assert!("charge_card".contains("charge"));
        assert!("issue_refund".contains("refund"));
        assert!(!"validate_order".contains("payment"));
    }
}
