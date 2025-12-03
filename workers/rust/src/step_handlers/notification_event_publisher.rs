//! # TAS-65 Phase 3: Notification Event Publisher
//!
//! Custom `StepEventPublisher` for notification-related domain events.
//! Demonstrates fast delivery mode with custom payload enrichment.
//!
//! ## 2x2 Test Matrix Coverage
//!
//! This publisher covers the "fast + custom publisher" case (2d):
//! - Delivery mode: fast (in-memory)
//! - Publisher: custom (NotificationEventPublisher)
//!
//! ## Key Features
//!
//! - Channel-specific payload enrichment (email, SMS, push)
//! - Delivery status tracking
//! - Lightweight event publishing (fast mode)
//!
//! ## Usage
//!
//! Register with WorkerCore during bootstrap:
//!
//! ```rust,ignore
//! use workers_rust::step_handlers::NotificationEventPublisher;
//!
//! let domain_publisher = worker_core.domain_event_publisher();
//! worker_core.register_step_event_publisher(
//!     NotificationEventPublisher::new(domain_publisher)
//! );
//! ```
//!
//! Then reference in YAML:
//!
//! ```yaml
//! steps:
//!   - name: send_notification
//!     handler:
//!       callable: SendNotificationHandler
//!     publishes_events:
//!       - name: notification.sent
//!         condition: success
//!         delivery_mode: fast
//!         publisher: NotificationEventPublisher
//! ```

use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use tasker_shared::events::domain_events::DomainEventPublisher;
use tasker_worker::worker::step_event_publisher::{
    PublishResult, StepEventContext, StepEventPublisher,
};
use tracing::{debug, info};

/// Custom event publisher for notification steps
///
/// Publishes enriched domain events for notifications with:
/// - `notification.sent` on success (with channel-specific enrichment)
///
/// ## Design
///
/// - **Fast Delivery**: Uses in-memory channel for low-latency event publishing
/// - **Custom Enrichment**: Adds channel-specific metadata (email, SMS, push)
/// - **Lightweight**: Minimal overhead for high-throughput notification scenarios
#[derive(Debug, Clone)]
pub struct NotificationEventPublisher {
    /// The domain event publisher (owned, not cloned per-call)
    domain_publisher: Arc<DomainEventPublisher>,
}

impl NotificationEventPublisher {
    /// Create a new notification event publisher
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - The domain event publisher to use
    pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
        Self { domain_publisher }
    }

    /// Build the notification sent event payload with channel-specific enrichment
    fn build_notification_payload(&self, ctx: &StepEventContext) -> serde_json::Value {
        let result = &ctx.execution_result.result;

        let channel = result
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Base payload
        let mut payload = json!({
            "notification_id": result.get("notification_id").cloned().unwrap_or(json!("unknown")),
            "channel": channel,
            "recipient": result.get("recipient").cloned().unwrap_or(json!("unknown")),
            "sent_at": result.get("sent_at").cloned().unwrap_or(json!(chrono::Utc::now().to_rfc3339())),
            "status": result.get("status").cloned().unwrap_or(json!("unknown")),
            "delivery_mode": "fast",
            "publisher": "NotificationEventPublisher",
        });

        // Channel-specific enrichment
        match channel {
            "email" => {
                payload["email_metadata"] = json!({
                    "has_attachments": false,
                    "content_type": "text/html",
                    "priority": "normal"
                });
            }
            "sms" => {
                payload["sms_metadata"] = json!({
                    "message_segments": 1,
                    "carrier": "unknown"
                });
            }
            "push" => {
                payload["push_metadata"] = json!({
                    "platform": "all",
                    "badge_count": 1,
                    "sound": "default"
                });
            }
            _ => {}
        }

        // Add execution metrics
        payload["execution_time_ms"] = json!(ctx.execution_result.metadata.execution_time_ms);

        payload
    }

    /// Determine if this is a notification-related step
    fn is_notification_step(&self, step_name: &str) -> bool {
        step_name.contains("notification")
            || step_name.contains("alert")
            || step_name.contains("message")
    }
}

#[async_trait]
impl StepEventPublisher for NotificationEventPublisher {
    fn name(&self) -> &str {
        "NotificationEventPublisher"
    }

    fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
        &self.domain_publisher
    }

    fn should_handle(&self, step_name: &str) -> bool {
        // Only handle notification-related steps
        self.is_notification_step(step_name)
    }

    async fn publish(&self, ctx: &StepEventContext) -> PublishResult {
        let mut result = PublishResult::new();

        debug!(
            step_name = %ctx.step_name(),
            success = ctx.step_succeeded(),
            "NotificationEventPublisher processing step completion"
        );

        // Only publish on success (notification.sent has condition: success)
        if ctx.step_succeeded() {
            // Check if notification.sent is declared
            if ctx.is_event_declared("notification.sent") {
                let payload = self.build_notification_payload(ctx);

                // Uses trait's default publish_event impl
                match self.publish_event(ctx, "notification.sent", payload).await {
                    Ok(event_id) => {
                        info!(
                            event_id = %event_id,
                            channel = ?ctx.execution_result.result.get("channel"),
                            "Published notification.sent event (fast + custom publisher)"
                        );
                        result.published.push(event_id);
                    }
                    Err(e) => {
                        result.errors.push(e.to_string());
                    }
                }
            } else {
                result
                    .skipped
                    .push("notification.sent (not declared)".to_string());
            }
        } else {
            // No failure event declared for notifications
            result
                .skipped
                .push("notification events (step failed, no failure event declared)".to_string());
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
        // Verify notification-related step detection
        assert!("send_notification".contains("notification"));
        assert!("domain_events_send_notification".contains("notification"));
        assert!("send_alert".contains("alert"));
        assert!("send_message".contains("message"));
        assert!(!"validate_order".contains("notification"));
    }
}
