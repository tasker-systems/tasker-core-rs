use async_trait::async_trait;
use sqlx::PgPool;
use serde_json::Value;
use chrono::Utc;
use crate::models::{Task, WorkflowStep};
use crate::events::publisher::EventPublisher;
use super::errors::{ActionError, ActionResult};
use super::states::{TaskState, WorkflowStepState};

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
        pool: &PgPool
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
        _pool: &PgPool
    ) -> ActionResult<()> {
        let event_name = determine_task_event_name(&from_state, &to_state);
        
        if let Some(event_name) = event_name {
            let context = build_task_event_context(task, &from_state, &to_state, event);
            
            self.event_publisher.publish(event_name, context).await
                .map_err(|_| ActionError::EventPublishFailed { 
                    event_name: event_name.to_string() 
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
        _pool: &PgPool
    ) -> ActionResult<()> {
        let event_name = determine_step_event_name(&from_state, &to_state);
        
        if let Some(event_name) = event_name {
            let context = build_step_event_context(step, &from_state, &to_state, event);
            
            self.event_publisher.publish(event_name, context).await
                .map_err(|_| ActionError::EventPublishFailed { 
                    event_name: event_name.to_string() 
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
        _pool: &PgPool
    ) -> ActionResult<()> {
        if to_state == TaskState::Complete.to_string() {
            // Log the completion for now - actual database updates will be handled
            // by the transition persistence layer and separate update operations
            tracing::info!(
                task_id = task.task_id,
                "Task marked as complete"
            );
        }
        
        Ok(())
    }

    fn description(&self) -> &'static str {
        "Update task completion metadata"
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
        _pool: &PgPool
    ) -> ActionResult<()> {
        if to_state == WorkflowStepState::Complete.to_string() {
            // Extract results from event if available
            let results = extract_results_from_event(event)?;
            
            // Log the completion for now - actual database updates will be handled
            // by the transition persistence layer and separate update operations
            tracing::info!(
                step_id = step.workflow_step_id,
                task_id = step.task_id,
                ?results,
                "Step marked as complete with results"
            );
        }
        
        Ok(())
    }

    fn description(&self) -> &'static str {
        "Update step results and completion metadata"
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
        _pool: &PgPool
    ) -> ActionResult<()> {
        if to_state == WorkflowStepState::Complete.to_string() {
            // Trigger discovery of newly viable steps
            let context = serde_json::json!({
                "task_id": step.task_id,
                "completed_step_id": step.workflow_step_id,
                "triggered_at": Utc::now()
            });
            
            self.event_publisher.publish("step.discovery_triggered", context).await
                .map_err(|_| ActionError::EventPublishFailed { 
                    event_name: "step.discovery_triggered".to_string() 
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
        _pool: &PgPool
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
        _pool: &PgPool
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
    event: &str
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
    event: &str
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
    // Try to parse event as JSON to extract results
    if let Ok(event_data) = serde_json::from_str::<Value>(event) {
        if let Some(results) = event_data.get("results") {
            return Ok(Some(results.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_name_determination() {
        assert_eq!(
            determine_task_event_name(&None, "in_progress"), 
            Some("task.started")
        );
        
        assert_eq!(
            determine_task_event_name(&Some("pending".to_string()), "complete"), 
            Some("task.completed")
        );
        
        assert_eq!(
            determine_task_event_name(&Some("error".to_string()), "pending"), 
            Some("task.reset")
        );
        
        assert_eq!(
            determine_step_event_name(&None, "complete"), 
            Some("step.completed")
        );
    }

    #[test]
    fn test_error_extraction() {
        let event_with_error = r#"{"error": "Database connection failed"}"#;
        assert_eq!(
            extract_error_from_event(event_with_error), 
            Some("Database connection failed".to_string())
        );
        
        let plain_error = "Simple error message";
        assert_eq!(
            extract_error_from_event(plain_error), 
            Some("Simple error message".to_string())
        );
    }

    #[test]
    fn test_results_extraction() {
        let event_with_results = r#"{"results": {"count": 42, "status": "ok"}}"#;
        let results = extract_results_from_event(event_with_results).unwrap();
        assert!(results.is_some());
        
        let parsed_results = results.unwrap();
        assert_eq!(parsed_results["count"], 42);
    }
}