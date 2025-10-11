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
