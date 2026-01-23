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
//! // FfiDispatchChannel is an internal module - accessed via FFI bindings
//! use tasker_worker::worker::handlers::ffi_dispatch_channel::{
//!     FfiDispatchChannel, FfiDispatchChannelConfig
//! };
//! use tasker_shared::messaging::StepExecutionResult;
//! use std::time::Duration;
//!
//! // FFI side polling pattern (called from Ruby/Python):
//! fn poll_loop(ffi_channel: &FfiDispatchChannel) {
//!     loop {
//!         if let Some(event) = ffi_channel.poll() {
//!             // Execute handler in FFI language
//!             let result = StepExecutionResult::default();
//!             ffi_channel.complete(event.event_id, result);
//!         }
//!         std::thread::sleep(Duration::from_millis(10));
//!     }
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::models::batch_worker::CheckpointYieldData;
use tasker_shared::types::base::{StepEventPayload, StepExecutionEvent};

use super::dispatch_service::PostHandlerCallback;
use super::ffi_completion_circuit_breaker::FfiCompletionCircuitBreaker;
use crate::worker::actors::{DispatchHandlerMessage, TraceContext};
use crate::worker::services::{CheckpointError, CheckpointService};

/// Metrics about pending events for monitoring
///
/// TAS-67 Phase 2: Provides observability into FFI dispatch channel health.
/// Used for detecting polling starvation and capacity planning.
#[derive(Debug, Clone, Default)]
pub struct FfiDispatchMetrics {
    /// Total pending events in buffer
    pub pending_count: usize,
    /// Age of oldest pending event in milliseconds
    pub oldest_pending_age_ms: Option<u64>,
    /// Age of newest pending event in milliseconds
    pub newest_pending_age_ms: Option<u64>,
    /// UUID of oldest event (for debugging)
    pub oldest_event_id: Option<Uuid>,
    /// Whether any events exceed the starvation warning threshold
    pub starvation_detected: bool,
    /// Number of events exceeding starvation threshold
    pub starving_event_count: usize,
}

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
    /// Timeout for post-handler callbacks (domain event publishing)
    /// TAS-67 Risk Mitigation: Prevents indefinite blocking of FFI threads
    pub callback_timeout: Duration,
    /// TAS-67 Phase 2: Warn if any pending event exceeds this age (milliseconds)
    /// Used for detecting polling starvation before timeout occurs
    pub starvation_warning_threshold_ms: u64,
    /// TAS-67 Phase 2: Maximum time to wait when completion channel is full
    /// If exceeded, logs error and returns false
    pub completion_send_timeout: Duration,
}

impl FfiDispatchChannelConfig {
    /// Create a new config with the given runtime handle
    pub fn new(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            completion_timeout: Duration::from_secs(30),
            service_id: "ffi-dispatch".to_string(),
            runtime_handle,
            callback_timeout: Duration::from_secs(5),
            starvation_warning_threshold_ms: 10_000, // 10 seconds
            completion_send_timeout: Duration::from_secs(10),
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

    /// Create config with a custom callback timeout
    pub fn with_callback_timeout(mut self, timeout: Duration) -> Self {
        self.callback_timeout = timeout;
        self
    }

    /// Create config with a custom starvation warning threshold
    pub fn with_starvation_threshold_ms(mut self, threshold_ms: u64) -> Self {
        self.starvation_warning_threshold_ms = threshold_ms;
        self
    }

    /// Create config with a custom completion send timeout
    pub fn with_completion_send_timeout(mut self, timeout: Duration) -> Self {
        self.completion_send_timeout = timeout;
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
#[derive(Clone)]
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
/// ## Circuit Breaker Integration (TAS-75)
///
/// Optionally, the channel can be configured with a latency-based circuit
/// breaker that protects against completion channel backpressure. When
/// enabled, the circuit breaker tracks send latency and fails fast when
/// the circuit is open.
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
///
/// // Or with circuit breaker
/// let ffi_channel = FfiDispatchChannel::with_circuit_breaker(
///     dispatch_handles.dispatch_receiver,
///     dispatch_handles.completion_sender,
///     config,
///     domain_event_callback,
///     circuit_breaker,
/// );
/// ```
pub struct FfiDispatchChannel {
    /// Receiver for dispatch messages (from WorkerCore via DispatchHandles)
    dispatch_receiver: Arc<Mutex<mpsc::Receiver<DispatchHandlerMessage>>>,
    /// Pending events waiting for completion
    pending_events: Arc<DashMap<Uuid, PendingEvent>>,
    /// Completion sender for sending results back (from DispatchHandles)
    completion_sender: mpsc::Sender<StepExecutionResult>,
    /// Configuration
    config: FfiDispatchChannelConfig,
    /// Post-handler callback for domain events, metrics, etc.
    post_handler_callback: Arc<dyn PostHandlerCallback>,
    /// TAS-75: Optional circuit breaker for completion channel sends
    circuit_breaker: Option<Arc<FfiCompletionCircuitBreaker>>,
    /// TAS-125: Optional checkpoint service for batch processing
    checkpoint_service: Option<CheckpointService>,
    /// TAS-125: Optional dispatch sender for checkpoint continuation re-dispatch
    dispatch_sender: Option<mpsc::Sender<DispatchHandlerMessage>>,
}

impl std::fmt::Debug for FfiDispatchChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FfiDispatchChannel")
            .field("service_id", &self.config.service_id)
            .field("pending_count", &self.pending_count())
            .field("callback", &self.post_handler_callback.name())
            .field("circuit_breaker_enabled", &self.circuit_breaker.is_some())
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
            pending_events: Arc::new(DashMap::new()),
            completion_sender,
            config,
            post_handler_callback: callback,
            circuit_breaker: None,
            checkpoint_service: None,
            dispatch_sender: None,
        }
    }

    /// Create a new FFI dispatch channel with circuit breaker protection
    ///
    /// TAS-75 Phase 5a: Adds latency-based circuit breaker for completion channel sends.
    /// When the circuit is open, completions fail fast and the step returns to the queue
    /// via PGMQ visibility timeout.
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Receiver from `DispatchHandles.dispatch_receiver`
    /// * `completion_sender` - Sender from `DispatchHandles.completion_sender`
    /// * `config` - Channel configuration
    /// * `callback` - Post-handler callback for domain events, metrics, etc.
    /// * `circuit_breaker` - Circuit breaker for completion channel sends
    pub fn with_circuit_breaker(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        config: FfiDispatchChannelConfig,
        callback: Arc<dyn PostHandlerCallback>,
        circuit_breaker: Arc<FfiCompletionCircuitBreaker>,
    ) -> Self {
        Self {
            dispatch_receiver: Arc::new(Mutex::new(dispatch_receiver)),
            pending_events: Arc::new(DashMap::new()),
            completion_sender,
            config,
            post_handler_callback: callback,
            circuit_breaker: Some(circuit_breaker),
            checkpoint_service: None,
            dispatch_sender: None,
        }
    }

    /// Configure checkpoint support for batch processing (TAS-125)
    ///
    /// This enables the `checkpoint_yield()` method which persists checkpoint
    /// data and re-dispatches the step for continuation.
    ///
    /// # Arguments
    ///
    /// * `checkpoint_service` - Service for persisting checkpoints
    /// * `dispatch_sender` - Sender for re-dispatching steps after checkpoint
    pub fn with_checkpoint_support(
        mut self,
        checkpoint_service: CheckpointService,
        dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
    ) -> Self {
        self.checkpoint_service = Some(checkpoint_service);
        self.dispatch_sender = Some(dispatch_sender);
        self
    }

    /// Get reference to the circuit breaker (if configured)
    ///
    /// TAS-75: Useful for exposing circuit breaker metrics in health endpoints.
    pub fn circuit_breaker(&self) -> Option<&Arc<FfiCompletionCircuitBreaker>> {
        self.circuit_breaker.as_ref()
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
                self.pending_events.insert(
                    event_id,
                    PendingEvent {
                        event: event.clone(),
                        dispatched_at: std::time::Instant::now(),
                    },
                );

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
                self.pending_events.insert(
                    event_id,
                    PendingEvent {
                        event: event.clone(),
                        dispatched_at: std::time::Instant::now(),
                    },
                );

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
    /// TAS-75: When circuit breaker is configured, checks breaker state before
    /// sending and records latency-based results. Fails fast when circuit is open.
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
        let pending_event = self.pending_events.remove(&event_id).map(|(_, v)| v);

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

            // TAS-75: Check circuit breaker BEFORE attempting send
            // If circuit is open, fail fast - step will return to queue via visibility timeout
            if let Some(ref cb) = self.circuit_breaker {
                let allowed = cb.should_allow();
                if !allowed {
                    warn!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        step_uuid = %pending.event.step_uuid,
                        "FFI dispatch: circuit breaker open, failing fast - step will return to queue"
                    );
                    // Re-add to pending so cleanup_timeouts can handle it
                    // Actually, no - we just return false. The step remains claimed but incomplete.
                    // PGMQ visibility timeout will make it available again for another worker.
                    return false;
                }
            }

            // Extract step data for callback (needed after send since result is moved)
            let step = pending
                .event
                .execution_event
                .payload
                .task_sequence_step
                .clone();
            let worker_id = self.config.service_id.clone();

            // 1. Send to completion channel FIRST with timeout and retry
            // This ensures the result is committed to the pipeline before domain events fire
            // TAS-67 Phase 2: Uses try_send with retry loop instead of blocking indefinitely
            // TAS-75: Track send latency for circuit breaker
            let send_timeout = self.config.completion_send_timeout;
            let send_start = std::time::Instant::now();
            let send_result = loop {
                match self.completion_sender.try_send(result.clone()) {
                    Ok(()) => {
                        let send_elapsed = send_start.elapsed();
                        if send_elapsed > Duration::from_millis(100) {
                            warn!(
                                service_id = %self.config.service_id,
                                event_id = %event_id,
                                elapsed_ms = send_elapsed.as_millis(),
                                "FFI dispatch: completion send was delayed due to channel backpressure"
                            );
                        }
                        break Ok(send_elapsed);
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        if send_start.elapsed() > send_timeout {
                            break Err("completion channel full - timeout exceeded");
                        }
                        // Brief sleep before retry (10ms)
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        break Err("completion channel closed");
                    }
                }
            };

            // TAS-75: Record send result with circuit breaker (latency-based)
            let send_success = send_result.is_ok();
            let send_elapsed = send_result.unwrap_or(send_start.elapsed());
            if let Some(ref cb) = self.circuit_breaker {
                cb.record_send_result(send_elapsed, send_success);
            }

            if send_success {
                debug!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    "FFI dispatch: completion sent to channel"
                );

                // 2. Invoke post-handler callback AFTER successful send
                // Domain events only fire after the result is committed to the pipeline
                //
                // TAS-67 Risk Mitigation: Fire-and-forget pattern with timeout
                // Instead of blocking the FFI thread with block_on(), we spawn the callback
                // as a separate task. This prevents the Ruby/Python thread from being held
                // indefinitely if the callback hangs or experiences network issues.
                debug!(
                    service_id = %self.config.service_id,
                    callback_name = %self.post_handler_callback.name(),
                    event_id = %event_id,
                    "FFI dispatch: spawning post-handler callback (fire-and-forget)"
                );

                // Clone values needed for the spawned task
                let callback = self.post_handler_callback.clone();
                let callback_timeout = self.config.callback_timeout;
                let service_id = self.config.service_id.clone();

                // Spawn callback as fire-and-forget - don't block FFI thread
                self.config.runtime_handle.spawn(async move {
                    match tokio::time::timeout(
                        callback_timeout,
                        callback.on_handler_complete(&step, &result, &worker_id),
                    )
                    .await
                    {
                        Ok(()) => {
                            debug!(
                                service_id = %service_id,
                                event_id = %event_id,
                                "FFI dispatch: post-handler callback completed"
                            );
                        }
                        Err(_) => {
                            // Timeout - log but don't fail the completion
                            // The step result is already committed to the pipeline
                            error!(
                                service_id = %service_id,
                                event_id = %event_id,
                                timeout_ms = callback_timeout.as_millis(),
                                "FFI dispatch: post-handler callback timed out"
                            );
                        }
                    }
                });

                true
            } else {
                // Send failed - extract reason for logging
                let reason = send_result.unwrap_err();
                error!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    reason = %reason,
                    timeout_ms = send_timeout.as_millis(),
                    "FFI dispatch: failed to send completion - domain events NOT fired"
                );
                false
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
        let pending_event = self.pending_events.remove(&event_id).map(|(_, v)| v);

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
        self.pending_events.len()
    }

    /// Get current metrics about pending events
    ///
    /// TAS-67 Phase 2: Provides comprehensive observability for FFI dispatch health.
    /// Returns metrics including pending count, event ages, and starvation detection.
    pub fn metrics(&self) -> FfiDispatchMetrics {
        if self.pending_events.is_empty() {
            return FfiDispatchMetrics::default();
        }

        let now = std::time::Instant::now();
        let threshold = Duration::from_millis(self.config.starvation_warning_threshold_ms);

        let mut ages: Vec<(Uuid, u64)> = self
            .pending_events
            .iter()
            .map(|entry| {
                let age = now.duration_since(entry.value().dispatched_at).as_millis() as u64;
                (*entry.key(), age)
            })
            .collect();

        // Sort by age ascending (newest first, oldest last)
        ages.sort_by_key(|(_, age)| *age);

        let starving_count = ages
            .iter()
            .filter(|(_, age)| Duration::from_millis(*age) > threshold)
            .count();

        FfiDispatchMetrics {
            pending_count: self.pending_events.len(),
            oldest_pending_age_ms: ages.last().map(|(_, age)| *age),
            newest_pending_age_ms: ages.first().map(|(_, age)| *age),
            oldest_event_id: ages.last().map(|(id, _)| *id),
            starvation_detected: starving_count > 0,
            starving_event_count: starving_count,
        }
    }

    /// Check for aging events and emit warnings
    ///
    /// TAS-67 Phase 2: Call this periodically (e.g., during poll()) to detect
    /// and log warnings about events that are approaching timeout.
    pub fn check_starvation_warnings(&self) {
        if self.pending_events.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        let threshold = Duration::from_millis(self.config.starvation_warning_threshold_ms);

        for entry in self.pending_events.iter() {
            let age = now.duration_since(entry.value().dispatched_at);
            if age > threshold {
                warn!(
                    service_id = %self.config.service_id,
                    event_id = %entry.key(),
                    step_uuid = %entry.value().event.step_uuid,
                    age_ms = age.as_millis(),
                    threshold_ms = self.config.starvation_warning_threshold_ms,
                    "FFI dispatch: pending event aging - slow polling detected"
                );
            }
        }
    }

    /// Clean up timed-out pending events
    ///
    /// Call this periodically to detect handlers that never completed.
    /// Returns the number of events that timed out.
    pub fn cleanup_timeouts(&self) -> usize {
        let timeout = self.config.completion_timeout;
        let now = std::time::Instant::now();

        let timed_out: Vec<Uuid> = self
            .pending_events
            .iter()
            .filter(|entry| now.duration_since(entry.value().dispatched_at) > timeout)
            .map(|entry| *entry.key())
            .collect();

        for event_id in &timed_out {
            if let Some((_, pending_event)) = self.pending_events.remove(event_id) {
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

    /// Check if checkpoint support is configured
    ///
    /// TAS-125: Returns true if both checkpoint_service and dispatch_sender are set.
    pub fn has_checkpoint_support(&self) -> bool {
        self.checkpoint_service.is_some() && self.dispatch_sender.is_some()
    }

    /// Handle checkpoint yield from FFI handler (TAS-125)
    ///
    /// Unlike `complete()`, this persists the checkpoint and re-dispatches the step
    /// without releasing the step claim. The step stays in `in_process` state and
    /// will be re-executed with the checkpoint data available.
    ///
    /// # Arguments
    ///
    /// * `event_id` - The event ID from the `FfiStepEvent`
    /// * `checkpoint_data` - The checkpoint data to persist
    ///
    /// # Returns
    ///
    /// `true` if the checkpoint was persisted and step re-dispatched successfully.
    /// `false` if checkpoint support is not configured or an error occurred.
    pub fn checkpoint_yield(&self, event_id: Uuid, checkpoint_data: CheckpointYieldData) -> bool {
        // Check that checkpoint support is configured
        let (checkpoint_service, dispatch_sender) = match (
            self.checkpoint_service.as_ref(),
            self.dispatch_sender.as_ref(),
        ) {
            (Some(cs), Some(ds)) => (cs, ds),
            _ => {
                warn!(
                    service_id = %self.config.service_id,
                    event_id = %event_id,
                    "Checkpoint yield called but checkpoint support not configured"
                );
                return false;
            }
        };

        // Get pending event (clone for processing - we'll remove after successful checkpoint)
        let pending_event = self
            .pending_events
            .get(&event_id)
            .map(|entry| entry.value().clone());

        if let Some(pending) = pending_event {
            let elapsed = pending.dispatched_at.elapsed();

            debug!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                step_uuid = %pending.event.step_uuid,
                elapsed_ms = elapsed.as_millis(),
                cursor = ?checkpoint_data.cursor,
                items_processed = checkpoint_data.items_processed,
                "FFI dispatch: checkpoint yield received"
            );

            // Execute checkpoint persistence and re-dispatch in async context
            let result = self.config.runtime_handle.block_on(async {
                self.handle_checkpoint_yield_async(
                    &pending.event,
                    &checkpoint_data,
                    checkpoint_service,
                    dispatch_sender,
                )
                .await
            });

            match result {
                Ok(()) => {
                    // Remove the old pending event - a new continuation with a new event_id
                    // has been dispatched and will create its own pending entry
                    self.pending_events.remove(&event_id);

                    info!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        step_uuid = %pending.event.step_uuid,
                        items_processed = checkpoint_data.items_processed,
                        "Checkpoint persisted and step re-dispatched"
                    );
                    true
                }
                Err(e) => {
                    error!(
                        service_id = %self.config.service_id,
                        event_id = %event_id,
                        step_uuid = %pending.event.step_uuid,
                        error = %e,
                        "Checkpoint yield failed"
                    );

                    // Remove the pending event - we're sending a failure result
                    // which will trigger a retry with a new event
                    self.pending_events.remove(&event_id);

                    // Send a failure result so the step can be retried
                    self.send_checkpoint_failure(event_id, &pending.event, e);
                    false
                }
            }
        } else {
            warn!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                "Checkpoint yield for unknown event"
            );
            false
        }
    }

    /// Handle checkpoint yield asynchronously
    async fn handle_checkpoint_yield_async(
        &self,
        event: &FfiStepEvent,
        checkpoint_data: &CheckpointYieldData,
        checkpoint_service: &CheckpointService,
        dispatch_sender: &mpsc::Sender<DispatchHandlerMessage>,
    ) -> Result<(), CheckpointError> {
        // 1. Persist checkpoint atomically
        checkpoint_service
            .persist_checkpoint(event.step_uuid, checkpoint_data)
            .await?;

        // 2. Re-dispatch step (stays claimed, in_progress)
        // Create trace context from event
        let trace_context = match (&event.trace_id, &event.span_id) {
            (Some(trace_id), Some(span_id)) => Some(TraceContext {
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
            }),
            _ => None,
        };

        // 3. Clone task_sequence_step and inject checkpoint data
        // so the handler can read it on resume via workflow_step.checkpoint
        let mut task_sequence_step = event.execution_event.payload.task_sequence_step.clone();
        task_sequence_step.workflow_step.checkpoint = serde_json::to_value(checkpoint_data).ok();

        let continuation_msg = DispatchHandlerMessage::from_checkpoint_continuation(
            event.step_uuid,
            event.task_uuid,
            task_sequence_step,
            event.correlation_id,
            trace_context,
        );

        dispatch_sender
            .send(continuation_msg)
            .await
            .map_err(|_| CheckpointError::RedispatchFailed)?;

        Ok(())
    }

    /// Send a failure result when checkpoint yield fails
    fn send_checkpoint_failure(
        &self,
        event_id: Uuid,
        event: &FfiStepEvent,
        error: CheckpointError,
    ) {
        let result = StepExecutionResult::failure(
            event.step_uuid,
            format!("Checkpoint yield failed: {}", error),
            None,
            Some("checkpoint_error".to_string()),
            true, // Retryable
            0,
            None,
        );

        // Non-blocking send
        if self.completion_sender.try_send(result).is_err() {
            error!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                "Failed to send checkpoint failure result"
            );
        }
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
    async fn test_ffi_dispatch_channel_config_callback_timeout() {
        let handle = tokio::runtime::Handle::current();
        // Default callback timeout should be 5 seconds
        let config = FfiDispatchChannelConfig::new(handle.clone());
        assert_eq!(config.callback_timeout, Duration::from_secs(5));

        // Custom callback timeout via builder
        let config =
            FfiDispatchChannelConfig::new(handle).with_callback_timeout(Duration::from_secs(10));
        assert_eq!(config.callback_timeout, Duration::from_secs(10));
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

    #[tokio::test]
    async fn test_ffi_dispatch_channel_checkpoint_yield_not_configured() {
        // TAS-125: Test that checkpoint_yield returns false when not configured
        let (_dispatch_tx, dispatch_rx) = mpsc::channel(100);
        let (completion_tx, _completion_rx) = mpsc::channel(100);
        let handle = tokio::runtime::Handle::current();
        let config = FfiDispatchChannelConfig::new(handle);
        let callback = Arc::new(NoOpCallback);
        let channel = FfiDispatchChannel::new(dispatch_rx, completion_tx, config, callback);

        // Without checkpoint_service and dispatch_sender, checkpoint_yield should return false
        let checkpoint_data = CheckpointYieldData {
            step_uuid: Uuid::new_v4(),
            cursor: serde_json::json!(1000),
            items_processed: 1000,
            accumulated_results: None,
        };

        let result = channel.checkpoint_yield(Uuid::new_v4(), checkpoint_data);
        assert!(
            !result,
            "checkpoint_yield should return false when not configured"
        );
    }

    #[tokio::test]
    async fn test_ffi_dispatch_channel_checkpoint_data_serialization() {
        // TAS-125: Test that CheckpointYieldData can be created and serialized correctly
        let step_uuid = Uuid::new_v4();
        let checkpoint_data = CheckpointYieldData {
            step_uuid,
            cursor: serde_json::json!({"offset": 5000, "partition": "A"}),
            items_processed: 5000,
            accumulated_results: Some(serde_json::json!({
                "total_processed": 5000,
                "sum": 125000.50
            })),
        };

        // Verify all fields are correctly set
        assert_eq!(checkpoint_data.step_uuid, step_uuid);
        assert_eq!(checkpoint_data.items_processed, 5000);
        assert_eq!(checkpoint_data.cursor["offset"], 5000);
        assert_eq!(checkpoint_data.cursor["partition"], "A");

        // Verify serialization round-trip
        let json = serde_json::to_value(&checkpoint_data).unwrap();
        let deserialized: CheckpointYieldData = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.items_processed, 5000);
        assert_eq!(deserialized.cursor["offset"], 5000);
    }
}
