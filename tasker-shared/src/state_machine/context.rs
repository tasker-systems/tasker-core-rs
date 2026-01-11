//! # Transition Context
//!
//! Context data passed to state machine transitions for audit enrichment.
//!
//! This module provides the `TransitionContext` struct which carries attribution
//! data (worker_uuid, correlation_id) through state machine transitions. This
//! context is merged into transition metadata and used by the SQL trigger to
//! populate the `tasker.workflow_step_result_audit` table.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::state_machine::TransitionContext;
//! use uuid::Uuid;
//!
//! // Create context with worker attribution
//! let context = TransitionContext::with_worker(
//!     worker_uuid,
//!     Some(correlation_id),
//! );
//!
//! // Pass to state machine transition
//! state_machine.transition_with_context(event, Some(context)).await?;
//! ```
//!
//! ## SOC2 Compliance
//!
//! The attribution data captured in `TransitionContext` enables SOC2-compliant
//! audit trails by recording:
//!
//! - **worker_uuid**: Which worker instance processed the step
//! - **correlation_id**: Distributed tracing identifier for request correlation

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Context data passed to state machine transitions for audit enrichment.
///
/// This struct carries attribution data that gets merged into transition metadata.
/// The SQL trigger (`create_step_result_audit`) extracts these values to populate
/// the audit table with full attribution for SOC2 compliance.
///
/// # Fields
///
/// * `worker_uuid` - UUID of the worker instance that processed the step
/// * `correlation_id` - Distributed tracing correlation ID for request tracking
///
/// # Examples
///
/// ```rust
/// use tasker_shared::state_machine::TransitionContext;
/// use uuid::Uuid;
///
/// // Create context with full attribution
/// let worker_id = Uuid::new_v4();
/// let correlation_id = Uuid::new_v4();
/// let context = TransitionContext::with_worker(worker_id, Some(correlation_id));
///
/// assert_eq!(context.worker_uuid, Some(worker_id));
/// assert_eq!(context.correlation_id, Some(correlation_id));
///
/// // Create empty context (backward compatibility)
/// let empty = TransitionContext::default();
/// assert!(empty.worker_uuid.is_none());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionContext {
    /// UUID of the worker instance that processed this step.
    /// Used for attribution in audit trails and debugging distributed issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_uuid: Option<Uuid>,

    /// Correlation ID for distributed tracing.
    /// Links this transition to the original request across service boundaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
}

impl TransitionContext {
    /// Create a new `TransitionContext` with worker attribution.
    ///
    /// # Arguments
    ///
    /// * `worker_uuid` - UUID of the worker processing the step
    /// * `correlation_id` - Optional correlation ID for distributed tracing
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tasker_shared::state_machine::TransitionContext;
    /// use uuid::Uuid;
    ///
    /// let worker_id = Uuid::new_v4();
    /// let context = TransitionContext::with_worker(worker_id, None);
    /// assert_eq!(context.worker_uuid, Some(worker_id));
    /// ```
    #[must_use]
    pub fn with_worker(worker_uuid: Uuid, correlation_id: Option<Uuid>) -> Self {
        Self {
            worker_uuid: Some(worker_uuid),
            correlation_id,
        }
    }

    /// Create a new `TransitionContext` with only correlation ID.
    ///
    /// Useful when the caller has a correlation ID but not a worker UUID
    /// (e.g., orchestration-side transitions).
    ///
    /// # Arguments
    ///
    /// * `correlation_id` - Correlation ID for distributed tracing
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tasker_shared::state_machine::TransitionContext;
    /// use uuid::Uuid;
    ///
    /// let correlation_id = Uuid::new_v4();
    /// let context = TransitionContext::with_correlation_id(correlation_id);
    /// assert!(context.worker_uuid.is_none());
    /// assert_eq!(context.correlation_id, Some(correlation_id));
    /// ```
    #[must_use]
    pub fn with_correlation_id(correlation_id: Uuid) -> Self {
        Self {
            worker_uuid: None,
            correlation_id: Some(correlation_id),
        }
    }

    /// Check if this context has any attribution data.
    ///
    /// Returns `true` if either `worker_uuid` or `correlation_id` is set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tasker_shared::state_machine::TransitionContext;
    /// use uuid::Uuid;
    ///
    /// let empty = TransitionContext::default();
    /// assert!(!empty.has_attribution());
    ///
    /// let with_worker = TransitionContext::with_worker(Uuid::new_v4(), None);
    /// assert!(with_worker.has_attribution());
    /// ```
    #[must_use]
    pub fn has_attribution(&self) -> bool {
        self.worker_uuid.is_some() || self.correlation_id.is_some()
    }

    /// Merge this context into a JSON metadata object.
    ///
    /// Adds `worker_uuid` and `correlation_id` fields to the provided metadata.
    /// If either field is `None`, it is not added to the metadata.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Existing metadata to merge into
    ///
    /// # Returns
    ///
    /// The metadata with attribution fields added
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tasker_shared::state_machine::TransitionContext;
    /// use serde_json::json;
    /// use uuid::Uuid;
    ///
    /// let worker_id = Uuid::new_v4();
    /// let context = TransitionContext::with_worker(worker_id, None);
    ///
    /// let metadata = json!({"event": "some_event"});
    /// let enriched = context.merge_into_metadata(metadata);
    ///
    /// assert!(enriched.get("worker_uuid").is_some());
    /// assert_eq!(enriched["event"], "some_event");
    /// ```
    #[must_use]
    pub fn merge_into_metadata(&self, mut metadata: serde_json::Value) -> serde_json::Value {
        if let serde_json::Value::Object(ref mut map) = metadata {
            if let Some(worker_uuid) = self.worker_uuid {
                map.insert(
                    "worker_uuid".to_string(),
                    serde_json::Value::String(worker_uuid.to_string()),
                );
            }
            if let Some(correlation_id) = self.correlation_id {
                map.insert(
                    "correlation_id".to_string(),
                    serde_json::Value::String(correlation_id.to_string()),
                );
            }
        }
        metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_context_is_empty() {
        let context = TransitionContext::default();
        assert!(context.worker_uuid.is_none());
        assert!(context.correlation_id.is_none());
        assert!(!context.has_attribution());
    }

    #[test]
    fn test_with_worker_creates_context() {
        let worker_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();

        let context = TransitionContext::with_worker(worker_id, Some(correlation_id));

        assert_eq!(context.worker_uuid, Some(worker_id));
        assert_eq!(context.correlation_id, Some(correlation_id));
        assert!(context.has_attribution());
    }

    #[test]
    fn test_with_correlation_id_only() {
        let correlation_id = Uuid::new_v4();

        let context = TransitionContext::with_correlation_id(correlation_id);

        assert!(context.worker_uuid.is_none());
        assert_eq!(context.correlation_id, Some(correlation_id));
        assert!(context.has_attribution());
    }

    #[test]
    fn test_merge_into_metadata() {
        let worker_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let context = TransitionContext::with_worker(worker_id, Some(correlation_id));

        let metadata = serde_json::json!({
            "event": "test_event",
            "timestamp": "2025-12-03T00:00:00Z"
        });

        let enriched = context.merge_into_metadata(metadata);

        assert_eq!(enriched["event"], "test_event");
        assert_eq!(enriched["timestamp"], "2025-12-03T00:00:00Z");
        assert_eq!(enriched["worker_uuid"], worker_id.to_string());
        assert_eq!(enriched["correlation_id"], correlation_id.to_string());
    }

    #[test]
    fn test_merge_skips_none_values() {
        let context = TransitionContext::default();

        let metadata = serde_json::json!({"event": "test"});
        let enriched = context.merge_into_metadata(metadata);

        assert_eq!(enriched["event"], "test");
        assert!(enriched.get("worker_uuid").is_none());
        assert!(enriched.get("correlation_id").is_none());
    }

    #[test]
    fn test_serialization_skips_none() {
        let context = TransitionContext::with_worker(Uuid::new_v4(), None);
        let json = serde_json::to_string(&context).unwrap();

        assert!(json.contains("worker_uuid"));
        assert!(!json.contains("correlation_id")); // Should be skipped
    }

    #[test]
    fn test_roundtrip_serialization() {
        let worker_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let original = TransitionContext::with_worker(worker_id, Some(correlation_id));

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TransitionContext = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }
}
