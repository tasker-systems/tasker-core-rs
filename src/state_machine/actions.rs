use super::errors::{ActionError, ActionResult};
use super::states::{TaskState, WorkflowStepState};
use crate::events::publisher::EventPublisher;
use crate::models::{Task, WorkflowStep};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;

/// Trait for implementing state transition actions
#[async_trait]
pub trait StateAction<T> {
    /// Execute the action
    async fn execute(
        &self,
        entity: &T,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        pool: &PgPool,
    ) -> ActionResult<()>;

    /// Get a description of this action for logging
    fn description(&self) -> &'static str;
}

/// Action to publish lifecycle events when state transitions occur
pub struct PublishTransitionEventAction {
    event_publisher: EventPublisher,
}

impl PublishTransitionEventAction {
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self { event_publisher }
    }
}

#[async_trait]
impl StateAction<Task> for PublishTransitionEventAction {
    async fn execute(
        &self,
        task: &Task,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        _pool: &PgPool,
    ) -> ActionResult<()> {
        let event_name = determine_task_event_name(&from_state, &to_state);

        if let Some(event_name) = event_name {
            let context = build_task_event_context(task, &from_state, &to_state, event);

            self.event_publisher
                .publish(event_name, context)
                .await
                .map_err(|_| ActionError::EventPublishFailed {
                    event_name: event_name.to_string(),
                })?;
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Publish lifecycle event for task transition"
    }
}

#[async_trait]
impl StateAction<WorkflowStep> for PublishTransitionEventAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        _pool: &PgPool,
    ) -> ActionResult<()> {
        let event_name = determine_step_event_name(&from_state, &to_state);

        if let Some(event_name) = event_name {
            let context = build_step_event_context(step, &from_state, &to_state, event);

            self.event_publisher
                .publish(event_name, context)
                .await
                .map_err(|_| ActionError::EventPublishFailed {
                    event_name: event_name.to_string(),
                })?;
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Publish lifecycle event for step transition"
    }
}

/// Action to update task metadata when completing
pub struct UpdateTaskCompletionAction;

#[async_trait]
impl StateAction<Task> for UpdateTaskCompletionAction {
    async fn execute(
        &self,
        task: &Task,
        _from_state: Option<String>,
        to_state: String,
        _event: &str,
        pool: &PgPool,
    ) -> ActionResult<()> {
        if to_state == TaskState::Complete.to_string() {
            // Update the legacy complete boolean flag in the database
            // This ensures compatibility with SQL functions and queries that rely on this flag
            let mut task_clone = task.clone();
            task_clone.mark_complete(pool).await.map_err(|e| {
                ActionError::DatabaseUpdateFailed {
                    entity_type: "Task".to_string(),
                    entity_id: task.task_id,
                    reason: format!("Failed to mark task as complete: {e}"),
                }
            })?;

            tracing::info!(
                task_id = task.task_id,
                "Task marked as complete with legacy flag updated"
            );
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Update task completion metadata and legacy flags"
    }
}

/// Action to update step results when completing
pub struct UpdateStepResultsAction;

#[async_trait]
impl StateAction<WorkflowStep> for UpdateStepResultsAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        _from_state: Option<String>,
        to_state: String,
        event: &str,
        pool: &PgPool,
    ) -> ActionResult<()> {
        // Handle transition to in_progress state
        if to_state == WorkflowStepState::InProgress.to_string() {
            let mut step_clone = step.clone();
            step_clone.mark_in_process(pool).await.map_err(|e| {
                ActionError::DatabaseUpdateFailed {
                    entity_type: "WorkflowStep".to_string(),
                    entity_id: step.workflow_step_id,
                    reason: format!("Failed to mark step as in process: {e}"),
                }
            })?;

            tracing::info!(
                step_id = step.workflow_step_id,
                task_id = step.task_id,
                "Step marked as in_process with legacy flag updated"
            );
        }

        // Handle transition to complete state
        if to_state == WorkflowStepState::Complete.to_string() {
            // Extract results from event if available
            let results = extract_results_from_event(event)?;

            // Update the legacy processed boolean flag and processed_at timestamp
            // This ensures compatibility with SQL functions and queries that rely on these flags
            let mut step_clone = step.clone();
            step_clone
                .mark_processed(pool, results.clone())
                .await
                .map_err(|e| ActionError::DatabaseUpdateFailed {
                    entity_type: "WorkflowStep".to_string(),
                    entity_id: step.workflow_step_id,
                    reason: format!("Failed to mark step as processed: {e}"),
                })?;

            tracing::info!(
                step_id = step.workflow_step_id,
                task_id = step.task_id,
                ?results,
                "Step marked as complete with results and legacy flags updated"
            );
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Update step results and completion metadata with legacy flags"
    }
}

/// Action to trigger next steps discovery when a step completes
pub struct TriggerStepDiscoveryAction {
    event_publisher: EventPublisher,
}

impl TriggerStepDiscoveryAction {
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self { event_publisher }
    }
}

#[async_trait]
impl StateAction<WorkflowStep> for TriggerStepDiscoveryAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        _from_state: Option<String>,
        to_state: String,
        _event: &str,
        _pool: &PgPool,
    ) -> ActionResult<()> {
        if to_state == WorkflowStepState::Complete.to_string() {
            // Trigger discovery of newly viable steps
            let context = serde_json::json!({
                "task_id": step.task_id,
                "completed_step_id": step.workflow_step_id,
                "triggered_at": Utc::now()
            });

            self.event_publisher
                .publish("step.discovery_triggered", context)
                .await
                .map_err(|_| ActionError::EventPublishFailed {
                    event_name: "step.discovery_triggered".to_string(),
                })?;
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Trigger discovery of newly viable steps"
    }
}

/// Action to handle error state cleanup
pub struct ErrorStateCleanupAction;

#[async_trait]
impl StateAction<Task> for ErrorStateCleanupAction {
    async fn execute(
        &self,
        task: &Task,
        _from_state: Option<String>,
        to_state: String,
        event: &str,
        _pool: &PgPool,
    ) -> ActionResult<()> {
        if to_state == TaskState::Error.to_string() {
            // Extract error message from event for logging
            let error_message = extract_error_from_event(event);

            // Log the error for debugging (error details will be stored in transition metadata)
            tracing::error!(
                task_id = task.task_id,
                error_message = error_message,
                "Task transitioned to error state"
            );
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Handle error state cleanup and logging"
    }
}

#[async_trait]
impl StateAction<WorkflowStep> for ErrorStateCleanupAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        _from_state: Option<String>,
        to_state: String,
        event: &str,
        _pool: &PgPool,
    ) -> ActionResult<()> {
        if to_state == WorkflowStepState::Error.to_string() {
            let error_message = extract_error_from_event(event);

            // Log the error for debugging (error details will be stored in transition metadata)
            tracing::error!(
                step_id = step.workflow_step_id,
                task_id = step.task_id,
                error_message = error_message,
                "Workflow step transitioned to error state"
            );
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Handle error state cleanup and logging"
    }
}

// Helper functions for event processing

fn determine_task_event_name(from_state: &Option<String>, to_state: &str) -> Option<&'static str> {
    match (from_state.as_deref(), to_state) {
        (_, "in_progress") => Some("task.started"),
        (_, "complete") => Some("task.completed"),
        (_, "error") => Some("task.failed"),
        (_, "cancelled") => Some("task.cancelled"),
        (_, "resolved_manually") => Some("task.resolved_manually"),
        (Some("error"), "pending") => Some("task.reset"),
        _ => None,
    }
}

fn determine_step_event_name(from_state: &Option<String>, to_state: &str) -> Option<&'static str> {
    match (from_state.as_deref(), to_state) {
        (_, "in_progress") => Some("step.started"),
        (_, "complete") => Some("step.completed"),
        (_, "error") => Some("step.failed"),
        (_, "cancelled") => Some("step.cancelled"),
        (_, "resolved_manually") => Some("step.resolved_manually"),
        (Some("error"), "pending") => Some("step.retried"),
        _ => None,
    }
}

fn build_task_event_context(
    task: &Task,
    from_state: &Option<String>,
    to_state: &str,
    event: &str,
) -> Value {
    serde_json::json!({
        "task_id": task.task_id,
        "named_task_id": task.named_task_id,
        "from_state": from_state,
        "to_state": to_state,
        "event": event,
        "transitioned_at": Utc::now(),
        "context": task.context
    })
}

fn build_step_event_context(
    step: &WorkflowStep,
    from_state: &Option<String>,
    to_state: &str,
    event: &str,
) -> Value {
    serde_json::json!({
        "task_id": step.task_id,
        "step_id": step.workflow_step_id,
        "named_step_id": step.named_step_id,
        "from_state": from_state,
        "to_state": to_state,
        "event": event,
        "transitioned_at": Utc::now()
    })
}

fn extract_results_from_event(event: &str) -> ActionResult<Option<Value>> {
    // Try to parse event as JSON to extract results from StepEvent::Complete
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        // StepEvent::Complete serializes as: {"type": "Complete", "data": { results }}
        if let Some(event_type) = event_data.get("type") {
            if event_type == "Complete" {
                // Extract the data field which contains the actual step results
                if let Some(results) = event_data.get("data") {
                    return Ok(Some(results.clone()));
                }
            }
        }
    }
    Ok(None)
}

fn extract_error_from_event(event: &str) -> Option<String> {
    // Try to parse event as JSON to extract error message
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        if let Some(error) = event_data.get("error") {
            return error.as_str().map(|s| s.to_string());
        }
    }

    // Fallback: use the event string itself as error message
    Some(event.to_string())
}
