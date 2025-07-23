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
//! use tasker_core::orchestration::task_config_finder::TaskConfigFinder;
//! use tasker_core::orchestration::config::ConfigurationManager;
//! use tasker_core::events::publisher::EventPublisher;
//! use tasker_core::registry::TaskHandlerRegistry;
//! use tasker_core::database::sql_functions::SqlFunctionExecutor;
//! use sqlx::PgPool;
//! use std::sync::Arc;
//!
//! # tokio_test::block_on(async {
//! let pool = PgPool::connect("postgresql://localhost/test_db").await.unwrap();
//! let sql_executor = SqlFunctionExecutor::new(pool.clone());
//! let event_publisher = EventPublisher::new();
//! let state_manager = StateManager::new(sql_executor, event_publisher.clone(), pool.clone());
//! let registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());
//! let config_manager = Arc::new(ConfigurationManager::new());
//! let registry_arc = Arc::new(registry.clone());
//! let task_config_finder = TaskConfigFinder::new(config_manager, registry_arc);
//! let step_executor = StepExecutor::new(state_manager, registry, event_publisher, task_config_finder);
//!
//! // StepExecutor coordinates step execution through delegation
//! // Actual execution happens in framework-specific handlers
//! let _executor = step_executor;
//! # });
//! ```

use crate::events::{EventPublisher, StepResult as EventsStepResult};
use crate::orchestration::backoff_calculator::{BackoffCalculator, BackoffContext};
use crate::orchestration::errors::{ExecutionError, OrchestrationError, OrchestrationResult};
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::task_config_finder::TaskConfigFinder;
use crate::orchestration::types::{
    FrameworkIntegration, StepResult, StepStatus, TaskContext, ViableStep,
};
use crate::registry::TaskHandlerRegistry;
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
    backoff_calculator: BackoffCalculator,
    config: StepExecutionConfig,
    /// Task configuration finder for loading task templates
    task_config_finder: TaskConfigFinder,
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
        task_config_finder: TaskConfigFinder,
    ) -> Self {
        let config = StepExecutionConfig::default();
        Self::with_config(
            state_manager,
            registry,
            event_publisher,
            task_config_finder,
            config,
        )
    }

    /// Create new step executor with custom configuration
    pub fn with_config(
        state_manager: StateManager,
        registry: TaskHandlerRegistry,
        event_publisher: EventPublisher,
        task_config_finder: TaskConfigFinder,
        config: StepExecutionConfig,
    ) -> Self {
        let execution_semaphore = Arc::new(Semaphore::new(config.max_concurrent_steps));

        // Get pool from state_manager for BackoffCalculator
        let pool = state_manager.pool().clone();
        let backoff_calculator = BackoffCalculator::with_defaults(pool);

        Self {
            state_manager,
            registry,
            event_publisher,
            backoff_calculator,
            config,
            task_config_finder,
            execution_semaphore,
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

        // Execute steps sequentially to maintain Ruby thread context
        // This ensures all Ruby FFI calls happen on the same thread as the LocalSet
        let mut results = Vec::new();
        for request in sorted_requests {
            let result = self.execute_step(request, framework.clone()).await?;
            results.push(result);
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

        // Step timeout resolution order:
        // 1. Request-specific timeout (if provided)
        // 2. Step template timeout_seconds (when handler config integration is added)
        // 3. Global default timeout
        // Always capped by max_timeout for safety
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

                // Clear backoff for successful step completion
                if let Err(e) = self.backoff_calculator.clear_backoff(step_id).await {
                    warn!(
                        "Failed to clear backoff for successful step {}: {}",
                        step_id, e
                    );
                }

                step_result
            }
            Ok(Err(e)) => {
                self.create_error_result(
                    step_id,
                    task_id,
                    e.to_string(),
                    execution_duration,
                    request.retry_attempt,
                    "EXECUTION_ERROR",
                    metrics,
                )
                .await
            }
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
                .await
            }
        }
    }

    /// Create error result for failed execution
    async fn create_error_result(
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
            error_message: Some(error_message.clone()),
            retry_after: self
                .calculate_retry_delay(step_id, retry_attempt, Some(error_message))
                .await,
            error_code: Some(error_code.to_string()),
            error_context: Some(HashMap::from([
                ("step_id".to_string(), serde_json::json!(step_id)),
                ("task_id".to_string(), serde_json::json!(_task_id)),
                ("attempt".to_string(), serde_json::json!(retry_attempt)),
            ])),
        }
    }

    /// Create timeout result for timed out execution
    async fn create_timeout_result(
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
            error_message: Some(error_message.clone()),
            retry_after: self
                .calculate_retry_delay(step_id, retry_attempt, Some(error_message))
                .await,
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
        // Update state based on execution result using the appropriate StateManager method
        match step_result.status {
            StepStatus::Completed => {
                // Use complete_step_with_results to preserve step execution results
                if let Err(e) = self.state_manager.complete_step_with_results(step_id, Some(step_result.output.clone())).await {
                    warn!(
                        step_id = step_id,
                        error = %e,
                        "Failed to complete step with results"
                    );
                }
            },
            StepStatus::Skipped => {
                // Use complete_step_with_results for skipped steps too
                let skipped_output = serde_json::json!({"skipped": true});
                if let Err(e) = self.state_manager.complete_step_with_results(step_id, Some(skipped_output)).await {
                    warn!(
                        step_id = step_id,
                        error = %e,
                        "Failed to complete skipped step"
                    );
                }
            },
            StepStatus::Failed | StepStatus::Retrying => {
                // For failed steps, properly mark them as failed in the database
                let error_message = step_result.error_message.clone().unwrap_or_else(|| "Step execution failed".to_string());
                if let Err(e) = self.state_manager.fail_step_with_error(step_id, error_message).await {
                    warn!(
                        step_id = step_id,
                        error = %e,
                        "Failed to mark step as failed"
                    );
                }
            }
        }

        // Publish step execution completed event
        let events_step_result = match step_result.status {
            StepStatus::Completed => EventsStepResult::Success,
            StepStatus::Failed => EventsStepResult::Failed {
                error: step_result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string()),
            },
            StepStatus::Skipped => EventsStepResult::Skipped {
                reason: "Step was skipped".to_string(),
            },
            _ => EventsStepResult::Success,
        };

        self.event_publisher
            .publish_step_execution_completed(step_id, task_id, events_step_result)
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

        // 2. Get task orchestration info (name, namespace, version) from database
        let task_orchestration_info = {
            use crate::models::core::task::Task;

            // First get the task from database
            let task = Task::find_by_id(self.state_manager.pool(), task_id)
                .await
                .map_err(|e| ExecutionError::StateTransitionError {
                    step_id,
                    reason: format!("Failed to find task {task_id}: {e}"),
                })?
                .ok_or_else(|| ExecutionError::StateTransitionError {
                    step_id,
                    reason: format!("Task {task_id} not found"),
                })?;

            // Get orchestration info with task name, version, and namespace
            task.for_orchestration(self.state_manager.pool())
                .await
                .map_err(|e| ExecutionError::StateTransitionError {
                    step_id,
                    reason: format!("Failed to get task orchestration info: {e}"),
                })?
        };

        // 3. Try to resolve task handler metadata from registry
        let task_request = crate::models::core::task_request::TaskRequest::new(
            task_orchestration_info.task_name.clone(),
            task_orchestration_info.namespace_name.clone(),
        )
        .with_version(task_orchestration_info.task_version.clone());

        let task_handler_metadata = self.registry.resolve_handler(&task_request).map_err(|e| {
            ExecutionError::StateTransitionError {
                step_id,
                reason: format!("Failed to resolve task handler metadata: {e}"),
            }
        })?;

        debug!(
            step_id = step_id,
            task_name = task_orchestration_info.task_name,
            namespace = task_orchestration_info.namespace_name,
            version = task_orchestration_info.task_version,
            handler_class = task_handler_metadata.handler_class,
            "Resolved task handler metadata"
        );

        // 4. Try to load task configuration using TaskConfigFinder
        let task_config = match self
            .task_config_finder
            .find_task_template(
                &task_orchestration_info.namespace_name,
                &task_orchestration_info.task_name,
                &task_orchestration_info.task_version,
            )
            .await
        {
            Ok(config) => {
                debug!(
                    step_id = step_id,
                    namespace = task_orchestration_info.namespace_name,
                    task_name = task_orchestration_info.task_name,
                    version = task_orchestration_info.task_version,
                    step_template_count = config.step_templates.len(),
                    "Loaded task configuration"
                );
                Some(config)
            }
            Err(e) => {
                warn!(
                    step_id = step_id,
                    namespace = task_orchestration_info.namespace_name,
                    task_name = task_orchestration_info.task_name,
                    version = task_orchestration_info.task_version,
                    error = %e,
                    "Failed to load task configuration, falling back to framework delegation"
                );
                None
            }
        };

        // 5. Use direct framework integration for step execution when step template is available
        if let Some(ref config) = task_config {
            if let Some(template) = config
                .step_templates
                .iter()
                .find(|template| template.name == request.step.name)
            {
                let handler_class = &template.handler_class;

                debug!(
                    step_id = step_id,
                    step_name = request.step.name,
                    handler_class = handler_class,
                    "Using direct framework integration for step execution"
                );

                // Build step configuration from template
                let step_config = match template.handler_config.clone().unwrap_or_default() {
                    serde_json::Value::Object(map) => map.into_iter().collect(),
                    _ => std::collections::HashMap::new(),
                };

                // Get timeout from step template or use default
                let timeout_seconds = 30u64; // TODO: Get from template when available

                // Load the WorkflowStep object to get its dependencies
                let workflow_step = {
                    use crate::models::WorkflowStep;
                    WorkflowStep::find_by_id(self.state_manager.pool(), step_id)
                        .await
                        .map_err(|e| ExecutionError::StateTransitionError {
                            step_id,
                            reason: format!("Failed to load WorkflowStep: {e}"),
                        })?
                        .ok_or_else(|| ExecutionError::StateTransitionError {
                            step_id,
                            reason: format!("WorkflowStep {step_id} not found"),
                        })?
                };

                // Load dependency steps using WorkflowStep::get_dependencies()
                let previous_steps = workflow_step
                    .get_dependencies(self.state_manager.pool())
                    .await
                    .map_err(|e| ExecutionError::StateTransitionError {
                        step_id,
                        reason: format!("Failed to load step dependencies: {e}"),
                    })?;

                debug!(
                    step_id = step_id,
                    dependency_count = previous_steps.len(),
                    "Loaded step dependencies for direct execution"
                );

                // Create step execution context
                let execution_context = crate::orchestration::step_handler::StepExecutionContext {
                    step_id,
                    task_id,
                    step_name: request.step.name.clone(),
                    input_data: task_context.data.clone(),
                    previous_steps,
                    step_config: step_config.clone(),
                    attempt_number: request.retry_attempt,
                    max_retry_attempts: request.step.retry_limit as u32,
                    timeout_seconds,
                    is_retryable: request.step.retry_eligible,
                    environment: "production".to_string(), // TODO: Get from configuration
                    metadata: std::collections::HashMap::new(),
                };

                // Call framework integration directly with handler class and config
                let step_result = framework
                    .execute_step_with_handler(&execution_context, handler_class, &step_config)
                    .await
                    .map_err(|e| ExecutionError::StepExecutionFailed {
                        step_id,
                        reason: format!("Framework integration step execution failed: {e}"),
                        error_code: Some("FRAMEWORK_EXECUTION_ERROR".to_string()),
                    })?;

                debug!(
                    step_id = step_id,
                    status = ?step_result.status,
                    "Direct framework step execution completed"
                );

                Ok(step_result)
            } else {
                warn!(
                    step_id = step_id,
                    step_name = request.step.name,
                    "Step template not found in task configuration, cannot execute step"
                );
                Err(OrchestrationError::StepHandlerNotFound {
                    step_id,
                    reason: format!("Step template '{}' not found in task configuration", request.step.name),
                })
            }
        } else {
            warn!(
                step_id = step_id,
                task_id = task_id,
                "No task configuration available, cannot execute step"
            );
            Err(OrchestrationError::StepHandlerNotFound {
                step_id,
                reason: "No task configuration available for step execution".to_string(),
            })
        }
    }

    /// Calculate retry delay based on attempt number using BackoffCalculator
    async fn calculate_retry_delay(
        &self,
        step_id: i64,
        attempt: u32,
        error_message: Option<String>,
    ) -> Option<Duration> {
        if attempt >= self.config.retry_config.max_attempts {
            return None;
        }

        let context = BackoffContext::new()
            .with_error(error_message.unwrap_or_else(|| "Step execution failed".to_string()))
            .with_metadata("attempt".to_string(), serde_json::json!(attempt))
            .with_metadata(
                "max_attempts".to_string(),
                serde_json::json!(self.config.retry_config.max_attempts),
            );

        match self
            .backoff_calculator
            .calculate_and_apply_backoff(step_id, context)
            .await
        {
            Ok(result) => Some(Duration::from_secs(result.delay_seconds as u64)),
            Err(e) => {
                warn!("Failed to calculate backoff for step {}: {}", step_id, e);
                // Fallback to simple exponential backoff
                let base_delay = self.config.retry_config.base_delay;
                let multiplier = self.config.retry_config.backoff_multiplier;
                let max_delay = self.config.retry_config.max_delay;

                let delay = base_delay.mul_f64(multiplier.powi(attempt as i32));
                Some(delay.min(max_delay))
            }
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
            backoff_calculator: self.backoff_calculator.clone(),
            config: self.config.clone(),
            task_config_finder: self.task_config_finder.clone(),
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

impl StepExecutionConfig {
    /// Create configuration from ConfigurationManager
    pub fn from_config_manager(
        config_manager: &crate::orchestration::config::ConfigurationManager,
    ) -> Self {
        let system_config = config_manager.system_config();
        Self {
            max_concurrent_steps: system_config.execution.max_concurrent_steps,
            default_timeout: Duration::from_secs(
                system_config.execution.step_execution_timeout_seconds,
            ),
            max_timeout: Duration::from_secs(system_config.execution.default_timeout_seconds),
            enable_metrics: true,
            retry_config: RetryConfig::from_config_manager(config_manager),
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

impl RetryConfig {
    /// Create configuration from ConfigurationManager
    pub fn from_config_manager(
        config_manager: &crate::orchestration::config::ConfigurationManager,
    ) -> Self {
        let system_config = config_manager.system_config();
        let backoff_config = &system_config.backoff;
        Self {
            max_attempts: backoff_config.default_backoff_seconds.len() as u32,
            base_delay: Duration::from_secs(
                backoff_config
                    .default_backoff_seconds
                    .first()
                    .map(|&x| x as u64)
                    .unwrap_or(1),
            ),
            max_delay: Duration::from_secs(backoff_config.max_backoff_seconds as u64),
            backoff_multiplier: backoff_config.backoff_multiplier,
            jitter: backoff_config.jitter_enabled,
        }
    }
}
