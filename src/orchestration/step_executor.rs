//! # Step Executor
//!
//! ## Architecture: Delegation-Based Step Execution with Performance Monitoring
//!
//! The StepExecutor handles individual workflow step execution within the orchestration core,
//! following the delegation pattern where Rust coordinates execution while frameworks handle
//! the actual business logic implementation.
//!
//! ## Key Features
//!
//! - **Concurrent execution**: Parallel step processing with proper resource management
//! - **Performance monitoring**: Detailed execution timing and metrics collection
//! - **State coordination**: Integration with StateManager for atomic state transitions
//! - **Event integration**: Real-time execution events through EventPublisher
//! - **Error handling**: Comprehensive error recovery and retry coordination
//! - **Timeout management**: Configurable step execution timeouts
//!
//! ## Integration with Orchestration Core
//!
//! The StepExecutor serves as the execution engine that coordinates with:
//! - **ViableStepDiscovery**: Receives steps ready for execution
//! - **StateManager**: Manages step state transitions (pending → in_progress → complete/error)
//! - **TaskHandlerRegistry**: Resolves appropriate handlers for step execution
//! - **EventPublisher**: Publishes execution lifecycle events
//! - **Framework Integration**: Delegates actual execution through FrameworkIntegration trait
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::step_executor::StepExecutor;
//! use tasker_core::orchestration::state_manager::StateManager;
//! use tasker_core::orchestration::event_publisher::EventPublisher;
//! use tasker_core::registry::TaskHandlerRegistry;
//! use tasker_core::database::sql_functions::SqlFunctionExecutor;
//! use sqlx::PgPool;
//!
//! # tokio_test::block_on(async {
//! let pool = PgPool::connect("postgresql://localhost/test_db").await.unwrap();
//! let sql_executor = SqlFunctionExecutor::new(pool.clone());
//! let event_publisher = EventPublisher::new();
//! let state_manager = StateManager::new(sql_executor, event_publisher.clone(), pool.clone());
//! let registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());
//! let step_executor = StepExecutor::new(state_manager, registry, event_publisher);
//!
//! // StepExecutor coordinates step execution through delegation
//! // Actual execution happens in framework-specific handlers
//! let _executor = step_executor;
//! # });
//! ```

use crate::orchestration::errors::{ExecutionError, OrchestrationResult};
use crate::orchestration::event_publisher::EventPublisher;
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::types::{
    FrameworkIntegration, StepResult, StepStatus, TaskContext, ViableStep,
};
use crate::registry::TaskHandlerRegistry;
use crate::state_machine::events::StepEvent;
use fastrand;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Configuration for step execution
#[derive(Debug, Clone)]
pub struct StepExecutionConfig {
    /// Maximum number of concurrent step executions
    pub max_concurrent_steps: usize,
    /// Default timeout for step execution
    pub default_timeout: Duration,
    /// Maximum timeout allowed for any step
    pub max_timeout: Duration,
    /// Enable detailed performance metrics collection
    pub enable_metrics: bool,
    /// Retry configuration
    pub retry_config: RetryConfig,
}

/// Retry configuration for step execution
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay between retries
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Add jitter to retry delays
    pub jitter: bool,
}

/// Execution metrics for performance monitoring
#[derive(Debug, Clone)]
pub struct StepExecutionMetrics {
    pub step_id: i64,
    pub task_id: i64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub execution_duration: Option<Duration>,
    pub queue_duration: Duration,
    pub attempt_number: u32,
    pub final_status: Option<StepStatus>,
    pub error_details: Option<String>,
}

/// Step execution request with context
#[derive(Debug, Clone)]
pub struct StepExecutionRequest {
    pub step: ViableStep,
    pub task_context: TaskContext,
    pub timeout: Option<Duration>,
    pub priority: ExecutionPriority,
    pub retry_attempt: u32,
}

/// Execution priority for step scheduling
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ExecutionPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// Step executor for concurrent workflow step execution
pub struct StepExecutor {
    state_manager: StateManager,
    registry: TaskHandlerRegistry,
    event_publisher: EventPublisher,
    config: StepExecutionConfig,
    /// Semaphore to control concurrent execution
    execution_semaphore: Arc<Semaphore>,
    /// Active execution metrics
    active_executions: Arc<tokio::sync::Mutex<HashMap<i64, StepExecutionMetrics>>>,
}

impl StepExecutor {
    /// Create new step executor with default configuration
    pub fn new(
        state_manager: StateManager,
        registry: TaskHandlerRegistry,
        event_publisher: EventPublisher,
    ) -> Self {
        let config = StepExecutionConfig::default();
        Self::with_config(state_manager, registry, event_publisher, config)
    }

    /// Create new step executor with custom configuration
    pub fn with_config(
        state_manager: StateManager,
        registry: TaskHandlerRegistry,
        event_publisher: EventPublisher,
        config: StepExecutionConfig,
    ) -> Self {
        let execution_semaphore = Arc::new(Semaphore::new(config.max_concurrent_steps));

        Self {
            state_manager,
            registry,
            event_publisher,
            execution_semaphore,
            config,
            active_executions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Execute a single step with comprehensive monitoring and state management
    #[instrument(skip(self, framework), fields(step_id = request.step.step_id, task_id = request.step.task_id))]
    pub async fn execute_step(
        &self,
        request: StepExecutionRequest,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> OrchestrationResult<StepResult> {
        let step_id = request.step.step_id;
        let task_id = request.step.task_id;

        debug!(
            step_id = step_id,
            task_id = task_id,
            attempt = request.retry_attempt,
            "Starting step execution"
        );

        // Phase 1: Setup and resource acquisition
        let (_permit, mut metrics) = self.setup_execution(&request).await?;

        // Phase 2: State validation and transition
        self.validate_and_transition_step_state(step_id).await?;

        // Phase 3: Publish start event
        self.event_publisher
            .publish_step_execution_started(step_id, task_id, &request.step.name)
            .await?;

        // Phase 4: Execute step with timeout
        let execution_result = self.execute_step_with_timeout(&request, framework).await;

        // Phase 5: Process execution result
        let step_result = self
            .process_execution_result(execution_result, &request, &mut metrics)
            .await;

        // Phase 6: State transition and cleanup
        self.finalize_execution(step_id, task_id, &step_result, &metrics)
            .await?;

        Ok(step_result)
    }

    /// Execute multiple steps concurrently with proper resource management
    #[instrument(skip(self, framework), fields(step_count = requests.len()))]
    pub async fn execute_steps_concurrent(
        &self,
        requests: Vec<StepExecutionRequest>,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> OrchestrationResult<Vec<StepResult>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            step_count = requests.len(),
            "Starting concurrent step execution"
        );

        // Sort requests by priority (highest first)
        let mut sorted_requests = requests;
        sorted_requests.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Execute steps concurrently with tokio::spawn
        let mut handles = Vec::new();
        for request in sorted_requests {
            let executor = self.clone();
            let framework_clone = framework.clone();

            let handle =
                tokio::spawn(async move { executor.execute_step(request, framework_clone).await });
            handles.push(handle);
        }

        // Collect results
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result?),
                Err(e) => {
                    error!(error = %e, "Step execution task panicked");
                    return Err(ExecutionError::ConcurrencyError {
                        step_id: 0, // Unknown step_id due to panic
                        reason: format!("Task panicked: {e}"),
                    }
                    .into());
                }
            }
        }

        info!(
            executed_steps = results.len(),
            successful_steps = results
                .iter()
                .filter(|r| r.status == StepStatus::Completed)
                .count(),
            failed_steps = results
                .iter()
                .filter(|r| r.status == StepStatus::Failed)
                .count(),
            "Concurrent step execution completed"
        );

        Ok(results)
    }

    /// Get current execution statistics
    pub async fn get_execution_stats(&self) -> ExecutionStats {
        let active = self.active_executions.lock().await;
        let available_permits = self.execution_semaphore.available_permits();

        ExecutionStats {
            active_executions: active.len(),
            available_capacity: available_permits,
            max_concurrent_steps: self.config.max_concurrent_steps,
            total_capacity_used: self.config.max_concurrent_steps - available_permits,
        }
    }

    /// Setup execution environment and acquire resources
    async fn setup_execution(
        &self,
        request: &StepExecutionRequest,
    ) -> OrchestrationResult<(tokio::sync::SemaphorePermit, StepExecutionMetrics)> {
        let step_id = request.step.step_id;
        let task_id = request.step.task_id;

        // Start execution metrics tracking
        let queue_start = Instant::now();
        let mut metrics = StepExecutionMetrics {
            step_id,
            task_id,
            started_at: chrono::Utc::now(),
            completed_at: None,
            execution_duration: None,
            queue_duration: Duration::default(),
            attempt_number: request.retry_attempt,
            final_status: None,
            error_details: None,
        };

        // Acquire execution semaphore (controls concurrency)
        let permit = self.execution_semaphore.acquire().await.map_err(|e| {
            ExecutionError::ConcurrencyError {
                step_id,
                reason: e.to_string(),
            }
        })?;

        metrics.queue_duration = queue_start.elapsed();

        // Track active execution
        if self.config.enable_metrics {
            let mut active = self.active_executions.lock().await;
            active.insert(step_id, metrics.clone());
        }

        Ok((permit, metrics))
    }

    /// Validate step state and transition to in_progress
    async fn validate_and_transition_step_state(&self, step_id: i64) -> OrchestrationResult<()> {
        let state_result = self
            .state_manager
            .evaluate_step_state(step_id)
            .await
            .map_err(|e| ExecutionError::StateTransitionError {
                step_id,
                reason: e.to_string(),
            })?;

        if !state_result.transition_required
            || !matches!(
                state_result.recommended_state.as_deref(),
                Some("in_progress")
            )
        {
            return Err(ExecutionError::InvalidStepState {
                step_id,
                current_state: state_result.current_state,
                expected_state: "pending".to_string(),
            }
            .into());
        }

        Ok(())
    }

    /// Execute step with timeout handling
    async fn execute_step_with_timeout(
        &self,
        request: &StepExecutionRequest,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> (
        Result<OrchestrationResult<StepResult>, tokio::time::error::Elapsed>,
        Duration,
        Duration,
    ) {
        let execution_start = Instant::now();
        let execution_timeout = request
            .timeout
            .unwrap_or(self.config.default_timeout)
            .min(self.config.max_timeout);

        let execution_result = timeout(
            execution_timeout,
            self.execute_step_internal(request, framework),
        )
        .await;

        let execution_duration = execution_start.elapsed();

        (execution_result, execution_duration, execution_timeout)
    }

    /// Process execution result and create StepResult
    async fn process_execution_result(
        &self,
        execution_result: (
            Result<OrchestrationResult<StepResult>, tokio::time::error::Elapsed>,
            Duration,
            Duration,
        ),
        request: &StepExecutionRequest,
        metrics: &mut StepExecutionMetrics,
    ) -> StepResult {
        let (result, execution_duration, execution_timeout) = execution_result;
        let step_id = request.step.step_id;
        let task_id = request.step.task_id;

        metrics.execution_duration = Some(execution_duration);
        metrics.completed_at = Some(chrono::Utc::now());

        match result {
            Ok(Ok(step_result)) => {
                metrics.final_status = Some(step_result.status.clone());
                debug!(
                    step_id = step_id,
                    duration_ms = execution_duration.as_millis(),
                    status = ?step_result.status,
                    "Step execution completed successfully"
                );
                step_result
            }
            Ok(Err(e)) => self.create_error_result(
                step_id,
                task_id,
                e.to_string(),
                execution_duration,
                request.retry_attempt,
                "EXECUTION_ERROR",
                metrics,
            ),
            Err(_) => {
                // Timeout occurred
                let error_message = format!("Step execution timed out after {execution_timeout:?}");
                self.create_timeout_result(
                    step_id,
                    task_id,
                    error_message,
                    execution_duration,
                    execution_timeout,
                    request.retry_attempt,
                    metrics,
                )
            }
        }
    }

    /// Create error result for failed execution
    fn create_error_result(
        &self,
        step_id: i64,
        _task_id: i64,
        error_message: String,
        execution_duration: Duration,
        retry_attempt: u32,
        error_code: &str,
        metrics: &mut StepExecutionMetrics,
    ) -> StepResult {
        metrics.final_status = Some(StepStatus::Failed);
        metrics.error_details = Some(error_message.clone());

        warn!(
            step_id = step_id,
            error = %error_message,
            duration_ms = execution_duration.as_millis(),
            "Step execution failed"
        );

        StepResult {
            step_id,
            status: StepStatus::Failed,
            output: serde_json::json!({
                "error": error_message,
                "execution_duration_ms": execution_duration.as_millis()
            }),
            execution_duration,
            error_message: Some(error_message),
            retry_after: self.calculate_retry_delay(retry_attempt),
            error_code: Some(error_code.to_string()),
            error_context: Some(HashMap::from([
                ("step_id".to_string(), serde_json::json!(step_id)),
                ("task_id".to_string(), serde_json::json!(_task_id)),
                ("attempt".to_string(), serde_json::json!(retry_attempt)),
            ])),
        }
    }

    /// Create timeout result for timed out execution
    fn create_timeout_result(
        &self,
        step_id: i64,
        _task_id: i64,
        error_message: String,
        execution_duration: Duration,
        execution_timeout: Duration,
        retry_attempt: u32,
        metrics: &mut StepExecutionMetrics,
    ) -> StepResult {
        metrics.final_status = Some(StepStatus::Failed);
        metrics.error_details = Some(error_message.clone());

        error!(
            step_id = step_id,
            timeout_ms = execution_timeout.as_millis(),
            "Step execution timed out"
        );

        StepResult {
            step_id,
            status: StepStatus::Failed,
            output: serde_json::json!({
                "error": "execution_timeout",
                "timeout_duration_ms": execution_timeout.as_millis(),
                "execution_duration_ms": execution_duration.as_millis()
            }),
            execution_duration,
            error_message: Some(error_message),
            retry_after: self.calculate_retry_delay(retry_attempt),
            error_code: Some("EXECUTION_TIMEOUT".to_string()),
            error_context: Some(HashMap::from([
                ("step_id".to_string(), serde_json::json!(step_id)),
                (
                    "timeout_ms".to_string(),
                    serde_json::json!(execution_timeout.as_millis()),
                ),
            ])),
        }
    }

    /// Finalize execution with state transitions and cleanup
    async fn finalize_execution(
        &self,
        step_id: i64,
        task_id: i64,
        step_result: &StepResult,
        metrics: &StepExecutionMetrics,
    ) -> OrchestrationResult<()> {
        // Update state based on execution result
        let _state_event = match step_result.status {
            StepStatus::Completed => StepEvent::Complete(Some(step_result.output.clone())),
            StepStatus::Failed => StepEvent::Fail(
                step_result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ),
            StepStatus::Retrying => StepEvent::Fail("Retrying after failure".to_string()),
            StepStatus::Skipped => StepEvent::Complete(Some(serde_json::json!({"skipped": true}))),
        };

        // Transition step state
        if let Err(e) = self.state_manager.evaluate_step_state(step_id).await {
            warn!(
                step_id = step_id,
                error = %e,
                "Failed to update step state after execution"
            );
        }

        // Publish step execution completed event
        self.event_publisher
            .publish_step_execution_completed(step_id, task_id, step_result.clone())
            .await?;

        // Clean up active execution tracking
        if self.config.enable_metrics {
            let mut active = self.active_executions.lock().await;
            active.remove(&step_id);
        }

        info!(
            step_id = step_id,
            task_id = task_id,
            status = ?step_result.status,
            duration_ms = metrics.execution_duration.map(|d| d.as_millis()).unwrap_or(0),
            queue_duration_ms = metrics.queue_duration.as_millis(),
            "Step execution completed"
        );

        Ok(())
    }

    /// Internal step execution logic
    async fn execute_step_internal(
        &self,
        request: &StepExecutionRequest,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> OrchestrationResult<StepResult> {
        let step_id = request.step.step_id;
        let task_id = request.step.task_id;

        // 1. Get task context for step execution
        let task_context = framework.get_task_context(task_id).await.map_err(|e| {
            ExecutionError::StateTransitionError {
                step_id,
                reason: format!("Failed to get task context: {e}"),
            }
        })?;

        // 2. Resolve step handler through registry
        // TODO: In a full implementation, this would:
        // - Look up the handler class from the step configuration
        // - Resolve the handler through the registry
        // - Call handler.process() with the task context
        // - Call handler.process_results() with the execution results

        // 3. Execute the single step through framework delegation
        let step_result = framework
            .execute_single_step(&request.step, &task_context)
            .await
            .map_err(|e| ExecutionError::StepExecutionFailed {
                step_id,
                reason: e.to_string(),
                error_code: Some("FRAMEWORK_EXECUTION_ERROR".to_string()),
            })?;

        // 4. Validate that we got a result for the correct step
        if step_result.step_id != step_id {
            return Err(ExecutionError::StepExecutionFailed {
                step_id,
                reason: format!(
                    "Framework returned result for step {} but expected step {}",
                    step_result.step_id, step_id
                ),
                error_code: Some("STEP_ID_MISMATCH".to_string()),
            }
            .into());
        }

        Ok(step_result)
    }

    /// Calculate retry delay based on attempt number
    fn calculate_retry_delay(&self, attempt: u32) -> Option<Duration> {
        if attempt >= self.config.retry_config.max_attempts {
            return None;
        }

        let base_delay = self.config.retry_config.base_delay;
        let multiplier = self.config.retry_config.backoff_multiplier;
        let max_delay = self.config.retry_config.max_delay;

        let delay = base_delay.mul_f64(multiplier.powi(attempt as i32));
        let delay = delay.min(max_delay);

        if self.config.retry_config.jitter {
            let jitter = fastrand::f64() * 0.1; // 10% jitter
            let jittered_delay = delay.mul_f64(1.0 + jitter);
            Some(jittered_delay.min(max_delay))
        } else {
            Some(delay)
        }
    }
}

// Clone implementation for concurrent usage
impl Clone for StepExecutor {
    fn clone(&self) -> Self {
        Self {
            state_manager: self.state_manager.clone(),
            registry: self.registry.clone(),
            event_publisher: self.event_publisher.clone(),
            config: self.config.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
            active_executions: self.active_executions.clone(),
        }
    }
}

/// Current execution statistics
#[derive(Debug, Clone)]
pub struct ExecutionStats {
    pub active_executions: usize,
    pub available_capacity: usize,
    pub max_concurrent_steps: usize,
    pub total_capacity_used: usize,
}

impl Default for StepExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_steps: 10,
            default_timeout: Duration::from_secs(300), // 5 minutes
            max_timeout: Duration::from_secs(3600),    // 1 hour
            enable_metrics: true,
            retry_config: RetryConfig::default(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}
