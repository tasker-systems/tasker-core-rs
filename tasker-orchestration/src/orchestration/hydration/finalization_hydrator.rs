//! # Task Finalization Hydrator
//!
//! Hydrates task finalization requests from PGMQ messages.
//!
//! ## Purpose
//!
//! Workers and orchestration components send finalization notifications to the
//! orchestration_task_finalization queue. This hydrator extracts the task_uuid
//! from these messages for finalization processing.
//!
//! ## Process
//!
//! 1. Parse PGMQ message payload
//! 2. Extract task_uuid field
//! 3. Validate UUID format
//! 4. Return task_uuid for finalization

use pgmq::Message as PgmqMessage;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Hydrates task_uuid from finalization messages
///
/// This service extracts and validates task_uuid from PGMQ finalization messages.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::hydration::FinalizationHydrator;
///
/// # async fn example(message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
/// let hydrator = FinalizationHydrator::new();
/// let task_uuid = hydrator.hydrate_from_message(&message).await?;
/// // task_uuid is now ready for finalization processing
/// # Ok(())
/// # }
/// ```
pub struct FinalizationHydrator;

impl FinalizationHydrator {
    /// Create a new FinalizationHydrator
    pub fn new() -> Self {
        Self
    }

    /// Hydrate task_uuid from PGMQ finalization message
    ///
    /// Performs message parsing and validation:
    /// 1. Extract task_uuid field from message
    /// 2. Parse UUID string
    /// 3. Validate format
    ///
    /// # Arguments
    ///
    /// * `message` - PGMQ message containing task_uuid
    ///
    /// # Returns
    ///
    /// Validated `Uuid` ready for finalization processing
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid or missing task_uuid
    pub async fn hydrate_from_message(&self, message: &PgmqMessage) -> TaskerResult<Uuid> {
        debug!(
            msg_id = message.msg_id,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting finalization hydration"
        );

        // Extract task_uuid from message
        let task_uuid = message
            .message
            .get("task_uuid")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| {
                error!(
                    msg_id = message.msg_id,
                    message_content = %message.message,
                    "HYDRATOR: Invalid or missing task_uuid in finalization message"
                );
                TaskerError::ValidationError(
                    "Invalid or missing task_uuid in finalization message".to_string(),
                )
            })?;

        info!(
            msg_id = message.msg_id,
            task_uuid = %task_uuid,
            "HYDRATOR: Successfully extracted task_uuid from finalization message"
        );

        Ok(task_uuid)
    }
}

impl Default for FinalizationHydrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalization_hydrator_construction() {
        // Verify the hydrator can be constructed
        let _hydrator = FinalizationHydrator::new();
        let _hydrator = FinalizationHydrator::default();
    }
}
