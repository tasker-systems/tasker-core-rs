use crate::execution::message_protocols::{
    ResultMessage, StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
};
use crate::models::{
    core::{
        step_execution_batch::{NewStepExecutionBatch, StepExecutionBatch},
        step_execution_batch_received_result::{
            NewStepExecutionBatchReceivedResult, StepExecutionBatchReceivedResult,
        },
        step_execution_batch_step::{NewStepExecutionBatchStep, StepExecutionBatchStep},
        step_execution_batch_transition::{
            NewStepExecutionBatchTransition, StepExecutionBatchTransition,
        },
    },
    Task, TaskExecutionContext,
};
use crate::orchestration::{
    errors::{ExecutionError, OrchestrationError},
    state_manager::StateManager,
    step_handler::StepExecutionContext,
    types::{FrameworkIntegration, StepResult, StepStatus, TaskContext},
};
use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use zmq::{Context, Socket, PUB, SUB};

/// Default worker ID for batch orchestrator operations
const BATCH_ORCHESTRATOR_WORKER_ID: &str = "rust_batch_orchestrator";

/// Default handler class when none can be determined
const UNKNOWN_HANDLER_CLASS: &str = "UnknownStepHandler";

/// Result of attempting to receive a message from ZeroMQ
enum MessageReceiveResult {
    Success(Vec<u8>),
    NoMessage,
    Error,
}

/// Result of parsing a received message
enum MessageParseResult {
    PartialResult {
        batch_id: String,
        message: ResultMessage,
    },
    BatchCompletion {
        batch_id: String,
        message: ResultMessage,
    },
    WrongTopic(String),
    Error(String),
}

/// Tracks partial results for a batch during execution
#[derive(Debug, Clone)]
struct BatchTracker {
    batch_id: String,
    task_id: i64, // ADDED: Task ID for TaskFinalizer integration
    /// Total steps count - will be used for progress tracking and completion detection
    #[allow(dead_code)]
    total_steps: u32,
    partial_results: HashMap<i64, ResultMessage>, // step_id -> partial result
    expected_sequences: HashMap<i64, u32>,        // step_id -> expected sequence number
    batch_complete: bool,
    /// Batch creation timestamp - will be used for timeout detection and metrics
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

impl BatchTracker {
    /// Create new batch tracker - will be used for batch lifecycle management
    #[allow(dead_code)]
    fn new(batch_id: String, task_id: i64, total_steps: u32) -> Self {
        Self {
            batch_id,
            task_id,
            total_steps,
            partial_results: HashMap::new(),
            expected_sequences: HashMap::new(),
            batch_complete: false,
            created_at: std::time::Instant::now(),
        }
    }

    /// Add a partial result and check for sequence consistency
    fn add_partial_result(&mut self, step_id: i64, result: ResultMessage) -> bool {
        if let ResultMessage::PartialResult { sequence, .. } = &result {
            let expected = self.expected_sequences.get(&step_id).unwrap_or(&1);
            let sequence_num = *sequence;
            if sequence_num == *expected {
                self.partial_results.insert(step_id, result);
                self.expected_sequences.insert(step_id, sequence_num + 1);
                true
            } else {
                tracing::warn!(
                    "Out-of-order partial result for step {} in batch {}: expected sequence {}, got {}",
                    step_id, self.batch_id, expected, sequence_num
                );
                false
            }
        } else {
            false
        }
    }

    /// Check if batch is complete based on partial results - will be used for result reconciliation
    #[allow(dead_code)]
    fn is_complete(&self) -> bool {
        self.partial_results.len() as u32 >= self.total_steps || self.batch_complete
    }
}

/// ZeroMQ pub-sub executor for language-agnostic step execution
///
/// This executor implements fire-and-forget step execution using ZeroMQ pub-sub pattern:
/// - Publishes step batches to "steps" topic
/// - Subscribes to "results" topic for async result collection
/// - Correlates results using unique batch IDs
/// - Eliminates timeout/idempotency issues of REQ-REP pattern
pub struct ZmqPubSubExecutor {
    /// ZMQ context - will be used for socket lifecycle management and cleanup
    #[allow(dead_code)]
    context: Context,
    /// Publisher endpoint - needed for reconnection and configuration display
    #[allow(dead_code)]
    pub_endpoint: String,
    /// Subscriber endpoint - needed for reconnection and configuration display
    #[allow(dead_code)]
    sub_endpoint: String,
    pub_socket: Socket,
    /// Result handlers for async response correlation - will be used for REQ-REP pattern fallback
    #[allow(dead_code)]
    result_handlers: Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
    /// Batch trackers for progress monitoring - will be used for dual result pattern reconciliation
    #[allow(dead_code)]
    batch_trackers: Arc<Mutex<HashMap<String, BatchTracker>>>,
    pool: PgPool,
    /// State manager for step transitions - will be used for result processing integration
    #[allow(dead_code)]
    state_manager: StateManager,
    /// Task handler registry for proper handler resolution (shared reference)
    task_handler_registry: std::sync::Arc<crate::registry::TaskHandlerRegistry>,
    _result_listener_handle: tokio::task::JoinHandle<()>,
}

impl ZmqPubSubExecutor {
    /// Create new ZeroMQ pub-sub executor
    ///
    /// # Arguments
    /// * `pub_endpoint` - ZeroMQ endpoint for publishing steps (e.g., "inproc://steps")
    /// * `sub_endpoint` - ZeroMQ endpoint for subscribing to results (e.g., "inproc://results")
    /// * `pool` - Database connection pool for loading task data
    /// * `state_manager` - StateManager for immediate step state updates
    /// * `task_finalizer` - TaskFinalizer for workflow completion logic
    /// * `task_handler_registry` - TaskHandlerRegistry for proper handler resolution
    pub async fn new(
        pub_endpoint: &str,
        sub_endpoint: &str,
        pool: PgPool,
        state_manager: StateManager,
        task_finalizer: crate::orchestration::task_finalizer::TaskFinalizer,
        task_handler_registry: std::sync::Arc<crate::registry::TaskHandlerRegistry>,
    ) -> Result<Self, OrchestrationError> {
        let context = Context::new();

        // Publisher socket for sending step batches
        let pub_socket = context.socket(PUB).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to create PUB socket: {e}"),
                error_code: Some("ZMQ_PUB_CREATE_FAILED".to_string()),
            })
        })?;
        pub_socket.bind(pub_endpoint).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to bind PUB socket to {pub_endpoint}: {e}"),
                error_code: Some("ZMQ_PUB_BIND_FAILED".to_string()),
            })
        })?;

        // Subscriber socket for receiving results
        let sub_socket = context.socket(SUB).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to create SUB socket: {e}"),
                error_code: Some("ZMQ_SUB_CREATE_FAILED".to_string()),
            })
        })?;
        sub_socket.connect(sub_endpoint).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to connect SUB socket to {sub_endpoint}: {e}"),
                error_code: Some("ZMQ_SUB_CONNECT_FAILED".to_string()),
            })
        })?;
        sub_socket.set_subscribe(b"results").map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to subscribe to 'results' topic: {e}"),
                error_code: Some("ZMQ_SUB_SUBSCRIBE_FAILED".to_string()),
            })
        })?;

        let result_handlers = Arc::new(Mutex::new(HashMap::new()));
        let batch_trackers = Arc::new(Mutex::new(HashMap::new()));

        // Start background task to listen for results (takes ownership of sub_socket)
        let result_listener_handle = Self::start_result_listener(
            result_handlers.clone(),
            batch_trackers.clone(),
            sub_socket,
            state_manager.clone(),
            pool.clone(),
            task_finalizer,
        );

        Ok(Self {
            context,
            pub_endpoint: pub_endpoint.to_string(),
            sub_endpoint: sub_endpoint.to_string(),
            pub_socket,
            result_handlers,
            batch_trackers,
            pool,
            state_manager,
            task_handler_registry,
            _result_listener_handle: result_listener_handle,
        })
    }

    /// Start background task to listen for result messages
    fn start_result_listener(
        result_handlers: Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
        batch_trackers: Arc<Mutex<HashMap<String, BatchTracker>>>,
        sub_socket: Socket,
        state_manager: StateManager,
        pool: PgPool,
        task_finalizer: crate::orchestration::task_finalizer::TaskFinalizer,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Starting ZeroMQ result listener with dual message support");

            loop {
                match Self::receive_message_from_socket(&sub_socket) {
                    MessageReceiveResult::Success(msg_bytes) => {
                        Self::handle_received_message(
                            msg_bytes,
                            &result_handlers,
                            &batch_trackers,
                            &state_manager,
                            &pool,
                            &task_finalizer,
                        )
                        .await;
                    }
                    MessageReceiveResult::NoMessage => {
                        // Small sleep to prevent busy-waiting
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    MessageReceiveResult::Error => {
                        // Brief pause before retrying on error
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        })
    }

    /// Receive a message from ZeroMQ socket with non-blocking semantics
    fn receive_message_from_socket(sub_socket: &Socket) -> MessageReceiveResult {
        match sub_socket.recv_bytes(zmq::DONTWAIT) {
            Ok(msg_bytes) => MessageReceiveResult::Success(msg_bytes),
            Err(zmq::Error::EAGAIN) => {
                // No message available, this is expected with DONTWAIT
                MessageReceiveResult::NoMessage
            }
            Err(e) => {
                tracing::error!("Error receiving from ZeroMQ socket: {}", e);
                MessageReceiveResult::Error
            }
        }
    }

    /// Handle a received message by parsing and routing it
    async fn handle_received_message(
        msg_bytes: Vec<u8>,
        result_handlers: &Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
        batch_trackers: &Arc<Mutex<HashMap<String, BatchTracker>>>,
        state_manager: &StateManager,
        pool: &PgPool,
        task_finalizer: &crate::orchestration::task_finalizer::TaskFinalizer,
    ) {
        match Self::parse_message(msg_bytes) {
            MessageParseResult::PartialResult { batch_id, message } => {
                Self::handle_partial_result(
                    batch_id,
                    message,
                    batch_trackers,
                    state_manager,
                    pool,
                    task_finalizer,
                )
                .await;
            }
            MessageParseResult::BatchCompletion { batch_id, message } => {
                Self::handle_batch_completion(
                    batch_id,
                    message,
                    batch_trackers,
                    result_handlers,
                    pool,
                    task_finalizer,
                )
                .await;
            }
            MessageParseResult::WrongTopic(topic) => {
                tracing::debug!("Ignoring message with topic: {}", topic);
            }
            MessageParseResult::Error(error_msg) => {
                tracing::warn!("Message parsing error: {}", error_msg);
            }
        }
    }

    /// Parse raw message bytes into structured result
    fn parse_message(msg_bytes: Vec<u8>) -> MessageParseResult {
        // Convert bytes to string
        let msg_str = match String::from_utf8(msg_bytes) {
            Ok(s) => s,
            Err(_) => return MessageParseResult::Error("Received non-UTF8 message".to_string()),
        };

        // Split topic from message content
        let (topic, json_data) = match msg_str.split_once(' ') {
            Some((t, j)) => (t, j),
            None => {
                return MessageParseResult::Error(
                    "Malformed message (no topic separator)".to_string(),
                )
            }
        };

        // Check if this is a results message
        if topic != "results" {
            return MessageParseResult::WrongTopic(topic.to_string());
        }

        // Try to parse as new dual message format first
        match serde_json::from_str::<ResultMessage>(json_data) {
            Ok(message) => match &message {
                ResultMessage::PartialResult { batch_id, .. } => {
                    tracing::debug!("Received partial result for batch: {}", batch_id);
                    MessageParseResult::PartialResult {
                        batch_id: batch_id.clone(),
                        message,
                    }
                }
                ResultMessage::BatchCompletion { batch_id, .. } => {
                    tracing::debug!("Received batch completion for batch: {}", batch_id);
                    MessageParseResult::BatchCompletion {
                        batch_id: batch_id.clone(),
                        message,
                    }
                }
            },
            Err(e) => {
                tracing::error!("Failed to parse result message: {}", e);
                MessageParseResult::Error(format!("JSON parsing failed: {e}"))
            }
        }
    }

    /// Handle partial result message (immediate step state update)
    async fn handle_partial_result(
        batch_id: String,
        message: ResultMessage,
        batch_trackers: &Arc<Mutex<HashMap<String, BatchTracker>>>,
        state_manager: &StateManager,
        pool: &PgPool,
        task_finalizer: &crate::orchestration::task_finalizer::TaskFinalizer,
    ) {
        if let ResultMessage::PartialResult {
            step_id,
            status,
            output,
            error,
            execution_time_ms,
            worker_id,
            ..
        } = &message
        {
            tracing::info!(
                "Processing partial result for step {} in batch {}: status={} worker={} exec_time={}ms",
                step_id, batch_id, status, worker_id, execution_time_ms
            );

            // Phase 2.3 - Update step state immediately using StateManager
            let state_update_result = match status.as_str() {
                "completed" => {
                    state_manager
                        .complete_step_with_results(*step_id, output.clone())
                        .await
                }
                "failed" => {
                    let error_message = error
                        .as_ref()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "Step execution failed".to_string());
                    state_manager
                        .fail_step_with_error(*step_id, error_message)
                        .await
                }
                "in_progress" => state_manager.mark_step_in_progress(*step_id).await,
                _ => {
                    tracing::warn!(
                        "Unknown status '{}' for step {}, skipping state update",
                        status,
                        step_id
                    );
                    Ok(())
                }
            };

            // Log state update result and check for task finalization
            match state_update_result {
                Ok(()) => {
                    tracing::debug!(
                        "Successfully updated step {} state to '{}' via StateManager",
                        step_id,
                        status
                    );

                    // CRITICAL FIX: Check if this step completion triggers task finalization
                    // We'll check if there are any more viable steps for this task
                    // If not, we can trigger task finalization even on partial result
                    tracing::debug!(
                        "Step {} completed via partial result - checking for task finalization",
                        step_id
                    );

                    // Get task_id from step_id and check for finalization
                    match crate::models::WorkflowStep::find_by_id(pool, *step_id).await {
                        Ok(Some(workflow_step)) => {
                            let task_id = workflow_step.task_id;
                            tracing::debug!(
                                "Found task {} for step {} - checking for task finalization",
                                task_id,
                                step_id
                            );

                            // Check if task has any remaining viable steps
                            match task_finalizer.handle_no_viable_steps(task_id).await {
                                Ok(finalization_result) => {
                                    tracing::info!(
                                        "Task {} finalization check via partial result for step {}: action={:?}, reason={:?}",
                                        task_id, step_id, finalization_result.action, finalization_result.reason
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to check task {} finalization via partial result for step {}: {}",
                                        task_id, step_id, e
                                    );
                                }
                            }
                        }
                        Ok(None) => {
                            tracing::warn!(
                                "Could not find WorkflowStep record for step {} - cannot trigger task finalization",
                                step_id
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to lookup WorkflowStep for step {} during finalization check: {}",
                                step_id, e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to update step {} state to '{}': {}",
                        step_id,
                        status,
                        e
                    );
                }
            }

            // Save partial result to audit ledger
            if let Err(e) =
                Self::save_partial_result_to_audit_ledger(pool, &batch_id, *step_id, &message).await
            {
                tracing::error!(
                    "Failed to save partial result to audit ledger for step {} in batch {}: {}",
                    step_id,
                    batch_id,
                    e
                );
            }

            // Track partial result in batch tracker
            let mut trackers = batch_trackers.lock().await;
            if let Some(tracker) = trackers.get_mut(&batch_id) {
                tracker.add_partial_result(*step_id, message.clone());
            } else {
                tracing::warn!("Received partial result for unknown batch: {}", batch_id);
            }
        }
    }

    /// Handle batch completion message (reconciliation check and task finalization)
    async fn handle_batch_completion(
        batch_id: String,
        message: ResultMessage,
        batch_trackers: &Arc<Mutex<HashMap<String, BatchTracker>>>,
        _result_handlers: &Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
        pool: &PgPool,
        task_finalizer: &crate::orchestration::task_finalizer::TaskFinalizer,
    ) {
        if let ResultMessage::BatchCompletion { step_summaries, .. } = &message {
            tracing::info!(
                "Processing batch completion for batch {} with {} step summaries",
                batch_id,
                step_summaries.len()
            );

            // Save batch completion to audit ledger
            if let Err(e) =
                Self::save_batch_completion_to_audit_ledger(pool, &batch_id, &message).await
            {
                tracing::error!(
                    "Failed to save batch completion to audit ledger for batch {}: {}",
                    batch_id,
                    e
                );
            }

            let mut trackers = batch_trackers.lock().await;
            if let Some(mut tracker) = trackers.remove(&batch_id) {
                // Phase 2.2 - Reconciliation logic: Compare step_summaries with tracked partial results
                Self::perform_batch_reconciliation(&batch_id, step_summaries, &tracker).await;

                tracker.batch_complete = true;

                // Batch completion tracked and reconciled - ready for final processing
                tracing::info!("Batch {} marked as complete", batch_id);

                // TODO: Check for missing steps in database and update them
                tracing::debug!(
                    "TODO: Should check for steps in batch {} that haven't been recorded in database yet",
                    batch_id
                );

                // CRITICAL FIX: Get task_id from batch and check for task finalization
                if let Ok(Some(step_execution_batch)) =
                    StepExecutionBatch::find_by_uuid(pool, &batch_id).await
                {
                    // Get task_id from the batch record
                    let task_id = step_execution_batch.task_id;

                    tracing::info!(
                        "Checking if batch {} completion for task {} triggers task finalization",
                        batch_id,
                        task_id
                    );

                    // CRITICAL: Check if task has any remaining viable steps
                    match task_finalizer.handle_no_viable_steps(task_id).await {
                        Ok(finalization_result) => {
                            tracing::info!(
                                "Task {} finalization result: action={:?}, reason={:?}",
                                task_id,
                                finalization_result.action,
                                finalization_result.reason
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to finalize task {} after batch {} completion: {}",
                                task_id,
                                batch_id,
                                e
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "Could not find StepExecutionBatch record for batch {} - cannot trigger task finalization",
                        batch_id
                    );
                }

                // TODO: Publish event for batch reconciliation discrepancies
                tracing::debug!(
                    "TODO: Should publish events for any reconciliation discrepancies found in batch {}",
                    batch_id
                );
            } else {
                tracing::warn!("Received batch completion for unknown batch: {}", batch_id);
            }
        }
    }

    /// Publish step batch to ZeroMQ with fire-and-forget semantics
    pub async fn publish_batch(
        &self,
        step_ids: Vec<i64>,
        task_id: i64,
    ) -> Result<String, OrchestrationError> {
        let batch_uuid = StepExecutionBatch::generate_batch_uuid();

        // Get step_id for error context - use first step ID or 0
        let error_step_id = step_ids.first().copied().unwrap_or(0);

        // CRITICAL FIX: Build proper StepExecutionRequest objects instead of using placeholder data
        let mut step_execution_requests = Vec::new();
        let mut workflow_steps = Vec::new();

        // First, load all WorkflowStep objects
        for step_id in &step_ids {
            let workflow_step = crate::models::WorkflowStep::find_by_id(&self.pool, *step_id)
                .await
                .map_err(|e| {
                    OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                        step_id: *step_id,
                        reason: format!("Failed to load workflow step: {e}"),
                        error_code: Some("WORKFLOW_STEP_LOAD_FAILED".to_string()),
                    })
                })?
                .ok_or_else(|| {
                    OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                        step_id: *step_id,
                        reason: "Workflow step not found".to_string(),
                        error_code: Some("WORKFLOW_STEP_NOT_FOUND".to_string()),
                    })
                })?;
            workflow_steps.push(workflow_step);
        }

        // Load task context for step execution requests
        let task = crate::models::Task::find_by_id(&self.pool, task_id)
            .await
            .map_err(|e| {
                OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                    step_id: task_id,
                    reason: format!("Failed to load task for context: {e}"),
                    error_code: Some("TASK_LOAD_FAILED".to_string()),
                })
            })?
            .ok_or_else(|| {
                OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                    step_id: task_id,
                    reason: "Task not found".to_string(),
                    error_code: Some("TASK_NOT_FOUND".to_string()),
                })
            })?;

        // Convert task context to JSON for step execution
        let task_context = task.context.unwrap_or_else(|| serde_json::json!({}));

        // Resolve handler classes using TaskHandlerRegistry
        let step_handler_classes = self.resolve_step_names(&workflow_steps).await?;

        // Build step execution requests using the existing methods
        for workflow_step in &workflow_steps {
            let handler_class = step_handler_classes
                .get(&workflow_step.workflow_step_id)
                .ok_or_else(|| {
                    OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                        step_id: workflow_step.workflow_step_id,
                        reason: "Handler class not found for step".to_string(),
                        error_code: Some("HANDLER_CLASS_NOT_FOUND".to_string()),
                    })
                })?;

            // Create StepExecutionContext from WorkflowStep and task data
            let step_execution_context = crate::orchestration::step_handler::StepExecutionContext {
                step_id: workflow_step.workflow_step_id,
                task_id: workflow_step.task_id,
                step_name: {
                    // Load real step name from named_steps table
                    let step_name_result = sqlx::query!(
                        "SELECT name FROM tasker_named_steps WHERE named_step_id = $1",
                        workflow_step.named_step_id
                    )
                    .fetch_optional(&self.pool)
                    .await;

                    match step_name_result {
                        Ok(Some(row)) => row.name,
                        _ => format!("step_{}", workflow_step.workflow_step_id), // Fallback
                    }
                },
                input_data: task_context.clone(),
                previous_steps: {
                    // Load dependencies for this step
                    match workflow_step.get_dependencies(&self.pool).await {
                        Ok(dependencies) => dependencies,
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load dependencies for step {}: {}",
                                workflow_step.workflow_step_id,
                                e
                            );
                            Vec::new()
                        }
                    }
                },
                step_config: {
                    // For now, load default step configuration
                    // TODO: Integrate with TaskConfigFinder to load step configuration from YAML templates
                    // based on the handler_class and step template configuration
                    tracing::debug!(
                        "Loading default configuration for handler class: {}",
                        handler_class
                    );
                    std::collections::HashMap::new()
                },
                attempt_number: workflow_step.attempts.unwrap_or(0) as u32,
                max_retry_attempts: workflow_step.retry_limit.unwrap_or(0) as u32,
                timeout_seconds: 300, // 5 minutes default timeout
                is_retryable: workflow_step.retryable,
                environment: std::env::var("RAILS_ENV")
                    .unwrap_or_else(|_| "development".to_string()),
                metadata: std::collections::HashMap::new(),
            };

            // Use the existing build_step_request method
            let step_request = self
                .build_step_request(&step_execution_context, handler_class)
                .await?;
            step_execution_requests.push(step_request);
        }

        // Create database records before publishing to ZeroMQ - use database batch_id as the primary identifier
        let database_batch_id = self
            .create_batch_with_database_records(&batch_uuid, &step_execution_requests, None)
            .await
            .map_err(|e| {
                OrchestrationError::ExecutionError(ExecutionError::BatchCreationFailed {
                    batch_id: batch_uuid.clone(),
                    reason: format!("Failed to create batch database records: {e}"),
                    error_code: Some("BATCH_DATABASE_CREATE_FAILED".to_string()),
                })
            })?;

        // Use database batch_id as the ZeroMQ batch identifier for better traceability
        let batch_id = database_batch_id.to_string();

        let batch = StepBatchRequest::new(batch_id.clone(), step_execution_requests);

        let request_json = serde_json::to_string(&batch).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: error_step_id,
                reason: format!("Failed to serialize step batch: {e}"),
                error_code: Some("ZMQ_SERIALIZATION_FAILED".to_string()),
            })
        })?;

        // Publish with "steps" topic
        let topic_msg = format!("steps {request_json}");
        self.pub_socket.send(&topic_msg, 0).map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: error_step_id,
                reason: format!("Failed to publish step batch to ZeroMQ: {e}"),
                error_code: Some("ZMQ_PUBLISH_FAILED".to_string()),
            })
        })?;

        // Update batch state to 'published' after successful ZeroMQ publishing
        self.update_batch_state(
            database_batch_id,
            "created",
            "published",
            "zmq_publish_success",
        )
        .await
        .map_err(|e| {
            OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: error_step_id,
                reason: format!("Failed to update batch state to published: {e}"),
                error_code: Some("BATCH_STATE_UPDATE_FAILED".to_string()),
            })
        })?;

        tracing::debug!(
            "Published step batch {} with {} steps (batch_id is database ID for traceability)",
            batch_id,
            batch.steps.len()
        );

        Ok(batch_id)
    }

    /// Convert orchestration context to ZeroMQ step execution request
    async fn build_step_request(
        &self,
        context: &StepExecutionContext,
        handler_class: &str,
    ) -> Result<StepExecutionRequest, OrchestrationError> {
        // Extract previous step results from the previous_steps field with proper step names
        let previous_results = self
            .extract_previous_step_results_with_names(context)
            .await?;

        Ok(StepExecutionRequest::new(
            context.step_id,
            context.task_id,
            context.step_name.clone(),
            handler_class.to_string(), // Use the handler class from orchestration layer
            context.step_config.clone(),
            context.input_data.clone(),
            previous_results,
            context.max_retry_attempts as i32, // Convert u32 to i32
            (context.timeout_seconds * 1000) as i64, // Convert to milliseconds
        ))
    }

    /// Extract previous step results with proper step names from database
    async fn extract_previous_step_results_with_names(
        &self,
        context: &StepExecutionContext,
    ) -> Result<HashMap<String, serde_json::Value>, OrchestrationError> {
        let mut previous_results = HashMap::new();

        if context.previous_steps.is_empty() {
            return Ok(previous_results);
        }

        // Get step names from database for meaningful keys
        let step_names = self
            .resolve_step_names(&context.previous_steps)
            .await
            .map_err(|e| {
                OrchestrationError::ExecutionError(
                    crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                        step_id: context.step_id,
                        reason: format!("Failed to resolve step names: {e}"),
                        error_code: Some("STEP_NAME_RESOLUTION_FAILED".to_string()),
                    },
                )
            })?;

        for previous_step in &context.previous_steps {
            // Use the resolved step name or fallback to generic ID
            let step_name = step_names
                .get(&previous_step.workflow_step_id)
                .cloned()
                .unwrap_or_else(|| format!("step_{}", previous_step.workflow_step_id));

            // Extract the results from the WorkflowStep if available
            if let Some(results) = &previous_step.results {
                previous_results.insert(step_name, results.clone());
            } else {
                // If no results, insert null placeholder to indicate step completed without output
                previous_results.insert(step_name, serde_json::Value::Null);
            }
        }

        tracing::debug!(
            "Extracted {} previous step results with names for step {} in task {}",
            previous_results.len(),
            context.step_id,
            context.task_id
        );

        Ok(previous_results)
    }

    /// Resolve handler class names from workflow steps using TaskHandlerRegistry
    ///
    /// This method gets the task namespace, name, and version, then looks up the proper
    /// handler class through the TaskHandlerRegistry for accurate handler resolution.
    /// This replaces the old approach of just looking up step names from the database.
    async fn resolve_step_names(
        &self,
        workflow_steps: &[crate::models::WorkflowStep],
    ) -> Result<HashMap<i64, String>, crate::orchestration::errors::OrchestrationError> {
        if workflow_steps.is_empty() {
            return Ok(HashMap::new());
        }

        // Get task_id from first workflow step (all steps in batch have same task_id)
        let task_id = workflow_steps[0].task_id;

        // Load task with orchestration metadata
        let task = crate::models::Task::find_by_id(&self.pool, task_id)
            .await
            .map_err(|e| {
                crate::orchestration::errors::OrchestrationError::ExecutionError(
                    crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                        step_id: task_id,
                        reason: format!("Failed to load task for handler resolution: {e}"),
                        error_code: Some("TASK_LOAD_FAILED".to_string()),
                    },
                )
            })?
            .ok_or_else(|| {
                crate::orchestration::errors::OrchestrationError::ExecutionError(
                    crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                        step_id: task_id,
                        reason: "Task not found".to_string(),
                        error_code: Some("TASK_NOT_FOUND".to_string()),
                    },
                )
            })?;

        let task_for_orchestration = task.for_orchestration(&self.pool).await.map_err(|e| {
            crate::orchestration::errors::OrchestrationError::ExecutionError(
                crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                    step_id: task_id,
                    reason: format!("Failed to load task orchestration metadata: {e}"),
                    error_code: Some("TASK_ORCHESTRATION_LOAD_FAILED".to_string()),
                },
            )
        })?;

        // Use TaskHandlerRegistry to resolve handler metadata
        let handler_metadata = self
            .task_handler_registry
            .get_handler_metadata(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .map_err(|e| {
                crate::orchestration::errors::OrchestrationError::ExecutionError(
                    crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                        step_id: task_id,
                        reason: format!("Failed to resolve handler metadata: {e}"),
                        error_code: Some("HANDLER_METADATA_RESOLUTION_FAILED".to_string()),
                    },
                )
            })?;

        // Return the handler class for all steps in this task batch
        let mut step_handler_classes = HashMap::new();
        for workflow_step in workflow_steps {
            step_handler_classes.insert(
                workflow_step.workflow_step_id,
                handler_metadata.handler_class.clone(),
            );
        }

        tracing::info!(
            task_id = task_id,
            namespace = task_for_orchestration.namespace_name,
            task_name = task_for_orchestration.task_name,
            task_version = task_for_orchestration.task_version,
            handler_class = handler_metadata.handler_class,
            step_count = workflow_steps.len(),
            "✅ HANDLER RESOLUTION SUCCESS: Using TaskHandlerRegistry to resolve proper handler class"
        );

        Ok(step_handler_classes)
    }

    /// Create database records for batch execution tracking
    async fn create_batch_with_database_records(
        &self,
        batch_uuid: &str,
        steps: &[StepExecutionRequest],
        step_context: Option<&StepExecutionContext>,
    ) -> Result<i64, sqlx::Error> {
        // Get task_id and handler_class from first step or context
        let task_id = step_context
            .map(|ctx| ctx.task_id)
            .or_else(|| steps.first().map(|s| s.task_id))
            .ok_or_else(|| sqlx::Error::RowNotFound)?;

        let handler_class = steps
            .first()
            .map(|s| s.handler_class.clone())
            .unwrap_or_else(|| UNKNOWN_HANDLER_CLASS.to_string());

        // Create main batch record
        let new_batch = NewStepExecutionBatch {
            task_id,
            handler_class,
            batch_uuid: batch_uuid.to_string(),
            initiated_by: Some("zeromq_pub_sub_executor".to_string()),
            batch_size: steps.len() as i32,
            timeout_seconds: 300, // 5 minutes default
            metadata: Some(serde_json::json!({
                "zeromq_batch_uuid": batch_uuid,
                "step_count": steps.len(),
                "created_by": "ZmqPubSubExecutor"
            })),
        };

        let batch_record = StepExecutionBatch::create(&self.pool, new_batch).await?;
        let database_batch_id = batch_record.batch_id;

        // Create HABTM join table entries for each step
        let mut batch_steps = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let new_batch_step = NewStepExecutionBatchStep {
                batch_id: database_batch_id,
                workflow_step_id: step.step_id,
                sequence_order: index as i32,
                expected_handler_class: step.handler_class.clone(),
                metadata: Some(serde_json::json!({
                    "step_name": step.step_name,
                    "zeromq_batch_uuid": batch_uuid
                })),
            };

            batch_steps.push(new_batch_step);
        }

        // Use batch creation for better performance
        StepExecutionBatchStep::create_batch(&self.pool, batch_steps).await?;

        // Create initial state transition: created
        let initial_transition = NewStepExecutionBatchTransition {
            batch_id: database_batch_id,
            from_state: None,
            to_state: "created".to_string(),
            event_name: Some("batch_initialization".to_string()),
            metadata: Some(serde_json::json!({
                "steps_included": steps.len(),
                "initiated_by": "ZmqPubSubExecutor"
            })),
            sort_key: 1,
        };

        StepExecutionBatchTransition::create(&self.pool, initial_transition).await?;

        tracing::debug!(
            "Created batch database records: batch_id={}, uuid={}, steps={}",
            database_batch_id,
            batch_uuid,
            steps.len()
        );

        Ok(database_batch_id)
    }

    /// Update batch state with transition tracking
    async fn update_batch_state(
        &self,
        batch_id: i64,
        from_state: &str,
        to_state: &str,
        event_name: &str,
    ) -> Result<(), sqlx::Error> {
        // Find the current sort_key to increment for new transition
        let current_max_sort_key = sqlx::query!(
            "SELECT COALESCE(MAX(sort_key), 0) as max_sort_key FROM tasker_step_execution_batch_transitions WHERE batch_id = $1",
            batch_id
        )
        .fetch_one(&self.pool)
        .await?;

        let new_transition = NewStepExecutionBatchTransition {
            batch_id,
            from_state: Some(from_state.to_string()),
            to_state: to_state.to_string(),
            event_name: Some(event_name.to_string()),
            metadata: Some(serde_json::json!({
                "transition_timestamp": chrono::Utc::now(),
                "triggered_by": "ZmqPubSubExecutor"
            })),
            sort_key: current_max_sort_key.max_sort_key.unwrap_or(0) + 1,
        };

        StepExecutionBatchTransition::create(&self.pool, new_transition).await?;

        tracing::debug!(
            "Updated batch {} state: {} -> {} via {}",
            batch_id,
            from_state,
            to_state,
            event_name
        );

        Ok(())
    }

    /// Save partial result to audit ledger for forensic analysis
    async fn save_partial_result_to_audit_ledger(
        pool: &PgPool,
        batch_id: &str,
        step_id: i64,
        message: &ResultMessage,
    ) -> Result<(), sqlx::Error> {
        // Look up the batch-step relationship to get batch_step_id
        let batch_step = sqlx::query!(
            r#"
            SELECT bs.id as batch_step_id
            FROM tasker_step_execution_batch_steps bs
            JOIN tasker_step_execution_batches b ON b.batch_id = bs.batch_id
            WHERE b.batch_uuid = $1 AND bs.workflow_step_id = $2
            "#,
            batch_id,
            step_id
        )
        .fetch_optional(pool)
        .await?;

        let batch_step_id = match batch_step {
            Some(record) => record.batch_step_id,
            None => {
                tracing::warn!(
                    "Could not find batch-step relationship for batch {} step {} - skipping audit ledger",
                    batch_id, step_id
                );
                return Ok(());
            }
        };

        if let ResultMessage::PartialResult {
            worker_id,
            sequence,
            status,
            execution_time_ms,
            ..
        } = message
        {
            let new_result = NewStepExecutionBatchReceivedResult {
                batch_step_id,
                message_type: "partial_result".to_string(),
                worker_id: Some(worker_id.clone()),
                sequence_number: Some(*sequence as i32),
                status: Some(status.clone()),
                execution_time_ms: Some(*execution_time_ms),
                raw_message_json: serde_json::to_value(message)
                    .unwrap_or_else(|_| serde_json::json!({"error": "serialization_failed"})),
                processed_at: Some(chrono::Utc::now().naive_utc()),
                processing_errors: None,
            };

            StepExecutionBatchReceivedResult::create(pool, new_result).await?;

            tracing::debug!(
                "Saved partial result to audit ledger: batch={} step={} worker={} seq={}",
                batch_id,
                step_id,
                worker_id,
                sequence
            );
        }

        Ok(())
    }

    /// Save batch completion to audit ledger for reconciliation analysis
    async fn save_batch_completion_to_audit_ledger(
        pool: &PgPool,
        batch_id: &str,
        message: &ResultMessage,
    ) -> Result<(), sqlx::Error> {
        // For batch completion, we need to create a record for each step in the batch
        // We'll use the first step's batch_step_id as the primary reference
        let batch_steps = sqlx::query!(
            r#"
            SELECT bs.id as batch_step_id, bs.workflow_step_id
            FROM tasker_step_execution_batch_steps bs
            JOIN tasker_step_execution_batches b ON b.batch_id = bs.batch_id
            WHERE b.batch_uuid = $1
            ORDER BY bs.sequence_order
            LIMIT 1
            "#,
            batch_id
        )
        .fetch_optional(pool)
        .await?;

        let batch_step_id = match batch_steps {
            Some(record) => record.batch_step_id,
            None => {
                tracing::warn!(
                    "Could not find batch for batch_uuid {} - skipping batch completion audit",
                    batch_id
                );
                return Ok(());
            }
        };

        if let ResultMessage::BatchCompletion {
            total_steps,
            completed_steps,
            failed_steps,
            step_summaries,
            ..
        } = message
        {
            let new_result = NewStepExecutionBatchReceivedResult {
                batch_step_id,
                message_type: "batch_completion".to_string(),
                worker_id: Some(BATCH_ORCHESTRATOR_WORKER_ID.to_string()),
                sequence_number: None, // Batch completion doesn't have sequence
                status: None,          // Batch completion has aggregate status
                execution_time_ms: {
                    let total: i64 = step_summaries
                        .iter()
                        .filter_map(|s| s.execution_time_ms)
                        .sum();
                    if total > 0 {
                        Some(total)
                    } else {
                        None
                    }
                },
                raw_message_json: serde_json::to_value(message)
                    .unwrap_or_else(|_| serde_json::json!({"error": "serialization_failed"})),
                processed_at: Some(chrono::Utc::now().naive_utc()),
                processing_errors: None,
            };

            StepExecutionBatchReceivedResult::create(pool, new_result).await?;

            tracing::debug!(
                "Saved batch completion to audit ledger: batch={} total={} completed={} failed={}",
                batch_id,
                total_steps,
                completed_steps,
                failed_steps
            );
        }

        Ok(())
    }

    /// Perform reconciliation between batch completion step summaries and tracked partial results
    async fn perform_batch_reconciliation(
        batch_id: &str,
        step_summaries: &[crate::execution::message_protocols::StepSummary],
        tracker: &BatchTracker,
    ) {
        let mut discrepancies_found = 0;

        tracing::debug!(
            "Starting reconciliation for batch {} (task {}) with {} step summaries and {} partial results",
            batch_id,
            tracker.task_id,
            step_summaries.len(),
            tracker.partial_results.len()
        );

        for summary in step_summaries {
            let step_id = summary.step_id;

            match tracker.partial_results.get(&step_id) {
                Some(partial_result) => {
                    // Compare partial result with summary data
                    let partial_status = match partial_result {
                        crate::execution::message_protocols::ResultMessage::PartialResult {
                            status,
                            ..
                        } => status,
                        _ => {
                            tracing::warn!(
                                "Found non-partial result in partial_results for step {}",
                                step_id
                            );
                            continue;
                        }
                    };

                    if partial_status != &summary.final_status {
                        discrepancies_found += 1;
                        tracing::warn!(
                            "Status discrepancy for step {} in batch {}: partial='{}', summary='{}'",
                            step_id, batch_id, partial_status, summary.final_status
                        );
                    } else {
                        tracing::debug!(
                            "Step {} status reconciled successfully: '{}'",
                            step_id,
                            summary.final_status
                        );
                    }
                }
                None => {
                    discrepancies_found += 1;
                    tracing::warn!(
                        "Missing partial result for step {} in batch {} (summary status: '{}')",
                        step_id,
                        batch_id,
                        summary.final_status
                    );
                }
            }
        }

        // Check for partial results without corresponding summaries
        for step_id in tracker.partial_results.keys() {
            if !step_summaries.iter().any(|s| s.step_id == *step_id) {
                discrepancies_found += 1;
                tracing::warn!(
                    "Found partial result for step {} without corresponding summary in batch {}",
                    step_id,
                    batch_id
                );
            }
        }

        if discrepancies_found == 0 {
            tracing::info!(
                "Batch {} reconciliation successful - all {} steps consistent",
                batch_id,
                step_summaries.len()
            );
        } else {
            tracing::error!(
                "Batch {} reconciliation found {} discrepancies out of {} steps",
                batch_id,
                discrepancies_found,
                step_summaries.len()
            );

            // Reconciliation discrepancies logged - consider implementing alerts in production
        }
    }

    /// Convert ZMQ step result to orchestration StepResult - will be used for result processing
    #[allow(dead_code)]
    fn convert_result(
        &self,
        zmq_result: StepExecutionResult,
    ) -> Result<StepResult, OrchestrationError> {
        let status = match zmq_result.status.as_str() {
            "completed" => StepStatus::Completed,
            "failed" => StepStatus::Failed,
            "error" => StepStatus::Failed, // Map error to failed for now
            _ => {
                return Err(OrchestrationError::ExecutionError(
                    ExecutionError::StepExecutionFailed {
                        step_id: zmq_result.step_id,
                        reason: format!("Unknown step result status: {}", zmq_result.status),
                        error_code: Some("UNKNOWN_STATUS".to_string()),
                    },
                ))
            }
        };

        let error_message = zmq_result.error.as_ref().map(|e| e.message.clone());
        let error_code = zmq_result.error.as_ref().and_then(|e| e.error_type.clone());

        Ok(StepResult {
            step_id: zmq_result.step_id,
            status,
            output: zmq_result.output.unwrap_or(serde_json::Value::Null),
            execution_duration: Duration::from_millis(zmq_result.metadata.execution_time_ms as u64),
            error_message,
            retry_after: None, // ZeroMQ doesn't specify retry timing
            error_code,
            error_context: None, // Could be enhanced later
        })
    }
}

#[async_trait]
impl FrameworkIntegration for ZmqPubSubExecutor {
    fn framework_name(&self) -> &'static str {
        "ZeroMQ"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, OrchestrationError> {
        // Load basic task data from database
        let task = Task::find_by_id(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::TaskExecutionFailed {
                task_id,
                reason: format!("Database error loading task: {e}"),
                error_code: Some("DATABASE_ERROR".to_string()),
            })?
            .ok_or_else(|| OrchestrationError::TaskExecutionFailed {
                task_id,
                reason: "Task not found".to_string(),
                error_code: Some("TASK_NOT_FOUND".to_string()),
            })?;

        // Load execution context for additional metadata
        let execution_context = TaskExecutionContext::get_for_task(&self.pool, task_id)
            .await
            .map_err(|e| OrchestrationError::TaskExecutionFailed {
                task_id,
                reason: format!("Error loading task execution context: {e}"),
                error_code: Some("CONTEXT_LOAD_ERROR".to_string()),
            })?;

        // Build metadata from multiple sources
        let mut metadata = HashMap::new();

        // Add basic task metadata
        if let Some(initiator) = task.initiator {
            metadata.insert(
                "initiator".to_string(),
                serde_json::Value::String(initiator),
            );
        }
        if let Some(source_system) = task.source_system {
            metadata.insert(
                "source_system".to_string(),
                serde_json::Value::String(source_system),
            );
        }
        if let Some(reason) = task.reason {
            metadata.insert("reason".to_string(), serde_json::Value::String(reason));
        }

        // Add tags from JSONB field
        if let Some(serde_json::Value::Object(tag_map)) = task.tags {
            for (key, value) in tag_map {
                metadata.insert(format!("tag_{key}"), value);
            }
        }

        // Add execution context metadata if available
        if let Some(ctx) = execution_context {
            metadata.insert(
                "total_steps".to_string(),
                serde_json::Value::Number(ctx.total_steps.into()),
            );
            metadata.insert(
                "ready_steps".to_string(),
                serde_json::Value::Number(ctx.ready_steps.into()),
            );
            metadata.insert(
                "completed_steps".to_string(),
                serde_json::Value::Number(ctx.completed_steps.into()),
            );
            metadata.insert(
                "execution_status".to_string(),
                serde_json::Value::String(ctx.execution_status),
            );
            metadata.insert(
                "health_status".to_string(),
                serde_json::Value::String(ctx.health_status),
            );
        }

        // Extract context data from JSONB field
        let data = task
            .context
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        Ok(TaskContext {
            task_id,
            data,
            metadata,
        })
    }

    async fn enqueue_task(
        &self,
        _task_id: i64,
        _delay: Option<Duration>,
    ) -> Result<(), OrchestrationError> {
        // ZeroMQ executor doesn't handle task enqueueing directly
        // This would be delegated to another system
        Ok(())
    }

    fn supports_batch_execution(&self) -> bool {
        true // ZeroMQ executor supports native batching
    }
}

// Implement Send + Sync for thread safety
unsafe impl Send for ZmqPubSubExecutor {}
unsafe impl Sync for ZmqPubSubExecutor {}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::orchestration::step_handler::StepExecutionContext; // Unused import
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_message_protocol_serialization() {
        // This test doesn't actually need a ZmqPubSubExecutor, just tests the message protocols
        let step_request = StepExecutionRequest::new(
            123,
            456,
            "test_step".to_string(),
            "TestHandler".to_string(),
            HashMap::new(),
            serde_json::json!({"test": true}),
            HashMap::new(),
            3,
            30000,
        );

        let batch = StepBatchRequest::new("test_batch".to_string(), vec![step_request]);

        // Test serialization
        let json = serde_json::to_string(&batch).expect("Should serialize");
        assert!(json.contains("test_batch"));
        assert!(json.contains("test_step"));

        // Test deserialization would work
        let result =
            StepExecutionResult::success(123, serde_json::json!({"completed": true}), 1500);

        let response = StepBatchResponse::new("test_batch".to_string(), vec![result]);
        let response_json = serde_json::to_string(&response).expect("Should serialize");
        assert!(response_json.contains("completed"));
    }
}
