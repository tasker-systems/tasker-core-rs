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
//! # async fn example(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
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

use crate::events::{Event, EventPublisher, OrchestrationEvent as EventsOrchestrationEvent};
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::orchestration::state_manager::{StateHealthSummary, StateManager};
use crate::orchestration::system_events::SystemEventsManager;
use crate::orchestration::types::TaskOrchestrationResult;
use crate::orchestration::types::{StepResult, StepStatus, ViableStep};
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
    batch_results: Option<Vec<StepResult>>,
}

/// Main workflow coordinator that orchestrates task execution
pub struct WorkflowCoordinator {
    /// Discovers viable steps for execution
    viable_step_discovery: ViableStepDiscovery,
    /// Executes individual steps
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
    /// ZeroMQ pub-sub executor for comprehensive batch execution with database tracking
    zmq_pub_sub_executor: Option<Arc<crate::execution::zeromq_pub_sub_executor::ZmqPubSubExecutor>>,
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
        _shared_registry: TaskHandlerRegistry,
    ) -> Self {
        let sql_executor = crate::database::sql_functions::SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Use the provided shared registry directly

        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer =
            crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
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
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
            zmq_pub_sub_executor: None,
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
        let _registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());

        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer =
            crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
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
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
            zmq_pub_sub_executor: None,
        }
    }

    /// Create a new workflow coordinator with shared BatchPublisher for ZeroMQ integration
    ///
    /// This constructor is specifically designed for OrchestrationSystem integration where
    /// the BatchPublisher instance should be shared to avoid ZeroMQ socket conflicts.
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - Workflow coordinator configuration
    /// * `config_manager` - Shared configuration manager
    /// * `event_publisher` - Shared EventPublisher instance
    /// * `shared_registry` - Shared TaskHandlerRegistry instance
    /// * `zmq_pub_sub_executor` - Shared ZmqPubSubExecutor for comprehensive batch execution
    pub fn with_zmq_pub_sub_executor(
        pool: sqlx::PgPool,
        config: WorkflowCoordinatorConfig,
        config_manager: Arc<ConfigurationManager>,
        event_publisher: crate::events::publisher::EventPublisher,
        _shared_registry: TaskHandlerRegistry,
        zmq_pub_sub_executor: Arc<crate::execution::zeromq_pub_sub_executor::ZmqPubSubExecutor>,
    ) -> Self {
        let sql_executor = crate::database::sql_functions::SqlFunctionExecutor::new(pool.clone());
        let state_manager =
            StateManager::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Use the provided shared registry directly

        // let task_config_finder = crate::orchestration::task_config_finder::TaskConfigFinder::new(
        //     config_manager.clone(),
        //     registry_arc.clone(),
        // );

        let viable_step_discovery =
            ViableStepDiscovery::new(sql_executor.clone(), event_publisher.clone(), pool.clone());

        // Create task finalizer with shared event publisher
        let task_finalizer =
            crate::orchestration::task_finalizer::TaskFinalizer::with_event_publisher(
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
            state_manager,
            event_publisher,
            task_finalizer,
            config,
            config_manager,
            events_manager,
            zmq_pub_sub_executor: Some(zmq_pub_sub_executor),
        }
    }

    /// Execute a complete task workflow
    ///
    /// In batch execution architecture, this method:
    /// 1. Publishes viable steps to ZeroMQ as fire-and-forget batches
    /// 2. Returns when no more steps are immediately ready for execution
    /// 3. Does NOT finalize the workflow - that happens via the result listener
    #[instrument(skip(self), fields(task_id = task_id))]
    pub async fn execute_task_workflow(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        info!(task_id = task_id, "Starting task workflow execution");

        let mut metrics = self.initialize_workflow_metrics(task_id).await?;

        // Publish workflow started event
        self.publish_workflow_started_event(task_id, &metrics)
            .await?;

        // Transition task to in_progress if needed
        self.ensure_task_in_progress(task_id).await?;

        // Execute main orchestration loop - publishes batches and returns
        self.execute_step_batch_publication(task_id, &mut metrics)
            .await?;

        // Return Published result - finalization happens via result listener
        info!(
            task_id = task_id,
            steps_discovered = metrics.steps_discovered,
            steps_published = metrics.steps_executed,
            "Workflow orchestration complete - batches published to ZeroMQ"
        );

        Ok(TaskOrchestrationResult::Published {
            task_id,
            viable_steps_discovered: metrics.steps_discovered,
            steps_published: metrics.steps_executed,
            batch_id: None, // TODO: Get actual batch ID from ZmqPubSubExecutor
            publication_time_ms: metrics.execution_duration.as_millis() as u64,
            next_poll_delay_ms: 1000, // 1 second delay before next check
        })
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
                    framework: "fire_and_forget_zeromq".to_string(),
                    started_at: metrics.started_at,
                },
            ))
            .await
            .map_err(OrchestrationError::from)
    }

    /// Execute the main orchestration loop
    async fn execute_step_batch_publication(
        &self,
        task_id: i64,
        metrics: &mut WorkflowExecutionMetrics,
    ) -> OrchestrationResult<DiscoveryResult> {
        let mut consecutive_empty_discoveries = 0;
        // Discover and process viable steps
        let batch_results = self
            .discover_and_process_steps(task_id, metrics, &mut consecutive_empty_discoveries)
            .await?;
        // Continue loop for next discovery iteration
        debug!(
            task_id = task_id,
            "Continuing workflow loop - no break condition met"
        );

        Ok(batch_results)
    }

    /// Discover viable steps and process them
    async fn discover_and_process_steps(
        &self,
        task_id: i64,
        metrics: &mut WorkflowExecutionMetrics,
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
            let discovery_result = self
                .handle_empty_discovery(task_id, consecutive_empty_discoveries)
                .await?;

            return Ok(discovery_result);
        }

        // Reset counter when we find steps
        *consecutive_empty_discoveries = 0;

        // Publish viable steps discovery event
        let step_ids: Vec<i64> = viable_steps.iter().map(|step| step.step_id).collect();
        let viable_steps_payload = self.events_manager.create_viable_steps_discovered_payload(
            task_id,
            &step_ids,
            &self
                .config_manager
                .system_config()
                .execution
                .processing_mode,
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
        let batch_results = self
            .execute_discovered_steps(task_id, viable_steps, metrics)
            .await?;

        Ok(DiscoveryResult {
            should_break: false,
            batch_results: Some(batch_results),
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
                            info!(
                                task_id = task_id,
                                "Task re-enqueued - breaking workflow loop"
                            );
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Completed => {
                            info!(task_id = task_id, "Task completed - breaking workflow loop");
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Failed => {
                            info!(task_id = task_id, "Task failed - breaking workflow loop");
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::Pending => {
                            info!(
                                task_id = task_id,
                                "Task marked as pending - continuing loop"
                            );
                            *consecutive_empty_discoveries = 0; // Reset counter to continue trying
                            return Ok(DiscoveryResult {
                                should_break: false,
                                batch_results: None,
                            });
                        }
                        crate::orchestration::task_finalizer::FinalizationAction::NoAction => {
                            warn!(
                                task_id = task_id,
                                "Task finalizer took no action - breaking loop"
                            );
                            return Ok(DiscoveryResult {
                                should_break: true,
                                batch_results: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    error!(
                        task_id = task_id,
                        error = %e,
                        "Failed to finalize task - breaking workflow loop"
                    );
                    return Ok(DiscoveryResult {
                        should_break: true,
                        batch_results: None,
                    });
                }
            }
        }

        // Wait before retrying discovery
        tokio::time::sleep(self.config.discovery_retry_delay).await;
        Ok(DiscoveryResult {
            should_break: false,
            batch_results: None,
        })
    }

    /// Execute discovered viable steps in batches
    async fn execute_discovered_steps(
        &self,
        task_id: i64,
        viable_steps: Vec<ViableStep>,
        metrics: &mut WorkflowExecutionMetrics,
    ) -> OrchestrationResult<Vec<StepResult>> {
        let execution_start = Instant::now();

        let batch_results = self.execute_step_batch(task_id, viable_steps).await?;

        metrics.execution_duration += execution_start.elapsed();
        Ok(batch_results)
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

    /// Execute a batch of steps using ZmqPubSubExecutor with comprehensive database tracking
    async fn execute_step_batch(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
    ) -> OrchestrationResult<Vec<StepResult>> {
        tracing::info!(
            task_id = task_id,
            step_count = steps.len(),
            "WorkflowCoordinator: Starting ZeroMQ batch execution with comprehensive tracking"
        );

        // Check if we have a ZmqPubSubExecutor configured
        let zmq_executor = match &self.zmq_pub_sub_executor {
            Some(executor) => executor,
            None => {
                return Err(
                    crate::orchestration::errors::OrchestrationError::ExecutionError(
                        crate::orchestration::errors::ExecutionError::StepExecutionFailed {
                            step_id: 0,
                            reason: "No ZmqPubSubExecutor configured".to_string(),
                            error_code: Some("NO_ZMQ_EXECUTOR".to_string()),
                        },
                    ),
                )
            }
        };

        // Extract step IDs from ViableStep objects
        // ZmqPubSubExecutor.publish_batch() will handle all the complex logic of:
        // - Loading step execution contexts from database
        // - Resolving handler metadata and step names
        // - Loading task configuration and step templates
        // - Loading previous step results with proper step names
        // - Database batch tracking and audit trails

        let step_ids: Vec<i64> = steps.iter().map(|step| step.step_id).collect();

        // Delegate to ZmqPubSubExecutor for sophisticated batch publishing with database tracking
        let batch_id = zmq_executor
            .publish_batch(step_ids, task_id)
            .await
            .map_err(|e| {
                tracing::error!(
                    task_id = task_id,
                    error = %e,
                    "Failed to publish batch via ZmqPubSubExecutor"
                );
                // OrchestrationError is already returned by publish_batch
                e
            })?;

        tracing::info!(
            task_id = task_id,
            batch_id = %batch_id,
            step_count = steps.len(),
            "Successfully published batch to ZeroMQ - fire and forget"
        );

        // Return placeholder results indicating steps were published (fire-and-forget)
        // Actual execution results will be processed asynchronously by the result listener
        let mut results = Vec::with_capacity(steps.len());
        for step in &steps {
            results.push(StepResult {
                step_id: step.step_id,
                status: crate::orchestration::types::StepStatus::InProgress,
                output: serde_json::json!({
                    "status": "published",
                    "message": "Step published to ZeroMQ batch processing system",
                    "batch_id": batch_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "note": "Actual execution results will be processed asynchronously"
                }),
                execution_duration: std::time::Duration::from_millis(1), // Minimal time for publishing
                error_message: None,
                retry_after: None,
                error_code: None,
                error_context: None,
            });
        }

        Ok(results)
    }

    // NOTE: finalize_workflow method removed in fire-and-forget architecture
    // Task finalization is now handled by:
    // 1. TaskFinalizer in partial result processing (ZmqPubSubExecutor::handle_partial_result)
    // 2. TaskFinalizer in batch completion processing (ZmqPubSubExecutor::handle_batch_completion)
    // This eliminates redundant finalization logic and centralizes it in the appropriate
    // execution paths where actual step results determine task completion status.
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
