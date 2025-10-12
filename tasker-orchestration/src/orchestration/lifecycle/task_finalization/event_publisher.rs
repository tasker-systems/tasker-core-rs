//! Event Publisher
//!
//! Handles publishing lifecycle events for task completion and failure.

use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use tasker_shared::events::types::{Event, OrchestrationEvent, TaskResult};
use tasker_shared::models::orchestration::TaskExecutionContext;
use tasker_shared::system_context::SystemContext;

use super::FinalizationError;

/// Publishes task lifecycle events
#[derive(Clone)]
pub struct EventPublisher {
    context: Arc<SystemContext>,
}

impl EventPublisher {
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self { context }
    }

    /// Publish task completed event
    pub async fn publish_task_completed(
        &self,
        task_uuid: Uuid,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        // Publish structured orchestration event for task completion
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Success,
            completed_at: chrono::Utc::now(),
        });

        self.context
            .event_publisher
            .publish_event(event)
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task completed event: {e}"
                ))
            })?;

        // Also publish generic event for broader observability
        self.context
            .event_publisher
            .publish(
                "task.completed",
                json!({
                    "task_uuid": task_uuid,
                    "status": "success",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task completed generic event: {e}"
                ))
            })?;

        Ok(())
    }

    /// Publish task failed event
    pub async fn publish_task_failed(
        &self,
        task_uuid: Uuid,
        _context: &Option<TaskExecutionContext>,
    ) -> Result<(), FinalizationError> {
        // Publish structured orchestration event for task failure
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Failed {
                error: "Task finalization determined task failed".to_string(),
            },
            completed_at: chrono::Utc::now(),
        });

        self.context
            .event_publisher
            .publish_event(event)
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task failed event: {e}"
                ))
            })?;

        // Also publish generic event for broader observability
        self.context
            .event_publisher
            .publish(
                "task.failed",
                json!({
                    "task_uuid": task_uuid,
                    "status": "failed",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
            )
            .await
            .map_err(|e| {
                FinalizationError::EventPublishing(format!(
                    "Failed to publish task failed generic event: {e}"
                ))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_event_publisher_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that we can create EventPublisher
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let publisher = EventPublisher::new(context);

        // Verify it's created (basic smoke test)
        assert!(Arc::strong_count(&publisher.context) >= 1);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_event_publisher_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test that EventPublisher implements Clone
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let publisher = EventPublisher::new(context.clone());

        let cloned = publisher.clone();

        // Verify both share the same Arc
        assert_eq!(
            Arc::as_ptr(&publisher.context),
            Arc::as_ptr(&cloned.context)
        );
        Ok(())
    }

    #[test]
    fn test_task_completed_event_structure() {
        // Test that we can create the structured event for completion
        let task_uuid = Uuid::new_v4();
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Success,
            completed_at: chrono::Utc::now(),
        });

        // Verify event structure
        match event {
            Event::Orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
                task_uuid: id,
                result,
                ..
            }) => {
                assert_eq!(id, task_uuid);
                assert!(matches!(result, TaskResult::Success));
            }
            _ => panic!("Expected TaskOrchestrationCompleted event"),
        }
    }

    #[test]
    fn test_task_completed_generic_event_structure() {
        // Test that generic event JSON is properly structured
        let task_uuid = Uuid::new_v4();
        let event_data = json!({
            "task_uuid": task_uuid,
            "status": "success",
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        // Verify JSON structure
        assert_eq!(event_data["task_uuid"], task_uuid.to_string());
        assert_eq!(event_data["status"], "success");
        assert!(event_data["timestamp"].is_string());
    }

    #[test]
    fn test_task_failed_event_structure() {
        // Test that we can create the structured event for failure
        let task_uuid = Uuid::new_v4();
        let error_msg = "Task finalization determined task failed".to_string();
        let event = Event::orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
            task_uuid,
            result: TaskResult::Failed {
                error: error_msg.clone(),
            },
            completed_at: chrono::Utc::now(),
        });

        // Verify event structure
        match event {
            Event::Orchestration(OrchestrationEvent::TaskOrchestrationCompleted {
                task_uuid: id,
                result,
                ..
            }) => {
                assert_eq!(id, task_uuid);
                match result {
                    TaskResult::Failed { error } => {
                        assert_eq!(error, error_msg);
                    }
                    _ => panic!("Expected Failed result"),
                }
            }
            _ => panic!("Expected TaskOrchestrationCompleted event"),
        }
    }

    #[test]
    fn test_task_failed_generic_event_structure() {
        // Test that generic event JSON is properly structured
        let task_uuid = Uuid::new_v4();
        let event_data = json!({
            "task_uuid": task_uuid,
            "status": "failed",
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        // Verify JSON structure
        assert_eq!(event_data["task_uuid"], task_uuid.to_string());
        assert_eq!(event_data["status"], "failed");
        assert!(event_data["timestamp"].is_string());
    }
}
