//! # Ruby Event Handler
//!
//! Bridges WorkerEventSystem to Ruby dry-events without circular dependencies.
//! Events flow: Rust → Ruby for execution, Ruby → Rust for completion.
//!
//! Uses MPSC channel for thread-safe communication between tokio tasks and Ruby threads.

use crate::bridge;
use crate::conversions::convert_ruby_completion_to_rust;
use magnus::{Error as MagnusError, Value as RValue};
use std::sync::Arc;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    types::{StepExecutionCompletionEvent, StepExecutionEvent},
};
use tasker_shared::{TaskerError, TaskerResult};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

/// Ruby Event Handler - forwards Rust events to Ruby via MPSC channel
pub struct RubyEventHandler {
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
    event_sender: mpsc::UnboundedSender<StepExecutionEvent>,
}

impl RubyEventHandler {
    pub fn new(
        event_system: Arc<WorkerEventSystem>,
        worker_id: String,
    ) -> (Self, mpsc::UnboundedReceiver<StepExecutionEvent>) {
        let event_subscriber = Arc::new(WorkerEventSubscriber::new((*event_system).clone()));
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let handler = Self {
            event_subscriber,
            worker_id,
            event_sender,
        };

        (handler, event_receiver)
    }

    pub async fn start(&self) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            "Starting Ruby event handler - subscribing to step execution events"
        );

        let mut receiver = self.event_subscriber.subscribe_to_step_executions();
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        debug!(
                            event_id = %event.event_id,
                            step_name = %event.payload.task_sequence_step.workflow_step.name,
                            "Received step execution event - sending to channel for Ruby processing"
                        );

                        // Send to channel instead of directly calling Ruby
                        if let Err(e) = event_sender.send(event) {
                            error!(
                                error = %e,
                                "Failed to send event to Ruby channel"
                            );
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(lagged_count = count, "Ruby event handler lagged behind");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Event channel closed - stopping Ruby event handler");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    // This method is no longer used - events are sent through the channel instead
    // Ruby will poll for events using the receiver returned from new()

    /// Handle completion event from Ruby and forward to Rust event system
    pub async fn handle_completion(
        &self,
        completion: StepExecutionCompletionEvent,
    ) -> TaskerResult<()> {
        // Get the global event system
        let event_system = crate::global_event_system::get_global_event_system();

        // Publish completion to the event system
        event_system
            .publish_step_completion(completion)
            .await
            .map_err(|err| TaskerError::WorkerError(err.to_string()))?;

        Ok(())
    }
}

/// Called by Ruby when step processing completes
/// This properly sends the completion event to the global event system
pub fn send_step_completion_event(completion_data: RValue) -> Result<(), MagnusError> {
    // Convert Ruby completion to Rust event
    let rust_completion = convert_ruby_completion_to_rust(completion_data).map_err(|err| {
        error!("Could not convert magnus Value to StepExecutionCompletionEvent: {err}");
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Could not convert magnus Value to StepExecutionCompletionEvent: {err}"),
        )
    })?;

    // Get the bridge handle to access the event handler
    let handle_guard = bridge::WORKER_SYSTEM.lock().map_err(|err| {
        error!("Could not acquire WORKER_SYSTEM handle lock: {err}");
        MagnusError::new(
            magnus::exception::runtime_error(),
            format!("Could not acquire WORKER_SYSTEM handle lock: {err}"),
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        error!("Could not acquire WORKER_SYSTEM handle");
        MagnusError::new(
            magnus::exception::runtime_error(),
            "Could not acquire WORKER_SYSTEM handle".to_string(),
        )
    })?;

    // Use the event handler to publish the completion
    handle.runtime_handle().block_on(async {
        handle
            .event_handler()
            .handle_completion(rust_completion)
            .await
            .map_err(|err| {
                error!("Unable to handle event completion: {err}");
                MagnusError::new(
                    magnus::exception::runtime_error(),
                    format!("Unable to handle event completion: {err}"),
                )
            })
    })?;
    Ok(())
}
