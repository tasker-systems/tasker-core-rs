//! Event subscriber for handling step execution events

use std::sync::Arc;
use tracing::info;

use tasker_shared::types::StepExecutionResult;

use crate::{
    error::Result,
    events::{InProcessEventSystem, StepEventPayload},
};

/// Event subscriber for handling step execution events
pub struct EventSubscriber {
    event_system: Arc<InProcessEventSystem>,
}

impl EventSubscriber {
    /// Create new event subscriber
    pub fn new(event_system: Arc<InProcessEventSystem>) -> Self {
        Self { event_system }
    }

    /// Subscribe to step events with a handler function
    pub async fn subscribe_to_step_events<F>(&self, step_name: &str, handler: F) -> Result<()>
    where
        F: Fn(StepEventPayload) -> Result<StepExecutionResult> + Send + Sync + 'static,
    {
        info!("📡 Subscribing to step events for: {}", step_name);
        
        self.event_system
            .subscribe_step_handler(step_name.to_string(), handler)
            .await?;
        
        Ok(())
    }

    /// Subscribe to result events with a handler function
    pub async fn subscribe_to_result_events<F>(&self, handler: F) -> Result<()>
    where
        F: Fn(StepExecutionResult) -> Result<()> + Send + Sync + 'static,
    {
        info!("📡 Subscribing to result events");
        
        self.event_system
            .subscribe_result_handler(handler)
            .await?;
        
        Ok(())
    }

    /// Get number of registered step handlers
    pub async fn step_handler_count(&self) -> usize {
        self.event_system.step_handler_count().await
    }

    /// Get number of registered result handlers
    pub async fn result_handler_count(&self) -> usize {
        self.event_system.result_handler_count().await
    }
}