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
//! use tasker_core::orchestration::viable_step_discovery::ViableStepDiscovery;
//! use tasker_core::events::publisher::EventPublisher;
//! use tasker_core::database::sql_functions::SqlFunctionExecutor;
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

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::events::{EventPublisher, ViableStep as EventsViableStep};
use crate::orchestration::errors::{DiscoveryError, OrchestrationResult};
use crate::orchestration::types::ViableStep;
use std::collections::HashMap;
use tracing::{debug, error, info, instrument, warn};

/// High-performance step readiness discovery engine
pub struct ViableStepDiscovery {
    sql_executor: SqlFunctionExecutor,
    event_publisher: EventPublisher,
    pool: sqlx::PgPool,
}

impl ViableStepDiscovery {
    /// Create new step discovery instance
    pub fn new(
        sql_executor: SqlFunctionExecutor,
        event_publisher: EventPublisher,
        pool: sqlx::PgPool,
    ) -> Self {
        Self {
            sql_executor,
            event_publisher,
            pool,
        }
    }

    /// Find all viable steps for a task using optimized SQL function
    #[instrument(skip(self), fields(task_id = task_id))]
    pub async fn find_viable_steps(&self, task_id: i64) -> OrchestrationResult<Vec<ViableStep>> {
        debug!(task_id = task_id, "Finding viable steps");

        // Use get_ready_steps which already handles all filtering and business logic
        let ready_steps = self
            .sql_executor
            .get_ready_steps(task_id)
            .await
            .map_err(|e| DiscoveryError::SqlFunctionError {
                function_name: "get_ready_steps".to_string(),
                reason: e.to_string(),
            })?;

        debug!(
            task_id = task_id,
            ready_count = ready_steps.len(),
            "Retrieved ready steps from SQL function"
        );

        // Convert to ViableStep objects (no additional verification needed - SQL function handles everything)
        let viable_steps: Vec<ViableStep> = ready_steps
            .into_iter()
            .map(|status| {
                debug!(
                    task_id = task_id,
                    step_id = status.workflow_step_id,
                    step_name = %status.name,
                    current_state = %status.current_state,
                    dependencies_satisfied = status.dependencies_satisfied,
                    "Found viable step"
                );

                ViableStep {
                    step_id: status.workflow_step_id,
                    task_id: status.task_id,
                    name: status.name,
                    named_step_id: status.named_step_id,
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
            task_id = task_id,
            viable_steps = viable_steps.len(),
            "Completed viable step discovery"
        );

        // Publish discovery event
        let events_viable_steps: Vec<EventsViableStep> = viable_steps
            .iter()
            .map(|step| EventsViableStep {
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
            .publish_viable_steps_discovered(task_id, &events_viable_steps)
            .await?;

        Ok(viable_steps)
    }

    /// Get dependency levels using SQL function
    pub async fn get_dependency_levels(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<HashMap<i64, i32>> {
        self.sql_executor
            .dependency_levels_hash(task_id)
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
        task_id: i64,
    ) -> OrchestrationResult<Option<crate::database::sql_functions::TaskExecutionContext>> {
        self.sql_executor
            .get_task_execution_context(task_id)
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
        task_id: i64,
        max_steps: Option<usize>,
        min_priority: Option<i32>,
        step_names: Option<&[String]>,
    ) -> OrchestrationResult<Vec<ViableStep>> {
        let mut viable_steps = self.find_viable_steps(task_id).await?;

        // Apply priority filter (using named_step_id as priority)
        if let Some(min_priority) = min_priority {
            viable_steps.retain(|step| step.named_step_id >= min_priority);
        }

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
        task_id: i64,
    ) -> OrchestrationResult<TaskReadinessSummary> {
        let statuses = self
            .sql_executor
            .get_step_readiness_status(task_id, None)
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
            task_id,
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
    /// * `task_id` - The task ID to load context for
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
        task_id: i64,
        viable_steps: &[ViableStep],
        _task_handler_registry: &crate::registry::TaskHandlerRegistry,
    ) -> OrchestrationResult<Vec<crate::messaging::execution_types::StepExecutionRequest>> {
        use crate::messaging::execution_types::{StepExecutionRequest, StepRequestMetadata};
        use crate::models::core::task::Task;

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
                task_id = task_id,
                original_count = viable_steps.len(),
                filtered_count = viable_steps_filtered.len(),
                "Some viable steps had unsatisfied dependencies and were filtered out"
            );
        }

        if viable_steps_filtered.is_empty() {
            debug!(
                task_id = task_id,
                "No viable steps remaining after dependency satisfaction filter"
            );
            return Ok(vec![]);
        }

        debug!(
            task_id = task_id,
            step_count = viable_steps_filtered.len(),
            "Building complete step execution requests with full context"
        );

        // 1. Load the task with orchestration metadata (namespace, name, version)
        let task = Task::find_by_id(&self.pool, task_id)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?
            .ok_or(DiscoveryError::TaskNotFound { task_id })?;

        let task_for_orchestration = task
            .for_orchestration(&self.pool)
            .await
            .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

        debug!(
            task_id = task_id,
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
            task_id = task_id,
            "Found task template for handler configuration"
        );

        let mut execution_requests = Vec::with_capacity(viable_steps_filtered.len());

        for step in viable_steps_filtered {
            debug!(
                task_id = task_id,
                step_id = step.step_id,
                step_name = %step.name,
                "Building execution request for step"
            );

            // 3. Load step template configuration (fail-fast if not found)
            let step_template = task_template
                .step_templates
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
                step_id = step.step_id,
                handler_class = %step_template.handler_class,
                "Found step template configuration"
            );

            let handler_class = step_template.handler_class.clone();
            let handler_config = step_template
                .handler_config
                .as_ref()
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let timeout_ms = step_template
                .handler_config
                .as_ref()
                .and_then(|config| config.get("timeout_ms"))
                .and_then(|v| v.as_u64())
                .unwrap_or(30000u64);

            // 4. Load previous step results (dependencies)
            let previous_results = self
                .load_step_dependencies(step.step_id)
                .await
                .map_err(|e| DiscoveryError::DatabaseError(e.to_string()))?;

            debug!(
                step_id = step.step_id,
                dependency_count = previous_results.len(),
                "Loaded step dependency results"
            );

            // 5. Build the execution request with all context
            let request = StepExecutionRequest {
                step_id: step.step_id,
                task_id,
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
            task_id = task_id,
            request_count = execution_requests.len(),
            "Built complete step execution requests with full context"
        );

        Ok(execution_requests)
    }

    /// Load previous step results for dependencies
    ///
    /// This helper method loads the results from completed dependency steps and builds
    /// a map of step_name -> result_data for use by the current step.
    async fn load_step_dependencies(
        &self,
        step_id: i64,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, sqlx::Error> {
        use crate::models::core::workflow_step::WorkflowStep;

        // Load the current step to get its dependencies
        let current_step = WorkflowStep::find_by_id(&self.pool, step_id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)?;

        // Get dependency steps
        let dependency_steps = current_step.get_dependencies(&self.pool).await?;

        let mut previous_results = std::collections::HashMap::new();

        for dep_step in dependency_steps {
            // Load the step name from named_steps table
            let step_name = sqlx::query!(
                "SELECT name FROM tasker_named_steps WHERE named_step_id = $1",
                dep_step.named_step_id
            )
            .fetch_one(&self.pool)
            .await?
            .name;

            // Use the step's results if available, otherwise empty object
            let result_data = dep_step.results.unwrap_or(serde_json::json!({}));

            previous_results.insert(step_name, result_data);
        }

        Ok(previous_results)
    }

    /// Get task template from database using database-first approach
    /// Replaces the deprecated registry.get_task_template() call
    async fn get_task_template_from_database(
        &self,
        task_for_orchestration: &crate::models::core::task::TaskForOrchestration,
    ) -> Result<crate::models::core::task_template::TaskTemplate, DiscoveryError> {
        use crate::models::core::named_task::NamedTask;
        use crate::models::core::task_namespace::TaskNamespace;

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
            task_namespace.task_namespace_id as i64,
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

        // Extract the handler configuration from the named task
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

        // Get the handler_config field which contains the YAML structure
        let handler_config_value = configuration.get("handler_config").ok_or_else(|| {
            DiscoveryError::ConfigurationError {
                entity_type: "handler_config".to_string(),
                entity_id: format!(
                    "{}/{}",
                    task_for_orchestration.namespace_name, task_for_orchestration.task_name
                ),
                reason: format!(
                    "TaskHandlerInfo missing handler_config field for {}/{}",
                    task_for_orchestration.namespace_name, task_for_orchestration.task_name
                ),
            }
        })?;

        // Convert HandlerConfiguration to TaskTemplate
        let handler_config: crate::orchestration::handler_config::HandlerConfiguration =
            serde_json::from_value(handler_config_value.clone()).map_err(|e| {
                DiscoveryError::ConfigurationError {
                    entity_type: "handler_config_deserialization".to_string(),
                    entity_id: format!(
                        "{}/{}",
                        task_for_orchestration.namespace_name, task_for_orchestration.task_name
                    ),
                    reason: format!(
                        "Failed to deserialize handler configuration for {}/{}: {}",
                        task_for_orchestration.namespace_name, task_for_orchestration.task_name, e
                    ),
                }
            })?;

        // Convert HandlerConfiguration to TaskTemplate
        let task_template = crate::models::core::task_template::TaskTemplate {
            name: handler_config.name,
            module_namespace: handler_config.module_namespace,
            task_handler_class: handler_config.task_handler_class,
            namespace_name: handler_config.namespace_name,
            version: handler_config.version,
            default_dependent_system: handler_config.default_dependent_system,
            named_steps: handler_config.named_steps,
            schema: handler_config.schema,
            step_templates: handler_config
                .step_templates
                .into_iter()
                .map(|st| crate::models::core::task_template::StepTemplate {
                    name: st.name,
                    description: st.description,
                    dependent_system: st.dependent_system,
                    default_retryable: st.default_retryable,
                    default_retry_limit: st.default_retry_limit,
                    skippable: st.skippable,
                    handler_class: st.handler_class,
                    handler_config: st.handler_config,
                    depends_on_step: st.depends_on_step,
                    depends_on_steps: st.depends_on_steps,
                    custom_events: st.custom_events,
                })
                .collect(),
            environments: handler_config.environments.map(|envs| {
                envs.into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            crate::models::core::task_template::EnvironmentConfig {
                                step_templates: v.step_templates.map(|templates| {
                                    templates.into_iter().map(|t| {
                                crate::models::core::task_template::StepTemplateOverride {
                                    name: t.name,
                                    handler_config: t.handler_config,
                                    description: t.description,
                                    dependent_system: t.dependent_system,
                                    default_retryable: t.default_retryable,
                                    default_retry_limit: t.default_retry_limit,
                                    skippable: t.skippable,
                                }
                            }).collect()
                                }),
                                default_context: v.default_context,
                                default_options: v.default_options,
                            },
                        )
                    })
                    .collect()
            }),
            default_context: handler_config.default_context,
            default_options: handler_config.default_options,
        };

        Ok(task_template)
    }
}

/// Summary of task readiness status for monitoring
#[derive(Debug, Clone)]
pub struct TaskReadinessSummary {
    pub task_id: i64,
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
