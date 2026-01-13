//! # State Machine Actions
//!
//! Side effects executed during state transitions for tasks and workflow steps.
//!
//! ## Overview
//!
//! Actions are the "do" part of state transitions - they perform side effects like:
//! - **Database Updates**: Setting completion timestamps, updating legacy flags
//! - **Event Publishing**: Emitting lifecycle events for observability (TAS-65)
//! - **Ownership Management**: Clearing processor ownership when entering waiting states
//! - **Result Handling**: Persisting step execution results and error information
//!
//! ## Action Types
//!
//! | Action | Purpose | Triggered By |
//! |--------|---------|--------------|
//! | `TransitionActions` | Coordinates pre/post transition logic | State machine |
//! | `PublishTransitionEventAction` | Emits lifecycle events | State transitions |
//! | `UpdateTaskCompletionAction` | Sets task completion flags | Task → Complete |
//! | `UpdateStepResultsAction` | Persists step execution results | Step state changes |
//! | `ErrorStateCleanupAction` | Logs error transitions | Step/Task → Error |
//! | `ResetAttemptsAction` | Resets retry counter | Operator retry reset |
//!
//! ## State Action Trait
//!
//! All actions implement the `StateAction<T>` trait:
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait StateAction<T> {
//!     async fn execute(
//!         &self,
//!         entity: &T,
//!         from_state: Option<String>,
//!         to_state: String,
//!         event: &str,
//!         pool: &PgPool,
//!     ) -> ActionResult<()>;
//!
//!     fn description(&self) -> &'static str;
//! }
//! ```

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

// TAS-65 Phase 1: OpenTelemetry instrumentation
use crate::metrics::orchestration::task_state_transitions_total;
use crate::metrics::worker::step_state_transitions_total;
use opentelemetry::KeyValue;
use tracing::{event, Level};

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

crate::debug_with_pgpool!(TransitionActions {
    pool: PgPool,
    event_publisher
});

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
            UPDATE tasker.tasks
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

            // TAS-65 Phase 1.2a: Emit OpenTelemetry span event
            event!(
                Level::INFO,
                event_name = event_name,
                task_uuid = %task.task_uuid,
                correlation_id = %task.correlation_id,
                from_state = from_state.as_str(),
                to_state = to_state.as_str(),
                "Task state transition"
            );

            // TAS-65 Phase 1.3a: Emit OpenTelemetry metric
            // Low-cardinality labels only: namespace, from_state, to_state
            // High-cardinality IDs (task_uuid, correlation_id) belong in spans/logs, NOT metrics
            let namespace = task
                .context
                .as_ref()
                .and_then(|ctx| ctx.get("namespace"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            task_state_transitions_total().add(
                1,
                &[
                    KeyValue::new("namespace", namespace.to_string()),
                    KeyValue::new("from_state", from_state.as_str().to_string()),
                    KeyValue::new("to_state", to_state.as_str().to_string()),
                ],
            );

            // Legacy EventPublisher for backwards compatibility
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
// TAS-65 Phase 1.2a: Comprehensive event mapping for all 14 task lifecycle events
fn determine_task_event_name_from_states(from: TaskState, to: TaskState) -> Option<&'static str> {
    use TaskState::*;

    match (from, to) {
        // Initial state transitions
        (Pending, Initializing) => Some("task.initialize_requested"),

        // TAS-41 orchestration lifecycle transitions
        (Initializing, EnqueuingSteps) => Some("task.steps_discovery_completed"),
        (EnqueuingSteps, StepsInProcess) => Some("task.steps_enqueued"),
        (StepsInProcess, EvaluatingResults) => Some("task.step_results_received"),

        // Waiting state transitions
        (_, WaitingForDependencies) => Some("task.awaiting_dependencies"),
        (WaitingForDependencies, EvaluatingResults) => Some("task.dependencies_satisfied"),
        (_, WaitingForRetry) => Some("task.retry_backoff_started"),
        (_, BlockedByFailures) => Some("task.blocked_by_failures"),

        // Terminal state transitions
        (_, Complete) => Some("task.completed"),
        (_, Error) => Some("task.failed"),
        (_, Cancelled) => Some("task.cancelled"),
        (_, ResolvedManually) => Some("task.resolved_manually"),

        // Retry transitions
        (Error, Pending) => Some("task.retry_requested"),
        (WaitingForRetry, Pending) => Some("task.retry_requested"),

        _ => None,
    }
}

// Helper function to build event context
// TAS-65 Phase 1.2a: Add correlation_id for distributed tracing (TAS-29)
fn build_task_event_context(
    task: &Task,
    from_state: TaskState,
    to_state: TaskState,
    event: &TaskEvent,
) -> Value {
    serde_json::json!({
        "task_uuid": task.task_uuid,
        "named_task_uuid": task.named_task_uuid,
        "correlation_id": task.correlation_id,
        "parent_correlation_id": task.parent_correlation_id,
        "from_state": from_state.as_str(),
        "to_state": to_state.as_str(),
        "event": format!("{:?}", event),
        "transitioned_at": Utc::now(),
        "context": task.context
    })
}

// Legacy helper function to build event context for string-based parameters
// TAS-65 Phase 1.2a: Add correlation_id for distributed tracing (TAS-29)
fn build_task_event_context_legacy(
    task: &Task,
    from_state: &Option<String>,
    to_state: &String,
    event: &str,
) -> Value {
    serde_json::json!({
        "task_uuid": task.task_uuid,
        "named_task_uuid": task.named_task_uuid,
        "correlation_id": task.correlation_id,
        "parent_correlation_id": task.parent_correlation_id,
        "from_state": from_state.as_deref().unwrap_or("unknown"),
        "to_state": to_state,
        "event": event,
        "transitioned_at": Utc::now(),
        "context": task.context
    })
}

/// Action to publish lifecycle events when state transitions occur
pub(crate) struct PublishTransitionEventAction {
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

            // TAS-65 Phase 1.2b: Emit OpenTelemetry span event
            // Note: step_name requires async DB lookup, not available in this context
            event!(
                Level::INFO,
                event_name = event_name,
                step_uuid = %step.workflow_step_uuid,
                task_uuid = %step.task_uuid,
                named_step_uuid = %step.named_step_uuid,
                from_state = from_state.as_deref().unwrap_or("none"),
                to_state = %to_state,
                "Step state transition"
            );

            // TAS-65 Phase 1.3b: Emit OpenTelemetry metric
            // Low-cardinality labels only: namespace, from_state, to_state
            // High-cardinality IDs (step_uuid, task_uuid) belong in spans/logs, NOT metrics
            // Note: Step namespace would ideally be fetched from task context
            // For now using "unknown" - will be enhanced in Phase 2
            step_state_transitions_total().add(
                1,
                &[
                    KeyValue::new("namespace", "unknown".to_string()),
                    KeyValue::new(
                        "from_state",
                        from_state.as_deref().unwrap_or("none").to_string(),
                    ),
                    KeyValue::new("to_state", to_state.clone()),
                ],
            );

            // Legacy EventPublisher for backwards compatibility
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
#[derive(Debug)]
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
#[derive(Debug)]
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
        UPDATE tasker.workflow_steps
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
        UPDATE tasker.workflow_steps
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
        UPDATE tasker.workflow_steps
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

    pub async fn handle_move_to_enqueued_as_error_for_orchestration(
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
        UPDATE tasker.workflow_steps
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
            reason: format!("Failed to mark step as enqueued as error for orchestration: {e}"),
        })?;

        tracing::info!(
            step_uuid = %step.workflow_step_uuid,
            task_uuid = %step.task_uuid,
            ?results,
            "Step marked as enqueued as error for orchestration with results preserved"
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
        UPDATE tasker.workflow_steps
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

impl Default for UpdateStepResultsAction {
    fn default() -> Self {
        Self::new()
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
                    from_state,
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
            WorkflowStepState::EnqueuedAsErrorForOrchestration => {
                self.handle_move_to_enqueued_as_error_for_orchestration(step, pool, event)
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
            WorkflowStepState::WaitingForRetry => {
                tracing::info!(
                    step_uuid = %step.workflow_step_uuid,
                    task_uuid = %step.task_uuid,
                    "Step moved to waiting for retry - will be re-enqueued after backoff delay"
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
pub(crate) struct TriggerStepDiscoveryAction {
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
#[derive(Debug)]
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

/// Action to reset step attempt counter atomically during state transition
///
/// This action is triggered when an operator uses `ResetForRetry` to reset
/// a failed step's attempt counter and return it to `pending` state for
/// automatic retry. The attempt reset happens atomically within the same
/// transaction as the state transition.
#[derive(Debug, Default)]
pub struct ResetAttemptsAction;

impl ResetAttemptsAction {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl StateAction<WorkflowStep> for ResetAttemptsAction {
    async fn execute(
        &self,
        step: &WorkflowStep,
        from_state: Option<String>,
        to_state: String,
        event: &str,
        pool: &PgPool,
    ) -> ActionResult<()> {
        // Only reset attempts when operator explicitly uses ResetForRetry event
        // Do NOT reset for automatic retries (StepEvent::Retry)
        if from_state.as_deref() == Some(WorkflowStepState::Error.as_str())
            && to_state == WorkflowStepState::Pending.as_str()
            && event.contains("reset_for_retry")
        {
            sqlx::query!(
                r#"
                UPDATE tasker.workflow_steps
                SET attempts = 0,
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
                reason: format!("Failed to reset attempt counter: {e}"),
            })?;

            tracing::info!(
                step_uuid = %step.workflow_step_uuid,
                task_uuid = %step.task_uuid,
                "Reset attempt counter to 0 for operator-initiated retry"
            );
        }

        Ok(())
    }

    fn description(&self) -> &'static str {
        "Reset attempt counter for operator-initiated retry"
    }
}

// Helper functions for event processing
// TAS-65 Phase 1.2a: Legacy string-based event mapping with comprehensive TAS-41 coverage

fn determine_task_event_name(from_state: &Option<String>, to_state: &str) -> Option<&'static str> {
    match (from_state.as_deref(), to_state) {
        // Initial state transitions
        (Some("pending"), "initializing") => Some("task.initialize_requested"),

        // TAS-41 orchestration lifecycle transitions
        (Some("initializing"), "enqueuing_steps") => Some("task.steps_discovery_completed"),
        (Some("enqueuing_steps"), "steps_in_process") => Some("task.steps_enqueued"),
        (Some("steps_in_process"), "evaluating_results") => Some("task.step_results_received"),

        // Waiting state transitions
        (_, "waiting_for_dependencies") => Some("task.awaiting_dependencies"),
        (Some("waiting_for_dependencies"), "evaluating_results") => {
            Some("task.dependencies_satisfied")
        }
        (_, "waiting_for_retry") => Some("task.retry_backoff_started"),
        (_, "blocked_by_failures") => Some("task.blocked_by_failures"),

        // Terminal state transitions
        (_, "complete") => Some("task.completed"),
        (_, "error") => Some("task.failed"),
        (_, "cancelled") => Some("task.cancelled"),
        (_, "resolved_manually") => Some("task.resolved_manually"),

        // Retry transitions
        (Some("error"), "pending") => Some("task.retry_requested"),
        (Some("waiting_for_retry"), "pending") => Some("task.retry_requested"),

        // Legacy compatibility (deprecated, use snake_case state names above)
        (_, "in_progress") => Some("task.started"), // Maps to old "in_progress" state

        _ => None,
    }
}

// TAS-65 Phase 1.2b: Comprehensive step event mapping with all 12 lifecycle events
fn determine_step_event_name(from_state: &Option<String>, to_state: &str) -> Option<&'static str> {
    match (from_state.as_deref(), to_state) {
        // Processing pipeline transitions
        (Some("pending"), "enqueued") => Some("step.enqueue_requested"),
        (Some("enqueued"), "in_progress") => Some("step.execution_requested"),
        (_, "in_progress") => Some("step.handle"), // Generic in-progress transition (handler execution)

        // Orchestration coordination transitions
        (_, "enqueued_for_orchestration") => Some("step.enqueued_for_orchestration"),
        (_, "enqueued_as_error_for_orchestration") => {
            Some("step.enqueued_as_error_for_orchestration")
        }

        // Retry transitions
        (_, "waiting_for_retry") => Some("step.retry_requested"),
        (Some("error"), "pending") => Some("step.retry_requested"),
        (Some("waiting_for_retry"), "pending") => Some("step.retry_requested"),

        // Terminal state transitions
        (_, "complete") => Some("step.completed"),
        (_, "error") => Some("step.failed"),
        (_, "cancelled") => Some("step.cancelled"),
        (_, "resolved_manually") => Some("step.resolved_manually"),

        _ => None,
    }
}

// TAS-65 Phase 1.2b: Add correlation_id for distributed tracing (TAS-29)
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
    // Note: WorkflowStep doesn't have correlation_id field - it's inherited from Task
    // Future enhancement: Add task correlation_id lookup when needed for distributed tracing
}

fn extract_results_from_event(event: &str) -> ActionResult<Option<Value>> {
    // Try to parse event as JSON to extract results from StepEvent::Complete, StepEvent::CompleteManually, StepEvent::EnqueueForOrchestration, or StepEvent::EnqueueAsErrorForOrchestration
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        // StepEvent serializes as: {"type": "Complete", "data": { results }}, {"type": "CompleteManually", "data": { results }}, {"type": "EnqueueForOrchestration", "data": { results }}, or {"type": "EnqueueAsErrorForOrchestration", "data": { results }}
        // Note: Uses variant names (PascalCase) not event_type() values (snake_case)
        if let Some(event_type) = event_data.get("type") {
            if event_type == "Complete"
                || event_type == "CompleteManually"
                || event_type == "EnqueueForOrchestration"
                || event_type == "EnqueueAsErrorForOrchestration"
            {
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
