//! # Task Request Hydrator
//!
//! Hydrates task request messages from PGMQ messages.
//!
//! ## Purpose
//!
//! External clients submit TaskRequestMessage to the orchestration_task_requests queue.
//! This hydrator parses and validates these messages for task initialization.
//!
//! ## Process
//!
//! 1. Parse TaskRequestMessage from PGMQ message payload
//! 2. Validate message format
//! 3. Return hydrated TaskRequestMessage for processing
//!
//! Unlike StepResultHydrator, this hydrator doesn't need database lookup because
//! the message itself contains all required data for task initialization.

use pgmq::Message as PgmqMessage;
use tasker_shared::messaging::TaskRequestMessage;
use tasker_shared::{TaskerError, TaskerResult};
use tracing::{debug, error, info};

/// Hydrates TaskRequestMessage from PGMQ messages
///
/// This service performs message parsing and validation, converting PGMQ messages
/// into TaskRequestMessage for orchestration processing.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::hydration::TaskRequestHydrator;
///
/// # async fn example(message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
/// let hydrator = TaskRequestHydrator::new();
/// let task_request = hydrator.hydrate_from_message(&message).await?;
/// // task_request is now ready for task initialization
/// # Ok(())
/// # }
/// ```
pub struct TaskRequestHydrator;

impl TaskRequestHydrator {
    /// Create a new TaskRequestHydrator
    pub fn new() -> Self {
        Self
    }

    /// Hydrate TaskRequestMessage from PGMQ message
    ///
    /// Performs message parsing and validation:
    /// 1. Parse TaskRequestMessage from message payload
    /// 2. Validate message format
    /// 3. Return hydrated message
    ///
    /// # Arguments
    ///
    /// * `message` - PGMQ message containing TaskRequestMessage payload
    ///
    /// # Returns
    ///
    /// Fully hydrated `TaskRequestMessage` ready for task initialization
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid message format or missing data
    pub async fn hydrate_from_message(
        &self,
        message: &PgmqMessage,
    ) -> TaskerResult<TaskRequestMessage> {
        debug!(
            msg_id = message.msg_id,
            message_size = message.message.to_string().len(),
            "HYDRATOR: Starting task request hydration"
        );

        // Parse the task request message
        let task_request: TaskRequestMessage = serde_json::from_value(message.message.clone())
            .map_err(|e| {
                error!(
                    msg_id = message.msg_id,
                    error = %e,
                    message_content = %message.message,
                    "HYDRATOR: Failed to parse task request message"
                );
                TaskerError::ValidationError(format!("Invalid task request message format: {e}"))
            })?;

        info!(
            msg_id = message.msg_id,
            namespace = %task_request.task_request.namespace,
            handler_name = %task_request.task_request.name,
            "HYDRATOR: Successfully parsed TaskRequestMessage"
        );

        Ok(task_request)
    }
}

impl Default for TaskRequestHydrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_request_hydrator_construction() {
        // Verify the hydrator can be constructed
        let _hydrator = TaskRequestHydrator::new();
        let _hydrator = TaskRequestHydrator;
    }
}
