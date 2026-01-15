//! # Step Message Hydrator
//!
//! TAS-69: Transforms PGMQ step messages into typed actor messages.
//!
//! This hydrator handles:
//! - `PgmqMessage<StepMessage>` → ExecuteStepMessage
//! - Raw PgmqMessage → ExecuteStepFromPgmqMessage
//! - MessageEvent → ExecuteStepFromEventMessage (TAS-133)

use pgmq::Message as PgmqMessage;

use tasker_shared::messaging::message::StepMessage;
use tasker_shared::messaging::service::MessageEvent;
use tasker_shared::{TaskerError, TaskerResult};

use crate::worker::actors::{
    ExecuteStepFromEventMessage, ExecuteStepFromPgmqMessage, ExecuteStepMessage,
    ExecuteStepWithCorrelationMessage,
};

/// Step Message Hydrator
///
/// TAS-69: Transforms PGMQ step messages into typed actor messages.
///
/// This follows the orchestration hydration pattern, providing type-safe
/// message transformation with validation.
#[derive(Debug, Clone, Copy, Default)]
pub struct StepMessageHydrator;

impl StepMessageHydrator {
    /// Hydrate a typed PGMQ message into an ExecuteStepMessage
    ///
    /// # Arguments
    /// * `message` - The typed PGMQ message containing StepMessage
    /// * `queue_name` - The source queue name
    ///
    /// # Returns
    /// A hydrated ExecuteStepMessage ready for actor handling
    pub fn hydrate_execute_step(
        message: PgmqMessage<StepMessage>,
        queue_name: String,
    ) -> ExecuteStepMessage {
        ExecuteStepMessage {
            message,
            queue_name,
        }
    }

    /// Hydrate a typed PGMQ message with correlation ID
    ///
    /// # Arguments
    /// * `message` - The typed PGMQ message
    /// * `queue_name` - The source queue name
    /// * `correlation_id` - Correlation ID for event tracking
    ///
    /// # Returns
    /// A hydrated ExecuteStepWithCorrelationMessage
    pub fn hydrate_execute_step_with_correlation(
        message: PgmqMessage<StepMessage>,
        queue_name: String,
        correlation_id: uuid::Uuid,
    ) -> ExecuteStepWithCorrelationMessage {
        ExecuteStepWithCorrelationMessage {
            message,
            queue_name,
            correlation_id,
        }
    }

    /// Hydrate a raw PGMQ message into an ExecuteStepFromPgmqMessage
    ///
    /// This is used when the message needs deserialization during handling.
    ///
    /// # Arguments
    /// * `message` - The raw PGMQ message (untyped JSON)
    /// * `queue_name` - The source queue name
    ///
    /// # Returns
    /// A hydrated ExecuteStepFromPgmqMessage
    pub fn hydrate_from_raw_message(
        message: PgmqMessage,
        queue_name: String,
    ) -> ExecuteStepFromPgmqMessage {
        ExecuteStepFromPgmqMessage {
            message,
            queue_name,
        }
    }

    /// Hydrate a MessageEvent into an ExecuteStepFromEventMessage
    ///
    /// TAS-133: Updated to use provider-agnostic `MessageEvent`
    ///
    /// # Arguments
    /// * `event` - The provider-agnostic message event
    ///
    /// # Returns
    /// A hydrated ExecuteStepFromEventMessage
    pub fn hydrate_from_event(event: MessageEvent) -> ExecuteStepFromEventMessage {
        ExecuteStepFromEventMessage {
            message_event: event,
        }
    }

    /// Parse a raw PGMQ message into a typed StepMessage
    ///
    /// This is useful when you need to inspect the message before creating
    /// the actor message.
    ///
    /// # Arguments
    /// * `message` - The raw PGMQ message
    ///
    /// # Returns
    /// The typed `PgmqMessage<StepMessage>` or an error
    pub fn parse_step_message(
        message: PgmqMessage,
    ) -> TaskerResult<PgmqMessage<StepMessage>> {
        let step_message: StepMessage = serde_json::from_value(message.message.clone())
            .map_err(|e| {
                TaskerError::MessagingError(format!("Failed to deserialize step message: {}", e))
            })?;

        Ok(PgmqMessage {
            msg_id: message.msg_id,
            message: step_message,
            vt: message.vt,
            read_ct: message.read_ct,
            enqueued_at: message.enqueued_at,
        })
    }

    /// Validate a StepMessage has required fields
    ///
    /// # Arguments
    /// * `message` - The step message to validate
    ///
    /// # Returns
    /// Ok(()) if valid, Err with details if invalid
    pub fn validate_step_message(message: &StepMessage) -> TaskerResult<()> {
        // UUID validation is handled by the type system
        // Add any additional business logic validation here

        if message.task_uuid.is_nil() {
            return Err(TaskerError::ValidationError(
                "task_uuid cannot be nil".to_string(),
            ));
        }

        if message.step_uuid.is_nil() {
            return Err(TaskerError::ValidationError(
                "step_uuid cannot be nil".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_step_message() -> StepMessage {
        StepMessage {
            task_uuid: Uuid::new_v4(),
            step_uuid: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
        }
    }

    fn create_test_pgmq_message(step_message: StepMessage) -> PgmqMessage<StepMessage> {
        PgmqMessage {
            msg_id: 1,
            message: step_message,
            vt: Utc::now(),
            read_ct: 1,
            enqueued_at: Utc::now(),
        }
    }

    #[test]
    fn test_hydrate_execute_step() {
        let step_message = create_test_step_message();
        let pgmq_message = create_test_pgmq_message(step_message.clone());
        let queue_name = "test_queue".to_string();

        let hydrated = StepMessageHydrator::hydrate_execute_step(pgmq_message, queue_name.clone());

        assert_eq!(hydrated.queue_name, queue_name);
        assert_eq!(hydrated.message.message.task_uuid, step_message.task_uuid);
    }

    #[test]
    fn test_hydrate_with_correlation() {
        let step_message = create_test_step_message();
        let pgmq_message = create_test_pgmq_message(step_message);
        let queue_name = "test_queue".to_string();
        let correlation_id = Uuid::new_v4();

        let hydrated = StepMessageHydrator::hydrate_execute_step_with_correlation(
            pgmq_message,
            queue_name.clone(),
            correlation_id,
        );

        assert_eq!(hydrated.queue_name, queue_name);
        assert_eq!(hydrated.correlation_id, correlation_id);
    }

    #[test]
    fn test_hydrate_from_event() {
        let event = MessageEvent::new("test_queue", "test_namespace", "123");

        let hydrated = StepMessageHydrator::hydrate_from_event(event.clone());

        assert_eq!(hydrated.message_event.queue_name, "test_queue");
        assert_eq!(hydrated.message_event.namespace, "test_namespace");
        assert_eq!(hydrated.message_event.message_id.as_str(), "123");
    }

    #[test]
    fn test_parse_step_message() {
        let step_message = create_test_step_message();
        let json_value = serde_json::to_value(&step_message).unwrap();

        let raw_message = PgmqMessage {
            msg_id: 1,
            message: json_value,
            vt: Utc::now(),
            read_ct: 1,
            enqueued_at: Utc::now(),
        };

        let parsed = StepMessageHydrator::parse_step_message(raw_message).unwrap();

        assert_eq!(parsed.message.task_uuid, step_message.task_uuid);
        assert_eq!(parsed.message.step_uuid, step_message.step_uuid);
    }

    #[test]
    fn test_validate_step_message_valid() {
        let step_message = create_test_step_message();
        assert!(StepMessageHydrator::validate_step_message(&step_message).is_ok());
    }

    #[test]
    fn test_validate_step_message_nil_task_uuid() {
        let step_message = StepMessage {
            task_uuid: Uuid::nil(),
            step_uuid: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
        };

        let result = StepMessageHydrator::validate_step_message(&step_message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task_uuid"));
    }

    #[test]
    fn test_validate_step_message_nil_step_uuid() {
        let step_message = StepMessage {
            task_uuid: Uuid::new_v4(),
            step_uuid: Uuid::nil(),
            correlation_id: Uuid::new_v4(),
        };

        let result = StepMessageHydrator::validate_step_message(&step_message);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("step_uuid"));
    }
}
