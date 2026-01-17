//! # Handler Dispatch Service
//!
//! TAS-67: Non-blocking handler dispatch with bounded parallelism.
//!
//! This service receives step dispatch messages from a channel and invokes
//! handlers concurrently using a semaphore to bound parallelism. Completions
//! are sent to a separate channel for downstream processing.
//!
//! ## Non-Blocking Design
//!
//! The dispatch channel pattern ensures:
//! - StepExecutorActor never blocks waiting for handler completion
//! - Handlers execute concurrently up to the configured limit
//! - Completion flows through a separate channel
//!
//! ## Error Handling
//!
//! The service catches:
//! - Handler errors (converted to failure results)
//! - Handler panics (converted to failure results)
//! - Timeouts (converted to failure results)
//!
//! ## Post-Handler Callbacks
//!
//! The service supports optional post-handler callbacks via the `PostHandlerCallback` trait.
//! This enables language-specific extensions like domain event publishing without coupling
//! the shared infrastructure to specific implementations.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, error, info, warn};

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::base::TaskSequenceStep;

use super::traits::StepHandlerRegistry;
use crate::worker::actors::DispatchHandlerMessage;

/// TAS-75: Shared capacity checker for load shedding
///
/// This struct provides thread-safe access to handler capacity information,
/// allowing the StepExecutorActor to check capacity before claiming steps.
#[derive(Clone)]
pub struct CapacityChecker {
    /// Semaphore tracking concurrent handler executions
    semaphore: Arc<Semaphore>,
    /// Maximum concurrent handlers
    max_concurrent: usize,
    /// Load shedding configuration
    load_shedding: LoadSheddingConfig,
    /// Service ID for logging
    service_id: String,
}

impl CapacityChecker {
    /// Create a new capacity checker
    pub fn new(
        semaphore: Arc<Semaphore>,
        max_concurrent: usize,
        load_shedding: LoadSheddingConfig,
        service_id: String,
    ) -> Self {
        Self {
            semaphore,
            max_concurrent,
            load_shedding,
            service_id,
        }
    }

    /// Check if the service has capacity to handle more work
    ///
    /// Returns true if the service is below the configured capacity threshold.
    ///
    /// # Returns
    ///
    /// `(has_capacity, usage_percent)` - Whether capacity is available and current usage
    pub fn has_capacity(&self) -> (bool, f64) {
        if !self.load_shedding.enabled {
            return (true, 0.0);
        }

        let available = self.semaphore.available_permits();
        let in_use = self.max_concurrent.saturating_sub(available);
        let usage_percent = if self.max_concurrent > 0 {
            (in_use as f64 / self.max_concurrent as f64) * 100.0
        } else {
            0.0
        };

        // Log warning if at warning threshold
        if usage_percent >= self.load_shedding.warning_threshold_percent {
            warn!(
                service_id = %self.service_id,
                usage_percent = %usage_percent,
                available = available,
                max = self.max_concurrent,
                warning_threshold = %self.load_shedding.warning_threshold_percent,
                "Handler dispatch service approaching capacity"
            );
        }

        let has_capacity = usage_percent < self.load_shedding.capacity_threshold_percent;
        (has_capacity, usage_percent)
    }

    /// Get current usage statistics
    pub fn get_usage(&self) -> (usize, usize, f64) {
        let available = self.semaphore.available_permits();
        let in_use = self.max_concurrent.saturating_sub(available);
        let usage_percent = if self.max_concurrent > 0 {
            (in_use as f64 / self.max_concurrent as f64) * 100.0
        } else {
            0.0
        };
        (in_use, self.max_concurrent, usage_percent)
    }
}

impl std::fmt::Debug for CapacityChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapacityChecker")
            .field("service_id", &self.service_id)
            .field("max_concurrent", &self.max_concurrent)
            .field("load_shedding", &self.load_shedding)
            .finish()
    }
}

/// Trait for post-handler callbacks
///
/// Implementations can perform actions after a handler completes but before
/// the result is sent to the completion channel. This is useful for:
/// - Domain event publishing
/// - Metrics collection
/// - Audit logging
///
/// Callbacks are fire-and-forget: failures are logged but do not affect step completion.
#[async_trait]
pub trait PostHandlerCallback: Send + Sync + 'static {
    /// Called after a handler completes
    ///
    /// # Arguments
    /// * `step` - The step that was executed
    /// * `result` - The execution result
    /// * `worker_id` - The worker identifier
    async fn on_handler_complete(
        &self,
        step: &TaskSequenceStep,
        result: &StepExecutionResult,
        worker_id: &str,
    );

    /// Name of this callback for logging purposes
    fn name(&self) -> &str;
}

/// No-op callback implementation
#[derive(Debug)]
pub struct NoOpCallback;

#[async_trait]
impl PostHandlerCallback for NoOpCallback {
    async fn on_handler_complete(
        &self,
        _step: &TaskSequenceStep,
        _result: &StepExecutionResult,
        _worker_id: &str,
    ) {
        // No-op
    }

    fn name(&self) -> &str {
        "no-op"
    }
}

/// TAS-75: Load shedding configuration
#[derive(Debug, Clone)]
pub struct LoadSheddingConfig {
    /// Enable load shedding (refuse claims when at capacity)
    pub enabled: bool,
    /// Capacity threshold percentage (0-100) - refuse claims above this
    pub capacity_threshold_percent: f64,
    /// Warning threshold percentage (0-100) - log warnings above this
    pub warning_threshold_percent: f64,
}

impl Default for LoadSheddingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capacity_threshold_percent: 80.0,
            warning_threshold_percent: 70.0,
        }
    }
}

/// Configuration for the handler dispatch service
#[derive(Debug, Clone)]
pub struct HandlerDispatchConfig {
    /// Maximum number of concurrent handler executions
    pub max_concurrent_handlers: usize,
    /// Timeout for handler execution
    pub handler_timeout: Duration,
    /// Service identifier for logging
    pub service_id: String,
    /// TAS-75: Load shedding configuration
    pub load_shedding: LoadSheddingConfig,
}

impl Default for HandlerDispatchConfig {
    fn default() -> Self {
        Self {
            max_concurrent_handlers: 10,
            handler_timeout: Duration::from_secs(30),
            service_id: "handler-dispatch".to_string(),
            load_shedding: LoadSheddingConfig::default(),
        }
    }
}

impl HandlerDispatchConfig {
    /// Create config from TOML configuration values
    pub fn from_config(
        max_concurrent_handlers: usize,
        handler_timeout_ms: u64,
        service_id: String,
    ) -> Self {
        Self {
            max_concurrent_handlers,
            handler_timeout: Duration::from_millis(handler_timeout_ms),
            service_id,
            load_shedding: LoadSheddingConfig::default(),
        }
    }

    /// Create config with custom load shedding settings
    pub fn with_load_shedding(
        max_concurrent_handlers: usize,
        handler_timeout_ms: u64,
        service_id: String,
        load_shedding: LoadSheddingConfig,
    ) -> Self {
        Self {
            max_concurrent_handlers,
            handler_timeout: Duration::from_millis(handler_timeout_ms),
            service_id,
            load_shedding,
        }
    }
}

/// Handler dispatch service for non-blocking step execution
///
/// TAS-67: Receives dispatch messages and invokes handlers with bounded
/// parallelism. Results are sent to a completion channel.
///
/// ## Architecture
///
/// ```text
/// dispatch_receiver → [Semaphore] → handler.call() → [callback] → completion_sender
///                          │                              │
///                          └─→ Bounded to N concurrent    └─→ Domain events, metrics
///                               tasks
/// ```
pub struct HandlerDispatchService<R: StepHandlerRegistry, C: PostHandlerCallback = NoOpCallback> {
    /// Channel receiver for dispatch messages
    dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
    /// Channel sender for completion results
    completion_sender: mpsc::Sender<StepExecutionResult>,
    /// Handler registry for looking up handlers
    handler_registry: Arc<R>,
    /// Semaphore for bounding concurrent handler executions
    concurrency_semaphore: Arc<Semaphore>,
    /// Handler execution timeout
    handler_timeout: Duration,
    /// Service identifier for logging
    service_id: String,
    /// Optional post-handler callback for domain events, metrics, etc.
    post_handler_callback: Option<Arc<C>>,
}

impl<R: StepHandlerRegistry, C: PostHandlerCallback> std::fmt::Debug
    for HandlerDispatchService<R, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerDispatchService")
            .field("service_id", &self.service_id)
            .field("handler_timeout", &self.handler_timeout)
            .field("has_callback", &self.post_handler_callback.is_some())
            .finish()
    }
}

impl<R: StepHandlerRegistry + 'static> HandlerDispatchService<R, NoOpCallback> {
    /// Create a new handler dispatch service without a post-handler callback
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Channel to receive dispatch messages
    /// * `completion_sender` - Channel to send completion results
    /// * `handler_registry` - Registry for handler lookup
    /// * `config` - Service configuration
    ///
    /// # Returns
    ///
    /// Returns `(service, capacity_checker)` where the capacity_checker can be shared
    /// with StepExecutorActor for load shedding
    pub fn new(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        handler_registry: Arc<R>,
        config: HandlerDispatchConfig,
    ) -> (Self, CapacityChecker) {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_handlers));

        let capacity_checker = CapacityChecker::new(
            semaphore.clone(),
            config.max_concurrent_handlers,
            config.load_shedding.clone(),
            config.service_id.clone(),
        );

        let service = Self {
            dispatch_receiver,
            completion_sender,
            handler_registry,
            concurrency_semaphore: semaphore,
            handler_timeout: config.handler_timeout,
            service_id: config.service_id.clone(),
            post_handler_callback: None,
        };

        (service, capacity_checker)
    }

    /// TAS-75: Create a handler dispatch service with an external semaphore (no callback)
    ///
    /// Use this constructor when the semaphore is created externally (e.g., by WorkerActorRegistry)
    /// and shared with both StepExecutorActor (via CapacityChecker) and this service.
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Channel to receive dispatch messages
    /// * `completion_sender` - Channel to send completion results
    /// * `handler_registry` - Registry for handler lookup
    /// * `semaphore` - Pre-created semaphore shared with CapacityChecker
    /// * `config` - Service configuration
    pub fn with_semaphore(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        handler_registry: Arc<R>,
        semaphore: Arc<Semaphore>,
        config: HandlerDispatchConfig,
    ) -> Self {
        Self {
            dispatch_receiver,
            completion_sender,
            handler_registry,
            concurrency_semaphore: semaphore,
            handler_timeout: config.handler_timeout,
            service_id: config.service_id.clone(),
            post_handler_callback: None,
        }
    }
}

impl<R: StepHandlerRegistry + 'static, C: PostHandlerCallback> HandlerDispatchService<R, C> {
    /// Create a new handler dispatch service with a post-handler callback
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Channel to receive dispatch messages
    /// * `completion_sender` - Channel to send completion results
    /// * `handler_registry` - Registry for handler lookup
    /// * `config` - Service configuration
    /// * `callback` - Callback invoked after handler completion
    ///
    /// # Returns
    ///
    /// Returns `(service, capacity_checker)` where the capacity_checker can be shared
    /// with StepExecutorActor for load shedding
    pub fn with_callback(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        handler_registry: Arc<R>,
        config: HandlerDispatchConfig,
        callback: Arc<C>,
    ) -> (Self, CapacityChecker) {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_handlers));

        let capacity_checker = CapacityChecker::new(
            semaphore.clone(),
            config.max_concurrent_handlers,
            config.load_shedding.clone(),
            config.service_id.clone(),
        );

        let service = Self {
            dispatch_receiver,
            completion_sender,
            handler_registry,
            concurrency_semaphore: semaphore,
            handler_timeout: config.handler_timeout,
            service_id: config.service_id.clone(),
            post_handler_callback: Some(callback),
        };

        (service, capacity_checker)
    }

    /// TAS-75: Create a handler dispatch service with an external semaphore and callback
    ///
    /// Use this constructor when the semaphore is created externally (e.g., by WorkerActorRegistry)
    /// and shared with both StepExecutorActor (via CapacityChecker) and this service.
    ///
    /// # Arguments
    ///
    /// * `dispatch_receiver` - Channel to receive dispatch messages
    /// * `completion_sender` - Channel to send completion results
    /// * `handler_registry` - Registry for handler lookup
    /// * `semaphore` - Pre-created semaphore shared with CapacityChecker
    /// * `config` - Service configuration
    /// * `callback` - Callback invoked after handler completion
    pub fn with_semaphore_and_callback(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        handler_registry: Arc<R>,
        semaphore: Arc<Semaphore>,
        config: HandlerDispatchConfig,
        callback: Arc<C>,
    ) -> Self {
        Self {
            dispatch_receiver,
            completion_sender,
            handler_registry,
            concurrency_semaphore: semaphore,
            handler_timeout: config.handler_timeout,
            service_id: config.service_id.clone(),
            post_handler_callback: Some(callback),
        }
    }

    /// Run the dispatch service
    ///
    /// This method runs until the dispatch channel is closed. Each received
    /// message spawns a task (bounded by the semaphore) to execute the handler.
    pub async fn run(mut self) {
        info!(
            service_id = %self.service_id,
            "Handler dispatch service starting"
        );

        while let Some(msg) = self.dispatch_receiver.recv().await {
            debug!(
                service_id = %self.service_id,
                step_uuid = %msg.step_uuid,
                event_id = %msg.event_id,
                "Received dispatch message"
            );

            // Clone what we need for the spawned task
            let registry = self.handler_registry.clone();
            let sender = self.completion_sender.clone();
            let timeout = self.handler_timeout;
            let service_id = self.service_id.clone();
            let semaphore = self.concurrency_semaphore.clone();
            let callback = self.post_handler_callback.clone();

            // Spawn a task for this handler execution
            tokio::spawn(async move {
                // Acquire semaphore permit (bounds concurrency)
                let permit = match semaphore.acquire().await {
                    Ok(permit) => permit,
                    Err(_) => {
                        // TAS-67 Risk Mitigation: Generate failure result on semaphore closure
                        // instead of silent exit, so orchestration knows the step failed
                        error!(
                            service_id = %service_id,
                            step_uuid = %msg.step_uuid,
                            "Semaphore closed - cannot execute handler, generating failure result"
                        );

                        let failure_result = StepExecutionResult::failure(
                            msg.step_uuid,
                            "Handler execution failed - semaphore closed during acquisition"
                                .to_string(),
                            None,                                             // error_code
                            Some("semaphore_acquisition_failed".to_string()), // error_type
                            true, // retryable - infrastructure issue, retry may succeed
                            0,    // execution_time_ms
                            Some(std::collections::HashMap::from([
                                ("service_id".to_string(), serde_json::json!(service_id)),
                                ("reason".to_string(), serde_json::json!("semaphore closed")),
                                (
                                    "task_uuid".to_string(),
                                    serde_json::json!(msg.task_uuid.to_string()),
                                ),
                            ])),
                        );

                        // Send failure result to completion channel
                        match sender.send(failure_result.clone()).await {
                            Ok(()) => {
                                debug!(
                                    service_id = %service_id,
                                    step_uuid = %msg.step_uuid,
                                    "Semaphore failure result sent to completion channel"
                                );

                                // Invoke callback after successful send
                                if let Some(ref cb) = callback {
                                    cb.on_handler_complete(
                                        &msg.task_sequence_step,
                                        &failure_result,
                                        &service_id,
                                    )
                                    .await;
                                }
                            }
                            Err(e) => {
                                error!(
                                    service_id = %service_id,
                                    step_uuid = %msg.step_uuid,
                                    error = %e,
                                    "Failed to send semaphore failure result - channel closed"
                                );
                            }
                        }
                        return;
                    }
                };

                debug!(
                    service_id = %service_id,
                    step_uuid = %msg.step_uuid,
                    "Acquired semaphore permit, executing handler"
                );

                // Execute handler with timeout and panic catching
                let result =
                    Self::execute_with_timeout(&registry, &msg, timeout, &service_id).await;

                // TAS-67 Risk Mitigation: Release permit BEFORE sending to completion channel
                // This prevents backpressure cascade where blocked sends hold semaphore permits
                drop(permit);

                // 1. Send result to completion channel FIRST
                // This ensures the result is committed to the pipeline before domain events fire
                match sender.send(result.clone()).await {
                    Ok(()) => {
                        debug!(
                            service_id = %service_id,
                            step_uuid = %msg.step_uuid,
                            "Completion result sent to channel"
                        );

                        // 2. Invoke post-handler callback AFTER successful send
                        // Domain events only fire after the result is committed to the pipeline
                        if let Some(ref cb) = callback {
                            debug!(
                                service_id = %service_id,
                                step_uuid = %msg.step_uuid,
                                callback_name = cb.name(),
                                "Invoking post-handler callback"
                            );
                            cb.on_handler_complete(&msg.task_sequence_step, &result, &service_id)
                                .await;
                        }
                    }
                    Err(e) => {
                        error!(
                            service_id = %service_id,
                            step_uuid = %msg.step_uuid,
                            error = %e,
                            "Failed to send completion result - channel closed, domain events NOT fired"
                        );
                    }
                }
            });
        }

        info!(
            service_id = %self.service_id,
            "Handler dispatch service stopped - channel closed"
        );
    }

    /// Execute a handler with timeout and panic catching
    async fn execute_with_timeout(
        registry: &Arc<R>,
        msg: &DispatchHandlerMessage,
        timeout: Duration,
        service_id: &str,
    ) -> StepExecutionResult {
        let step_uuid = msg.step_uuid;
        let task_uuid = msg.task_uuid;

        // Look up handler
        let handler = match registry.get(&msg.task_sequence_step).await {
            Some(h) => h,
            None => {
                let handler_callable = &msg.task_sequence_step.step_definition.handler.callable;
                error!(
                    service_id = %service_id,
                    step_uuid = %step_uuid,
                    handler_callable = %handler_callable,
                    "Handler not found"
                );
                return StepExecutionResult::failure(
                    step_uuid,
                    format!("Handler not found: {handler_callable}"),
                    None,                                  // error_code
                    Some("handler_not_found".to_string()), // error_type
                    false,                                 // retryable
                    0,                                     // execution_time_ms
                    None,                                  // context
                );
            }
        };

        let handler_name = handler.name().to_string();
        debug!(
            service_id = %service_id,
            step_uuid = %step_uuid,
            handler_name = %handler_name,
            "Invoking handler"
        );

        let start_time = std::time::Instant::now();

        // Execute with timeout and panic catching
        let execution_result = tokio::time::timeout(
            timeout,
            AssertUnwindSafe(handler.call(&msg.task_sequence_step)).catch_unwind(),
        )
        .await;

        let execution_time_ms = start_time.elapsed().as_millis() as i64;

        match execution_result {
            // Success within timeout
            Ok(Ok(Ok(step_result))) => {
                debug!(
                    service_id = %service_id,
                    step_uuid = %step_uuid,
                    handler_name = %handler_name,
                    success = step_result.success,
                    execution_time_ms = execution_time_ms,
                    "Handler completed"
                );
                step_result
            }
            // Handler returned an error
            Ok(Ok(Err(handler_error))) => {
                error!(
                    service_id = %service_id,
                    step_uuid = %step_uuid,
                    handler_name = %handler_name,
                    error = %handler_error,
                    execution_time_ms = execution_time_ms,
                    "Handler returned error"
                );
                StepExecutionResult::failure(
                    step_uuid,
                    handler_error.to_string(),
                    None,                              // error_code
                    Some("handler_error".to_string()), // error_type
                    true, // retryable - handler errors may be transient
                    execution_time_ms,
                    Some(std::collections::HashMap::from([
                        ("handler_name".to_string(), serde_json::json!(handler_name)),
                        (
                            "task_uuid".to_string(),
                            serde_json::json!(task_uuid.to_string()),
                        ),
                    ])),
                )
            }
            // Handler panicked
            Ok(Err(panic_error)) => {
                let panic_msg = if let Some(s) = panic_error.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_error.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                error!(
                    service_id = %service_id,
                    step_uuid = %step_uuid,
                    handler_name = %handler_name,
                    panic_msg = %panic_msg,
                    execution_time_ms = execution_time_ms,
                    "Handler panicked"
                );
                StepExecutionResult::failure(
                    step_uuid,
                    format!("Handler panicked: {panic_msg}"),
                    None,                              // error_code
                    Some("handler_panic".to_string()), // error_type
                    false,                             // retryable - panics are not retryable
                    execution_time_ms,
                    Some(std::collections::HashMap::from([
                        ("handler_name".to_string(), serde_json::json!(handler_name)),
                        (
                            "task_uuid".to_string(),
                            serde_json::json!(task_uuid.to_string()),
                        ),
                    ])),
                )
            }
            // Timeout
            Err(_) => {
                let elapsed_ms = start_time.elapsed().as_millis() as i64;
                error!(
                    service_id = %service_id,
                    step_uuid = %step_uuid,
                    handler_name = %handler_name,
                    timeout_ms = timeout.as_millis(),
                    "Handler timed out"
                );
                StepExecutionResult::failure(
                    step_uuid,
                    format!("Handler timed out after {}ms", timeout.as_millis()),
                    None,                                // error_code
                    Some("handler_timeout".to_string()), // error_type
                    true,                                // retryable - timeouts may be transient
                    elapsed_ms,
                    Some(std::collections::HashMap::from([
                        ("handler_name".to_string(), serde_json::json!(handler_name)),
                        (
                            "task_uuid".to_string(),
                            serde_json::json!(task_uuid.to_string()),
                        ),
                        (
                            "timeout_ms".to_string(),
                            serde_json::json!(timeout.as_millis()),
                        ),
                    ])),
                )
            }
        }
    }
}

/// Create dispatch and completion channels for the handler dispatch service
///
/// Returns (dispatch_sender, dispatch_receiver, completion_sender, completion_receiver)
pub fn create_dispatch_channels(
    dispatch_buffer_size: usize,
    completion_buffer_size: usize,
) -> (
    mpsc::Sender<DispatchHandlerMessage>,
    mpsc::Receiver<DispatchHandlerMessage>,
    mpsc::Sender<StepExecutionResult>,
    mpsc::Receiver<StepExecutionResult>,
) {
    let (dispatch_tx, dispatch_rx) = mpsc::channel(dispatch_buffer_size);
    let (completion_tx, completion_rx) = mpsc::channel(completion_buffer_size);
    (dispatch_tx, dispatch_rx, completion_tx, completion_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::handlers::traits::StepHandler;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use tasker_shared::TaskerResult;
    use uuid::Uuid;

    // Test handler for integration tests
    #[expect(
        dead_code,
        reason = "Test handler used for dispatch service integration tests"
    )]
    struct TestHandler {
        name: String,
        delay_ms: u64,
        should_fail: bool,
        should_panic: bool,
    }

    #[async_trait]
    impl StepHandler for TestHandler {
        async fn call(&self, _step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }

            if self.should_panic {
                panic!("Test panic!");
            }

            if self.should_fail {
                return Err(tasker_shared::TaskerError::WorkerError(
                    "Test error".to_string(),
                ));
            }

            Ok(StepExecutionResult::success(
                Uuid::new_v4(),
                serde_json::json!({"handled_by": self.name}),
                0,    // execution_time_ms
                None, // custom_metadata
            ))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    // Test registry for integration tests
    struct TestRegistry {
        handlers: RwLock<HashMap<String, Arc<dyn StepHandler>>>,
    }

    impl TestRegistry {
        #[expect(
            dead_code,
            reason = "Test constructor used for dispatch service integration tests"
        )]
        fn new() -> Self {
            Self {
                handlers: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StepHandlerRegistry for TestRegistry {
        async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
            let handlers = self.handlers.read().unwrap();
            handlers
                .get(&step.step_definition.handler.callable)
                .cloned()
        }

        fn register(&self, name: &str, handler: Arc<dyn StepHandler>) {
            let mut handlers = self.handlers.write().unwrap();
            handlers.insert(name.to_string(), handler);
        }

        fn handler_available(&self, name: &str) -> bool {
            let handlers = self.handlers.read().unwrap();
            handlers.contains_key(name)
        }

        fn registered_handlers(&self) -> Vec<String> {
            let handlers = self.handlers.read().unwrap();
            handlers.keys().cloned().collect()
        }
    }

    #[test]
    fn test_dispatch_config_default() {
        let config = HandlerDispatchConfig::default();
        assert_eq!(config.max_concurrent_handlers, 10);
        assert_eq!(config.handler_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_dispatch_config_from_config() {
        let config = HandlerDispatchConfig::from_config(20, 5000, "test-service".to_string());
        assert_eq!(config.max_concurrent_handlers, 20);
        assert_eq!(config.handler_timeout, Duration::from_millis(5000));
        assert_eq!(config.service_id, "test-service");
    }

    #[test]
    fn test_create_dispatch_channels() {
        let (dispatch_tx, _dispatch_rx, completion_tx, _completion_rx) =
            create_dispatch_channels(100, 100);

        // Verify channels are bounded
        assert!(!dispatch_tx.is_closed());
        assert!(!completion_tx.is_closed());
    }

    #[test]
    fn test_handler_dispatch_service_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<HandlerDispatchService<TestRegistry>>();
    }
}
