//! # Ruby Event Handler
//!
//! Bridges `WorkerEventSystem` to Ruby dry-events without circular dependencies.
//! Events flow: Rust → Ruby for execution, Ruby → Rust for completion.
//!
//! Uses MPSC channel for thread-safe communication between tokio tasks and Ruby threads.

use crate::bridge;
use crate::conversions::convert_ruby_completion_to_rust;
use magnus::{Error as MagnusError, Value as RValue};
use std::sync::Arc;
use tasker_shared::monitoring::ChannelMonitor;
use tasker_shared::{
    events::{WorkerEventSubscriber, WorkerEventSystem},
    types::{StepExecutionCompletionEvent, StepExecutionEvent},
};
use tasker_shared::{TaskerError, TaskerResult};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

/// Ruby Event Handler - forwards Rust events to Ruby via MPSC channel (TAS-51: bounded)
pub struct RubyEventHandler {
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
    event_sender: mpsc::Sender<StepExecutionEvent>,
    channel_monitor: ChannelMonitor,
}

impl RubyEventHandler {
    /// Create new Ruby event handler with bounded channel
    ///
    /// # Arguments
    /// * `event_system` - Worker event system to subscribe to
    /// * `worker_id` - Worker identifier
    /// * `buffer_size` - MPSC channel buffer size (TAS-51: bounded channels)
    ///
    /// # Note
    /// TAS-51: Migrated from unbounded to bounded channel to provide backpressure.
    /// Buffer size should come from: `config.mpsc_channels.shared.ffi.ruby_event_buffer_size`
    pub fn new(
        event_system: Arc<WorkerEventSystem>,
        worker_id: String,
        buffer_size: usize,
    ) -> (Self, mpsc::Receiver<StepExecutionEvent>) {
        let event_subscriber = Arc::new(WorkerEventSubscriber::new((*event_system).clone()));
        let (event_sender, event_receiver) = mpsc::channel(buffer_size);

        // TAS-51: Initialize channel monitor for observability
        let channel_monitor = ChannelMonitor::new("ruby_ffi_event_handler", buffer_size);

        let handler = Self {
            event_subscriber,
            worker_id,
            event_sender,
            channel_monitor,
        };

        (handler, event_receiver)
    }

    pub async fn start(&self) -> TaskerResult<()> {
        info!(
            worker_id = %self.worker_id,
            channel_monitor = %self.channel_monitor.channel_name(),
            buffer_size = self.channel_monitor.buffer_size(),
            "Starting Ruby event handler with channel monitoring"
        );

        let mut receiver = self.event_subscriber.subscribe_to_step_executions();
        let event_sender = self.event_sender.clone();
        // TAS-51: Clone channel monitor for observability in spawned task
        let monitor = self.channel_monitor.clone();

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
                        match event_sender.send(event).await {
                            Ok(()) => {
                                // TAS-51: Record send and periodically check saturation (optimized)
                                if monitor.record_send_success() {
                                    monitor.check_and_warn_saturation(event_sender.capacity());
                                }
                            }
                            Err(e) => {
                                error!(
                                    error = %e,
                                    "Failed to send event to Ruby channel"
                                );
                            }
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
