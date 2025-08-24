//! Event publisher for step execution events

use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use tasker_shared::{
    models::{Sequence, Task, WorkflowStep},
};

use crate::{
    error::Result,
    events::{InProcessEventSystem, StepEventPayload},
};

/// Event publisher for step execution events
pub struct EventPublisher {
    event_system: Arc<InProcessEventSystem>,
}

impl EventPublisher {
    /// Create new event publisher
    pub fn new(event_system: Arc<InProcessEventSystem>) -> Self {
        Self { event_system }
    }

    /// Fire step execution event with (task, sequence, step) payload
    pub async fn fire_step_event(
        &self,
        task: Task,
        sequence: Sequence,
        step: WorkflowStep,
    ) -> Result<()> {
        info!(
            "🔥 Firing step event - task: {}, step: {} ({})",
            task.task_uuid, step.step_uuid, step.named_step.name
        );

        let payload = StepEventPayload {
            task,
            sequence,
            step,
        };

        self.event_system.publish_step_event(payload).await?;
        Ok(())
    }

    /// Fire step event from components (convenience method)
    pub async fn fire_step_event_from_ids(
        &self,
        task_id: Uuid,
        step_id: Uuid,
        database: &tasker_shared::database::Connection,
    ) -> Result<()> {
        // Load task
        let task = Task::find_by_uuid(database, task_id)
            .await
            .map_err(|e| crate::error::WorkerError::Database(e))?
            .ok_or_else(|| {
                crate::error::WorkerError::StepProcessing(format!("Task not found: {}", task_id))
            })?;

        // Load step
        let step = WorkflowStep::find_by_uuid(database, step_id)
            .await
            .map_err(|e| crate::error::WorkerError::Database(e))?
            .ok_or_else(|| {
                crate::error::WorkerError::StepProcessing(format!("Step not found: {}", step_id))
            })?;

        // Build sequence
        let sequence = Sequence::build_for_step(database, &step)
            .await
            .map_err(|e| crate::error::WorkerError::Database(e))?;

        self.fire_step_event(task, sequence, step).await
    }
}