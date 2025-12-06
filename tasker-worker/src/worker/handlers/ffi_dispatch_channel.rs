//! # FFI Dispatch Channel
//!
//! TAS-67: Polling-based dispatch channel for FFI languages (Ruby, Python).
//!
//! Unlike Rust handlers that can be called directly via async traits, FFI
//! languages operate in their own runtime and need to poll for work. This
//! module provides a polling-based dispatch pattern that matches Ruby's
//! existing `EventBridge.poll` approach.
//!
//! ## Architecture
//!
//! ```text
//! WorkerCore creates DispatchHandles:
//!     dispatch_receiver ──→ FfiDispatchChannel
//!     completion_sender ──→ FfiDispatchChannel
//!
//! FfiDispatchChannel provides:
//!     poll()     ← Ruby/Python runtime calls this
//!         │
//!         └─→ Returns FfiStepEvent
//!
//!     complete() ← Ruby/Python handler calls this
//!         │
//!         ├─→ Invokes PostHandlerCallback (domain events)
//!         └─→ Sends StepExecutionResult to completion channel
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Rust side: Create channel from DispatchHandles with domain event callback
//! let dispatch_handles = worker_handle.take_dispatch_handles().unwrap();
//! let ffi_channel = FfiDispatchChannel::new(
//!     dispatch_handles.dispatch_receiver,
//!     dispatch_handles.completion_sender,
//!     FfiDispatchChannelConfig::default(),
//!     domain_event_callback,  // Required for domain event publishing
//! );
//!
//! // FFI side (called from Ruby/Python):
//! loop {
//!     if let Some(event) = ffi_channel.poll() {
//!         // Execute handler
//!         let result = handler.call(event.step_data);
//!         ffi_channel.complete(event.event_id, result);
//!     }
//!     std::thread::sleep(Duration::from_millis(10));
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, warn};
use uuid::Uuid;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::base::{StepEventPayload, StepExecutionEvent};

use super::dispatch_service::PostHandlerCallback;
use crate::worker::actors::DispatchHandlerMessage;

/// Configuration for FFI dispatch channel
#[derive(Debug, Clone)]
pub struct FfiDispatchChannelConfig {
    /// Maximum time to wait for completion before timing out
    pub completion_timeout: Duration,
    /// Service identifier for logging
    pub service_id: String,
    /// Tokio runtime handle for executing async callbacks from FFI threads
    /// This is required because FFI languages (Ruby/Python) call complete() from
    /// their own threads which don't have a Tokio runtime context.
    pub runtime_handle: tokio::runtime::Handle,
}

impl FfiDispatchChannelConfig {
    /// Create a new config with the given runtime handle
    pub fn new(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            completion_timeout: Duration::from_secs(30),
            service_id: "ffi-dispatch".to_string(),
            runtime_handle,
        }
    }

    /// Create config with a custom service ID
    pub fn with_service_id(mut self, service_id: impl Into<String>) -> Self {
        self.service_id = service_id.into();
        self
    }

    /// Create config with a custom completion timeout
    pub fn with_completion_timeout(mut self, timeout: Duration) -> Self {
        self.completion_timeout = timeout;
        self
    }
}

/// Step event for FFI dispatch
///
/// This is the struct that FFI languages receive when polling for work.
/// It contains all the information needed to execute a step handler.
#[derive(Debug, Clone)]
pub struct FfiStepEvent {
    /// Unique event identifier for correlation
    pub event_id: Uuid,
    /// Task UUID
    pub task_uuid: Uuid,
    /// Step UUID
    pub step_uuid: Uuid,
    /// Correlation ID for tracing
    pub correlation_id: Uuid,
    /// The step execution event with full payload
    pub execution_event: StepExecutionEvent,
    /// Optional trace context
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

impl FfiStepEvent {
    /// Create from a dispatch message
    fn from_dispatch_message(msg: &DispatchHandlerMessage) -> Self {
        let payload =
            StepEventPayload::new(msg.task_uuid, msg.step_uuid, msg.task_sequence_step.clone());

        let (trace_id, span_id) = msg
            .trace_context
            .as_ref()
            .map(|tc| (Some(tc.trace_id.clone()), Some(tc.span_id.clone())))
            .unwrap_or((None, None));

        let execution_event = StepExecutionEvent::with_event_id_and_trace(
            msg.event_id,
            payload,
            trace_id.clone(),
            span_id.clone(),
        );

        Self {
            event_id: msg.event_id,
            task_uuid: msg.task_uuid,
            step_uuid: msg.step_uuid,
            correlation_id: msg.correlation_id,
            execution_event,
            trace_id,
            span_id,
        }
    }
}

/// Pending event tracking for completion correlation
struct PendingEvent {
    event: FfiStepEvent,
    dispatched_at: std::time::Instant,
}

/// FFI dispatch channel for polling-based languages
///
/// TAS-67: This channel allows FFI languages to poll for step execution
/// events and submit completions. It bridges the async Rust world with
/// the synchronous polling pattern used by Ruby/Python.
///
/// The channel always invokes the post-handler callback on completion,
/// which handles domain event publishing based on step configuration.
///
/// ## Integration with WorkerCore
///
/// This channel is designed to work with `DispatchHandles` from `WorkerCore`:
///
/// ```rust,ignore
/// let dispatch_handles = worker_handle.take_dispatch_handles().unwrap();
/// let ffi_channel = FfiDispatchChannel::new(
///     dispatch_handles.dispatch_receiver,
///     dispatch_handles.completion_sender,
///     config,
///     domain_event_callback,
/// );
/// ```
pub struct FfiDispatchChannel {
    /// Receiver for dispatch messages (from WorkerCore via DispatchHandles)
    dispatch_receiver: Arc<Mutex<mpsc::Receiver<DispatchHandlerMessage>>>,
    /// Pending events waiting for completion
    pending_events: Arc<RwLock<HashMap<Uuid, PendingEvent>>>,
    /// Completion sender for sending results back (from DispatchHandles)
    completion_sender: mpsc::Sender<StepExecutionResult>,
    /// Configuration
    config: FfiDispatchChannelConfig,
    /// Post-handler callback for domain events, metrics, etc.
    post_handler_callback: Arc<dyn PostHandlerCallback>,
}

impl std::fmt::Debug for FfiDispatchChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfiDispatchChannel")
            .field("service_id", &self.config.service_id)
            .field("pending_count", &self.pending_count())
            .field("callback", &self.post_handler_callback.name())
            .finish()
    }
}

impl FfiDispatchChannel {
    /// Create a new FFI dispatch channel from DispatchHandles
    ///
    /// TAS-67: The post-handler callback is required and invoked after each
    /// handler completes. This enables domain event publishing through the
    /// step configuration.
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Receiver from `DispatchHandles.dispatch_receiver`
    /// * `completion_sender` - Sender from `DispatchHandles.completion_sender`
    /// * `config` - Channel configuration
    /// * `callback` - Post-handler callback for domain events, metrics, etc.
    pub fn new(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        config: FfiDispatchChannelConfig,
        callback: Arc<dyn PostHandlerCallback>,
    ) -> Self {
        Self {
            dispatch_receiver: Arc::new(Mutex::new(dispatch_receiver)),
            pending_events: Arc::new(RwLock::new(HashMap::new())),
            completion_sender,
            config,
            post_handler_callback: callback,
        }
    }

    /// Poll for the next step event (non-blocking)
    ///
    /// This method is called from FFI languages (Ruby/Python) to get the next
    /// step to execute. Returns `None` if no event is currently available.
    ///
    /// The caller is responsible for implementing their own polling loop with
    /// appropriate sleep intervals. This matches the Ruby `EventBridge.poll(10ms)`
    /// pattern where the timeout controls sleep between polls, not blocking wait.
    ///
    /// # Returns
    ///
    /// `Some(FfiStepEvent)` if an event is available, `None` otherwise.
    pub fn poll(&self) -> Option<FfiStepEvent> {
        // Use try_lock to avoid blocking if another thread is polling
        let mut receiver = match self.dispatch_receiver.try_lock() {
            Ok(r) => r,
            Err(_) => {
                warn!(
                    service_id = %self.config.service_id,
                    "FfiDispatchChannel.poll: receiver lock contention"
                );
                return None;
            }
        };

        // Use try_recv for non-blocking poll
        match receiver.try_recv() {
            Ok(msg) => {
                let event = FfiStepEvent::from_dispatch_message(&msg);
                let event_id = event.event_id;

                debug!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    step_uuid = %event.step_uuid,
                    "FFI dispatch: event polled"
                );

                // Track pending event for completion
                {
                    let mut pending = self
                        .pending_events
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    pending.insert(
                        event_id,
                        PendingEvent {
                            event: event.clone(),
                            dispatched_at: std::time::Instant::now(),
                        },
                    );
                }

                Some(event)
            }
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                debug!(
                    service_id = %self.config.service_id,
                    "FFI dispatch: channel disconnected"
                );
                None
            }
        }
    }

    /// Poll for the next step event asynchronously
    ///
    /// For use within Rust async context (e.g., in tests or async FFI bridges).
    pub async fn poll_async(&self, timeout: Duration) -> Option<FfiStepEvent> {
        let mut receiver = self.dispatch_receiver.lock().await;

        match tokio::time::timeout(timeout, receiver.recv()).await {
            Ok(Some(msg)) => {
                let event = FfiStepEvent::from_dispatch_message(&msg);
                let event_id = event.event_id;

                debug!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    step_uuid = %event.step_uuid,
                    "FFI dispatch: event polled (async)"
                );

                // Track pending event
                {
                    let mut pending = self
                        .pending_events
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    pending.insert(
                        event_id,
                        PendingEvent {
                            event: event.clone(),
                            dispatched_at: std::time::Instant::now(),
                        },
                    );
                }

                Some(event)
            }
            Ok(None) => {
                debug!(
                    service_id = %self.config.service_id,
                    "FFI dispatch: channel closed"
                );
                None
            }
            Err(_) => None, // Timeout
        }
    }

    /// Submit a completion result (blocking)
    ///
    /// Called from FFI languages after a handler has completed execution.
    /// This is the blocking version for use from Ruby/Python threads.
    ///
    /// TAS-67: Invokes the post-handler callback for domain event publishing
    /// before sending the result to the completion channel.
    ///
    /// # Arguments
    ///
    /// * `event_id` - The event ID from the `FfiStepEvent`
    /// * `result` - The step execution result
    ///
    /// # Returns
    ///
    /// `true` if the completion was successfully submitted, `false` otherwise.
    pub fn complete(&self, event_id: Uuid, result: StepExecutionResult) -> bool {
        // Remove from pending
        let pending_event = {
            let mut pending = self
                .pending_events
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pending.remove(&event_id)
        };

        if let Some(pending) = pending_event {
            let elapsed = pending.dispatched_at.elapsed();

            debug!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                step_uuid = %pending.event.step_uuid,
                elapsed_ms = elapsed.as_millis(),
                success = result.success,
                "FFI dispatch: completion received"
            );

            // Extract step data for callback (needed after send since result is moved)
            let step = pending
                .event
                .execution_event
                .payload
                .task_sequence_step
                .clone();
            let worker_id = self.config.service_id.clone();

            // 1. Send to completion channel FIRST (blocking)
            // This ensures the result is committed to the pipeline before domain events fire
            match self.completion_sender.blocking_send(result.clone()) {
                Ok(()) => {
                    debug!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        "FFI dispatch: completion sent to channel"
                    );

                    // 2. Invoke post-handler callback AFTER successful send
                    // Domain events only fire after the result is committed to the pipeline
                    debug!(
                        service_id = %self.config.service_id,
                        callback_name = %self.post_handler_callback.name(),
                        event_id = %event_id,
                        "FFI dispatch: invoking post-handler callback"
                    );

                    // Use the stored runtime handle to call async callback from FFI thread
                    // This is required because FFI languages (Ruby/Python) call complete() from
                    // their own threads which don't have a Tokio runtime context.
                    self.config.runtime_handle.block_on(
                        self.post_handler_callback
                            .on_handler_complete(&step, &result, &worker_id),
                    );

                    true
                }
                Err(e) => {
                    error!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        error = %e,
                        "FFI dispatch: failed to send completion - domain events NOT fired"
                    );
                    false
                }
            }
        } else {
            warn!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                "FFI dispatch: completion for unknown event"
            );
            false
        }
    }

    /// Submit a completion result asynchronously
    ///
    /// TAS-67: Invokes the post-handler callback for domain event publishing
    /// AFTER successfully sending the result to the completion channel.
    pub async fn complete_async(&self, event_id: Uuid, result: StepExecutionResult) -> bool {
        // Remove from pending
        let pending_event = {
            let mut pending = self
                .pending_events
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pending.remove(&event_id)
        };

        if let Some(pending) = pending_event {
            let elapsed = pending.dispatched_at.elapsed();

            debug!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                step_uuid = %pending.event.step_uuid,
                elapsed_ms = elapsed.as_millis(),
                success = result.success,
                "FFI dispatch: completion received (async)"
            );

            // Extract step data for callback (needed after send since result is moved)
            let step = pending
                .event
                .execution_event
                .payload
                .task_sequence_step
                .clone();
            let worker_id = self.config.service_id.clone();

            // 1. Send to completion channel FIRST
            // This ensures the result is committed to the pipeline before domain events fire
            match self.completion_sender.send(result.clone()).await {
                Ok(()) => {
                    debug!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        "FFI dispatch: completion sent to channel (async)"
                    );

                    // 2. Invoke post-handler callback AFTER successful send
                    // Domain events only fire after the result is committed to the pipeline
                    debug!(
                        service_id = %self.config.service_id,
                        callback_name = %self.post_handler_callback.name(),
                        event_id = %event_id,
                        "FFI dispatch: invoking post-handler callback (async)"
                    );

                    self.post_handler_callback
                        .on_handler_complete(&step, &result, &worker_id)
                        .await;

                    true
                }
                Err(e) => {
                    error!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        error = %e,
                        "FFI dispatch: failed to send completion (async) - domain events NOT fired"
                    );
                    false
                }
            }
        } else {
            warn!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                "FFI dispatch: completion for unknown event (async)"
            );
            false
        }
    }

    /// Get the number of pending events
    pub fn pending_count(&self) -> usize {
        self.pending_events
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    /// Clean up timed-out pending events
    ///
    /// Call this periodically to detect handlers that never completed.
    /// Returns the number of events that timed out.
    pub fn cleanup_timeouts(&self) -> usize {
        let mut pending = self
            .pending_events
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let timeout = self.config.completion_timeout;
        let now = std::time::Instant::now();

        let timed_out: Vec<Uuid> = pending
            .iter()
            .filter(|(_, p)| now.duration_since(p.dispatched_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for event_id in &timed_out {
            if let Some(pending_event) = pending.remove(event_id) {
                error!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    step_uuid = %pending_event.event.step_uuid,
                    timeout_ms = timeout.as_millis(),
                    "FFI dispatch: event timed out without completion"
                );

                // Send timeout failure to completion channel
                let result = StepExecutionResult::failure(
                    pending_event.event.step_uuid,
                    format!("FFI handler timed out after {}ms", timeout.as_millis()),
                    None,
                    Some("ffi_timeout".to_string()),
                    true, // Retryable
                    timeout.as_millis() as i64,
                    None,
                );

                // Non-blocking send - if channel is full, we just log
                if self.completion_sender.try_send(result).is_err() {
                    error!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        "FFI dispatch: failed to send timeout result"
                    );
                }
            }
        }

        timed_out.len()
    }
}

#[cfg(test)]
mod tests {
    use super::super::dispatch_service::NoOpCallback;
    use super::*;

    #[tokio::test]
    async fn test_ffi_dispatch_channel_config_new() {
        let handle = tokio::runtime::Handle::current();
        let config = FfiDispatchChannelConfig::new(handle);
        assert_eq!(config.completion_timeout, Duration::from_secs(30));
        assert_eq!(config.service_id, "ffi-dispatch");
    }

    #[tokio::test]
    async fn test_ffi_dispatch_channel_config_builder() {
        let handle = tokio::runtime::Handle::current();
        let config = FfiDispatchChannelConfig::new(handle)
            .with_service_id("test-service")
            .with_completion_timeout(Duration::from_secs(60));
        assert_eq!(config.completion_timeout, Duration::from_secs(60));
        assert_eq!(config.service_id, "test-service");
    }

    #[tokio::test]
    async fn test_ffi_dispatch_channel_creation() {
        let (dispatch_tx, dispatch_rx) = mpsc::channel(100);
        let (completion_tx, _completion_rx) = mpsc::channel(100);
        let handle = tokio::runtime::Handle::current();
        let config = FfiDispatchChannelConfig::new(handle);
        let callback = Arc::new(NoOpCallback);
        let channel = FfiDispatchChannel::new(dispatch_rx, completion_tx, config, callback);

        assert_eq!(channel.pending_count(), 0);
        drop(dispatch_tx); // Keep sender alive until here
    }

    #[tokio::test]
    async fn test_ffi_dispatch_channel_poll_empty() {
        let (_dispatch_tx, dispatch_rx) = mpsc::channel(100);
        let (completion_tx, _completion_rx) = mpsc::channel(100);
        let handle = tokio::runtime::Handle::current();
        let config = FfiDispatchChannelConfig::new(handle);
        let callback = Arc::new(NoOpCallback);
        let channel = FfiDispatchChannel::new(dispatch_rx, completion_tx, config, callback);

        // Poll should return None immediately when empty
        let result = channel.poll_async(Duration::from_millis(10)).await;
        assert!(result.is_none());
    }
}
