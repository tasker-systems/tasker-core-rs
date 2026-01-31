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
//! ```rust,no_run
//! use tasker_orchestration::orchestration::viable_step_discovery::ViableStepDiscovery;
//! use tasker_shared::system_context::SystemContext;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create orchestration system context (TAS-50 Phase 2: context-specific loading)
//! let context = Arc::new(SystemContext::new_for_orchestration().await?);
//! let discovery = ViableStepDiscovery::new(context);
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
use tasker_shared::events::EventPublisher;
use tasker_shared::events::ViableStep;
use tasker_shared::system_context::SystemContext;
use tasker_shared::StepExecutionResult;
use tracing::{debug, info, instrument, warn};

/// High-performance step readiness discovery engine
#[derive(Debug)]
pub struct ViableStepDiscovery {
    sql_executor: SqlFunctionExecutor,
    event_publisher: Arc<EventPublisher>,
    pool: sqlx::PgPool,
}

impl ViableStepDiscovery {
    /// Create new step discovery instance
    pub fn new(system_context: Arc<SystemContext>) -> Self {
        let sql_executor = SqlFunctionExecutor::new(system_context.database_pool().clone());
        Self {
            sql_executor,
            event_publisher: system_context.event_publisher.clone(),
            pool: system_context.database_pool().clone(),
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

        // Convert to ViableStep objects with SQL type conversions
        // (no additional verification needed - SQL function handles everything)
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
                    // Convert from SQL i32 to u32
                    attempts: status.attempts as u32,
                    max_attempts: status.max_attempts as u32,
                    // Convert from SQL NaiveDateTime to DateTime<Utc>
                    last_failure_at: status.last_failure_at.map(|dt| dt.and_utc()),
                    next_retry_at: status.next_retry_at.map(|dt| dt.and_utc()),
                }
            })
            .collect();

        info!(
            task_uuid = task_uuid.to_string(),
            viable_steps = viable_steps.len(),
            "Completed viable step discovery"
        );

        // Publish discovery event (ViableStep is now the unified type)
        self.event_publisher
            .publish_viable_steps_discovered(task_uuid, &viable_steps)
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

        let task_label = format!(
            "{}/{}/{}",
            task_for_orchestration.namespace_name,
            task_for_orchestration.task_name,
            task_for_orchestration.task_version
        );

        let mut execution_requests = Vec::with_capacity(viable_steps_filtered.len());

        for step in viable_steps_filtered {
            // Load previous step results (dependencies) — async DB call stays in caller
            let previous_results = self
                .load_step_dependencies(step.step_uuid)
                .await
                .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

            debug!(
                step_uuid = %step.step_uuid,
                dependency_count = previous_results.len(),
                "Loaded step dependency results"
            );

            let request = build_single_execution_request(
                step,
                task_uuid,
                &task_label,
                task_for_orchestration.task.context.as_ref(),
                &task_template,
                previous_results,
            )?;

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
    ) -> Result<std::collections::HashMap<String, StepExecutionResult>, sqlx::Error> {
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

/// Build a single step execution request from pre-loaded context.
///
/// Pure function extracting the per-step logic from `build_step_execution_requests`:
/// template lookup, handler configuration extraction, timeout resolution, and
/// request construction. The async dependency loading stays in the caller.
///
/// # Timeout Resolution
///
/// Timeout is resolved with a three-level fallback:
/// 1. `handler.initialization.timeout_ms` (explicit override)
/// 2. `step_template.timeout_seconds * 1000` (template-level)
/// 3. `30000ms` (system default)
fn build_single_execution_request(
    step: &ViableStep,
    task_uuid: Uuid,
    task_label: &str,
    task_context: Option<&serde_json::Value>,
    task_template: &tasker_shared::models::core::task_template::TaskTemplate,
    previous_results: std::collections::HashMap<String, StepExecutionResult>,
) -> OrchestrationResult<tasker_shared::messaging::execution_types::StepExecutionRequest> {
    use tasker_shared::messaging::execution_types::{StepExecutionRequest, StepRequestMetadata};

    // Find step template configuration (fail-fast if not found)
    let step_template = task_template
        .steps
        .iter()
        .find(|st| st.name == step.name)
        .ok_or_else(|| DiscoveryError::ConfigurationError {
            entity_type: "step_template".to_string(),
            entity_id: format!("{} (in task {})", step.name, task_label),
            reason: "Step template not found in task configuration".to_string(),
        })?;

    debug!(
        step_uuid = %step.step_uuid,
        handler_callable = %step_template.handler.callable,
        "Found step template configuration"
    );

    let handler_class = step_template.handler.callable.clone();
    let handler_config: std::collections::HashMap<String, serde_json::Value> = step_template
        .handler
        .initialization
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Timeout resolution: initialization.timeout_ms > timeout_seconds > 30000ms default
    let timeout_ms = step_template
        .handler
        .initialization
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| step_template.timeout_seconds.map(|t| t as u64 * 1000))
        .unwrap_or(30000u64);

    Ok(StepExecutionRequest {
        step_uuid: step.step_uuid,
        task_uuid,
        step_name: step.name.clone(),
        handler_class,
        handler_config,
        task_context: task_context.cloned().unwrap_or(serde_json::json!({})),
        previous_results,
        metadata: StepRequestMetadata {
            attempt: step.attempts as i32,
            max_attempts: step.max_attempts as i32,
            timeout_ms: timeout_ms as i64,
            created_at: chrono::Utc::now(),
        },
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_summary(
        total: usize,
        ready: usize,
        complete: usize,
        blocked: usize,
        failed: usize,
    ) -> TaskReadinessSummary {
        TaskReadinessSummary {
            task_uuid: Uuid::now_v7(),
            total_steps: total,
            ready_steps: ready,
            complete_steps: complete,
            blocked_steps: blocked,
            failed_steps: failed,
            progress_percentage: if total > 0 {
                ((complete * 100) / total) as u8
            } else {
                0
            },
        }
    }

    // --- is_complete ---

    #[test]
    fn test_is_complete_all_done() {
        let summary = make_summary(5, 0, 5, 0, 0);
        assert!(summary.is_complete());
    }

    #[test]
    fn test_is_complete_none_done() {
        let summary = make_summary(5, 3, 0, 2, 0);
        assert!(!summary.is_complete());
    }

    #[test]
    fn test_is_complete_zero_total() {
        // Zero total steps means not complete (guard against division-by-zero-style bugs)
        let summary = make_summary(0, 0, 0, 0, 0);
        assert!(!summary.is_complete());
    }

    #[test]
    fn test_is_complete_partial() {
        let summary = make_summary(5, 1, 3, 1, 0);
        assert!(!summary.is_complete());
    }

    // --- is_blocked ---

    #[test]
    fn test_is_blocked_no_ready_and_incomplete() {
        let summary = make_summary(5, 0, 2, 3, 0);
        assert!(summary.is_blocked());
    }

    #[test]
    fn test_is_blocked_has_ready_steps() {
        let summary = make_summary(5, 2, 1, 2, 0);
        assert!(!summary.is_blocked());
    }

    #[test]
    fn test_is_blocked_all_complete() {
        // All complete means complete_steps == total_steps, so not blocked
        let summary = make_summary(5, 0, 5, 0, 0);
        assert!(!summary.is_blocked());
    }

    #[test]
    fn test_is_blocked_zero_total() {
        // Zero total: ready_steps == 0 && complete_steps (0) < total_steps (0) is false
        let summary = make_summary(0, 0, 0, 0, 0);
        assert!(!summary.is_blocked());
    }

    // --- has_failures ---

    #[test]
    fn test_has_failures_some() {
        let summary = make_summary(5, 1, 2, 1, 1);
        assert!(summary.has_failures());
    }

    #[test]
    fn test_has_failures_none() {
        let summary = make_summary(5, 2, 3, 0, 0);
        assert!(!summary.has_failures());
    }

    // --- Edge cases ---

    #[test]
    fn test_all_failed() {
        let summary = make_summary(3, 0, 0, 0, 3);
        assert!(!summary.is_complete());
        assert!(summary.is_blocked()); // 0 ready, 0 < 3 total
        assert!(summary.has_failures());
    }

    #[test]
    fn test_mixed_state() {
        let summary = make_summary(10, 2, 5, 2, 1);
        assert!(!summary.is_complete());
        assert!(!summary.is_blocked()); // has 2 ready
        assert!(summary.has_failures());
        assert_eq!(summary.progress_percentage, 50);
    }

    #[test]
    fn test_single_step_complete() {
        let summary = make_summary(1, 0, 1, 0, 0);
        assert!(summary.is_complete());
        assert!(!summary.is_blocked());
        assert!(!summary.has_failures());
        assert_eq!(summary.progress_percentage, 100);
    }

    #[test]
    fn test_single_step_pending() {
        let summary = make_summary(1, 1, 0, 0, 0);
        assert!(!summary.is_complete());
        assert!(!summary.is_blocked()); // has 1 ready
        assert!(!summary.has_failures());
        assert_eq!(summary.progress_percentage, 0);
    }

    // --- build_single_execution_request ---

    use tasker_shared::models::core::task_template::{
        HandlerDefinition, StepDefinition, TaskTemplate,
    };

    fn make_handler(callable: &str) -> HandlerDefinition {
        HandlerDefinition::builder()
            .callable(callable.to_string())
            .build()
    }

    fn make_step_def(name: &str, handler: HandlerDefinition) -> StepDefinition {
        StepDefinition::builder()
            .name(name.to_string())
            .handler(handler)
            .build()
    }

    fn make_task_template(steps: Vec<StepDefinition>) -> TaskTemplate {
        TaskTemplate::builder()
            .name("test_task".to_string())
            .namespace_name("default".to_string())
            .version("1.0.0".to_string())
            .steps(steps)
            .build()
    }

    fn make_viable_step(name: &str) -> ViableStep {
        ViableStep {
            step_uuid: Uuid::now_v7(),
            task_uuid: Uuid::now_v7(),
            name: name.to_string(),
            named_step_uuid: Uuid::now_v7(),
            current_state: "pending".to_string(),
            dependencies_satisfied: true,
            retry_eligible: false,
            attempts: 1,
            max_attempts: 3,
            last_failure_at: None,
            next_retry_at: None,
        }
    }

    #[test]
    fn test_build_request_success() {
        let step = make_viable_step("validate");
        let task_uuid = Uuid::now_v7();
        let handler = make_handler("MyApp::ValidateHandler");
        let template = make_task_template(vec![make_step_def("validate", handler)]);

        let result = super::build_single_execution_request(
            &step,
            task_uuid,
            "default/test_task/1.0.0",
            None,
            &template,
            std::collections::HashMap::new(),
        );

        let request = result.unwrap();
        assert_eq!(request.step_uuid, step.step_uuid);
        assert_eq!(request.task_uuid, task_uuid);
        assert_eq!(request.step_name, "validate");
        assert_eq!(request.handler_class, "MyApp::ValidateHandler");
        assert_eq!(request.task_context, serde_json::json!({}));
        assert_eq!(request.metadata.attempt, 1);
        assert_eq!(request.metadata.max_attempts, 3);
        assert_eq!(request.metadata.timeout_ms, 30000); // default
    }

    #[test]
    fn test_build_request_with_task_context() {
        let step = make_viable_step("process");
        let template = make_task_template(vec![make_step_def("process", make_handler("Process"))]);
        let ctx = serde_json::json!({"key": "value"});

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            Some(&ctx),
            &template,
            std::collections::HashMap::new(),
        );

        assert_eq!(
            result.unwrap().task_context,
            serde_json::json!({"key": "value"})
        );
    }

    #[test]
    fn test_build_request_step_not_found() {
        let step = make_viable_step("missing_step");
        let template = make_task_template(vec![make_step_def("other_step", make_handler("Other"))]);

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            None,
            &template,
            std::collections::HashMap::new(),
        );

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing_step"));
    }

    #[test]
    fn test_build_request_timeout_from_initialization() {
        let step = make_viable_step("step_a");
        let mut handler = make_handler("Handler");
        handler
            .initialization
            .insert("timeout_ms".to_string(), serde_json::json!(60000));
        let mut step_def = make_step_def("step_a", handler);
        step_def.timeout_seconds = Some(10); // should be overridden by initialization
        let template = make_task_template(vec![step_def]);

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            None,
            &template,
            std::collections::HashMap::new(),
        );

        assert_eq!(result.unwrap().metadata.timeout_ms, 60000);
    }

    #[test]
    fn test_build_request_timeout_from_step_seconds() {
        let step = make_viable_step("step_b");
        let mut step_def = make_step_def("step_b", make_handler("Handler"));
        step_def.timeout_seconds = Some(45);
        let template = make_task_template(vec![step_def]);

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            None,
            &template,
            std::collections::HashMap::new(),
        );

        assert_eq!(result.unwrap().metadata.timeout_ms, 45000);
    }

    #[test]
    fn test_build_request_handler_config_extracted() {
        let step = make_viable_step("step_c");
        let mut handler = make_handler("ConfigHandler");
        handler.initialization.insert(
            "api_url".to_string(),
            serde_json::json!("https://api.example.com"),
        );
        handler
            .initialization
            .insert("retries".to_string(), serde_json::json!(3));
        let template = make_task_template(vec![make_step_def("step_c", handler)]);

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            None,
            &template,
            std::collections::HashMap::new(),
        );

        let request = result.unwrap();
        assert_eq!(
            request.handler_config.get("api_url"),
            Some(&serde_json::json!("https://api.example.com"))
        );
        assert_eq!(
            request.handler_config.get("retries"),
            Some(&serde_json::json!(3))
        );
    }

    #[test]
    fn test_build_request_with_previous_results() {
        let step = make_viable_step("step_d");
        let template = make_task_template(vec![make_step_def("step_d", make_handler("Handler"))]);

        let mut previous = std::collections::HashMap::new();
        previous.insert(
            "step_a".to_string(),
            tasker_shared::StepExecutionResult {
                step_uuid: Uuid::now_v7(),
                success: true,
                result: serde_json::json!({"output": "data"}),
                status: "completed".to_string(),
                ..Default::default()
            },
        );

        let result = super::build_single_execution_request(
            &step,
            Uuid::now_v7(),
            "ns/task/1.0",
            None,
            &template,
            previous,
        );

        let request = result.unwrap();
        assert!(request.previous_results.contains_key("step_a"));
    }
}
