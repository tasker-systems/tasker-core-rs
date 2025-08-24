//! In-process event system for step execution coordination

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use tasker_shared::{
    models::{Sequence, Task, WorkflowStep},
    types::StepExecutionResult,
};

use crate::error::{Result, WorkerError};

/// Step event payload containing task, sequence, and step data
#[derive(Debug, Clone)]
pub struct StepEventPayload {
    pub task: Task,
    pub sequence: Sequence,
    pub step: WorkflowStep,
}

/// Step handler callback type
pub type StepHandlerCallback =
    Arc<dyn Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync>;

/// Result handler callback type
pub type ResultHandlerCallback = Arc<dyn Fn(StepExecutionResult) -> Result<()> + Send + Sync>;

/// In-process event system for step execution coordination
pub struct InProcessEventSystem {
    step_handlers: Arc<RwLock<HashMap<String, StepHandlerCallback>>>,
    result_handlers: Arc<RwLock<Vec<ResultHandlerCallback>>>,
}

impl InProcessEventSystem {
    /// Create new event system
    pub fn new() -> Self {
        Self {
            step_handlers: Arc::new(RwLock::new(HashMap::new())),
            result_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Publish step event to registered handlers
    pub async fn publish_step_event(&self, payload: StepEventPayload) -> Result<()> {
        let step_name = payload.step.named_step.name.clone();
        debug!("Publishing step event for: {}", step_name);

        let handlers = self.step_handlers.read().await;

        // Find appropriate handler based on step name
        if let Some(handler) = handlers.get(&step_name) {
            debug!("Found handler for step: {}", step_name);

            // Execute handler
            match handler(payload.clone()) {
                Ok(result) => {
                    info!(
                        "Step handler executed successfully for: {} (status: {:?})",
                        step_name, result.status
                    );

                    // Publish result event
                    self.publish_result_event(result).await?;
                }
                Err(e) => {
                    error!("Step handler failed for {}: {}", step_name, e);
                    
                    // Create error result
                    let error_result = StepExecutionResult {
                        step_id: payload.step.step_uuid,
                        status: tasker_shared::constants::ExecutionStatus::Error,
                        outputs: serde_json::json!({
                            "error": e.to_string()
                        }),
                        error: Some(e.to_string()),
                        metadata: serde_json::json!({
                            "handler_error": true,
                            "step_name": step_name
                        }),
                    };

                    // Publish error result
                    self.publish_result_event(error_result).await?;
                }
            }
        } else {
            debug!("No handler registered for step: {}", step_name);
            
            // Create skipped result
            let skipped_result = StepExecutionResult {
                step_id: payload.step.step_uuid,
                status: tasker_shared::constants::ExecutionStatus::Skipped,
                outputs: serde_json::json!({}),
                error: Some(format!("No handler registered for step: {}", step_name)),
                metadata: serde_json::json!({
                    "no_handler": true,
                    "step_name": step_name
                }),
            };

            self.publish_result_event(skipped_result).await?;
        }

        Ok(())
    }

    /// Subscribe to step events with a handler function
    pub async fn subscribe_step_handler<F>(&self, step_name: String, handler: F) -> Result<()>
    where
        F: Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync + 'static,
    {
        let mut handlers = self.step_handlers.write().await;
        handlers.insert(step_name.clone(), Arc::new(handler));
        info!("Registered step handler for: {}", step_name);
        Ok(())
    }

    /// Subscribe to result events with a handler function
    pub async fn subscribe_result_handler<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(StepExecutionResult) -> Result<()> + Send + Sync + 'static,
    {
        let mut handlers = self.result_handlers.write().await;
        handlers.push(Arc::new(handler));
        info!("Registered result handler");
        Ok(())
    }

    /// Publish result event to registered handlers
    pub async fn publish_result_event(&self, result: StepExecutionResult) -> Result<()> {
        let handlers = self.result_handlers.read().await;

        debug!(
            "Publishing result event for step: {} (status: {:?})",
            result.step_id, result.status
        );

        // Notify all result handlers
        for handler in handlers.iter() {
            if let Err(e) = handler(result.clone()) {
                error!("Result handler failed: {}", e);
            }
        }

        Ok(())
    }

    /// Get registered step handler count
    pub async fn step_handler_count(&self) -> usize {
        self.step_handlers.read().await.len()
    }

    /// Get registered result handler count
    pub async fn result_handler_count(&self) -> usize {
        self.result_handlers.read().await.len()
    }

    /// Clear all handlers (useful for testing)
    pub async fn clear_handlers(&self) {
        self.step_handlers.write().await.clear();
        self.result_handlers.write().await.clear();
    }
}

impl Default for InProcessEventSystem {
    fn default() -> Self {
        Self::new()
    }
}