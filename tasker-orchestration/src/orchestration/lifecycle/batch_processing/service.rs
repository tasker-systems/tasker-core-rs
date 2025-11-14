//! Batch Processing Service (TAS-59)
//!
//! Handles dynamic creation of batch worker instances from batchable steps.
//! Analyzes dataset size, generates cursor configurations, and creates worker instances
//! using WorkflowStepCreator for transactional safety.

use sqlx::{Postgres, Transaction};
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
use tasker_shared::models::WorkflowStep;
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

        match &batch_outcome {
            BatchProcessingOutcome::NoBatches => {
                info!("Batchable step determined no batches needed - creating direct edge to convergence step");

                // Find the convergence step for this batchable step
                let convergence_step = self
                    .find_convergence_step(&mut tx, task_uuid, batchable_step.workflow_step_uuid)
                    .await?;

                // Create edge: Batchable step → Convergence step
                // This allows the convergence step to become ready immediately
                // since there are no batch workers to wait for
                self.create_edge(
                    &mut tx,
                    batchable_step.workflow_step_uuid,
                    convergence_step.workflow_step_uuid,
                    "batchable_to_convergence_nobatches",
                )
                .await?;

                info!(
                    batchable_step_uuid = %batchable_step.workflow_step_uuid,
                    convergence_step_uuid = %convergence_step.workflow_step_uuid,
                    "Created direct edge from batchable to convergence (NoBatches outcome)"
                );

                // Commit transaction
                tx.commit().await?;

                Ok(batch_outcome)
            }
            BatchProcessingOutcome::CreateBatches {
                worker_template_name,
                worker_count,
                cursor_configs,
                total_items,
            } => {
                info!(
                    worker_template = %worker_template_name,
                    worker_count = %worker_count,
                    total_items = %total_items,
                    "Processing batch worker creation"
                );

                // Validate template exists and is BatchWorker type
                let template = self
                    .validate_template(&mut tx, task_uuid, worker_template_name)
                    .await?;

                // Extract batch configuration from worker template (not batchable step)
                let batch_config = self
                    .extract_batch_configuration(&mut tx, task_uuid, worker_template_name)
                    .await?;

                // Create worker instances with cursor configs
                let created_workers = self
                    .create_worker_instances(
                        &mut tx,
                        task_uuid,
                        &template,
                        batchable_step.workflow_step_uuid,
                        cursor_configs,
                        &batch_config,
                    )
                    .await?;

                info!(
                    created_count = created_workers.len(),
                    "Successfully created batch worker instances"
                );

                // Create dependency edges for batch workers
                self.create_batch_worker_edges(
                    &mut tx,
                    task_uuid,
                    &template,
                    batchable_step.workflow_step_uuid,
                    &created_workers,
                )
                .await?;

                info!("Successfully created dependency edges for batch workers");

                // Commit transaction
                tx.commit().await?;

                Ok(batch_outcome)
            }
        }
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
    ) -> Result<Vec<WorkflowStep>, BatchProcessingError> {
        let mut created_workers = Vec::new();

        // Get the batchable step name for dependency
        let batchable_step_name = self.get_step_name(tx, batchable_step_uuid).await?;

        for (index, cursor_config) in cursor_configs.iter().enumerate() {
            debug!(
                batch_id = %cursor_config.batch_id,
                worker_index = index,
                "Creating batch worker instance"
            );

            // Generate unique worker name (batch_id already contains "batch_" prefix, just append it)
            let worker_name = format!("{}_{}", template.name, cursor_config.batch_id);

            // Build inputs for worker with cursor configuration
            let worker_inputs = self.build_worker_inputs(cursor_config.clone(), batch_config);

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
    ///
    /// # Returns
    ///
    /// Type-safe `BatchWorkerInputs` that can be serialized to JSONB
    fn build_worker_inputs(
        &self,
        cursor_config: CursorConfig,
        batch_config: &BatchConfiguration,
    ) -> BatchWorkerInputs {
        BatchWorkerInputs::new(cursor_config, batch_config)
    }

    /// Create dependency edges for batch workers
    ///
    /// Creates two sets of edges:
    /// 1. Batchable step → each batch worker (enables workers to run after batchable completes)
    /// 2. Each batch worker → deferred convergence step (enables convergence after all workers complete)
    ///
    /// Note: create_single_step() only creates the workflow step, not its edges.
    /// We must create edges manually here.
    #[instrument(skip(self, tx, created_workers))]
    async fn create_batch_worker_edges(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        worker_template: &StepDefinition,
        batchable_step_uuid: Uuid,
        created_workers: &[WorkflowStep],
    ) -> Result<(), BatchProcessingError> {
        // Find the deferred_convergence step that should depend on these workers
        let convergence_step = self
            .find_convergence_step(tx, task_uuid, batchable_step_uuid)
            .await?;

        debug!(
            worker_count = created_workers.len(),
            convergence_step_uuid = %convergence_step.workflow_step_uuid,
            "Creating batch worker dependency edges"
        );

        // Create edges for each worker
        for worker in created_workers {
            // Edge 1: Batchable step → Worker (enables worker after batchable completes)
            self.create_edge(
                tx,
                batchable_step_uuid,
                worker.workflow_step_uuid,
                "batch_dependency",
            )
            .await?;

            // Edge 2: Worker → Convergence step (ensures convergence waits for all workers)
            self.create_edge(
                tx,
                worker.workflow_step_uuid,
                convergence_step.workflow_step_uuid,
                "worker_to_convergence",
            )
            .await?;
        }

        Ok(())
    }

    /// Find the deferred_convergence step in the task
    ///
    /// For workflows with decision points, there may be multiple convergence steps.
    /// This finds the convergence step that depends on the given batchable step.
    #[instrument(skip(self, tx))]
    async fn find_convergence_step(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        task_uuid: Uuid,
        batchable_step_uuid: Uuid,
    ) -> Result<WorkflowStep, BatchProcessingError> {
        // Get the batchable step's name first
        let batchable_step_name = self.get_step_name(tx, batchable_step_uuid).await?;

        // Get the task template to find deferred_convergence step names
        let template = self.get_task_template(tx, task_uuid).await?;

        // Find the deferred_convergence step in the template that depends on the batchable step
        let convergence_step_name = template
            .steps
            .iter()
            .find(|s| {
                matches!(
                    s.step_type,
                    tasker_shared::models::core::task_template::StepType::DeferredConvergence
                ) && s.dependencies.contains(&batchable_step_name)
            })
            .map(|s| &s.name)
            .ok_or_else(|| {
                BatchProcessingError::InvalidConfiguration(format!(
                    "No DeferredConvergence step found that depends on batchable step '{}'",
                    batchable_step_name
                ))
            })?;

        // Query for the workflow step by name
        let workflow_step = sqlx::query_as!(
            WorkflowStep,
            r#"
            SELECT ws.workflow_step_uuid, ws.task_uuid, ws.named_step_uuid,
                   ws.retryable, ws.max_attempts, ws.in_process, ws.processed,
                   ws.processed_at, ws.attempts, ws.last_attempted_at,
                   ws.backoff_request_seconds, ws.inputs, ws.results,
                   ws.skippable, ws.created_at, ws.updated_at
            FROM tasker_workflow_steps ws
            JOIN tasker_named_steps ns ON ws.named_step_uuid = ns.named_step_uuid
            WHERE ws.task_uuid = $1
              AND ns.name = $2
            LIMIT 1
            "#,
            task_uuid,
            convergence_step_name
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            BatchProcessingError::InvalidConfiguration(format!(
                "DeferredConvergence step '{}' not found in task workflow",
                convergence_step_name
            ))
        })?;

        Ok(workflow_step)
    }

    /// Create a workflow step edge
    #[instrument(skip(self, tx))]
    async fn create_edge(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        from_step_uuid: Uuid,
        to_step_uuid: Uuid,
        edge_name: &str,
    ) -> Result<(), BatchProcessingError> {
        use tasker_shared::models::core::workflow_step_edge::NewWorkflowStepEdge;

        let new_edge = NewWorkflowStepEdge {
            from_step_uuid,
            to_step_uuid,
            name: edge_name.to_string(),
        };

        tasker_shared::models::WorkflowStepEdge::create_with_transaction(tx, new_edge)
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
