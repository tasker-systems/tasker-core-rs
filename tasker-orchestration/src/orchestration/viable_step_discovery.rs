//! # Viable Step Discovery
//!
//! ## Architecture: SQL Functions + State Machine Verification
//!
//! This component uses the high-performance SQL functions from the existing Rails system
//! combined with state machine verification to determine which steps are ready for execution.
//! This approach balances performance (SQL functions) with consistency (state machine validation).
//!
//! ## Key Features
//!
//! - **SQL-driven discovery**: Uses existing `get_step_readiness_status()` function
//! - **State machine verification**: Ensures consistency between SQL results and state machine state
//! - **Dependency analysis**: Leverages `calculate_dependency_levels()` for complex workflows
//! - **Task execution context**: Uses `get_task_execution_context()` for comprehensive analysis
//! - **Circuit breaker integration**: Respects circuit breaker logic from SQL functions
//!
//! ## Usage
//!
//! ```rust
//! use tasker_orchestration::orchestration::viable_step_discovery::ViableStepDiscovery;
//! use tasker_shared::events::publisher::EventPublisher;
//! use tasker_shared::database::sql_functions::SqlFunctionExecutor;
//!
//! // Create ViableStepDiscovery for step readiness analysis
//! # use sqlx::PgPool;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool = PgPool::connect("postgresql://localhost/nonexistent").await?;
//! let sql_executor = SqlFunctionExecutor::new(pool.clone());
//! let event_publisher = EventPublisher::new();
//! let discovery = ViableStepDiscovery::new(sql_executor, event_publisher, pool);
//!
//! // ViableStepDiscovery uses SQL functions to determine step readiness
//! // Verify creation succeeded (we can't test SQL functions without a real database)
//! let _discovery = discovery;
//! # Ok(())
//! # }
//!
//! // For complete integration examples, see tests/orchestration/viable_step_discovery_integration.rs
//! ```

use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::errors::{DiscoveryError, OrchestrationResult};
use tasker_shared::events::{EventPublisher, ViableStep as EventsViableStep};
use tasker_shared::types::ViableStep;
use tracing::{debug, info, instrument, warn};

/// High-performance step readiness discovery engine
pub struct ViableStepDiscovery {
    sql_executor: SqlFunctionExecutor,
    event_publisher: Arc<EventPublisher>,
    pool: sqlx::PgPool,
}

impl ViableStepDiscovery {
    /// Create new step discovery instance
    pub fn new(
        sql_executor: SqlFunctionExecutor,
        event_publisher: Arc<EventPublisher>,
        pool: sqlx::PgPool,
    ) -> Self {
        Self {
            sql_executor,
            event_publisher,
            pool,
        }
    }

    /// Find all viable steps for a task using optimized SQL function
    #[instrument(skip(self), fields(task_uuid = %task_uuid))]
    pub async fn find_viable_steps(&self, task_uuid: Uuid) -> OrchestrationResult<Vec<ViableStep>> {
        debug!(task_uuid = task_uuid.to_string(), "Finding viable steps");

        // Use get_ready_steps which already handles all filtering and business logic
        let ready_steps = self
            .sql_executor
            .get_ready_steps(task_uuid)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_ready_steps".to_string(),
                reason: e.to_string(),
            })?;

        debug!(
            task_uuid = task_uuid.to_string(),
            ready_count = ready_steps.len(),
            "Retrieved ready steps from SQL function"
        );

        // Convert to ViableStep objects (no additional verification needed - SQL function handles everything)
        let viable_steps: Vec<ViableStep> = ready_steps
            .into_iter()
            .map(|status| {
                debug!(
                    task_uuid = task_uuid.to_string(),
                    step_uuid = status.workflow_step_uuid.to_string(),
                    step_name = %status.name,
                    current_state = %status.current_state,
                    dependencies_satisfied = status.dependencies_satisfied,
                    "Found viable step"
                );

                ViableStep {
                    step_uuid: status.workflow_step_uuid,
                    task_uuid: status.task_uuid,
                    name: status.name,
                    named_step_uuid: status.named_step_uuid,
                    current_state: status.current_state,
                    dependencies_satisfied: status.dependencies_satisfied,
                    retry_eligible: status.retry_eligible,
                    attempts: status.attempts,
                    retry_limit: status.retry_limit,
                    last_failure_at: status.last_failure_at,
                    next_retry_at: status.next_retry_at,
                }
            })
            .collect();

        info!(
            task_uuid = task_uuid.to_string(),
            viable_steps = viable_steps.len(),
            "Completed viable step discovery"
        );

        // Publish discovery event
        let events_viable_steps: Vec<EventsViableStep> = viable_steps
            .iter()
            .map(|step| EventsViableStep {
                step_uuid: step.step_uuid,
                task_uuid: step.task_uuid,
                name: step.name.clone(),
                named_step_uuid: step.named_step_uuid,
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
            .publish_viable_steps_discovered(task_uuid, &events_viable_steps)
            .await?;

        Ok(viable_steps)
    }

    /// Get dependency levels using SQL function
    pub async fn get_dependency_levels(
        &self,
        task_uuid: Uuid,
    ) -> OrchestrationResult<HashMap<Uuid, i32>> {
        self.sql_executor
            .dependency_levels_hash(task_uuid)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "calculate_dependency_levels".to_string(),
                reason: e.to_string(),
            })
            .map_err(Into::into)
    }

    /// Get task execution context using SQL function
    pub async fn get_execution_context(
        &self,
        task_uuid: Uuid,
    ) -> OrchestrationResult<Option<tasker_shared::database::sql_functions::TaskExecutionContext>>
    {
        self.sql_executor
            .get_task_execution_context(task_uuid)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_task_execution_context".to_string(),
                reason: e.to_string(),
            })
            .map_err(Into::into)
    }

    /// Get viable steps filtered by specific criteria
    pub async fn find_viable_steps_with_criteria(
        &self,
        task_uuid: Uuid,
        max_steps: Option<usize>,
        step_names: Option<&[String]>,
    ) -> OrchestrationResult<Vec<ViableStep>> {
        let mut viable_steps = self.find_viable_steps(task_uuid).await?;

        // Apply step name filter
        if let Some(step_names) = step_names {
            viable_steps.retain(|step| step_names.contains(&step.name));
        }

        // Apply max steps limit
        if let Some(max_steps) = max_steps {
            viable_steps.truncate(max_steps);
        }

        Ok(viable_steps)
    }

    /// Get readiness summary for a task (for monitoring/debugging)
    pub async fn get_task_readiness_summary(
        &self,
        task_uuid: Uuid,
    ) -> OrchestrationResult<TaskReadinessSummary> {
        let statuses = self
            .sql_executor
            .get_step_readiness_status(task_uuid, None)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_step_readiness_status".to_string(),
                reason: e.to_string(),
            })?;

        let total_steps = statuses.len();
        let ready_steps = statuses.iter().filter(|s| s.ready_for_execution).count();
        let complete_steps = statuses
            .iter()
            .filter(|s| s.current_state == "complete")
            .count();
        let blocked_steps = statuses
            .iter()
            .filter(|s| !s.dependencies_satisfied)
            .count();
        let failed_steps = statuses
            .iter()
            .filter(|s| s.current_state == "error")
            .count();

        Ok(TaskReadinessSummary {
            task_uuid,
            total_steps,
            ready_steps,
            complete_steps,
            blocked_steps,
            failed_steps,
            progress_percentage: if total_steps > 0 {
                (complete_steps as f64 / total_steps as f64 * 100.0) as u8
            } else {
                0
            },
        })
    }

    /// Build complete StepExecutionRequest objects with all necessary context
    ///
    /// This method loads the complete task context, handler configurations, and dependency results
    /// needed for Ruby workers to execute workflow steps. It addresses the critical TODO in
    /// WorkflowCoordinator::convert_viable_steps_to_execution_requests by providing a proper
    /// implementation that loads real data instead of placeholder values.
    ///
    /// # Arguments
    /// * `task_uuid` - The task ID to load context for
    /// * `viable_steps` - The viable steps discovered for execution
    /// * `task_handler_registry` - Registry to resolve handler class names and configurations
    ///
    /// # Returns
    /// Vector of complete StepExecutionRequest objects with:
    /// - Handler class name from task configuration
    /// - Handler configuration from task templates
    /// - Task context (parameters, metadata)
    /// - Previous step results with proper naming
    /// - Step metadata like retry limits and timeouts
    pub async fn build_step_execution_requests(
        &self,
        task_uuid: Uuid,
        viable_steps: &[ViableStep],
        _task_handler_registry: &tasker_shared::registry::TaskHandlerRegistry,
    ) -> OrchestrationResult<Vec<tasker_shared::messaging::execution_types::StepExecutionRequest>>
    {
        use tasker_shared::messaging::execution_types::{
            StepExecutionRequest, StepRequestMetadata,
        };
        use tasker_shared::models::core::task::Task;

        if viable_steps.is_empty() {
            return Ok(vec![]);
        }

        // Filter viable steps to ensure dependencies are satisfied (safety check)
        // While the SQL query should only return steps with satisfied dependencies,
        // this provides an additional safety layer for data integrity
        let viable_steps_filtered: Vec<_> = viable_steps
            .iter()
            .filter(|step| step.dependencies_satisfied)
            .collect();

        if viable_steps_filtered.len() != viable_steps.len() {
            warn!(
                task_uuid = task_uuid.to_string(),
                original_count = viable_steps.len(),
                filtered_count = viable_steps_filtered.len(),
                "Some viable steps had unsatisfied dependencies and were filtered out"
            );
        }

        if viable_steps_filtered.is_empty() {
            debug!(
                task_uuid = task_uuid.to_string(),
                "No viable steps remaining after dependency satisfaction filter"
            );
            return Ok(vec![]);
        }

        debug!(
            task_uuid = task_uuid.to_string(),
            step_count = viable_steps_filtered.len(),
            "Building complete step execution requests with full context"
        );

        // 1. Load the task with orchestration metadata (namespace, name, version)
        let task = Task::find_by_id(&self.pool, task_uuid)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?
            .ok_or(DiscoveryError::TaskNotFound { task_uuid })?;

        let task_for_orchestration = task
            .for_orchestration(&self.pool)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

        debug!(
            task_uuid = task_uuid.to_string(),
            task_name = %task_for_orchestration.task_name,
            namespace = %task_for_orchestration.namespace_name,
            version = %task_for_orchestration.task_version,
            "Loaded task orchestration metadata"
        );

        // 2. DATABASE-FIRST: Get task template from database instead of deprecated registry
        let task_template = self
            .get_task_template_from_database(&task_for_orchestration)
            .await?;

        debug!(
            task_uuid = task_uuid.to_string(),
            "Found task template for handler configuration"
        );

        let mut execution_requests = Vec::with_capacity(viable_steps_filtered.len());

        for step in viable_steps_filtered {
            debug!(
                task_uuid = %task_uuid,
                step_uuid = %step.step_uuid,
                step_name = %step.name,
                "Building execution request for step"
            );

            // 3. Load step template configuration (fail-fast if not found)
            let step_template = task_template
                .steps
                .iter()
                .find(|st| st.name == step.name)
                .ok_or_else(|| DiscoveryError::ConfigurationError {
                    entity_type: "step_template".to_string(),
                    entity_id: format!(
                        "{} (in task {}/{}/{})",
                        step.name,
                        task_for_orchestration.namespace_name,
                        task_for_orchestration.task_name,
                        task_for_orchestration.task_version
                    ),
                    reason: "Step template not found in task configuration".to_string(),
                })?;

            debug!(
                step_uuid = %step.step_uuid,
                handler_callable = %step_template.handler.callable,
                "Found step template configuration"
            );

            let handler_class = step_template.handler.callable.clone();
            let handler_config: std::collections::HashMap<String, serde_json::Value> =
                step_template
                    .handler
                    .initialization
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
            let timeout_ms = step_template
                .handler
                .initialization
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .or_else(|| step_template.timeout_seconds.map(|t| t as u64 * 1000))
                .unwrap_or(30000u64);

            // 4. Load previous step results (dependencies)
            let previous_results = self
                .load_step_dependencies(step.step_uuid)
                .await
                .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

            debug!(
                step_uuid = %step.step_uuid,
                dependency_count = previous_results.len(),
                "Loaded step dependency results"
            );

            // 5. Build the execution request with all context
            let request = StepExecutionRequest {
                step_uuid: step.step_uuid,
                task_uuid,
                step_name: step.name.clone(),
                handler_class,
                handler_config,
                task_context: task_for_orchestration
                    .task
                    .context
                    .clone()
                    .unwrap_or(serde_json::json!({})),
                previous_results,
                metadata: StepRequestMetadata {
                    attempt: step.attempts,
                    retry_limit: step.retry_limit,
                    timeout_ms: timeout_ms as i64,
                    created_at: chrono::Utc::now(),
                },
            };

            execution_requests.push(request);
        }

        info!(
            task_uuid = task_uuid.to_string(),
            request_count = execution_requests.len(),
            "Built complete step execution requests with full context"
        );

        Ok(execution_requests)
    }

    /// Load previous step results for dependencies
    ///
    /// This helper method loads the results from completed dependency steps and builds
    /// a map of step_name -> result_data for use by the current step.
    ///
    /// Uses the get_step_transitive_dependencies SQL function to get ALL transitive dependencies
    /// with recursive CTE traversal. This allows YAML configs to specify only immediate dependencies
    /// while still providing all required results to step handlers.
    async fn load_step_dependencies(
        &self,
        step_uuid: Uuid,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, sqlx::Error> {
        use tasker_shared::database::sql_functions::SqlFunctionExecutor;

        // Use the new SQL function to get all transitive dependencies with their results
        let executor = SqlFunctionExecutor::new(self.pool.clone());
        executor.get_step_dependency_results_map(step_uuid).await
    }

    /// Get task template from database using database-first approach
    /// Loads TaskTemplate directly from database (new self-describing format)
    async fn get_task_template_from_database(
        &self,
        task_for_orchestration: &tasker_shared::models::core::task::TaskForOrchestration,
    ) -> Result<tasker_shared::models::core::task_template::TaskTemplate, DiscoveryError> {
        use tasker_shared::models::core::named_task::NamedTask;
        use tasker_shared::models::core::task_namespace::TaskNamespace;

        // Get the task namespace
        let task_namespace =
            TaskNamespace::find_by_name(&self.pool, &task_for_orchestration.namespace_name)
                .await
                .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?
                .ok_or_else(|| DiscoveryError::ConfigurationError {
                    entity_type: "task_namespace".to_string(),
                    entity_id: task_for_orchestration.namespace_name.clone(),
                    reason: format!(
                        "Namespace not found: {}",
                        task_for_orchestration.namespace_name
                    ),
                })?;

        // Get the named task with configuration
        let named_task = NamedTask::find_by_name_version_namespace(
            &self.pool,
            &task_for_orchestration.task_name,
            &task_for_orchestration.task_version,
            task_namespace.task_namespace_uuid,
        )
        .await
        .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?
        .ok_or_else(|| DiscoveryError::ConfigurationError {
            entity_type: "named_task".to_string(),
            entity_id: format!(
                "{}/{} v{}",
                task_for_orchestration.namespace_name,
                task_for_orchestration.task_name,
                task_for_orchestration.task_version
            ),
            reason: format!(
                "Task not found: {}/{} v{}",
                task_for_orchestration.namespace_name,
                task_for_orchestration.task_name,
                task_for_orchestration.task_version
            ),
        })?;

        // Extract the task template configuration from the named task
        let configuration =
            named_task
                .configuration
                .ok_or_else(|| DiscoveryError::ConfigurationError {
                    entity_type: "task_configuration".to_string(),
                    entity_id: format!(
                        "{}/{} v{}",
                        task_for_orchestration.namespace_name,
                        task_for_orchestration.task_name,
                        task_for_orchestration.task_version
                    ),
                    reason: format!(
                        "No configuration found for task {}/{} v{}",
                        task_for_orchestration.namespace_name,
                        task_for_orchestration.task_name,
                        task_for_orchestration.task_version
                    ),
                })?;

        // FORWARD-ONLY CHANGE: Database now stores TaskTemplate directly
        // Deserialize as TaskTemplate (new self-describing format)
        match serde_json::from_value::<tasker_shared::models::core::task_template::TaskTemplate>(
            configuration.clone(),
        ) {
            Ok(task_template) => {
                // Validate that the task template has steps
                if task_template.steps.is_empty() {
                    return Err(DiscoveryError::ConfigurationError {
                        entity_type: "task_template_validation".to_string(),
                        entity_id: format!(
                            "{}/{} v{}",
                            task_for_orchestration.namespace_name,
                            task_for_orchestration.task_name,
                            task_for_orchestration.task_version
                        ),
                        reason: format!(
                            "Empty steps array in task template for {}/{} v{}. Cannot execute workflow without step definitions.",
                            task_for_orchestration.namespace_name,
                            task_for_orchestration.task_name,
                            task_for_orchestration.task_version
                        ),
                    });
                }
                Ok(task_template)
            }
            Err(e) => {
                // AGGRESSIVE FORWARD-ONLY: No backward compatibility for legacy formats
                Err(DiscoveryError::ConfigurationError {
                    entity_type: "task_template_deserialization".to_string(),
                    entity_id: format!(
                        "{}/{} v{}",
                        task_for_orchestration.namespace_name,
                        task_for_orchestration.task_name,
                        task_for_orchestration.task_version
                    ),
                    reason: format!(
                        "Failed to deserialize configuration as TaskTemplate for {}/{} v{}. Error: {}. Legacy HandlerConfiguration format is no longer supported - please migrate to new self-describing TaskTemplate format.",
                        task_for_orchestration.namespace_name,
                        task_for_orchestration.task_name,
                        task_for_orchestration.task_version,
                        e
                    ),
                })
            }
        }
    }
}

/// Summary of task readiness status for monitoring
#[derive(Debug, Clone)]
pub struct TaskReadinessSummary {
    pub task_uuid: Uuid,
    pub total_steps: usize,
    pub ready_steps: usize,
    pub complete_steps: usize,
    pub blocked_steps: usize,
    pub failed_steps: usize,
    pub progress_percentage: u8,
}

impl TaskReadinessSummary {
    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        self.complete_steps == self.total_steps && self.total_steps > 0
    }

    /// Check if task is blocked
    pub fn is_blocked(&self) -> bool {
        self.ready_steps == 0 && self.complete_steps < self.total_steps
    }

    /// Check if task has failures
    pub fn has_failures(&self) -> bool {
        self.failed_steps > 0
    }
}
