use super::errors::{ActionError, ActionResult};
use super::events::TaskEvent;
use super::states::{TaskState, WorkflowStepState};
use crate::events::publisher::EventPublisher;
use crate::models::{Task, WorkflowStep};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

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

/// Actions to perform during state transitions (TAS-41 specification lines 304-443)
pub struct TransitionActions {
    pool: PgPool,
    event_publisher: Option<Arc<EventPublisher>>,
}

impl TransitionActions {
    pub fn new(pool: PgPool, event_publisher: Option<Arc<EventPublisher>>) -> Self {
        Self {
            pool,
            event_publisher,
        }
    }

    /// Execute actions for a state transition
    pub async fn execute(
        &self,
        task: &Task,
        from_state: TaskState,
        to_state: TaskState,
        event: &TaskEvent,
        _processor_uuid: Uuid,
        _metadata: Option<Value>,
    ) -> ActionResult<()> {
        // Pre-transition actions
        self.pre_transition_actions(task, from_state, to_state, event)
            .await?;

        // Record the transition (this is handled by the persistence layer)
        // The actual recording is done by StatePersistence.transition_with_ownership()

        // Post-transition actions
        self.post_transition_actions(task, from_state, to_state, event)
            .await?;

        // Publish events if configured
        if let Some(publisher) = &self.event_publisher {
            self.publish_transition_event(publisher, task, from_state, to_state, event)
                .await?;
        }

        Ok(())
    }

    async fn pre_transition_actions(
        &self,
        task: &Task,
        from: TaskState,
        to: TaskState,
        _event: &TaskEvent,
    ) -> ActionResult<()> {
        use TaskState::*;

        match (from, to) {
            // Clear processor when moving to waiting states
            (_, WaitingForDependencies) | (_, WaitingForRetry) | (_, BlockedByFailures) => {
                self.clear_processor_ownership(task.task_uuid).await?;
            }

            // Set completion timestamp when moving to Complete
            (_, Complete) => {
                self.set_task_completed(task.task_uuid).await?;
            }

            _ => {}
        }

        Ok(())
    }

    async fn post_transition_actions(
        &self,
        task: &Task,
        _from: TaskState,
        to: TaskState,
        _event: &TaskEvent,
    ) -> ActionResult<()> {
        use TaskState::*;

        // Log important transitions
        match to {
            Complete => {
                tracing::info!(task_uuid = %task.task_uuid, "Task completed successfully");
            }
            Error => {
                tracing::error!(task_uuid = %task.task_uuid, "Task failed permanently");
            }
            BlockedByFailures => {
                tracing::warn!(task_uuid = %task.task_uuid, "Task blocked by failures");
            }
            _ => {}
        }

        Ok(())
    }

    async fn clear_processor_ownership(&self, task_uuid: Uuid) -> ActionResult<()> {
        // This would typically update the task's processor_uuid to NULL
        // For now, this is a placeholder - the actual implementation would
        // update the transition record's processor_uuid field
        tracing::debug!(task_uuid = %task_uuid, "Clearing processor ownership");
        Ok(())
    }

    async fn set_task_completed(&self, task_uuid: Uuid) -> ActionResult<()> {
        // Update the legacy complete boolean flag in the database
        // This ensures compatibility with SQL functions and queries that rely on this flag
        sqlx::query!(
            r#"
            UPDATE tasker_tasks
            SET complete = true, completed_at = NOW(), updated_at = NOW()
            WHERE task_uuid = $1
            "#,
            task_uuid
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ActionError::DatabaseUpdateFailed {
            entity_type: "Task".to_string(),
            entity_id: task_uuid.to_string(),
            reason: format!("Failed to mark task as complete: {e}"),
        })?;

        tracing::info!(task_uuid = %task_uuid, "Task marked as complete with legacy flag updated");
        Ok(())
    }

    async fn publish_transition_event(
        &self,
        publisher: &EventPublisher,
        task: &Task,
        from_state: TaskState,
        to_state: TaskState,
        event: &TaskEvent,
    ) -> ActionResult<()> {
        let event_name = determine_task_event_name_from_states(from_state, to_state);

        if let Some(event_name) = event_name {
            let context = build_task_event_context(task, from_state, to_state, event);

            publisher.publish(event_name, context).await.map_err(|_| {
                ActionError::EventPublishFailed {
                    event_name: event_name.to_string(),
                }
            })?;
        }

        Ok(())
    }
}

// Helper function to determine event names from state transitions
fn determine_task_event_name_from_states(from: TaskState, to: TaskState) -> Option<&'static str> {
    use TaskState::*;

    match (from, to) {
        (Pending, Initializing) => Some("task.started"),
        (_, Complete) => Some("task.completed"),
        (_, Error) => Some("task.failed"),
        (_, Cancelled) => Some("task.cancelled"),
        (_, ResolvedManually) => Some("task.resolved_manually"),
        (Error, Pending) => Some("task.reset"),
        _ => None,
    }
}

// Helper function to build event context
fn build_task_event_context(
    task: &Task,
    from_state: TaskState,
    to_state: TaskState,
    event: &TaskEvent,
) -> Value {
    serde_json::json!({
        "task_uuid": task.task_uuid,
        "named_task_uuid": task.named_task_uuid,
        "from_state": from_state.as_str(),
        "to_state": to_state.as_str(),
        "event": format!("{:?}", event),
        "transitioned_at": Utc::now(),
        "context": task.context
    })
}

// Legacy helper function to build event context for string-based parameters
fn build_task_event_context_legacy(
    task: &Task,
    from_state: &Option<String>,
    to_state: &String,
    event: &str,
) -> Value {
    serde_json::json!({
        "task_uuid": task.task_uuid,
        "named_task_uuid": task.named_task_uuid,
        "from_state": from_state.as_deref().unwrap_or("unknown"),
        "to_state": to_state,
        "event": event,
        "transitioned_at": Utc::now(),
        "context": task.context
    })
}

/// Action to publish lifecycle events when state transitions occur
pub struct PublishTransitionEventAction {
    event_publisher: Arc<EventPublisher>,
}

impl PublishTransitionEventAction {
    pub fn new(event_publisher: Arc<EventPublisher>) -> Self {
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
            let context = build_task_event_context_legacy(task, &from_state, &to_state, event);

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
                    entity_id: task.task_uuid.to_string(),
                    reason: format!("Failed to mark task as complete: {e}"),
                }
            })?;

            tracing::info!(
                task_uuid = %task.task_uuid,
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

impl UpdateStepResultsAction {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn handle_move_to_in_progress(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        _event: &str,
    ) -> ActionResult<()> {
        let mut step_clone = step.clone();
        step_clone
            .mark_in_process(pool)
            .await
            .map_err(|e| ActionError::DatabaseUpdateFailed {
                entity_type: "WorkflowStep".to_string(),
                entity_id: step.workflow_step_uuid.to_string(),
                reason: format!("Failed to mark step as in process: {e}"),
            })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            "Step marked as in_process with legacy flag updated"
        );
        Ok(())
    }

    pub async fn handle_move_to_completed(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        event: &str,
    ) -> ActionResult<()> {
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
                entity_id: step.workflow_step_uuid.to_string(),
                reason: format!("Failed to mark step as processed: {e}"),
            })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            ?results,
            "Step marked as complete with results and legacy flags updated"
        );
        Ok(())
    }

    pub async fn handle_move_to_error(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        _event: &str,
    ) -> ActionResult<()> {
        // When a step fails, it's no longer in process
        sqlx::query!(
            r#"
        UPDATE tasker_workflow_steps
        SET in_process = false,
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
            step.workflow_step_uuid
        )
        .execute(pool)
        .await
        .map_err(|e| ActionError::DatabaseUpdateFailed {
            entity_type: "WorkflowStep".to_string(),
            entity_id: step.workflow_step_uuid.to_string(),
            reason: format!("Failed to mark step as not in process after error: {e}"),
        })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            "Step marked as not in_process after error"
        );
        Ok(())
    }

    pub async fn handle_move_to_cancelled(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        _event: &str,
    ) -> ActionResult<()> {
        sqlx::query!(
            r#"
        UPDATE tasker_workflow_steps
        SET in_process = false,
            processed = true,
            processed_at = NOW(),
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
            step.workflow_step_uuid
        )
        .execute(pool)
        .await
        .map_err(|e| ActionError::DatabaseUpdateFailed {
            entity_type: "WorkflowStep".to_string(),
            entity_id: step.workflow_step_uuid.to_string(),
            reason: format!("Failed to mark cancelled step as processed: {e}"),
        })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            "Cancelled step marked as processed and not in_process"
        );
        Ok(())
    }

    pub async fn handle_move_to_enqueued_for_orchestration(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        event: &str,
    ) -> ActionResult<()> {
        // Extract results from event if available for later orchestration processing
        let results = extract_results_from_event(event)?;

        // Update step to no longer be in_process since worker has completed
        // but don't mark as processed yet - orchestration will do that
        sqlx::query!(
            r#"
        UPDATE tasker_workflow_steps
        SET in_process = false,
            results = $2,
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
            step.workflow_step_uuid,
            results // Already a serde_json::Value, no need to convert again
        )
        .execute(pool)
        .await
        .map_err(|e| ActionError::DatabaseUpdateFailed {
            entity_type: "WorkflowStep".to_string(),
            entity_id: step.workflow_step_uuid.to_string(),
            reason: format!("Failed to mark step as enqueued for orchestration: {e}"),
        })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            ?results,
            "Step marked as enqueued for orchestration with results preserved"
        );
        Ok(())
    }

    pub async fn handle_move_to_resolved_manually(
        &self,
        step: &WorkflowStep,
        pool: &PgPool,
        _event: &str,
    ) -> ActionResult<()> {
        sqlx::query!(
            r#"
        UPDATE tasker_workflow_steps
        SET in_process = false,
            processed = true,
            processed_at = NOW(),
            updated_at = NOW()
        WHERE workflow_step_uuid = $1
        "#,
            step.workflow_step_uuid
        )
        .execute(pool)
        .await
        .map_err(|e| ActionError::DatabaseUpdateFailed {
            entity_type: "WorkflowStep".to_string(),
            entity_id: step.workflow_step_uuid.to_string(),
            reason: format!("Failed to mark manually resolved step as processed: {e}"),
        })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            "Manually resolved step marked as processed and not in_process"
        );
        Ok(())
    }
}

#[async_trait]
impl StateAction<WorkflowStep> for UpdateStepResultsAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        pool: &PgPool,
    ) -> ActionResult<()> {
        let moving_to_state = match WorkflowStepState::from_str(&to_state) {
            Ok(state) => state,
            Err(e) => {
                return Err(ActionError::InvalidStateTransition {
                    from_state: from_state,
                    to_state,
                    reason: format!("Failed to parse state: {e}"),
                })
            }
        };

        match moving_to_state {
            WorkflowStepState::InProgress => {
                self.handle_move_to_in_progress(step, pool, event).await?;
            }
            WorkflowStepState::Complete => {
                self.handle_move_to_completed(step, pool, event).await?;
            }
            WorkflowStepState::Error => {
                self.handle_move_to_error(step, pool, event).await?;
            }
            WorkflowStepState::Cancelled => {
                self.handle_move_to_cancelled(step, pool, event).await?;
            }
            WorkflowStepState::EnqueuedForOrchestration => {
                self.handle_move_to_enqueued_for_orchestration(step, pool, event)
                    .await?;
            }
            WorkflowStepState::ResolvedManually => {
                self.handle_move_to_resolved_manually(step, pool, event)
                    .await?;
            }
            WorkflowStepState::Pending => {
                tracing::info!(
                    step_uuid = %step.workflow_step_uuid,
                    task_uuid = %step.task_uuid,
                    "Step initialized as pending"
                );
            }
            WorkflowStepState::Enqueued => {
                tracing::info!(
                    step_uuid = %step.workflow_step_uuid,
                    task_uuid = %step.task_uuid,
                    "Step moved to enqueued for worker processing"
                );
            }
        };

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Update step results and completion metadata with legacy flags"
    }
}

/// Action to trigger next steps discovery when a step completes
pub struct TriggerStepDiscoveryAction {
    event_publisher: Arc<EventPublisher>,
}

impl TriggerStepDiscoveryAction {
    pub fn new(event_publisher: Arc<EventPublisher>) -> Self {
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
                "task_uuid": step.task_uuid,
                "completed_step_uuid": step.workflow_step_uuid,
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
                task_uuid = %task.task_uuid,
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
                step_uuid = %step.workflow_step_uuid,
                task_uuid = %step.task_uuid,
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
        (_, "enqueued_for_orchestration") => Some("step.enqueued_for_orchestration"),
        (_, "complete") => Some("step.completed"),
        (_, "error") => Some("step.failed"),
        (_, "cancelled") => Some("step.cancelled"),
        (_, "resolved_manually") => Some("step.resolved_manually"),
        (Some("error"), "pending") => Some("step.retried"),
        _ => None,
    }
}

fn build_step_event_context(
    step: &WorkflowStep,
    from_state: &Option<String>,
    to_state: &str,
    event: &str,
) -> Value {
    serde_json::json!({
        "task_uuid": step.task_uuid,
        "step_uuid": step.workflow_step_uuid,
        "named_step_uuid": step.named_step_uuid,
        "from_state": from_state,
        "to_state": to_state,
        "event": event,
        "transitioned_at": Utc::now()
    })
}

fn extract_results_from_event(event: &str) -> ActionResult<Option<Value>> {
    // Try to parse event as JSON to extract results from StepEvent::Complete or StepEvent::EnqueueForOrchestration
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        // StepEvent serializes as: {"type": "Complete", "data": { results }} or {"type": "EnqueueForOrchestration", "data": { results }}
        if let Some(event_type) = event_data.get("type") {
            if event_type == "Complete" || event_type == "EnqueueForOrchestration" {
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
