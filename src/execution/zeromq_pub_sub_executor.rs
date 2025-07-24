use crate::execution::message_protocols::{
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
    ResultMessage, StepSummary,
};
use crate::models::{Task, TaskExecutionContext};
use crate::orchestration::{
    errors::{OrchestrationError, ExecutionError},
    step_handler::StepExecutionContext,
    state_manager::StateManager,
    types::{StepResult, StepStatus, TaskContext, FrameworkIntegration},
};
use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;
use zmq::{Context, Socket, PUB, SUB};

/// Result of attempting to receive a message from ZeroMQ
enum MessageReceiveResult {
    Success(Vec<u8>),
    NoMessage,
    Error,
}

/// Result of parsing a received message
enum MessageParseResult {
    PartialResult { batch_id: String, message: ResultMessage },
    BatchCompletion { batch_id: String, message: ResultMessage },
    LegacyResult { batch_id: String, response: StepBatchResponse },
    WrongTopic(String),
    Error(String),
}

/// Tracks partial results for a batch during execution
#[derive(Debug, Clone)]
struct BatchTracker {
    batch_id: String,
    total_steps: u32,
    partial_results: HashMap<i64, ResultMessage>, // step_id -> partial result
    expected_sequences: HashMap<i64, u32>, // step_id -> expected sequence number
    batch_complete: bool,
    created_at: std::time::Instant,
}

impl BatchTracker {
    fn new(batch_id: String, total_steps: u32) -> Self {
        Self {
            batch_id,
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

    /// Check if batch is complete based on partial results
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
    context: Context,
    pub_endpoint: String,
    sub_endpoint: String,
    pub_socket: Socket,
    result_handlers: Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
    batch_trackers: Arc<Mutex<HashMap<String, BatchTracker>>>,
    pool: PgPool,
    state_manager: StateManager,
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
    pub async fn new(pub_endpoint: &str, sub_endpoint: &str, pool: PgPool, state_manager: StateManager) -> Result<Self, OrchestrationError> {
        let context = Context::new();
        
        // Publisher socket for sending step batches
        let pub_socket = context.socket(PUB)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to create PUB socket: {}", e),
                error_code: Some("ZMQ_PUB_CREATE_FAILED".to_string()),
            }))?;
        pub_socket.bind(pub_endpoint)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to bind PUB socket to {}: {}", pub_endpoint, e),
                error_code: Some("ZMQ_PUB_BIND_FAILED".to_string()),
            }))?;
        
        // Subscriber socket for receiving results
        let sub_socket = context.socket(SUB)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to create SUB socket: {}", e),
                error_code: Some("ZMQ_SUB_CREATE_FAILED".to_string()),
            }))?;
        sub_socket.connect(sub_endpoint)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to connect SUB socket to {}: {}", sub_endpoint, e),
                error_code: Some("ZMQ_SUB_CONNECT_FAILED".to_string()),
            }))?;
        sub_socket.set_subscribe(b"results")
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: 0,
                reason: format!("Failed to subscribe to 'results' topic: {}", e),
                error_code: Some("ZMQ_SUB_SUBSCRIBE_FAILED".to_string()),
            }))?;
        
        let result_handlers = Arc::new(Mutex::new(HashMap::new()));
        let batch_trackers = Arc::new(Mutex::new(HashMap::new()));
        
        // Start background task to listen for results (takes ownership of sub_socket)
        let result_listener_handle = Self::start_result_listener(
            result_handlers.clone(), 
            batch_trackers.clone(), 
            sub_socket,
            state_manager.clone()
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
            _result_listener_handle: result_listener_handle,
        })
    }
    
    /// Start background task to listen for result messages
    fn start_result_listener(
        result_handlers: Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
        batch_trackers: Arc<Mutex<HashMap<String, BatchTracker>>>,
        sub_socket: Socket,
        state_manager: StateManager,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            tracing::info!("Starting ZeroMQ result listener with dual message support");
            
            loop {
                match Self::receive_message_from_socket(&sub_socket) {
                    MessageReceiveResult::Success(msg_bytes) => {
                        Self::handle_received_message(msg_bytes, &result_handlers, &batch_trackers, &state_manager).await;
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
    ) {
        match Self::parse_message(msg_bytes) {
            MessageParseResult::PartialResult { batch_id, message } => {
                Self::handle_partial_result(batch_id, message, batch_trackers, state_manager).await;
            }
            MessageParseResult::BatchCompletion { batch_id, message } => {
                Self::handle_batch_completion(batch_id, message, batch_trackers, result_handlers).await;
            }
            MessageParseResult::LegacyResult { batch_id, response } => {
                Self::route_result_to_handler(batch_id, response, result_handlers).await;
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
            None => return MessageParseResult::Error("Malformed message (no topic separator)".to_string()),
        };

        // Check if this is a results message
        if topic != "results" {
            return MessageParseResult::WrongTopic(topic.to_string());
        }

        // Try to parse as new dual message format first
        match serde_json::from_str::<ResultMessage>(json_data) {
            Ok(message) => {
                match &message {
                    ResultMessage::PartialResult { batch_id, .. } => {
                        tracing::debug!("Received partial result for batch: {}", batch_id);
                        MessageParseResult::PartialResult { 
                            batch_id: batch_id.clone(), 
                            message 
                        }
                    }
                    ResultMessage::BatchCompletion { batch_id, .. } => {
                        tracing::debug!("Received batch completion for batch: {}", batch_id);
                        MessageParseResult::BatchCompletion { 
                            batch_id: batch_id.clone(), 
                            message 
                        }
                    }
                }
            }
            Err(_) => {
                // Fallback to legacy format for backward compatibility
                match serde_json::from_str::<StepBatchResponse>(json_data) {
                    Ok(response) => {
                        let batch_id = response.batch_id.clone();
                        tracing::debug!("Received legacy results for batch: {}", batch_id);
                        MessageParseResult::LegacyResult { batch_id, response }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse result message: {}", e);
                        MessageParseResult::Error(format!("JSON parsing failed: {}", e))
                    }
                }
            }
        }
    }

    /// Handle partial result message (immediate step state update)
    async fn handle_partial_result(
        batch_id: String,
        message: ResultMessage,
        batch_trackers: &Arc<Mutex<HashMap<String, BatchTracker>>>,
        state_manager: &StateManager,
    ) {
        if let ResultMessage::PartialResult { step_id, status, output, error, execution_time_ms, worker_id, .. } = &message {
            tracing::info!(
                "Processing partial result for step {} in batch {}: status={} worker={} exec_time={}ms",
                step_id, batch_id, status, worker_id, execution_time_ms
            );

            // Phase 2.3 - Update step state immediately using StateManager
            let state_update_result = match status.as_str() {
                "completed" => {
                    state_manager.complete_step_with_results(*step_id, output.clone()).await
                }
                "failed" => {
                    let error_message = error
                        .as_ref()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "Step execution failed".to_string());
                    state_manager.fail_step_with_error(*step_id, error_message).await
                }
                "in_progress" => {
                    state_manager.mark_step_in_progress(*step_id).await
                }
                _ => {
                    tracing::warn!("Unknown status '{}' for step {}, skipping state update", status, step_id);
                    Ok(())
                }
            };

            // Log state update result
            match state_update_result {
                Ok(()) => {
                    tracing::debug!(
                        "Successfully updated step {} state to '{}' via StateManager",
                        step_id, status
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to update step {} state to '{}': {}",
                        step_id, status, e
                    );
                }
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

    /// Handle batch completion message (reconciliation check)
    async fn handle_batch_completion(
        batch_id: String,
        message: ResultMessage,
        batch_trackers: &Arc<Mutex<HashMap<String, BatchTracker>>>,
        result_handlers: &Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
    ) {
        if let ResultMessage::BatchCompletion { step_summaries, .. } = &message {
            tracing::info!(
                "Processing batch completion for batch {} with {} step summaries",
                batch_id, step_summaries.len()
            );

            // TODO: Phase 2.2 - Implement reconciliation logic
            // Compare step_summaries with partial results from batch tracker
            // Detect and log any discrepancies

            let mut trackers = batch_trackers.lock().await;
            if let Some(mut tracker) = trackers.remove(&batch_id) {
                tracker.batch_complete = true;
                
                // TODO: Convert to legacy format for existing handlers
                // For now, just log that we received the completion
                tracing::info!("Batch {} marked as complete", batch_id);
            } else {
                tracing::warn!("Received batch completion for unknown batch: {}", batch_id);
            }
        }
    }

    /// Route parsed result to the appropriate waiting handler
    async fn route_result_to_handler(
        batch_id: String,
        response: StepBatchResponse,
        result_handlers: &Arc<Mutex<HashMap<String, oneshot::Sender<StepBatchResponse>>>>,
    ) {
        let mut handlers = result_handlers.lock().await;
        
        if let Some(sender) = handlers.remove(&batch_id) {
            // Send the response to the waiting caller
            if let Err(_) = sender.send(response) {
                tracing::warn!("Failed to send result to waiting handler for batch {}", batch_id);
            }
        } else {
            tracing::warn!("Received results for unknown batch: {}", batch_id);
        }
    }
    
    /// Publish step batch to ZeroMQ with fire-and-forget semantics
    async fn publish_batch(&self, steps: Vec<StepExecutionRequest>, step_context: Option<&StepExecutionContext>) -> Result<String, OrchestrationError> {
        let batch_id = format!(
            "batch_{}_{}",
            Uuid::new_v4().to_string().replace('-', ""),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        
        // Get step_id for error context - use from context if available, otherwise from first step
        let error_step_id = step_context
            .map(|ctx| ctx.step_id)
            .or_else(|| steps.first().map(|s| s.step_id))
            .unwrap_or(0);

        let batch = StepBatchRequest::new(batch_id.clone(), steps);

        let request_json = serde_json::to_string(&batch)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: error_step_id,
                reason: format!("Failed to serialize step batch: {}", e),
                error_code: Some("ZMQ_SERIALIZATION_FAILED".to_string()),
            }))?;
        
        // Publish with "steps" topic
        let topic_msg = format!("steps {}", request_json);
        self.pub_socket.send(&topic_msg, 0)
            .map_err(|e| OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: error_step_id,
                reason: format!("Failed to publish step batch to ZeroMQ: {}", e),
                error_code: Some("ZMQ_PUBLISH_FAILED".to_string()),
            }))?;
        
        tracing::debug!("Published step batch {} with {} steps", batch_id, batch.steps.len());
        
        Ok(batch_id)
    }
    
    
    /// Convert orchestration context to ZeroMQ step execution request
    async fn build_step_request(&self, context: &StepExecutionContext, handler_class: &str) -> Result<StepExecutionRequest, OrchestrationError> {
        // Extract previous step results from the previous_steps field with proper step names
        let previous_results = self.extract_previous_step_results_with_names(context).await?;

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
    async fn extract_previous_step_results_with_names(&self, context: &StepExecutionContext) -> Result<HashMap<String, serde_json::Value>, OrchestrationError> {
        let mut previous_results = HashMap::new();
        
        if context.previous_steps.is_empty() {
            return Ok(previous_results);
        }
        
        // Get step names from database for meaningful keys
        let step_names = self.resolve_step_names(&context.previous_steps)
            .await
            .map_err(|e| OrchestrationError::ExecutionError(
                crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                    step_id: context.step_id,
                    reason: format!("Failed to resolve step names: {}", e),
                    error_code: Some("STEP_NAME_RESOLUTION_FAILED".to_string()),
                }
            ))?;
        
        for previous_step in &context.previous_steps {
            // Use the resolved step name or fallback to generic ID
            let step_name = step_names.get(&previous_step.workflow_step_id)
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
    
    /// Resolve step names from database for better result keys
    /// 
    /// This method would look up the actual step names from the tasker_named_steps table
    /// to provide meaningful keys in the previous_results HashMap instead of generic
    /// step_N identifiers. This is important for handlers that need to access specific
    /// previous step outputs by name.
    async fn resolve_step_names(&self, workflow_steps: &[crate::models::WorkflowStep]) -> Result<HashMap<i64, String>, sqlx::Error> {
        if workflow_steps.is_empty() {
            return Ok(HashMap::new());
        }
        
        let named_step_ids: Vec<i32> = workflow_steps.iter()
            .map(|ws| ws.named_step_id)
            .collect();
            
        let name_mappings = sqlx::query!(
            r#"
            SELECT named_step_id, name 
            FROM tasker_named_steps 
            WHERE named_step_id = ANY($1)
            "#,
            &named_step_ids
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut step_names = HashMap::new();
        for workflow_step in workflow_steps {
            if let Some(named_step) = name_mappings.iter().find(|ns| ns.named_step_id == workflow_step.named_step_id) {
                step_names.insert(workflow_step.workflow_step_id, named_step.name.clone());
            } else {
                // Fallback to generic name if not found
                step_names.insert(workflow_step.workflow_step_id, format!("step_{}", workflow_step.workflow_step_id));
            }
        }
        
        Ok(step_names)
    }
    
    /// Convert ZeroMQ result to orchestration StepResult
    fn convert_result(&self, zmq_result: StepExecutionResult) -> Result<StepResult, OrchestrationError> {
        let status = match zmq_result.status.as_str() {
            "completed" => StepStatus::Completed,
            "failed" => StepStatus::Failed,
            "error" => StepStatus::Failed, // Map error to failed for now
            _ => return Err(OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
                step_id: zmq_result.step_id,
                reason: format!("Unknown step result status: {}", zmq_result.status),
                error_code: Some("UNKNOWN_STATUS".to_string()),
            })),
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
                reason: format!("Database error loading task: {}", e),
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
                reason: format!("Error loading task execution context: {}", e),
                error_code: Some("CONTEXT_LOAD_ERROR".to_string()),
            })?;

        // Build metadata from multiple sources
        let mut metadata = HashMap::new();
        
        // Add basic task metadata
        if let Some(initiator) = task.initiator {
            metadata.insert("initiator".to_string(), serde_json::Value::String(initiator));
        }
        if let Some(source_system) = task.source_system {
            metadata.insert("source_system".to_string(), serde_json::Value::String(source_system));
        }
        if let Some(reason) = task.reason {
            metadata.insert("reason".to_string(), serde_json::Value::String(reason));
        }
        
        // Add tags from JSONB field
        if let Some(tags) = task.tags {
            if let serde_json::Value::Object(tag_map) = tags {
                for (key, value) in tag_map {
                    metadata.insert(format!("tag_{}", key), value);
                }
            }
        }
        
        // Add execution context metadata if available
        if let Some(ctx) = execution_context {
            metadata.insert("total_steps".to_string(), serde_json::Value::Number(ctx.total_steps.into()));
            metadata.insert("ready_steps".to_string(), serde_json::Value::Number(ctx.ready_steps.into()));
            metadata.insert("completed_steps".to_string(), serde_json::Value::Number(ctx.completed_steps.into()));
            metadata.insert("execution_status".to_string(), serde_json::Value::String(ctx.execution_status));
            metadata.insert("health_status".to_string(), serde_json::Value::String(ctx.health_status));
        }

        // Extract context data from JSONB field
        let data = task.context.unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

        Ok(TaskContext {
            task_id,
            data,
            metadata,
        })
    }

    async fn enqueue_task(&self, _task_id: i64, _delay: Option<Duration>) -> Result<(), OrchestrationError> {
        // ZeroMQ executor doesn't handle task enqueueing directly
        // This would be delegated to another system
        Ok(())
    }

    async fn execute_step_batch(
        &self,
        contexts: Vec<(&StepExecutionContext, &str, &HashMap<String, serde_json::Value>)>,
    ) -> Result<Vec<StepResult>, OrchestrationError> {
        if contexts.is_empty() {
            return Ok(Vec::new());
        }

        tracing::info!("Publishing batch of {} steps to ZeroMQ (fire-and-forget)", contexts.len());

        // Build step requests for all contexts in the batch
        let mut step_requests = Vec::new();
        for (context, handler_class, _handler_config) in &contexts {
            let step_request = self.build_step_request(context, handler_class).await?;
            step_requests.push(step_request);
        }

        // Publish entire batch as single ZeroMQ message (fire-and-forget)
        let _batch_id = self.publish_batch(step_requests, contexts.first().map(|(ctx, _, _)| *ctx)).await?;

        // Create "in_progress" results for all steps since publish succeeded
        let mut results = Vec::new();
        for (context, _handler_class, _handler_config) in contexts {
            results.push(StepResult {
                step_id: context.step_id,
                status: StepStatus::InProgress, // Mark as in-progress after successful publish
                output: serde_json::Value::Null, // No output yet - will come from result listener
                execution_duration: Duration::from_millis(0), // Publish duration is negligible
                error_message: None,
                retry_after: None,
                error_code: None,
                error_context: None,
            });
        }

        tracing::info!("Successfully published {} steps to ZeroMQ, marked as in-progress", results.len());
        Ok(results)
    }

    fn supports_batch_execution(&self) -> bool {
        true // ZeroMQ executor supports native batching
    }

    async fn execute_step_with_handler(
        &self,
        context: &StepExecutionContext,
        handler_class: &str,
        _handler_config: &HashMap<String, serde_json::Value>,
    ) -> Result<StepResult, OrchestrationError> {
        tracing::info!("Publishing single step {} to ZeroMQ (fire-and-forget)", context.step_id);

        // Build step request from context with real handler class
        let step_request = self.build_step_request(context, handler_class).await?;
        
        // Publish step batch (fire-and-forget)  
        let _batch_id = self.publish_batch(vec![step_request], Some(context)).await?;
        
        // Return in-progress result since publish succeeded
        // The actual result will come from the background result listener
        Ok(StepResult {
            step_id: context.step_id,
            status: StepStatus::InProgress, // Mark as in-progress after successful publish
            output: serde_json::Value::Null, // No output yet - will come from result listener
            execution_duration: Duration::from_millis(0), // Publish duration is negligible
            error_message: None,
            retry_after: None,
            error_code: None,
            error_context: None,
        })
    }
}

// Implement Send + Sync for thread safety
unsafe impl Send for ZmqPubSubExecutor {}
unsafe impl Sync for ZmqPubSubExecutor {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::step_handler::StepExecutionContext;
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
        let result = StepExecutionResult::success(
            123,
            serde_json::json!({"completed": true}),
            1500,
        );
        
        let response = StepBatchResponse::new("test_batch".to_string(), vec![result]);
        let response_json = serde_json::to_string(&response).expect("Should serialize");
        assert!(response_json.contains("completed"));
    }
}