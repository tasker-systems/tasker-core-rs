//! Decision Point Service
//!
//! Main service that coordinates dynamic workflow step creation from decision outcomes.

use crate::orchestration::lifecycle::task_initialization::WorkflowStepCreator;
use opentelemetry::KeyValue;
use sqlx::types::Uuid;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
// TAS-61 Phase 6C: Use V2 DecisionPointsConfig
use tasker_shared::config::tasker::DecisionPointsConfig;
use tasker_shared::messaging::DecisionPointOutcome;
use tasker_shared::metrics::orchestration::*;
use tasker_shared::models::core::task_template::{StepDefinition, TaskTemplate};
use tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge;
use tasker_shared::models::{Task, WorkflowStep, WorkflowStepEdge};
use tasker_shared::system_context::SystemContext;
use tracing::{debug, error, info, instrument, warn};

/// Errors that can occur during decision point processing
#[derive(Debug, thiserror::Error)]
pub enum DecisionPointProcessingError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Step '{0}' not found in template")]
    StepNotFoundInTemplate(String),

    #[error("Step '{step_name}' is not a valid descendant of decision point '{decision_name}'")]
    InvalidDescendant {
        step_name: String,
        decision_name: String,
    },

    #[error("Cycle detected: adding edge '{from}' -> '{to}' would create a cycle")]
    CycleDetected { from: String, to: String },

    #[error("Task {0} not found")]
    TaskNotFound(Uuid),

    #[error("Workflow step {0} not found")]
    WorkflowStepNotFound(Uuid),

    #[error("Decision step '{0}' not found in template")]
    DecisionStepNotFound(String),

    #[error("Template serialization error: {0}")]
    TemplateSerialization(String),
}

impl From<sqlx::Error> for DecisionPointProcessingError {
    fn from(error: sqlx::Error) -> Self {
        DecisionPointProcessingError::Database(error.to_string())
    }
}

/// Service for processing decision point outcomes and dynamically creating steps
#[derive(Debug)]
pub struct DecisionPointService {
    context: Arc<SystemContext>,
    step_creator: WorkflowStepCreator,
    config: DecisionPointsConfig,
}

impl DecisionPointService {
    /// Create a new DecisionPointService
    pub fn new(context: Arc<SystemContext>) -> Self {
        let step_creator = WorkflowStepCreator::new(context.clone());
        // TAS-61 Phase 6C: Use V2 config directly (tasker_config is now TaskerConfig)
        let config = context
            .tasker_config
            .orchestration
            .as_ref()
            .map(|o| o.decision_points.clone())
            .unwrap_or_default();
        Self {
            context,
            step_creator,
            config,
        }
    }

    /// Process a decision point outcome and create specified steps
    ///
    /// This is the main entry point for decision point processing. It:
    /// 1. Loads task and template
    /// 2. Validates outcome against template
    /// 3. Creates workflow steps within a transaction
    /// 4. Creates dependencies between decision parent and new steps
    /// 5. Performs cycle detection to maintain DAG integrity
    ///
    /// Returns mapping of step names to created workflow step UUIDs.
    #[instrument(skip(self), fields(
        workflow_step_uuid = %decision_step_uuid,
        task_uuid = %task_uuid,
        step_count = outcome.step_names().len()
    ))]
    pub async fn process_decision_outcome(
        &self,
        decision_step_uuid: Uuid,
        task_uuid: Uuid,
        outcome: DecisionPointOutcome,
    ) -> Result<HashMap<String, Uuid>, DecisionPointProcessingError> {
        // TAS-53 Phase 7: Start timing for metrics
        let start_time = Instant::now();

        // TAS-53 Phase 7: Check if decision points are enabled
        if !self.config.is_enabled() {
            warn!(
                decision_step_uuid = %decision_step_uuid,
                "Decision points are disabled in configuration, skipping processing"
            );
            return Ok(HashMap::new());
        }

        // If no steps to create, return early
        if !outcome.requires_step_creation() {
            // TAS-53 Phase 7: Record metric for no_branches outcome
            decision_outcomes_processed_total().add(
                1,
                &[
                    KeyValue::new("decision_name", "unknown"),
                    KeyValue::new("outcome_type", "no_branches"),
                ],
            );

            debug!(
                decision_step_uuid = %decision_step_uuid,
                "No steps to create (NoBranches outcome)"
            );
            return Ok(HashMap::new());
        }

        let step_names = outcome.step_names();

        // TAS-53 Phase 7: Check maximum limits
        if self.config.exceeds_max_steps(step_names.len()) {
            error!(
                decision_step_uuid = %decision_step_uuid,
                step_count = step_names.len(),
                max_allowed = self.config.max_steps(),
                "Decision outcome exceeds maximum step count"
            );
            return Err(DecisionPointProcessingError::Database(format!(
                "Decision outcome creates {} steps, exceeding maximum of {}",
                step_names.len(),
                self.config.max_steps()
            )));
        }

        // TAS-53 Phase 7: Check warning thresholds
        if self.config.should_warn_steps(step_names.len()) {
            warn!(
                decision_step_uuid = %decision_step_uuid,
                step_count = step_names.len(),
                warn_threshold = self.config.warn_threshold_steps,
                max_allowed = self.config.max_steps(),
                "Decision outcome step count exceeds warning threshold"
            );

            // Record warning metric
            decision_warnings_total().add(
                1,
                &[
                    KeyValue::new("warning_type", "step_count"),
                ],
            );
        }

        info!(
            decision_step_uuid = %decision_step_uuid,
            task_uuid = %task_uuid,
            step_count = step_names.len(),
            steps = ?step_names,
            "Processing decision outcome"
        );

        // Load task and get orchestration metadata
        let task = Task::find_by_id(self.context.database_pool(), task_uuid)
            .await?
            .ok_or(DecisionPointProcessingError::TaskNotFound(task_uuid))?;

        let task_metadata = task
            .for_orchestration(self.context.database_pool())
            .await
            .map_err(|e| {
                DecisionPointProcessingError::Database(format!(
                    "Failed to load task orchestration metadata: {}",
                    e
                ))
            })?;

        // Load workflow step to get named step information
        let decision_workflow_step =
            WorkflowStep::find_by_id(self.context.database_pool(), decision_step_uuid)
                .await?
                .ok_or(DecisionPointProcessingError::WorkflowStepNotFound(
                    decision_step_uuid,
                ))?;

        // Load task template from registry
        let handler_metadata = self
            .context
            .task_handler_registry
            .get_task_template_from_registry(
                &task_metadata.namespace_name,
                &task_metadata.task_name,
                &task_metadata.task_version,
            )
            .await
            .map_err(|e| {
                DecisionPointProcessingError::TemplateSerialization(format!(
                    "Failed to load task template from registry: {}",
                    e
                ))
            })?;

        // Extract TaskTemplate from handler metadata
        let task_template: TaskTemplate =
            serde_json::from_value(handler_metadata.config_schema.ok_or_else(|| {
                DecisionPointProcessingError::TemplateSerialization(
                    "No config schema found in handler metadata".to_string(),
                )
            })?)
            .map_err(|e| {
                DecisionPointProcessingError::TemplateSerialization(format!(
                    "Failed to deserialize task template: {}",
                    e
                ))
            })?;

        // Find the decision step definition in the template
        let decision_step_def = self
            .find_decision_step_in_template(&task_template, decision_workflow_step.named_step_uuid)
            .await?;

        // Validate outcome against template
        // TAS-53 Phase 7: Record error metrics on validation failure
        if let Err(e) = self.validate_outcome(&task_template, &decision_step_def, &step_names) {
            let error_type = match &e {
                DecisionPointProcessingError::InvalidDescendant { .. } => "invalid_descendant",
                DecisionPointProcessingError::StepNotFoundInTemplate(_) => "step_not_found",
                _ => "other",
            };

            decision_validation_errors_total().add(
                1,
                &[
                    KeyValue::new("decision_name", decision_step_def.name.clone()),
                    KeyValue::new("error_type", error_type),
                ],
            );

            return Err(e);
        }

        // Create steps and dependencies within a transaction
        let mut tx = self.context.database_pool().begin().await?;

        // Determine which steps to create (includes exit decision points)
        let steps_to_create =
            self.determine_steps_to_create(&task_template, &decision_step_def, &step_names)?;

        let step_mapping = self
            .create_steps_from_outcome(&mut tx, task_uuid, &task_template, &steps_to_create)
            .await?;

        // TAS-53 Phase 7: Record error metrics on cycle detection
        if let Err(e) = self
            .create_dependencies(&mut tx, decision_step_uuid, &step_mapping, &task_template)
            .await
        {
            if matches!(e, DecisionPointProcessingError::CycleDetected { .. }) {
                decision_validation_errors_total().add(
                    1,
                    &[
                        KeyValue::new("decision_name", decision_step_def.name.clone()),
                        KeyValue::new("error_type", "cycle_detected"),
                    ],
                );
            }
            return Err(e);
        }

        tx.commit().await?;

        // TAS-53 Phase 7: Record success metrics
        let duration_ms = start_time.elapsed().as_millis() as f64;
        let step_count = step_mapping.len();
        let decision_name = &decision_step_def.name;

        // Record outcome processed
        decision_outcomes_processed_total().add(
            1,
            &[
                KeyValue::new("decision_name", decision_name.clone()),
                KeyValue::new("outcome_type", "create_steps"),
            ],
        );

        // Record steps created count
        decision_steps_created_total().add(
            step_count as u64,
            &[
                KeyValue::new("decision_name", decision_name.clone()),
            ],
        );

        // Record step count distribution
        decision_step_count_histogram().record(
            step_count as u64,
            &[KeyValue::new("decision_name", decision_name.clone())],
        );

        // Record processing duration
        decision_processing_duration().record(
            duration_ms,
            &[
                KeyValue::new("decision_name", decision_name.clone()),
                KeyValue::new("outcome_type", "create_steps"),
            ],
        );

        info!(
            decision_step_uuid = %decision_step_uuid,
            task_uuid = %task_uuid,
            steps_created = step_count,
            duration_ms = duration_ms,
            "Decision outcome processed successfully"
        );

        Ok(step_mapping)
    }

    /// Find the decision step definition in the template
    async fn find_decision_step_in_template(
        &self,
        template: &TaskTemplate,
        named_step_uuid: Uuid,
    ) -> Result<StepDefinition, DecisionPointProcessingError> {
        // Get the named step name
        let named_step = tasker_shared::models::NamedStep::find_by_uuid(
            self.context.database_pool(),
            named_step_uuid,
        )
        .await
        .map_err(|e| DecisionPointProcessingError::Database(e.to_string()))?
        .ok_or(DecisionPointProcessingError::WorkflowStepNotFound(
            named_step_uuid,
        ))?;

        // Find the step definition in the template
        template
            .steps
            .iter()
            .find(|s| s.name == named_step.name)
            .cloned()
            .ok_or(DecisionPointProcessingError::DecisionStepNotFound(
                named_step.name,
            ))
    }

    /// Determine which steps to create, including exit decision points and deferred convergence steps
    ///
    /// When creating steps from a decision outcome, we need to ensure that:
    /// 1. The explicitly requested steps are created
    /// 2. Any exit decision points are ALSO created so they can be discovered and executed
    /// 3. Any deferred convergence steps whose dependencies intersect with created steps
    ///
    /// For example, with nested decisions:
    /// - decision_step_1 → branch_a → decision_step_2 → final_step
    ///
    /// When decision_step_1 chooses to create "branch_a", we must also create "decision_step_2"
    /// so that it exists and can be discovered by the orchestrator.
    ///
    /// For deferred steps (convergence pattern):
    /// - routing_decision → auto_approve → finalize_approval (deferred)
    /// - routing_decision → manager_approval → finalize_approval (deferred)
    ///
    /// When routing_decision creates "auto_approve", we detect that finalize_approval is deferred
    /// with dependencies [auto_approve, manager_approval, finance_review]. Since auto_approve is
    /// being created, we compute the intersection auto_approve and include finalize_approval.
    fn determine_steps_to_create(
        &self,
        template: &TaskTemplate,
        decision_step_def: &StepDefinition,
        requested_steps: &[String],
    ) -> Result<Vec<String>, DecisionPointProcessingError> {
        let mut steps_to_create: std::collections::HashSet<String> =
            requested_steps.iter().cloned().collect();

        // Get graphs reachable from this decision
        let graphs = template.graphs_from_decision(&decision_step_def.name);

        // For each graph, if any of its steps are in requested_steps,
        // also include all exit decision points from that graph
        for graph in &graphs {
            let graph_has_requested_steps =
                requested_steps.iter().any(|step| graph.contains_step(step));

            if graph_has_requested_steps {
                // Add all exit decision points
                for exit_decision in &graph.exit_decisions {
                    debug!(
                        decision = %decision_step_def.name,
                        exit_decision = %exit_decision,
                        "Including exit decision point in step creation"
                    );
                    steps_to_create.insert(exit_decision.clone());
                }
            }
        }

        // Find and include deferred convergence steps
        let deferred_steps = template.deferred_convergence_steps();
        for deferred_step in &deferred_steps {
            // Check if any of this deferred step's dependencies are being created
            let intersection: Vec<String> = deferred_step
                .dependencies
                .iter()
                .filter(|dep| steps_to_create.contains(*dep))
                .cloned()
                .collect();

            if !intersection.is_empty() {
                debug!(
                    decision = %decision_step_def.name,
                    deferred_step = %deferred_step.name,
                    dependencies = ?deferred_step.dependencies,
                    intersection = ?intersection,
                    "Including deferred convergence step in step creation"
                );
                steps_to_create.insert(deferred_step.name.clone());
            }
        }

        Ok(steps_to_create.into_iter().collect())
    }

    /// Validate that requested steps exist in template and are valid descendants
    ///
    /// Uses WorkflowGraph to validate that steps are in graphs reachable from the decision point.
    fn validate_outcome(
        &self,
        template: &TaskTemplate,
        decision_step_def: &StepDefinition,
        step_names: &[String],
    ) -> Result<(), DecisionPointProcessingError> {
        // Get all workflow graphs reachable from this decision point
        let graphs = template.graphs_from_decision(&decision_step_def.name);

        if graphs.is_empty() {
            // Decision point has no downstream graphs (rare but valid - e.g., terminal decision)
            // In this case, no steps should be requested
            if !step_names.is_empty() {
                error!(
                    decision_name = %decision_step_def.name,
                    requested_steps = ?step_names,
                    "Decision point has no downstream graphs, but steps were requested"
                );
                return Err(DecisionPointProcessingError::InvalidDescendant {
                    step_name: step_names[0].clone(),
                    decision_name: decision_step_def.name.clone(),
                });
            }
            return Ok(());
        }

        // Validate each requested step is in one of the reachable graphs
        for step_name in step_names {
            let found_in_graph = graphs.iter().any(|g| g.contains_step(step_name));

            if !found_in_graph {
                error!(
                    step_name = %step_name,
                    decision_name = %decision_step_def.name,
                    "Step not found in any graph reachable from decision point"
                );
                return Err(DecisionPointProcessingError::InvalidDescendant {
                    step_name: step_name.clone(),
                    decision_name: decision_step_def.name.clone(),
                });
            }
        }

        Ok(())
    }

    /// Create workflow steps for the specified step names
    ///
    /// Steps have already been validated via WorkflowGraph in validate_outcome().
    /// This method simply extracts the StepDefinitions and creates them.
    async fn create_steps_from_outcome(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        task_uuid: Uuid,
        template: &TaskTemplate,
        step_names: &[String],
    ) -> Result<HashMap<String, Uuid>, DecisionPointProcessingError> {
        // Filter template steps to only those in step_names
        // (template is the canonical source of StepDefinitions)
        let step_defs: Vec<_> = template
            .steps
            .iter()
            .filter(|s| step_names.contains(&s.name))
            .cloned()
            .collect();

        debug!(
            task_uuid = %task_uuid,
            step_count = step_defs.len(),
            "Creating workflow steps from decision outcome"
        );

        // Use WorkflowStepCreator to create steps
        self.step_creator
            .create_steps_batch(tx, task_uuid, &step_defs)
            .await
            .map_err(|e| DecisionPointProcessingError::Database(e.to_string()))
    }

    /// Create dependencies between decision parent and new steps
    ///
    /// For deferred convergence steps, only creates edges to dependencies that were
    /// actually created (intersection of declared dependencies and created steps).
    async fn create_dependencies(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        decision_step_uuid: Uuid,
        step_mapping: &HashMap<String, Uuid>,
        template: &TaskTemplate,
    ) -> Result<(), DecisionPointProcessingError> {
        for (step_name, to_step_uuid) in step_mapping {
            // Get step dependencies from template
            let step_def = template
                .steps
                .iter()
                .find(|s| &s.name == step_name)
                .expect("Step should exist in template");

            // For each dependency, create an edge
            for dep_name in &step_def.dependencies {
                // The from_step_uuid is looked up in step_mapping (must exist for created steps)
                // For deferred steps, we skip dependencies that weren't created
                let from_step_uuid = if let Some(&from_uuid) = step_mapping.get(dep_name) {
                    from_uuid
                } else {
                    // For deferred steps, skip dependencies that weren't created
                    // For regular steps, fall back to decision_step_uuid (parent)
                    if step_def.is_deferred_convergence() {
                        debug!(
                            deferred_step = %step_name,
                            missing_dep = %dep_name,
                            "Skipping dependency not in created set (deferred step)"
                        );
                        continue; // Skip this dependency
                    }

                    // For regular steps, assume the dependency is the decision step itself
                    decision_step_uuid
                };

                // Check for cycles before creating edge
                let would_cycle = WorkflowStepEdge::would_create_cycle(
                    self.context.database_pool(),
                    from_step_uuid,
                    *to_step_uuid,
                )
                .await
                .map_err(|e| {
                    DecisionPointProcessingError::Database(format!(
                        "Failed to check for cycles: {}",
                        e
                    ))
                })?;

                if would_cycle {
                    error!(
                        from = %dep_name,
                        to = %step_name,
                        "Adding edge would create a cycle"
                    );
                    return Err(DecisionPointProcessingError::CycleDetected {
                        from: dep_name.clone(),
                        to: step_name.clone(),
                    });
                }

                // Create the edge
                let new_edge = NewWorkflowStepEdge {
                    from_step_uuid,
                    to_step_uuid: *to_step_uuid,
                    name: "provides".to_string(),
                };

                WorkflowStepEdge::create_with_transaction(tx, new_edge)
                    .await
                    .map_err(|e| {
                        DecisionPointProcessingError::Database(format!(
                            "Failed to create edge '{}' -> '{}': {}",
                            dep_name, step_name, e
                        ))
                    })?;

                debug!(
                    from = %dep_name,
                    to = %step_name,
                    from_uuid = %from_step_uuid,
                    to_uuid = %to_step_uuid,
                    step_type = if step_def.is_deferred_convergence() { "deferred" } else { "regular" },
                    "Created workflow step edge"
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_point_processing_error_display() {
        let error = DecisionPointProcessingError::StepNotFoundInTemplate("test_step".to_string());
        assert_eq!(error.to_string(), "Step 'test_step' not found in template");

        let error = DecisionPointProcessingError::InvalidDescendant {
            step_name: "step_a".to_string(),
            decision_name: "decision_1".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Step 'step_a' is not a valid descendant of decision point 'decision_1'"
        );

        let error = DecisionPointProcessingError::CycleDetected {
            from: "step_a".to_string(),
            to: "step_b".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Cycle detected: adding edge 'step_a' -> 'step_b' would create a cycle"
        );
    }

    #[test]
    fn test_decision_point_processing_error_from_sqlx() {
        let sqlx_error = sqlx::Error::RowNotFound;
        let error: DecisionPointProcessingError = sqlx_error.into();

        match error {
            DecisionPointProcessingError::Database(msg) => {
                assert!(msg.contains("no rows returned"));
            }
            _ => panic!("Expected Database error"),
        }
    }
}
