//! # TAS-40 Worker Event Subscriber
//!
//! Specialized event subscriber for workers to receive step execution completion
//! events from FFI handlers using the in-process event system.
//!
//! ## Key Features
//!
//! - **FFI Completion Handling**: Receives completion events from Ruby/Python/WASM handlers
//! - **Command Integration**: Converts events to StepExecutionResult for WorkerProcessor
//! - **Event Correlation**: Matches completion events to original execution requests
//! - **Error Handling**: Processes both successful and failed step executions
//! - **Async Processing**: Non-blocking event processing with tokio channels
//!
//! ## Architecture Integration
//!
//! ```text
//! FFI Handler completes step execution
//!     ↓ (publishes StepExecutionCompletionEvent)
//! WorkerEventSubscriber.handle_completion_event()
//!     ↓ (converts to StepExecutionResult)
//! WorkerProcessor receives completion via command channel
//! ```
//!
//! ## Usage in WorkerProcessor
//!
//! ```rust
//! use tasker_worker::event_subscriber::WorkerEventSubscriber;
//!
//! // Inside WorkerProcessor initialization
//! let event_subscriber = WorkerEventSubscriber::new(worker_id.clone());
//! let completion_receiver = event_subscriber.start_completion_listener();
//!
//! // In command processing loop alongside other commands
//! tokio::select! {
//!     command = command_receiver.recv() => {
//!         // Handle regular worker commands
//!     },
//!     completion = completion_receiver.recv() => {
//!         // Handle step completion from FFI handlers
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::events::{WorkerEventSubscriber as SharedEventSubscriber, WorkerEventSystem};
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::StepExecutionCompletionEvent;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Worker-specific event subscriber for handling FFI completion events
#[derive(Debug, Clone)]
pub struct WorkerEventSubscriber {
    /// Worker identifier for traceability
    worker_id: String,
    /// Namespace this worker is processing
    namespace: String,
    /// Shared event subscriber for cross-language communication
    shared_subscriber: SharedEventSubscriber,
    /// Event system for direct access if needed
    event_system: Arc<WorkerEventSystem>,
    /// Statistics tracking
    stats: Arc<std::sync::Mutex<WorkerEventSubscriberStats>>,
}

/// Statistics for worker event subscriber monitoring
#[derive(Debug, Clone, Default)]
pub struct WorkerEventSubscriberStats {
    pub worker_id: String,
    pub namespace: String,
    pub completions_received: u64,
    pub successful_completions: u64,
    pub failed_completions: u64,
    pub conversion_errors: u64,
    pub unmatched_correlations: u64,
}

/// Errors specific to worker event subscription
#[derive(Debug, thiserror::Error)]
pub enum WorkerEventSubscriberError {
    #[error("Failed to subscribe to events: {0}")]
    SubscriptionError(String),

    #[error("Event conversion error: {0}")]
    ConversionError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Event correlation error: {0}")]
    CorrelationError(String),
}

impl WorkerEventSubscriber {
    /// Create a new worker event subscriber
    pub fn new(worker_id: String, namespace: String) -> Self {
        let event_system = Arc::new(WorkerEventSystem::new());
        let shared_subscriber = event_system.create_subscriber();

        info!(
            worker_id = %worker_id,
            namespace = %namespace,
            "Creating WorkerEventSubscriber for FFI completion events"
        );

        let stats = Arc::new(std::sync::Mutex::new(WorkerEventSubscriberStats {
            worker_id: worker_id.clone(),
            namespace: namespace.clone(),
            ..Default::default()
        }));

        Self {
            worker_id,
            namespace,
            shared_subscriber,
            event_system,
            stats,
        }
    }

    /// Create a worker event subscriber with custom event system
    pub fn with_event_system(
        worker_id: String,
        namespace: String,
        event_system: Arc<WorkerEventSystem>,
    ) -> Self {
        let shared_subscriber = event_system.create_subscriber();

        let stats = Arc::new(std::sync::Mutex::new(WorkerEventSubscriberStats {
            worker_id: worker_id.clone(),
            namespace: namespace.clone(),
            ..Default::default()
        }));

        Self {
            worker_id,
            namespace,
            shared_subscriber,
            event_system,
            stats,
        }
    }

    /// Start listening for step completion events from FFI handlers
    ///
    /// Returns a receiver that the WorkerProcessor can use in its command loop
    /// to receive StepExecutionResult messages converted from FFI completion events.
    pub fn start_completion_listener(&self) -> mpsc::Receiver<StepExecutionResult> {
        let (completion_sender, completion_receiver) = mpsc::channel(1000);
        let mut event_receiver = self.shared_subscriber.subscribe_to_step_completions();

        let worker_id = self.worker_id.clone();
        let namespace = self.namespace.clone();
        let stats = Arc::clone(&self.stats);

        // Spawn background task to listen for completion events
        tokio::spawn(async move {
            info!(
                worker_id = %worker_id,
                namespace = %namespace,
                "Started completion event listener for FFI handlers"
            );

            while let Ok(completion_event) = event_receiver.recv().await {
                debug!(
                    worker_id = %worker_id,
                    event_id = %completion_event.event_id,
                    task_uuid = %completion_event.task_uuid,
                    step_uuid = %completion_event.step_uuid,
                    success = completion_event.success,
                    "Received step completion event from FFI handler"
                );

                // Update statistics
                {
                    let mut stats = stats.lock().unwrap();
                    stats.completions_received += 1;
                    if completion_event.success {
                        stats.successful_completions += 1;
                    } else {
                        stats.failed_completions += 1;
                    }
                }

                // Convert completion event to StepExecutionResult
                match Self::convert_completion_to_result(completion_event) {
                    Ok(step_result) => {
                        // Send converted result to worker processor
                        if let Err(e) = completion_sender.send(step_result).await {
                            warn!(
                                worker_id = %worker_id,
                                error = %e,
                                "Failed to send step completion to worker processor - channel closed"
                            );
                            break; // Channel closed, exit listener
                        }
                    }
                    Err(e) => {
                        error!(
                            worker_id = %worker_id,
                            error = %e,
                            "Failed to convert completion event to step result"
                        );

                        // Update error statistics
                        let mut stats = stats.lock().unwrap();
                        stats.conversion_errors += 1;
                    }
                }
            }

            info!(
                worker_id = %worker_id,
                "Completion event listener terminated"
            );
        });

        completion_receiver
    }

    /// Subscribe to raw completion events (for advanced use cases)
    pub fn subscribe_to_raw_completions(
        &self,
    ) -> broadcast::Receiver<StepExecutionCompletionEvent> {
        self.shared_subscriber.subscribe_to_step_completions()
    }

    /// Get access to the shared event system (for advanced integration)
    pub fn get_event_system(&self) -> Arc<WorkerEventSystem> {
        Arc::clone(&self.event_system)
    }

    /// Get event subscriber statistics
    pub fn get_statistics(&self) -> WorkerEventSubscriberStats {
        let stats = self.stats.lock().unwrap();
        
        // Access shared event system data (available for future enhancement)
        let _system_stats = self.event_system.get_statistics();
        
        // Return current statistics (can be enhanced with system_stats data later)
        WorkerEventSubscriberStats {
            worker_id: stats.worker_id.clone(),
            namespace: stats.namespace.clone(),
            completions_received: stats.completions_received,
            successful_completions: stats.successful_completions,
            failed_completions: stats.failed_completions,
            conversion_errors: stats.conversion_errors,
            unmatched_correlations: stats.unmatched_correlations,
        }
    }

    /// Convert StepExecutionCompletionEvent to StepExecutionResult
    ///
    /// This converts the FFI completion event format to the internal Rust format
    /// that WorkerProcessor expects for integration with orchestration.
    fn convert_completion_to_result(
        completion_event: StepExecutionCompletionEvent,
    ) -> Result<StepExecutionResult, WorkerEventSubscriberError> {
        let execution_time = completion_event
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("execution_time_ms"))
            .and_then(|time| time.as_i64())
            .unwrap_or(0);

        let metadata = completion_event.metadata.clone().unwrap_or_else(|| {
            serde_json::json!({
                "source": "ffi_handler",
                "event_driven": true,
                "architecture": "TAS-40_command_pattern"
            })
        });

        if completion_event.success {
            Ok(StepExecutionResult::success(
                completion_event.step_uuid,
                completion_event.result,
                execution_time,
                Some(
                    metadata
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ))
        } else {
            let error_message = completion_event
                .error_message
                .unwrap_or_else(|| "Step execution failed in FFI handler".to_string());

            Ok(StepExecutionResult::failure(
                completion_event.step_uuid,
                error_message,
                None, // error_code
                None, // error_type
                true, // retryable (default for now)
                execution_time,
                Some(
                    metadata
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ))
        }
    }
}

/// Enhanced completion listener with correlation tracking
#[derive(Debug)]
pub struct CorrelatedCompletionListener {
    subscriber: WorkerEventSubscriber,
    correlation_tracker: Arc<std::sync::Mutex<HashMap<Uuid, PendingExecution>>>,
}

/// Information about a pending step execution waiting for completion
#[derive(Debug, Clone)]
pub struct PendingExecution {
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub step_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl CorrelatedCompletionListener {
    /// Create a new correlated completion listener
    pub fn new(worker_id: String, namespace: String) -> Self {
        let subscriber = WorkerEventSubscriber::new(worker_id, namespace);
        let correlation_tracker = Arc::new(std::sync::Mutex::new(HashMap::new()));

        Self {
            subscriber,
            correlation_tracker,
        }
    }

    /// Track a pending execution (called when step execution event is published)
    pub fn track_pending_execution(
        &self,
        correlation_id: Uuid,
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: String,
    ) {
        let pending = PendingExecution {
            task_uuid,
            step_uuid,
            step_name: step_name.clone(),
            started_at: chrono::Utc::now(),
        };

        let mut tracker = self.correlation_tracker.lock().unwrap();
        tracker.insert(correlation_id, pending);

        debug!(
            correlation_id = %correlation_id,
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            step_name = %step_name,
            "Tracking pending step execution for correlation"
        );
    }

    /// Start correlated completion listener with timeout handling
    pub fn start_correlated_listener(
        &self,
        timeout_seconds: u64,
    ) -> mpsc::Receiver<CorrelatedStepResult> {
        let (result_sender, result_receiver) = mpsc::channel(1000);
        let mut completion_receiver = self.subscriber.subscribe_to_raw_completions();
        let correlation_tracker = Arc::clone(&self.correlation_tracker);

        tokio::spawn(async move {
            while let Ok(completion_event) = completion_receiver.recv().await {
                let event_id = completion_event.event_id;

                // Check if we have a pending execution for this correlation ID
                let pending = {
                    let mut tracker = correlation_tracker.lock().unwrap();
                    tracker.remove(&event_id)
                };

                let correlated_result = match pending {
                    Some(pending_execution) => {
                        let execution_duration = chrono::Utc::now()
                            .signed_duration_since(pending_execution.started_at)
                            .num_milliseconds();

                        debug!(
                            correlation_id = %event_id,
                            step_name = %pending_execution.step_name,
                            execution_duration_ms = execution_duration,
                            success = completion_event.success,
                            "Matched completion event to pending execution"
                        );

                        CorrelatedStepResult {
                            correlation_id: event_id,
                            pending_execution: Some(pending_execution),
                            completion_event: completion_event.clone(),
                            execution_duration_ms: execution_duration as i64,
                            correlated: true,
                        }
                    }
                    None => {
                        warn!(
                            correlation_id = %event_id,
                            "Received completion event without matching pending execution"
                        );

                        CorrelatedStepResult {
                            correlation_id: event_id,
                            pending_execution: None,
                            completion_event: completion_event.clone(),
                            execution_duration_ms: 0,
                            correlated: false,
                        }
                    }
                };

                if let Err(e) = result_sender.send(correlated_result).await {
                    warn!(error = %e, "Failed to send correlated result - channel closed");
                    break;
                }
            }
        });

        // Start timeout cleanup task
        let correlation_tracker_cleanup = Arc::clone(&self.correlation_tracker);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                interval.tick().await;

                let cutoff_time =
                    chrono::Utc::now() - chrono::Duration::seconds(timeout_seconds as i64);

                let mut tracker = correlation_tracker_cleanup.lock().unwrap();
                let initial_count = tracker.len();

                tracker.retain(|_, pending| pending.started_at > cutoff_time);

                let removed_count = initial_count - tracker.len();
                if removed_count > 0 {
                    warn!(
                        removed_count = removed_count,
                        remaining_count = tracker.len(),
                        "Cleaned up timed-out pending executions"
                    );
                }
            }
        });

        result_receiver
    }
}

/// Result with correlation information
#[derive(Debug, Clone)]
pub struct CorrelatedStepResult {
    pub correlation_id: Uuid,
    pub pending_execution: Option<PendingExecution>,
    pub completion_event: StepExecutionCompletionEvent,
    pub execution_duration_ms: i64,
    pub correlated: bool,
}

impl CorrelatedStepResult {
    /// Convert to StepExecutionResult
    pub fn to_step_execution_result(
        &self,
    ) -> Result<StepExecutionResult, WorkerEventSubscriberError> {
        WorkerEventSubscriber::convert_completion_to_result(self.completion_event.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::types::StepExecutionCompletionEvent;

    #[tokio::test]
    async fn test_worker_event_subscriber_creation() {
        let subscriber =
            WorkerEventSubscriber::new("test_worker_123".to_string(), "test_namespace".to_string());

        assert_eq!(subscriber.worker_id, "test_worker_123");
        assert_eq!(subscriber.namespace, "test_namespace");
    }

    #[tokio::test]
    async fn test_completion_event_conversion() {
        let completion_event = StepExecutionCompletionEvent::success(
            Uuid::new_v4(),
            Uuid::new_v4(),
            serde_json::json!({"result": "success"}),
            Some(serde_json::json!({"execution_time_ms": 150})),
        );

        let result = WorkerEventSubscriber::convert_completion_to_result(completion_event.clone());
        assert!(result.is_ok());

        let step_result = result.unwrap();
        assert_eq!(step_result.step_uuid, completion_event.step_uuid);
        assert_eq!(step_result.success, true);
        assert_eq!(step_result.metadata.execution_time_ms, 150);
    }

    #[tokio::test]
    async fn test_failed_completion_event_conversion() {
        let completion_event = StepExecutionCompletionEvent::failure(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "Step execution failed".to_string(),
            Some(serde_json::json!({"execution_time_ms": 75})),
        );

        let result = WorkerEventSubscriber::convert_completion_to_result(completion_event.clone());
        assert!(result.is_ok());

        let step_result = result.unwrap();
        assert_eq!(step_result.step_uuid, completion_event.step_uuid);
        assert_eq!(step_result.success, false);
        assert_eq!(step_result.metadata.execution_time_ms, 75);
        assert!(step_result.error.is_some());
        assert_eq!(step_result.error.unwrap().message, "Step execution failed");
    }

    #[tokio::test]
    async fn test_correlated_completion_listener() {
        let listener = CorrelatedCompletionListener::new(
            "test_worker_123".to_string(),
            "test_namespace".to_string(),
        );

        let correlation_id = Uuid::new_v4();
        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();

        // Track pending execution
        listener.track_pending_execution(
            correlation_id,
            task_uuid,
            step_uuid,
            "test_step".to_string(),
        );

        // Verify tracking
        let tracker = listener.correlation_tracker.lock().unwrap();
        assert!(tracker.contains_key(&correlation_id));
        assert_eq!(tracker.get(&correlation_id).unwrap().task_uuid, task_uuid);
    }

    #[tokio::test]
    async fn test_subscriber_statistics() {
        let subscriber =
            WorkerEventSubscriber::new("test_worker_123".to_string(), "test_namespace".to_string());

        let stats = subscriber.get_statistics();
        assert_eq!(stats.worker_id, "test_worker_123");
        assert_eq!(stats.namespace, "test_namespace");
        assert_eq!(stats.completions_received, 0);
        assert_eq!(stats.successful_completions, 0);
        assert_eq!(stats.failed_completions, 0);
    }
}
