//! # Domain Event Test Helpers
//!
//! TAS-65: Provides test observability for domain events in integration tests.
//!
//! ## Purpose
//!
//! When testing domain event publishing, we need to verify that events were actually
//! published to the correct queues with the expected payloads. This module provides
//! helpers for:
//!
//! 1. **Durable Event Inspection** - Query PGMQ directly (bypasses VTT)
//! 2. **Event Tracking** - Track published msg_ids for isolated cleanup
//! 3. **Event Cleanup** - Delete only tracked events, not all events in namespace
//!
//! ## Important Notes
//!
//! - Uses direct SQL queries to bypass PGMQ visibility timeout (VTT)
//! - Only works in integration tests (same database access)
//! - Does NOT work for fast/in-process events (those are in-memory only)
//! - Test isolation is maintained by tracking and cleaning up only our own events
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tests::common::domain_event_test_helpers::DurableEventCapture;
//!
//! #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
//! async fn test_event_publishing(pool: PgPool) -> Result<()> {
//!     let mut capture = DurableEventCapture::new(pool.clone());
//!
//!     // Run your workflow that publishes events...
//!
//!     // Inspect published events
//!     let events = capture.get_events_in_namespace("payments").await?;
//!     assert_eq!(events.len(), 1);
//!     assert!(events[0].event_name.contains("payment.processed"));
//!
//!     // Cleanup only our tracked events
//!     capture.cleanup().await?;
//!
//!     Ok(())
//! }
//! ```

#![expect(dead_code, reason = "Test module for domain event observability in integration tests")]

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Captured domain event from PGMQ queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedDomainEvent {
    /// PGMQ message ID (for cleanup)
    pub msg_id: i64,
    /// Queue name where event was found
    pub queue_name: String,
    /// Full event payload
    pub payload: Value,
    /// Event name extracted from payload
    pub event_name: String,
    /// Event ID (UUID) from payload
    pub event_id: Option<Uuid>,
    /// Task UUID from metadata
    pub task_uuid: Option<Uuid>,
    /// Step UUID from metadata
    pub step_uuid: Option<Uuid>,
    /// Correlation ID from metadata
    pub correlation_id: Option<Uuid>,
    /// When the event was enqueued
    pub enqueued_at: DateTime<Utc>,
    /// VTT (visibility timeout) - when message becomes visible again
    pub vt: DateTime<Utc>,
}

impl CapturedDomainEvent {
    /// Extract the business payload (step result data)
    pub fn business_payload(&self) -> Option<&Value> {
        self.payload.get("payload").and_then(|p| p.get("payload"))
    }

    /// Check if this event matches an expected name pattern
    pub fn matches_name(&self, pattern: &str) -> bool {
        self.event_name.contains(pattern)
    }

    /// Get the execution result from the payload
    pub fn execution_result(&self) -> Option<&Value> {
        self.payload
            .get("payload")
            .and_then(|p| p.get("execution_result"))
    }

    /// Check if the step execution was successful
    pub fn step_succeeded(&self) -> Option<bool> {
        self.execution_result()
            .and_then(|r| r.get("success"))
            .and_then(|s| s.as_bool())
    }
}

/// Captures and tracks durable domain events for test verification
///
/// ## Key Features
///
/// - **VTT Bypass**: Direct SQL queries see all messages regardless of visibility
/// - **Isolated Cleanup**: Only deletes events we've captured and tracked
/// - **Rich Inspection**: Extract and verify event contents easily
pub struct DurableEventCapture {
    pool: PgPool,
    /// Tracked message IDs per queue for cleanup
    tracked_messages: HashMap<String, Vec<i64>>,
}

impl DurableEventCapture {
    /// Create a new event capture instance
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tracked_messages: HashMap::new(),
        }
    }

    /// Get all events in a namespace's domain event queue
    ///
    /// Uses direct SQL to bypass VTT and see all messages including
    /// those currently being processed.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace (queue name will be `{namespace}_domain_events`)
    ///
    /// # Returns
    ///
    /// All events in the queue, regardless of visibility timeout
    pub async fn get_events_in_namespace(
        &mut self,
        namespace: &str,
    ) -> Result<Vec<CapturedDomainEvent>> {
        let queue_name = format!("{}_domain_events", namespace);
        self.get_events_in_queue(&queue_name).await
    }

    /// Get all events in a specific queue
    ///
    /// Uses direct SQL to bypass VTT.
    pub async fn get_events_in_queue(
        &mut self,
        queue_name: &str,
    ) -> Result<Vec<CapturedDomainEvent>> {
        // Direct query to PGMQ queue table - bypasses visibility timeout
        // Queue tables are named: pgmq.q_{queue_name}
        let table_name = format!("pgmq.q_{}", queue_name);

        // Check if queue table exists first
        let table_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT FROM pg_tables
                WHERE schemaname = 'pgmq' AND tablename = $1
            )",
        )
        .bind(format!("q_{}", queue_name))
        .fetch_one(&self.pool)
        .await?;

        if !table_exists {
            tracing::debug!(queue_name = %queue_name, "Queue table does not exist yet");
            return Ok(Vec::new());
        }

        // Query all messages in the queue (bypasses VTT)
        let query = format!(
            r#"
            SELECT msg_id, message, enqueued_at, vt
            FROM {}
            ORDER BY msg_id ASC
            "#,
            table_name
        );

        let rows = sqlx::query_as::<_, (i64, Value, DateTime<Utc>, DateTime<Utc>)>(&query)
            .fetch_all(&self.pool)
            .await?;

        let mut events = Vec::new();

        for (msg_id, message, enqueued_at, vt) in rows {
            // Track this message for cleanup
            self.tracked_messages
                .entry(queue_name.to_string())
                .or_default()
                .push(msg_id);

            // Extract event details from payload
            let event_name = message
                .get("event_name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();

            let event_id = message
                .get("event_id")
                .and_then(|id| id.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let metadata = message.get("metadata");

            let task_uuid = metadata
                .and_then(|m| m.get("task_uuid"))
                .and_then(|id| id.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let step_uuid = metadata
                .and_then(|m| m.get("step_uuid"))
                .and_then(|id| id.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let correlation_id = metadata
                .and_then(|m| m.get("correlation_id"))
                .and_then(|id| id.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            events.push(CapturedDomainEvent {
                msg_id,
                queue_name: queue_name.to_string(),
                payload: message,
                event_name,
                event_id,
                task_uuid,
                step_uuid,
                correlation_id,
                enqueued_at,
                vt,
            });
        }

        tracing::info!(
            queue_name = %queue_name,
            event_count = events.len(),
            "📬 Captured domain events from queue"
        );

        Ok(events)
    }

    /// Get events filtered by event name pattern
    pub async fn get_events_matching(
        &mut self,
        namespace: &str,
        name_pattern: &str,
    ) -> Result<Vec<CapturedDomainEvent>> {
        let all_events = self.get_events_in_namespace(namespace).await?;
        Ok(all_events
            .into_iter()
            .filter(|e| e.matches_name(name_pattern))
            .collect())
    }

    /// Get events for a specific task
    pub async fn get_events_for_task(
        &mut self,
        namespace: &str,
        task_uuid: Uuid,
    ) -> Result<Vec<CapturedDomainEvent>> {
        let all_events = self.get_events_in_namespace(namespace).await?;
        Ok(all_events
            .into_iter()
            .filter(|e| e.task_uuid == Some(task_uuid))
            .collect())
    }

    /// Get event count in a namespace
    pub async fn count_events_in_namespace(&mut self, namespace: &str) -> Result<usize> {
        let events = self.get_events_in_namespace(namespace).await?;
        Ok(events.len())
    }

    /// Assert that a specific event was published
    pub async fn assert_event_published(
        &mut self,
        namespace: &str,
        event_name_pattern: &str,
    ) -> Result<CapturedDomainEvent> {
        let events = self
            .get_events_matching(namespace, event_name_pattern)
            .await?;

        if events.is_empty() {
            anyhow::bail!(
                "Expected event matching '{}' in namespace '{}', but none found",
                event_name_pattern,
                namespace
            );
        }

        Ok(events.into_iter().next().unwrap())
    }

    /// Assert that no events matching a pattern were published
    pub async fn assert_no_event_published(
        &mut self,
        namespace: &str,
        event_name_pattern: &str,
    ) -> Result<()> {
        let events = self
            .get_events_matching(namespace, event_name_pattern)
            .await?;

        if !events.is_empty() {
            anyhow::bail!(
                "Expected no events matching '{}' in namespace '{}', but found {}",
                event_name_pattern,
                namespace,
                events.len()
            );
        }

        Ok(())
    }

    /// Cleanup only the events we've tracked
    ///
    /// This is critical for test isolation - we only delete events that
    /// we've explicitly captured, not all events in the namespace.
    pub async fn cleanup(&mut self) -> Result<()> {
        let mut total_deleted = 0;

        for (queue_name, msg_ids) in &self.tracked_messages {
            for msg_id in msg_ids {
                // Use pgmq.delete() to remove the message
                let result = sqlx::query("SELECT pgmq.delete($1, $2)")
                    .bind(queue_name)
                    .bind(*msg_id)
                    .execute(&self.pool)
                    .await;

                match result {
                    Ok(_) => {
                        total_deleted += 1;
                        tracing::debug!(
                            queue_name = %queue_name,
                            msg_id = %msg_id,
                            "🗑️ Deleted tracked event"
                        );
                    }
                    Err(e) => {
                        // Message might already be deleted by another process - log but don't fail
                        tracing::debug!(
                            queue_name = %queue_name,
                            msg_id = %msg_id,
                            error = %e,
                            "⚠️ Could not delete message (may already be consumed)"
                        );
                    }
                }
            }
        }

        tracing::info!(
            total_deleted = total_deleted,
            queues_cleaned = self.tracked_messages.len(),
            "🧹 Cleaned up tracked domain events"
        );

        self.tracked_messages.clear();
        Ok(())
    }

    /// Get the count of tracked messages awaiting cleanup
    pub fn tracked_message_count(&self) -> usize {
        self.tracked_messages.values().map(|v| v.len()).sum()
    }
}

/// Extension trait to add event capture to LifecycleTestManager
pub trait EventCaptureExtension {
    /// Create a durable event capture associated with this test manager
    fn create_event_capture(&self) -> DurableEventCapture;
}

// We'll implement this in the lifecycle_test_manager.rs file to avoid circular deps

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_captured_event_matches_name() {
        let event = CapturedDomainEvent {
            msg_id: 1,
            queue_name: "test_domain_events".to_string(),
            payload: serde_json::json!({}),
            event_name: "payment.processed".to_string(),
            event_id: None,
            task_uuid: None,
            step_uuid: None,
            correlation_id: None,
            enqueued_at: Utc::now(),
            vt: Utc::now(),
        };

        assert!(event.matches_name("payment"));
        assert!(event.matches_name("processed"));
        assert!(event.matches_name("payment.processed"));
        assert!(!event.matches_name("order"));
    }
}
