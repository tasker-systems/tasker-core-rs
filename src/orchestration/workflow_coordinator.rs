//! # Workflow Coordinator
//!
//! ## Architecture: Main Orchestration Engine
//!
//! The WorkflowCoordinator is the central orchestration engine that coordinates all workflow
//! execution activities. It brings together all orchestration components to provide complete
//! workflow lifecycle management from task creation to completion.
//!
//! ## Key Responsibilities
//!
//! - **Workflow Lifecycle Management**: Start, execute, monitor, and complete workflows
//! - **Step Discovery**: Use ViableStepDiscovery to find ready-to-execute steps
//! - **Concurrent Execution**: Leverage StepExecutor for parallel step execution
//! - **State Management**: Coordinate state transitions through StateManager
//! - **Event Coordination**: Publish workflow lifecycle events
//! - **Error Recovery**: Handle failures and coordinate retry logic
//! - **Task Finalization**: Determine when tasks are complete and finalize them
//!
//! ## Integration with Orchestration Components
//!
//! The WorkflowCoordinator serves as the conductor that orchestrates:
//! - **ViableStepDiscovery**: Finds steps ready for execution
//! - **StepExecutor**: Executes individual steps with concurrency control
//! - **StateManager**: Manages workflow and step state transitions
//! - **EventPublisher**: Publishes orchestration lifecycle events
//! - **TaskHandlerRegistry**: Resolves handlers for step execution
//! - **FrameworkIntegration**: Delegates to framework-specific execution
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
//! use tasker_core::orchestration::types::FrameworkIntegration;
//! use std::sync::Arc;
//!
//! # async fn example(pool: sqlx::PgPool, framework: Arc<dyn FrameworkIntegration>) -> Result<(), Box<dyn std::error::Error>> {
//! // Create the workflow coordinator
//! let coordinator = WorkflowCoordinator::new(pool);
//!
//! // Execute a task workflow
//! let task_id = 123;
//! let result = coordinator.execute_task_workflow(task_id, framework).await?;
//!
//! match result {
//!     tasker_core::orchestration::TaskOrchestrationResult::Complete { .. } => {
//!         println!("Task completed successfully!");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::InProgress { .. } => {
//!         println!("Task still in progress, will be re-queued");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::Failed { .. } => {
//!         println!("Task failed");
//!     },
//!     tasker_core::orchestration::TaskOrchestrationResult::Blocked { .. } => {
//!         println!("Task blocked waiting for dependencies");
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use crate::events::{
    Event, EventPublisher, OrchestrationEvent as EventsOrchestrationEvent, TaskResult,
};
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::orchestration::state_manager::{StateHealthSummary, StateManager};
use crate::orchestration::step_executor::{ExecutionPriority, StepExecutionRequest, StepExecutor};
use crate::orchestration::system_events::SystemEventsManager;
use crate::orchestration::types::TaskOrchestrationResult;
use crate::orchestration::types::{FrameworkIntegration, StepResult, StepStatus, ViableStep};
use crate::orchestration::viable_step_discovery::ViableStepDiscovery;
use crate::registry::TaskHandlerRegistry;
use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

/// Configuration for workflow coordination
#[derive(Debug, Clone)]
pub struct WorkflowCoordinatorConfig {
    /// Maximum number of discovery attempts before failing
    pub max_discovery_attempts: u32,
    /// Delay between discovery attempts when no steps are found
    pub discovery_retry_delay: Duration,
    /// Maximum execution time for a single workflow run
    pub max_workflow_duration: Duration,
    /// Enable detailed performance metrics
    pub enable_metrics: bool,
    /// Maximum steps to execute in a single workflow run
    pub max_steps_per_run: usize,
    /// Batch size for concurrent step execution
    pub step_batch_size: usize,
}

impl Default for WorkflowCoordinatorConfig {
    fn default() -> Self {
        Self {
            max_discovery_attempts: 3,
            discovery_retry_delay: Duration::from_secs(1),
            max_workflow_duration: Duration::from_secs(3600), // 1 hour
            enable_metrics: true,
            max_steps_per_run: 1000,
            step_batch_size: 10,
        }
    }
}

impl WorkflowCoordinatorConfig {
    /// Create configuration optimized for testing with short timeouts
    pub fn for_testing() -> Self {
        Self {
            max_discovery_attempts: 2,
            discovery_retry_delay: Duration::from_millis(100),
            max_workflow_duration: Duration::from_secs(2), // Short timeout for tests
            enable_metrics: true,
            max_steps_per_run: 10,
            step_batch_size: 5,
        }
    }

    /// Create configuration optimized for testing with custom timeout
    pub fn for_testing_with_timeout(timeout_secs: u64) -> Self {
        Self {
            max_discovery_attempts: 2,
            discovery_retry_delay: Duration::from_millis(100),
            max_workflow_duration: Duration::from_secs(timeout_secs),
            enable_metrics: true,
            max_steps_per_run: 10,
            step_batch_size: 5,
        }
    }

    /// Create configuration from ConfigurationManager
    pub fn from_config_manager(config_manager: &ConfigurationManager) -> Self {
        let system_config = config_manager.system_config();

        Self {
            max_discovery_attempts: system_config.execution.max_discovery_attempts,
            discovery_retry_delay: Duration::from_secs(
                system_config.backoff.default_reenqueue_delay as u64,
            ),
            max_workflow_duration: Duration::from_secs(
                system_config.execution.default_timeout_seconds,
            ),
            enable_metrics: system_config.telemetry.enabled,
            max_steps_per_run: system_config.execution.max_concurrent_steps,
            step_batch_size: system_config.execution.step_batch_size,
        }
    }
}

/// Workflow execution metrics
#[derive(Debug, Clone)]
pub struct WorkflowExecutionMetrics {
    pub task_id: i64,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_duration: Option<Duration>,
    pub steps_discovered: usize,
    pub steps_executed: usize,
    pub steps_succeeded: usize,
    pub steps_failed: usize,
    pub steps_retried: usize,
    pub discovery_attempts: u32,
    pub discovery_duration: Duration,
    pub execution_duration: Duration,
}

/// Result of step discovery and processing
#[derive(Debug)]
struct DiscoveryResult {
    should_break: bool,
}

/// Main workflow coordinator that orchestrates task execution
pub struct WorkflowCoordinator {
    /// Discovers viable steps for execution
    viable_step_discovery: ViableStepDiscovery,
    /// Executes individual steps
    step_executor: StepExecutor,
    /// Manages state transitions
    state_manager: StateManager,
    /// Publishes orchestration events
    event_publisher: EventPublisher,
    /// Finalizes tasks when no more viable steps
    task_finalizer: crate::orchestration::task_finalizer::TaskFinalizer,
    /// Configuration
    config: WorkflowCoordinatorConfig,
    /// Configuration manager for system settings
    config_manager: Arc<ConfigurationManager>,
    /// System events manager for structured event publishing
    events_manager: Arc<SystemEventsManager>,
}

impl WorkflowCoordinator {
    /// Create a new workflow coordinator with default configuration
    pub fn new(pool: sqlx::PgPool) -> Self {
        let config_manager = Arc::new(ConfigurationManager::new());
        let config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);
        Self::with_config_manager(pool, config, config_manager)
    }

    /// Create a new workflow coordinator with custom configuration
    pub fn with_config(pool: sqlx::PgPool, config: WorkflowCoordinatorConfig) -> Self {
        let config_manager = Arc::new(ConfigurationManager::new());
        Self::with_config_manager(pool, config, config_manager)
    }

    /// Create a new workflow coordinator optimized for testing with short timeouts
    pub fn for_testing(pool: sqlx::PgPool) -> Self {
        Self::with_config(pool, WorkflowCoordinatorConfig::for_testing())
    }

    /// Create a new workflow coordinator for testing with custom timeout
    pub fn for_testing_with_timeout(pool: sqlx::PgPool, timeout_secs: u64) -> Self {
        Self::with_config(
            pool,
            WorkflowCoordinatorConfig::for_testing_with_timeout(timeout_secs),
        )
    }

    /// Create a new workflow coordinator with configuration manager
    pub fn with_config_manager(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
    ) -> Self {
        Self::with_config_manager_and_publisher(pool, config, config_manager, None)
    }

    /// Create a new workflow coordinator with shared registry
    ///
    /// This constructor is specifically designed for shared orchestration systems where
    /// the TaskHandlerRegistry instance should be shared across components to ensure
    /// handlers registered via FFI are available during workflow execution.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - Workflow coordinator configuration
    /// * `config_manager` - Shared configuration manager
    /// * `event_publisher` - Shared EventPublisher instance
    /// * `shared_registry` - Shared TaskHandlerRegistry instance (critical for FFI integration)
    pub fn with_shared_registry(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
        event_publisher: crate::events::publisher::EventPublisher,
        shared_registry: TaskHandlerRegistry,
    ) -> Self {
        let sql_executor = crate::database::sql_functions::SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor.clone(), event_publisher.clone(), pool.clone());
        
        // Use the provided shared registry instead of creating a new one
        let registry_arc = Arc::new(shared_registry.clone());

        let task_config_finder = crate::orchestration::task_config_finder::TaskConfigFinder::new(
            config_manager.clone(),
            registry_arc.clone(),
        );

        let step_executor = StepExecutor::new(
            state_manager.clone(),
            shared_registry, // Use shared registry for step executor
            event_publisher.clone(),
            task_config_finder,
        );
        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer = crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
            pool,
            event_publisher.clone(),
        );

        // Create system events manager - in production this would be loaded from file
        let events_manager = Arc::new(SystemEventsManager::new(
            crate::orchestration::system_events::SystemEventsConfig {
                event_metadata: std::collections::HashMap::new(),
                state_machine_mappings: crate::orchestration::system_events::StateMachineMappings {
                    task_transitions: vec![],
                    step_transitions: vec![],
                },
            },
        ));

        Self {
            viable_step_discovery,
            step_executor,
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
        }
    }

    /// Create a new workflow coordinator with configuration manager and optional event publisher
    ///
    /// This allows injecting a specific EventPublisher instance (e.g., from global FFI state)
    /// while maintaining backward compatibility through the `with_config_manager` method.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - Workflow coordinator configuration
    /// * `config_manager` - Shared configuration manager
    /// * `event_publisher` - Optional EventPublisher instance. If None, creates a new one.
    pub fn with_config_manager_and_publisher(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
        event_publisher: Option<crate::events::publisher::EventPublisher>,
    ) -> Self {
        // Use provided event publisher or create new one
        let event_publisher = event_publisher.unwrap_or_default();

        let sql_executor = crate::database::sql_functions::SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor.clone(), event_publisher.clone(), pool.clone());
        let registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());
        let registry_arc = Arc::new(registry.clone());

        let task_config_finder = crate::orchestration::task_config_finder::TaskConfigFinder::new(
            config_manager.clone(),
            registry_arc.clone(),
        );

        let step_executor = StepExecutor::new(
            state_manager.clone(),
            registry,
            event_publisher.clone(),
            task_config_finder,
        );
        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer = crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
            pool,
            event_publisher.clone(),
        );

        // Create system events manager - in production this would be loaded from file
        let events_manager = Arc::new(SystemEventsManager::new(
            crate::orchestration::system_events::SystemEventsConfig {
                event_metadata: std::collections::HashMap::new(),
                state_machine_mappings: crate::orchestration::system_events::StateMachineMappings {
                    task_transitions: vec![],
                    step_transitions: vec![],
                },
            },
        ));

        Self {
            viable_step_discovery,
            step_executor,
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
        }
    }

    /// Execute a complete task workflow
    #[instrument(skip(self, framework), fields(task_id = task_id))]
    pub async fn execute_task_workflow(
        &self,
        task_id: i64,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        info!(task_id = task_id, "Starting task workflow execution");

        let workflow_start = Instant::now();
        let mut metrics = self.initialize_workflow_metrics(task_id).await?;

        // Publish workflow started event
        self.publish_workflow_started_event(task_id, &framework, &metrics)
            .await?;

        // Transition task to in_progress if needed
        self.ensure_task_in_progress(task_id).await?;

        // Execute main orchestration loop
        self.execute_orchestration_loop(task_id, framework, &mut metrics, workflow_start)
            .await?;

        // Finalize workflow execution
        self.finalize_workflow(task_id, metrics).await
    }

    /// Initialize workflow execution metrics
    async fn initialize_workflow_metrics(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<WorkflowExecutionMetrics> {
        Ok(WorkflowExecutionMetrics {
            task_id,
            started_at: Utc::now(),
            completed_at: None,
            total_duration: None,
            steps_discovered: 0,
            steps_executed: 0,
            steps_succeeded: 0,
            steps_failed: 0,
            steps_retried: 0,
            discovery_attempts: 0,
            discovery_duration: Duration::default(),
            execution_duration: Duration::default(),
        })
    }

    /// Publish workflow started event
    async fn publish_workflow_started_event(
        &self,
        task_id: i64,
        framework: &Arc<dyn FrameworkIntegration>,
        metrics: &WorkflowExecutionMetrics,
    ) -> OrchestrationResult<()> {
        // Publish structured system event
        let task_start_payload = serde_json::json!({
            "task_id": task_id.to_string(),
            "task_name": format!("workflow_task_{}", task_id), // Enhancement: Load task name from database or pass as parameter
            "timestamp": metrics.started_at.to_rfc3339()
        });

        info!(task_id = task_id, "Publishing task.start_requested event");

        // Validate event payload against schema
        if let Err(e) = self.events_manager.config().validate_event_payload(
            "task",
            "start_requested",
            &task_start_payload,
        ) {
            warn!(task_id = task_id, error = %e, "Task start event validation failed");
        }

        // Also publish the original orchestration event for backward compatibility
        self.event_publisher
            .publish_event(Event::orchestration(
                EventsOrchestrationEvent::TaskOrchestrationStarted {
                    task_id,
                    framework: framework.framework_name().to_string(),
                    started_at: metrics.started_at,
                },
            ))
            .await
            .map_err(OrchestrationError::from)
    }

    /// Execute the main orchestration loop
    async fn execute_orchestration_loop(
        &self,
        task_id: i64,
        framework: Arc<dyn FrameworkIntegration>,
        metrics: &mut WorkflowExecutionMetrics,
        workflow_start: Instant,
    ) -> OrchestrationResult<()> {
        let mut total_steps_executed = 0;
        let mut consecutive_empty_discoveries = 0;

        loop {
            // Check execution limits
            if self
                .should_stop_execution(task_id, workflow_start, total_steps_executed)
                .await?
            {
                break;
            }

            // Discover and process viable steps
            let discovery_result = self
                .discover_and_process_steps(
                    task_id,
                    framework.clone(),
                    metrics,
                    &mut total_steps_executed,
                    &mut consecutive_empty_discoveries,
                )
                .await?;

            // Break if no more steps to process
            if discovery_result.should_break {
                break;
            }
        }

        Ok(())
    }

    /// Check if execution should stop due to limits
    async fn should_stop_execution(
        &self,
        task_id: i64,
        workflow_start: Instant,
        total_steps_executed: usize,
    ) -> OrchestrationResult<bool> {
        // Check if we've exceeded max duration
        if workflow_start.elapsed() > self.config.max_workflow_duration {
            warn!(
                task_id = task_id,
                duration_secs = workflow_start.elapsed().as_secs(),
                "Workflow exceeded maximum duration"
            );
            return Ok(true);
        }

        // Check if we've executed too many steps
        if total_steps_executed >= self.config.max_steps_per_run {
            info!(
                task_id = task_id,
                steps_executed = total_steps_executed,
                "Reached maximum steps per run"
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Discover viable steps and process them
    async fn discover_and_process_steps(
        &self,
        task_id: i64,
        framework: Arc<dyn FrameworkIntegration>,
        metrics: &mut WorkflowExecutionMetrics,
        total_steps_executed: &mut usize,
        consecutive_empty_discoveries: &mut u32,
    ) -> OrchestrationResult<DiscoveryResult> {
        // Discover viable steps
        let discovery_start = Instant::now();
        metrics.discovery_attempts += 1;

        let viable_steps = match self.discover_viable_steps(task_id).await {
            Ok(steps) => steps,
            Err(e) => {
                error!(
                    task_id = task_id,
                    error = %e,
                    "Failed to discover viable steps"
                );
                return Err(e);
            }
        };

        metrics.discovery_duration += discovery_start.elapsed();
        metrics.steps_discovered += viable_steps.len();

        // Handle empty discovery
        if viable_steps.is_empty() {
            return self
                .handle_empty_discovery(task_id, consecutive_empty_discoveries)
                .await;
        }

        // Reset counter when we find steps
        *consecutive_empty_discoveries = 0;

        // Publish viable steps discovery event
        let step_ids: Vec<i64> = viable_steps.iter().map(|step| step.step_id).collect();
        let viable_steps_payload = self.events_manager.create_viable_steps_discovered_payload(
            task_id,
            &step_ids,
            "concurrent", // TODO: Determine actual processing mode
        );

        debug!(
            task_id = task_id,
            steps_count = viable_steps.len(),
            "Publishing workflow.viable_steps_discovered event"
        );

        if let Err(e) = self.events_manager.config().validate_event_payload(
            "workflow",
            "viable_steps_discovered",
            &viable_steps_payload,
        ) {
            warn!(task_id = task_id, error = %e, "Viable steps discovery event validation failed");
        }

        // Execute the discovered steps
        self.execute_discovered_steps(
            task_id,
            viable_steps,
            framework,
            metrics,
            total_steps_executed,
        )
        .await?;

        Ok(DiscoveryResult {
            should_break: false,
        })
    }

    /// Handle the case when no viable steps are discovered
    async fn handle_empty_discovery(
        &self,
        task_id: i64,
        consecutive_empty_discoveries: &mut u32,
    ) -> OrchestrationResult<DiscoveryResult> {
        *consecutive_empty_discoveries += 1;

        if *consecutive_empty_discoveries >= self.config.max_discovery_attempts {
            info!(
                task_id = task_id,
                attempts = consecutive_empty_discoveries,
                "No viable steps found after multiple attempts - calling task finalizer"
            );
            
            // Call TaskFinalizer to determine next action
            match self.task_finalizer.handle_no_viable_steps(task_id).await {
                Ok(finalization_result) => {
                    info!(
                        task_id = task_id,
                        action = ?finalization_result.action,
                        reason = ?finalization_result.reason,
                        "Task finalization completed"
                    );
                    
                    match finalization_result.action {
                        crate::orchestration::task_finalizer::FinalizationAction::Reenqueued => {
                            info!(task_id = task_id, "Task re-enqueued - breaking workflow loop");
                            return Ok(DiscoveryResult { should_break: true });
                        },
                        crate::orchestration::task_finalizer::FinalizationAction::Completed => {
                            info!(task_id = task_id, "Task completed - breaking workflow loop");
                            return Ok(DiscoveryResult { should_break: true });
                        },
                        crate::orchestration::task_finalizer::FinalizationAction::Failed => {
                            info!(task_id = task_id, "Task failed - breaking workflow loop");
                            return Ok(DiscoveryResult { should_break: true });
                        },
                        crate::orchestration::task_finalizer::FinalizationAction::Pending => {
                            info!(task_id = task_id, "Task marked as pending - continuing loop");
                            *consecutive_empty_discoveries = 0; // Reset counter to continue trying
                            return Ok(DiscoveryResult { should_break: false });
                        },
                        crate::orchestration::task_finalizer::FinalizationAction::NoAction => {
                            warn!(task_id = task_id, "Task finalizer took no action - breaking loop");
                            return Ok(DiscoveryResult { should_break: true });
                        },
                    }
                },
                Err(e) => {
                    error!(
                        task_id = task_id,
                        error = %e,
                        "Failed to finalize task - breaking workflow loop"
                    );
                    return Ok(DiscoveryResult { should_break: true });
                }
            }
        }

        // Wait before retrying discovery
        tokio::time::sleep(self.config.discovery_retry_delay).await;
        Ok(DiscoveryResult {
            should_break: false,
        })
    }

    /// Execute discovered viable steps in batches
    async fn execute_discovered_steps(
        &self,
        task_id: i64,
        viable_steps: Vec<ViableStep>,
        framework: Arc<dyn FrameworkIntegration>,
        metrics: &mut WorkflowExecutionMetrics,
        total_steps_executed: &mut usize,
    ) -> OrchestrationResult<()> {
        let execution_start = Instant::now();

        for batch in viable_steps.chunks(self.config.step_batch_size) {
            let batch_results = self
                .execute_step_batch(task_id, batch.to_vec(), framework.clone())
                .await?;

            // Process results and update metrics
            self.process_batch_results(batch_results, metrics, total_steps_executed)
                .await?;
        }

        metrics.execution_duration += execution_start.elapsed();
        Ok(())
    }

    /// Process batch execution results and update metrics
    async fn process_batch_results(
        &self,
        batch_results: Vec<StepResult>,
        metrics: &mut WorkflowExecutionMetrics,
        total_steps_executed: &mut usize,
    ) -> OrchestrationResult<()> {
        for result in batch_results {
            metrics.steps_executed += 1;
            *total_steps_executed += 1;

            match result.status {
                StepStatus::Completed => metrics.steps_succeeded += 1,
                StepStatus::Failed => metrics.steps_failed += 1,
                StepStatus::Retrying => metrics.steps_retried += 1,
                StepStatus::Skipped => {} // Don't count skipped
                StepStatus::InProgress => {} // Steps published but not yet completed (fire-and-forget)
            }
        }
        Ok(())
    }

    /// Get current workflow health summary
    pub async fn get_workflow_health(&self) -> OrchestrationResult<StateHealthSummary> {
        self.state_manager.get_state_health_summary().await
    }

    /// Ensure task is in progress state
    async fn ensure_task_in_progress(&self, task_id: i64) -> OrchestrationResult<()> {
        let evaluation = self.state_manager.evaluate_task_state(task_id).await?;

        if evaluation.transition_required {
            debug!(
                task_id = task_id,
                from_state = %evaluation.current_state,
                to_state = ?evaluation.recommended_state,
                "Transitioning task state"
            );
        }

        Ok(())
    }

    /// Discover viable steps for execution
    async fn discover_viable_steps(&self, task_id: i64) -> OrchestrationResult<Vec<ViableStep>> {
        let steps = self
            .viable_step_discovery
            .find_viable_steps(task_id)
            .await?;

        if !steps.is_empty() {
            // Publish discovery event
            // Convert viable steps to events format
            let events_viable_steps: Vec<crate::events::ViableStep> = steps
                .iter()
                .map(|step| crate::events::ViableStep {
                    step_id: step.step_id,
                    task_id: step.task_id,
                    name: step.name.clone(),
                    named_step_id: step.named_step_id as i64,
                    current_state: step.current_state.clone(),
                    dependencies_satisfied: step.dependencies_satisfied,
                    retry_eligible: step.retry_eligible,
                    attempts: step.attempts as u32,
                    retry_limit: step.retry_limit as u32,
                    last_failure_at: step.last_failure_at.map(|dt| dt.and_utc()),
                    next_retry_at: step.next_retry_at.map(|dt| dt.and_utc()),
                })
                .collect();

            self.event_publisher
                .publish_event(Event::orchestration(
                    EventsOrchestrationEvent::ViableStepsDiscovered {
                        task_id,
                        step_count: steps.len(),
                        steps: events_viable_steps,
                    },
                ))
                .await?;
        }

        Ok(steps)
    }

    /// Execute a batch of steps concurrently
    async fn execute_step_batch(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
        framework: Arc<dyn FrameworkIntegration>,
    ) -> OrchestrationResult<Vec<StepResult>> {
        // Get task context once for all steps
        let task_context = framework.get_task_context(task_id).await?;

        // Create execution requests
        let requests: Vec<StepExecutionRequest> = steps
            .into_iter()
            .map(|step| StepExecutionRequest {
                step,
                task_context: task_context.clone(),
                timeout: None, // Use default from StepExecutor config
                priority: ExecutionPriority::Normal,
                retry_attempt: 0,
            })
            .collect();

        // Execute steps concurrently
        self.step_executor
            .execute_steps_concurrent(requests, framework)
            .await
    }

    /// Finalize workflow execution
    async fn finalize_workflow(
        &self,
        task_id: i64,
        mut metrics: WorkflowExecutionMetrics,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        metrics.completed_at = Some(Utc::now());
        metrics.total_duration = Some(
            metrics
                .completed_at
                .unwrap()
                .signed_duration_since(metrics.started_at)
                .to_std()
                .unwrap_or_default(),
        );

        // Evaluate final task state
        let task_evaluation = self.state_manager.evaluate_task_state(task_id).await?;

        info!(
            task_id = task_id,
            state = %task_evaluation.current_state,
            steps_executed = metrics.steps_executed,
            steps_succeeded = metrics.steps_succeeded,
            steps_failed = metrics.steps_failed,
            duration_ms = metrics.total_duration.map(|d| d.as_millis()).unwrap_or(0),
            "Workflow execution completed"
        );

        // Determine result based on current state and execution metrics
        let result = match task_evaluation.current_state.as_str() {
            "complete" => TaskOrchestrationResult::Complete {
                task_id,
                steps_executed: metrics.steps_executed,
                total_execution_time_ms: metrics
                    .total_duration
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            },
            "error" => {
                TaskOrchestrationResult::Failed {
                    task_id,
                    error: format!(
                        "Task failed with {} failed steps out of {} executed",
                        metrics.steps_failed, metrics.steps_executed
                    ),
                    failed_steps: vec![], // Would need to track these during execution
                }
            }
            "pending" | "in_progress" => {
                // Task is still in progress - use configuration-driven delays
                let system_config = self.config_manager.system_config();
                let next_poll_delay = if metrics.steps_discovered == 0 {
                    // No steps found, use waiting_for_dependencies delay
                    system_config
                        .backoff
                        .reenqueue_delays
                        .get("waiting_for_dependencies")
                        .copied()
                        .unwrap_or(45)
                        * 1000 // Convert to milliseconds
                } else {
                    // Steps were found, use processing delay
                    system_config
                        .backoff
                        .reenqueue_delays
                        .get("processing")
                        .copied()
                        .unwrap_or(10)
                        * 1000 // Convert to milliseconds
                };

                TaskOrchestrationResult::InProgress {
                    task_id,
                    steps_executed: metrics.steps_executed,
                    next_poll_delay_ms: next_poll_delay as u64,
                }
            }
            _ => {
                // Unknown state, treat as blocked
                TaskOrchestrationResult::Blocked {
                    task_id,
                    blocking_reason: format!(
                        "Task in unexpected state: {}",
                        task_evaluation.current_state
                    ),
                }
            }
        };

        // Publish completion event with appropriate TaskResult
        let task_result = match &result {
            TaskOrchestrationResult::Complete { .. } => TaskResult::Success,
            TaskOrchestrationResult::Failed { error, .. } => TaskResult::Failed {
                error: error.clone(),
            },
            TaskOrchestrationResult::InProgress { .. } => {
                TaskResult::Success // For in-progress, we don't publish completion yet
            }
            TaskOrchestrationResult::Blocked { .. } => TaskResult::Failed {
                error: "Task blocked".to_string(),
            },
        };

        // Publish structured system events based on task completion state
        match &result {
            TaskOrchestrationResult::Complete {
                steps_executed,
                total_execution_time_ms,
                ..
            } => {
                let task_completed_payload = self.events_manager.create_task_completed_payload(
                    task_id,
                    &format!("workflow_task_{task_id}"), // Enhancement: Load task name from database or pass as parameter
                    metrics.steps_discovered as u32,
                    *steps_executed as u32,
                    Some(*total_execution_time_ms as f64 / 1000.0), // Convert to seconds
                );

                info!(task_id = task_id, "Publishing task.completed event");

                if let Err(e) = self.events_manager.config().validate_event_payload(
                    "task",
                    "completed",
                    &task_completed_payload,
                ) {
                    warn!(task_id = task_id, error = %e, "Task completed event validation failed");
                }
            }
            TaskOrchestrationResult::Failed { error, .. } => {
                let task_failed_payload = serde_json::json!({
                    "task_id": task_id.to_string(),
                    "task_name": format!("workflow_task_{}", task_id), // Enhancement: Load task name from database or pass as parameter
                    "error_message": error,
                    "failed_steps": [], // Enhancement: Collect failed step IDs during execution
                    "timestamp": Utc::now().to_rfc3339()
                });

                info!(task_id = task_id, "Publishing task.failed event");

                if let Err(e) = self.events_manager.config().validate_event_payload(
                    "task",
                    "failed",
                    &task_failed_payload,
                ) {
                    warn!(task_id = task_id, error = %e, "Task failed event validation failed");
                }
            }
            _ => {
                // In progress or blocked - no completion event needed
            }
        }

        // Publish original orchestration event for backward compatibility
        self.event_publisher
            .publish_event(Event::orchestration(
                EventsOrchestrationEvent::TaskOrchestrationCompleted {
                    task_id,
                    result: task_result,
                    completed_at: Utc::now(),
                },
            ))
            .await?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_coordinator_config_defaults() {
        let config = WorkflowCoordinatorConfig::default();
        assert_eq!(config.max_discovery_attempts, 3);
        assert_eq!(config.discovery_retry_delay, Duration::from_secs(1));
        assert_eq!(config.max_workflow_duration, Duration::from_secs(3600));
        assert!(config.enable_metrics);
        assert_eq!(config.max_steps_per_run, 1000);
        assert_eq!(config.step_batch_size, 10);
    }
}
