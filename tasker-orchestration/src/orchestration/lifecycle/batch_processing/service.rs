//! Batch Processing Service (TAS-59)
//!
//! Handles dynamic creation of batch worker instances from batchable steps.
//! Analyzes dataset size, generates cursor configurations, and creates worker instances
//! using WorkflowStepCreator for transactional safety.
//!
//! ## Architectural Pattern
//!
//! This service follows the decision_point pattern for dynamic step creation:
//! - Deferred convergence steps are created dynamically when their dependencies intersect
//!   with workers being created
//! - Uses WorkflowStepCreator for all step creation
//! - Uses WorkflowStepEdge models for all edge creation
//! - Uses StepStateMachine for all state transitions
//!
//! ## NoBatches vs CreateBatches
//!
//! Both paths follow the same pattern:
//! 1. Create workers (1 placeholder or N real workers)
//! 2. Determine if convergence step should be created (intersection check)
//! 3. Create convergence step if needed
//! 4. Create edges: batchable → workers, workers → convergence
//! 5. For NoBatches: transition placeholder to Complete via StepStateMachine

use serde_json::json;
use sqlx::{Postgres, Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use tasker_shared::messaging::execution_types::{
    BatchProcessingOutcome, CursorConfig, StepExecutionResult,
};
use tasker_shared::models::core::batch_worker::BatchWorkerInputs;
use tasker_shared::models::core::task_template::{
    BatchConfiguration, HandlerDefinition, StepDefinition, StepType, TaskTemplate,
};
use tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge;
use tasker_shared::models::{WorkflowStep, WorkflowStepEdge};
use tasker_shared::system_context::SystemContext;

use crate::orchestration::lifecycle::task_initialization::WorkflowStepCreator;

#[derive(Debug, Error)]
pub enum BatchProcessingError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid batch configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Worker template not found: {0}")]
    TemplateNotFound(String),

    #[error("Template {0} is not a BatchWorker type")]
    InvalidTemplateType(String),

    #[error("Batch result parsing failed: {0}")]
    ResultParsing(String),

    #[error("Worker creation failed: {0}")]
    WorkerCreation(String),

    #[error("Cursor generation failed: {0}")]
    CursorGeneration(String),

    #[error("State machine error: {0}")]
    StateMachine(String),
}

impl From<tasker_shared::state_machine::errors::StateMachineError> for BatchProcessingError {
    fn from(error: tasker_shared::state_machine::errors::StateMachineError) -> Self {
        BatchProcessingError::StateMachine(error.to_string())
    }
}

#[derive(Debug)]
pub struct BatchProcessingService {
    context: Arc<SystemContext>,
    step_creator: WorkflowStepCreator,
}

impl BatchProcessingService {
    /// Create a new BatchProcessingService
    pub fn new(context: Arc<SystemContext>) -> Self {
        let step_creator = WorkflowStepCreator::new(context.clone());
        Self {
            context,
            step_creator,
        }
    }

    /// Main entry point for batch processing a completed batchable step
    ///
    /// ## Preconditions
    ///
    /// This method assumes batch processing is enabled and the task was validated
    /// at acceptance time. If batch processing is disabled, tasks with batchable
    /// steps should be rejected at the API layer (POST /v1/tasks) with HTTP 422.
    ///
    /// By the time we reach this method:
    /// - The task has been accepted and enqueued
    /// - The batchable step has executed successfully
    /// - The step result contains a BatchProcessingOutcome
    ///
    /// Silently skipping batch processing here would be a silent failure.
    #[instrument(skip(self), fields(task_uuid = %task_uuid, step_uuid = %batchable_step.workflow_step_uuid))]
    pub async fn process_batchable_step(
        &self,
        task_uuid: Uuid,
        batchable_step: &WorkflowStep,
        step_result: &StepExecutionResult,
    ) -> Result<BatchProcessingOutcome, BatchProcessingError> {
        // Extract batch outcome from step result
        let batch_outcome =
            BatchProcessingOutcome::from_step_result(step_result).ok_or_else(|| {
                BatchProcessingError::ResultParsing(
                    "Step result does not contain batch processing outcome".to_string(),
                )
            })?;

        // Create transaction for atomic operations (both CreateBatches and NoBatches)
        let mut tx = self.context.database_pool().begin().await?;

        // Idempotency check: Verify workers haven't already been created
        // This prevents duplicate worker creation if process_batchable_step is called multiple times
        // (e.g., due to retry, duplicate message, or recovery scenarios)
        let existing_workers = sqlx::query!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM tasker_workflow_step_edges
            WHERE from_step_uuid = $1 AND name = 'batch_dependency'
            "#,
            batchable_step.workflow_step_uuid
        )
        .fetch_one(&mut *tx)
        .await?;

        if existing_workers.count > 0 {
            info!(
                task_uuid = %task_uuid,
                step_uuid = %batchable_step.workflow_step_uuid,
                existing_worker_count = existing_workers.count,
                "Workers already created for this batchable step (idempotency check), returning existing outcome"
            );
            // Rollback transaction since no changes needed
            tx.rollback().await?;
            // Return the outcome that was passed in (idempotent)
            return Ok(batch_outcome);
        }

        // Delegate to appropriate handler based on outcome type
        let result = match &batch_outcome {
            BatchProcessingOutcome::NoBatches => {
                self.handle_no_batches_outcome(&mut tx, task_uuid, batchable_step)
                    .await
            }
            BatchProcessingOutcome::CreateBatches {
                worker_template_name,
                worker_count,
                cursor_configs,
                total_items,
            } => {
                self.handle_create_batches_outcome(
                    &mut tx,
                    task_uuid,
                    batchable_step,
                    worker_template_name,
                    *worker_count as usize,
                    cursor_configs,
                    *total_items as usize,
                )
                .await
            }
        }?;

        // Commit transaction
        tx.commit().await?;

        Ok(result)
    }

    /// Handle NoBatches outcome - creates placeholder worker and optional convergence step
    ///
    /// This path is taken when the batchable step determines the dataset is too small
    /// for parallel processing. We create a single placeholder worker that:
    /// - Has no actual work to do (cursor 0-0)
    /// - Is immediately transitioned to Complete state via StepStateMachine
    /// - Allows convergence step dependencies to be satisfied
    #[instrument(skip(self, tx), fields(task_uuid = %task_uuid))]
    async fn handle_no_batches_outcome(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        batchable_step: &WorkflowStep,
    ) -> Result<BatchProcessingOutcome, BatchProcessingError> {
        info!("Batchable step determined no batches needed - creating placeholder worker");

        // Get the batchable step's worker template name
        let batchable_step_name = self
            .get_step_name(tx, batchable_step.workflow_step_uuid)
            .await?;
        let template = self.get_task_template(tx, task_uuid).await?;
        let batchable_step_def = template
            .steps
            .iter()
            .find(|s| s.name == batchable_step_name)
            .ok_or_else(|| {
                BatchProcessingError::InvalidConfiguration(format!(
                    "Batchable step '{}' not found in template",
                    batchable_step_name
                ))
            })?;

        let worker_template_name = batchable_step_def
            .handler
            .initialization
            .get("worker_template_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BatchProcessingError::InvalidConfiguration(format!(
                    "Batchable step '{}' missing worker_template_name",
                    batchable_step_name
                ))
            })?;

        // Validate and get the worker template
        let worker_template = self
            .validate_template(tx, task_uuid, worker_template_name)
            .await?;

        // Get batch configuration
        let batch_config = self
            .extract_batch_configuration(tx, task_uuid, worker_template_name)
            .await?;

        // Create a single placeholder worker with no cursor (no actual work)
        info!(
            worker_template = %worker_template_name,
            "Creating placeholder worker for NoBatches outcome"
        );

        let placeholder_cursor = CursorConfig {
            batch_id: "000".to_string(),
            start_cursor: json!(0),
            end_cursor: json!(0),
            batch_size: 0,
        };

        let created_workers = self
            .create_worker_instances(
                tx,
                task_uuid,
                &worker_template,
                batchable_step.workflow_step_uuid,
                &[placeholder_cursor],
                &batch_config,
                true, // is_no_op: NoBatches outcome creates placeholder worker
            )
            .await?;

        let placeholder_worker = created_workers.first().ok_or_else(|| {
            BatchProcessingError::WorkerCreation(
                "Failed to create placeholder worker - no workers returned".to_string(),
            )
        })?;

        info!(
            placeholder_worker_uuid = %placeholder_worker.workflow_step_uuid,
            placeholder_worker_name = %worker_template_name,
            "Created placeholder worker"
        );

        // Determine which steps to create (convergence step if intersection exists)
        let worker_names: Vec<String> = created_workers
            .iter()
            .map(|_w| worker_template_name.to_string())
            .collect();

        info!(
            worker_count = created_workers.len(),
            worker_template_name = %worker_template_name,
            worker_names = ?worker_names,
            "🔍 NoBatches: About to determine and create convergence steps"
        );

        let step_mapping = self
            .determine_and_create_convergence_step(tx, task_uuid, &template, &worker_names)
            .await?;

        info!(
            convergence_steps_created = step_mapping.len(),
            convergence_step_names = ?step_mapping.keys().collect::<Vec<_>>(),
            "🔍 NoBatches: Convergence step creation completed"
        );

        // Create edges: batchable → placeholder
        self.create_edge(
            tx,
            batchable_step.workflow_step_uuid,
            placeholder_worker.workflow_step_uuid,
            "batch_dependency",
        )
        .await?;

        // If convergence steps were created, create edges: placeholder → convergence(s)
        for (convergence_name, convergence_uuid) in &step_mapping {
            self.create_edge(
                tx,
                placeholder_worker.workflow_step_uuid,
                *convergence_uuid,
                "worker_to_convergence",
            )
            .await?;

            info!(
                convergence_step_name = %convergence_name,
                convergence_step_uuid = %convergence_uuid,
                "Created edge from placeholder worker to convergence step"
            );
        }

        // NOTE: Do NOT transition placeholder worker here!
        // Let it stay in Pending state and go through normal orchestration lifecycle:
        // Pending → Enqueued → InProgress → EnqueuedForOrchestration → Complete
        // This ensures the step result gets processed by OrchestrationResultProcessor,
        // which discovers and enqueues the next ready steps (convergence step).
        info!(
            placeholder_worker_uuid = %placeholder_worker.workflow_step_uuid,
            "Created placeholder worker in Pending state - will be processed through normal lifecycle"
        );

        Ok(BatchProcessingOutcome::NoBatches)
    }

    /// Handle CreateBatches outcome - creates N batch workers and optional convergence step
    ///
    /// This path is taken when the batchable step determines the dataset requires
    /// parallel processing. We create multiple batch workers that:
    /// - Each process a specific cursor range
    /// - Execute in parallel
    /// - Feed results to the convergence step when all complete
    #[instrument(skip(self, tx, cursor_configs), fields(task_uuid = %task_uuid, worker_count = %worker_count))]
    async fn handle_create_batches_outcome(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        batchable_step: &WorkflowStep,
        worker_template_name: &str,
        worker_count: usize,
        cursor_configs: &[CursorConfig],
        total_items: usize,
    ) -> Result<BatchProcessingOutcome, BatchProcessingError> {
        info!(
            worker_template = %worker_template_name,
            worker_count = %worker_count,
            total_items = %total_items,
            "Processing batch worker creation"
        );

        // Get task template
        let template = self.get_task_template(tx, task_uuid).await?;

        // Validate template exists and is BatchWorker type
        let worker_template = self
            .validate_template(tx, task_uuid, worker_template_name)
            .await?;

        // Extract batch configuration from worker template (not batchable step)
        let batch_config = self
            .extract_batch_configuration(tx, task_uuid, worker_template_name)
            .await?;

        // Create worker instances with cursor configs
        info!(
            expected_worker_count = worker_count,
            cursor_configs_count = cursor_configs.len(),
            cursor_batch_ids = ?cursor_configs.iter().map(|c| &c.batch_id).collect::<Vec<_>>(),
            "🔍 About to create batch worker instances"
        );

        let created_workers = self
            .create_worker_instances(
                tx,
                task_uuid,
                &worker_template,
                batchable_step.workflow_step_uuid,
                cursor_configs,
                &batch_config,
                false, // is_no_op: WithBatches outcome creates real workers
            )
            .await?;

        info!(
            expected_count = worker_count,
            actual_created_count = created_workers.len(),
            worker_uuids = ?created_workers.iter().map(|w| w.workflow_step_uuid).collect::<Vec<_>>(),
            "🔍 Successfully created batch worker instances"
        );

        // Determine which steps to create (convergence step if intersection exists)
        let worker_names = vec![worker_template_name.to_string()];

        info!(
            actual_worker_count = created_workers.len(),
            expected_worker_count = worker_count,
            worker_template_name = %worker_template_name,
            worker_names = ?worker_names,
            "🔍 CreateBatches: About to determine and create convergence steps"
        );

        let step_mapping = self
            .determine_and_create_convergence_step(tx, task_uuid, &template, &worker_names)
            .await?;

        info!(
            convergence_steps_created = step_mapping.len(),
            convergence_step_names = ?step_mapping.keys().collect::<Vec<_>>(),
            "🔍 CreateBatches: Convergence step creation completed"
        );

        // Create edges: batchable → workers
        for worker in &created_workers {
            self.create_edge(
                tx,
                batchable_step.workflow_step_uuid,
                worker.workflow_step_uuid,
                "batch_dependency",
            )
            .await?;
        }

        // If convergence steps were created, create edges: workers → convergence(s)
        for (convergence_name, convergence_uuid) in &step_mapping {
            for worker in &created_workers {
                self.create_edge(
                    tx,
                    worker.workflow_step_uuid,
                    *convergence_uuid,
                    "worker_to_convergence",
                )
                .await?;
            }

            info!(
                convergence_step_name = %convergence_name,
                convergence_step_uuid = %convergence_uuid,
                worker_count = created_workers.len(),
                "Created edges from {} batch workers to convergence step",
                created_workers.len()
            );
        }

        info!("Successfully created dependency edges for batch workers");

        Ok(BatchProcessingOutcome::CreateBatches {
            worker_template_name: worker_template_name.to_string(),
            worker_count: worker_count as u32,
            cursor_configs: cursor_configs.to_vec(),
            total_items: total_items as u64,
        })
    }

    /// Determine which steps to create and create them (following decision_point pattern)
    ///
    /// For batch processing, we check if there are deferred convergence steps whose
    /// dependencies intersect with the worker template name. If yes, we create the
    /// convergence step dynamically.
    ///
    /// This follows the same pattern as decision_point/service.rs:determine_steps_to_create
    /// and ensures convergence steps are created only when needed, avoiding race conditions.
    ///
    /// ## Example
    ///
    /// Template has:
    /// - process_batch (batch_worker template)
    /// - aggregate_results (deferred_convergence, depends on: [process_batch])
    ///
    /// When creating workers from process_batch template:
    /// - worker_names = ["process_batch"]
    /// - aggregate_results.dependencies ∩ worker_names = ["process_batch"]
    /// - Intersection exists → create aggregate_results
    ///
    /// ## Returns
    ///
    /// HashMap of created step names to their workflow_step_uuids
    #[instrument(skip(self, tx, template))]
    async fn determine_and_create_convergence_step(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        template: &TaskTemplate,
        worker_names: &[String],
    ) -> Result<HashMap<String, Uuid>, BatchProcessingError> {
        let mut step_mapping = HashMap::new();

        // Find deferred convergence steps in template
        let deferred_steps = template.deferred_convergence_steps();

        info!(
            deferred_step_count = deferred_steps.len(),
            deferred_step_names = ?deferred_steps.iter().map(|s| &s.name).collect::<Vec<_>>(),
            worker_names = ?worker_names,
            "🔍 Checking for convergence steps to create"
        );

        for deferred_step in &deferred_steps {
            // Check if any of this deferred step's dependencies are in worker_names
            let intersection: Vec<String> = deferred_step
                .dependencies
                .iter()
                .filter(|dep| worker_names.contains(dep))
                .cloned()
                .collect();

            info!(
                deferred_step = %deferred_step.name,
                dependencies = ?deferred_step.dependencies,
                worker_names = ?worker_names,
                intersection = ?intersection,
                has_intersection = !intersection.is_empty(),
                "🔍 Checking convergence step intersection"
            );

            if !intersection.is_empty() {
                info!(
                    deferred_step = %deferred_step.name,
                    dependencies = ?deferred_step.dependencies,
                    intersection = ?intersection,
                    "✅ Creating deferred convergence step (intersection exists)"
                );

                // Create the convergence step
                let (created_step, _named_step) = self
                    .step_creator
                    .create_single_step(tx, task_uuid, deferred_step)
                    .await
                    .map_err(|e| {
                        BatchProcessingError::WorkerCreation(format!(
                            "Failed to create convergence step {}: {}",
                            deferred_step.name, e
                        ))
                    })?;

                step_mapping.insert(deferred_step.name.clone(), created_step.workflow_step_uuid);

                info!(
                    convergence_step = %deferred_step.name,
                    convergence_uuid = %created_step.workflow_step_uuid,
                    "Created deferred convergence step"
                );
            } else {
                info!(
                    deferred_step = %deferred_step.name,
                    dependencies = ?deferred_step.dependencies,
                    worker_names = ?worker_names,
                    "❌ Skipping convergence step (no intersection with workers)"
                );
            }
        }

        info!(
            total_convergence_steps_created = step_mapping.len(),
            step_names = ?step_mapping.keys().collect::<Vec<_>>(),
            "🔍 Convergence step creation summary"
        );

        Ok(step_mapping)
    }

    /// Get the TaskTemplate for a task using the task_handler_registry
    ///
    /// This retrieves the template via the registry rather than direct SQL queries,
    /// maintaining consistency with the rest of the system and avoiding duplicate parsing.
    #[instrument(skip(self, tx))]
    async fn get_task_template(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
    ) -> Result<TaskTemplate, BatchProcessingError> {
        // Query to get the named task details (namespace, name, version)
        // Join with task_namespaces to get the namespace name
        let task_info = sqlx::query!(
            r#"
            SELECT tn.name as namespace_name, nt.name, nt.version
            FROM tasker_tasks t
            JOIN tasker_named_tasks nt ON t.named_task_uuid = nt.named_task_uuid
            JOIN tasker_task_namespaces tn ON nt.task_namespace_uuid = tn.task_namespace_uuid
            WHERE t.task_uuid = $1
            "#,
            task_uuid
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            BatchProcessingError::InvalidConfiguration(format!("Task {task_uuid} not found"))
        })?;

        // Use the task_handler_registry to get the template
        let template = self
            .context
            .task_handler_registry
            .get_task_template(
                &task_info.namespace_name,
                &task_info.name,
                &task_info.version,
            )
            .await
            .map_err(|e| {
                BatchProcessingError::InvalidConfiguration(format!(
                    "Failed to get task template from registry: {e}"
                ))
            })?;

        Ok(template)
    }

    /// Validate that the worker template exists and is a BatchWorker type
    #[instrument(skip(self, tx))]
    async fn validate_template(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        template_name: &str,
    ) -> Result<StepDefinition, BatchProcessingError> {
        debug!(template_name = %template_name, "Validating worker template");

        // Get the task template via registry
        let task_template = self.get_task_template(tx, task_uuid).await?;

        // Find the worker template step definition
        let template_step = task_template
            .steps
            .iter()
            .find(|s| s.name == template_name)
            .ok_or_else(|| BatchProcessingError::TemplateNotFound(template_name.to_string()))?;

        // Verify it's a BatchWorker type
        if template_step.step_type != StepType::BatchWorker {
            return Err(BatchProcessingError::InvalidTemplateType(
                template_name.to_string(),
            ));
        }

        debug!(
            template_name = %template_name,
            handler_callable = %template_step.handler.callable,
            "Worker template validated successfully"
        );

        Ok(template_step.clone())
    }

    /// Extract batch configuration from the batchable step by looking it up in the task template
    async fn extract_batch_configuration(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        step_name: &str,
    ) -> Result<BatchConfiguration, BatchProcessingError> {
        // Get the task template via registry
        let task_template = self.get_task_template(tx, task_uuid).await?;

        // Find the step definition
        let step_def = task_template
            .steps
            .iter()
            .find(|s| s.name == step_name)
            .ok_or_else(|| {
                BatchProcessingError::InvalidConfiguration(format!(
                    "Step {step_name} not found in task template"
                ))
            })?;

        step_def.batch_config.clone().ok_or_else(|| {
            BatchProcessingError::InvalidConfiguration(format!(
                "Step {step_name} missing batch configuration"
            ))
        })
    }

    /// Create worker instances from template with cursor configurations
    #[instrument(skip(self, tx, template, cursor_configs, batch_config))]
    async fn create_worker_instances(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        template: &StepDefinition,
        batchable_step_uuid: Uuid,
        cursor_configs: &[CursorConfig],
        batch_config: &BatchConfiguration,
        is_no_op: bool,
    ) -> Result<Vec<WorkflowStep>, BatchProcessingError> {
        let mut created_workers = Vec::new();

        // Get the batchable step name for dependency
        let batchable_step_name = self.get_step_name(tx, batchable_step_uuid).await?;

        for (index, cursor_config) in cursor_configs.iter().enumerate() {
            debug!(
                batch_id = %cursor_config.batch_id,
                worker_index = index,
                is_no_op = is_no_op,
                "Creating batch worker instance"
            );

            // Generate unique worker name (batch_id already contains "batch_" prefix, just append it)
            let worker_name = format!("{}_{}", template.name, cursor_config.batch_id);

            // Build inputs for worker with cursor configuration and explicit no-op flag
            let worker_inputs =
                self.build_worker_inputs(cursor_config.clone(), batch_config, is_no_op);

            // Merge handler initialization with batch worker inputs
            let mut initialization = template.handler.initialization.clone();
            let worker_inputs_json = worker_inputs.to_value();
            for (key, value) in worker_inputs_json.as_object().unwrap() {
                initialization.insert(key.clone(), value.clone());
            }

            // Add template step name so worker can find the step definition
            initialization.insert(
                "__template_step_name".to_string(),
                serde_json::Value::String(template.name.clone()),
            );

            // Create step definition for this worker instance
            let worker_step = StepDefinition {
                name: worker_name.clone(),
                description: Some(format!(
                    "Batch worker for {} (batch {})",
                    template.name, cursor_config.batch_id
                )),
                handler: HandlerDefinition {
                    callable: template.handler.callable.clone(),
                    initialization,
                },
                step_type: StepType::Standard, // Instantiated as Standard, not template
                system_dependency: template.system_dependency.clone(),
                dependencies: vec![batchable_step_name.clone()], // Depends on batchable step
                retry: template.retry.clone(),
                timeout_seconds: template.timeout_seconds,
                publishes_events: template.publishes_events.clone(),
                batch_config: None, // Workers don't have batch config
            };

            // Use WorkflowStepCreator for transactional creation
            let (created_step, _named_step) = self
                .step_creator
                .create_single_step(tx, task_uuid, &worker_step)
                .await
                .map_err(|e| {
                    BatchProcessingError::WorkerCreation(format!(
                        "Failed to create worker {worker_name}: {e}"
                    ))
                })?;

            created_workers.push(created_step);
        }

        Ok(created_workers)
    }

    /// Get the step name from a workflow step UUID
    async fn get_step_name(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        workflow_step_uuid: Uuid,
    ) -> Result<String, BatchProcessingError> {
        let record = sqlx::query!(
            r#"
            SELECT ns.name
            FROM tasker_workflow_steps ws
            JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
            WHERE ws.workflow_step_uuid = $1
            "#,
            workflow_step_uuid
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            BatchProcessingError::InvalidConfiguration(format!(
                "Workflow step {workflow_step_uuid} not found"
            ))
        })?;

        Ok(record.name)
    }

    /// Build inputs for a worker instance with cursor and batch metadata
    ///
    /// Creates a type-safe `BatchWorkerInputs` structure that composes:
    /// - Runtime cursor configuration (where to process)
    /// - Template metadata (how to process)
    ///
    /// This replaces manual JSON construction with a strongly-typed model
    /// that ensures consistency with the database schema and Ruby worker expectations.
    ///
    /// # Arguments
    ///
    /// * `cursor_config` - Runtime cursor boundaries from batchable handler analysis
    /// * `batch_config` - Template configuration with processing rules
    /// * `is_no_op` - Explicit flag set by outcome handler (true for NoBatches, false for WithBatches)
    ///
    /// # Returns
    ///
    /// Type-safe `BatchWorkerInputs` that can be serialized to JSONB
    fn build_worker_inputs(
        &self,
        cursor_config: CursorConfig,
        batch_config: &BatchConfiguration,
        is_no_op: bool,
    ) -> BatchWorkerInputs {
        BatchWorkerInputs::new(cursor_config, batch_config, is_no_op)
    }

    /// Create a workflow step edge using domain models
    #[instrument(skip(self, tx))]
    async fn create_edge(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        from_step_uuid: Uuid,
        to_step_uuid: Uuid,
        edge_name: &str,
    ) -> Result<(), BatchProcessingError> {
        let new_edge = NewWorkflowStepEdge {
            from_step_uuid,
            to_step_uuid,
            name: edge_name.to_string(),
        };

        WorkflowStepEdge::create_with_transaction(tx, new_edge)
            .await
            .map_err(|e| {
                BatchProcessingError::WorkerCreation(format!("Failed to create edge: {e}"))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_processing_error_display() {
        let error = BatchProcessingError::TemplateNotFound("worker_template".to_string());
        assert_eq!(
            error.to_string(),
            "Worker template not found: worker_template"
        );

        let error = BatchProcessingError::InvalidTemplateType("not_a_worker".to_string());
        assert_eq!(
            error.to_string(),
            "Template not_a_worker is not a BatchWorker type"
        );

        let error = BatchProcessingError::InvalidConfiguration("test config error".to_string());
        assert_eq!(
            error.to_string(),
            "Invalid batch configuration: test config error"
        );
    }
}
