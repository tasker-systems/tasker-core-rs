//! Individual WorkerExecutor that processes steps from namespaced queues
//! Mirrors OrchestratorExecutor patterns for consistency

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{
    database::Connection,
    messaging::UnifiedPgmqClient,
    models::{Sequence, Task, WorkflowStep},
    registry::{HandlerRegistry, TaskTemplateRegistry},
    state_machine::WorkflowStepState,
    types::{QueueMessageData, StepMessage},
};

use crate::{
    error::{Result, WorkerError},
    event_publisher::EventPublisher,
};

/// Individual WorkerExecutor that processes steps from namespaced queues
/// Mirrors OrchestratorExecutor patterns for consistency
pub struct WorkerExecutor {
    executor_id: String,
    namespace: String,
    messaging_client: Arc<UnifiedPgmqClient>,
    database_connection: Arc<Connection>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    handler_registry: Arc<HandlerRegistry>,
    event_publisher: Arc<EventPublisher>,

    // Executor state
    is_running: AtomicBool,
    is_shutting_down: AtomicBool,
    currently_processing: AtomicBool,
}

impl WorkerExecutor {
    pub async fn new(
        executor_id: String,
        namespace: String,
        messaging_client: Arc<UnifiedPgmqClient>,
        database_connection: Arc<Connection>,
        task_template_registry: Arc<TaskTemplateRegistry>,
        handler_registry: Arc<HandlerRegistry>,
        event_publisher: Arc<EventPublisher>,
    ) -> Result<Self> {
        info!(
            "Creating WorkerExecutor - namespace: {}, executor_id: {}",
            namespace, executor_id
        );

        Ok(Self {
            executor_id,
            namespace,
            messaging_client,
            database_connection,
            task_template_registry,
            handler_registry,
            event_publisher,
            is_running: AtomicBool::new(false),
            is_shutting_down: AtomicBool::new(false),
            currently_processing: AtomicBool::new(false),
        })
    }

    /// Process a step message from the queue
    pub async fn process_step_message(&self, message_data: QueueMessageData) -> Result<()> {
        if self.is_shutting_down.load(Ordering::Relaxed) {
            debug!("Skipping message processing during shutdown");
            return Ok(()); // Skip processing during shutdown
        }

        self.currently_processing.store(true, Ordering::Relaxed);

        let result = self.process_step_message_internal(message_data).await;

        self.currently_processing.store(false, Ordering::Relaxed);

        result
    }

    async fn process_step_message_internal(&self, message_data: QueueMessageData) -> Result<()> {
        let step_message = message_data.step_message;

        debug!(
            "Processing step message - namespace: {}, executor_id: {}, step_id: {}, task_id: {}",
            self.namespace, self.executor_id, step_message.step_id, step_message.task_id
        );

        // 1. Evaluate step against task template registry
        let can_handle = self.can_handle_step(&step_message).await?;
        if !can_handle {
            debug!(
                "Step not supported by this executor - namespace: {}, executor_id: {}, step_id: {}",
                self.namespace, self.executor_id, step_message.step_id
            );
            return Ok(()); // Skip this step, another executor may handle it
        }

        // 2. Claim step via state machine transition
        let claimed = self.claim_step(&step_message).await?;
        if !claimed {
            debug!(
                "Failed to claim step - namespace: {}, executor_id: {}, step_id: {}",
                self.namespace, self.executor_id, step_message.step_id
            );
            return Ok(()); // Another executor claimed it
        }

        // 3. Delete message from queue (claimed successfully)
        self.messaging_client
            .delete_message(&message_data.queue_message)
            .await
            .map_err(|e| WorkerError::Messaging(format!("Failed to delete message: {}", e)))?;

        // 4. Hydrate full task, sequence, and step data
        let (task, sequence, step) = self.hydrate_step_data(&step_message).await?;

        // 5. Fire in-process event with (task, sequence, step) payload
        self.event_publisher
            .fire_step_event(task, sequence, step)
            .await?;

        info!(
            "Step processing initiated - namespace: {}, executor_id: {}, step_id: {}, task_id: {}",
            self.namespace, self.executor_id, step_message.step_id, step_message.task_id
        );

        Ok(())
    }

    /// Evaluate step against task template registry to determine if this executor can handle it
    async fn can_handle_step(&self, step_message: &StepMessage) -> Result<bool> {
        // Check if this namespace is configured to handle this step's namespace
        if step_message.namespace != self.namespace {
            debug!(
                "Namespace mismatch - executor: {}, message: {}",
                self.namespace, step_message.namespace
            );
            return Ok(false);
        }

        // Check task template registry
        let task_template = self
            .task_template_registry
            .get_task_template(
                &step_message.namespace,
                &step_message.task_name,
                &step_message.task_version,
            )
            .await
            .map_err(|e| WorkerError::Configuration(e.to_string()))?;

        if let Some(template) = task_template {
            // Check if this task template has the step we're looking for
            let has_step = template
                .step_templates
                .iter()
                .any(|step_template| step_template.name == step_message.step_name);

            debug!(
                "Task template evaluation - namespace: {}, executor_id: {}, task_name: {}, step_name: {}, has_step: {}",
                self.namespace, self.executor_id, step_message.task_name, step_message.step_name, has_step
            );

            Ok(has_step)
        } else {
            warn!(
                "Task template not found - namespace: {}, executor_id: {}, task_name: {}, task_version: {}",
                self.namespace, self.executor_id, step_message.task_name, step_message.task_version
            );
            Ok(false)
        }
    }

    /// Claim step via state machine transition (atomic operation)
    async fn claim_step(&self, step_message: &StepMessage) -> Result<bool> {
        // Use state machine to atomically transition step to "in_progress"
        let transition_result = WorkflowStepState::transition_to_in_progress(
            &self.database_connection,
            step_message.step_id,
            Some(&self.executor_id),
        )
        .await
        .map_err(|e| WorkerError::Database(e))?;

        debug!(
            "Step claim attempt - namespace: {}, executor_id: {}, step_id: {}, success: {}",
            self.namespace, self.executor_id, step_message.step_id, transition_result.success
        );

        Ok(transition_result.success)
    }

    /// Hydrate full task, sequence, and step data for event payload
    async fn hydrate_step_data(
        &self,
        step_message: &StepMessage,
    ) -> Result<(Task, Sequence, WorkflowStep)> {
        debug!(
            "Hydrating step data - step_id: {}, task_id: {}",
            step_message.step_id, step_message.task_id
        );

        // Load full task data
        let task = Task::find_by_uuid(&self.database_connection, step_message.task_id)
            .await
            .map_err(|e| WorkerError::Database(e))?
            .ok_or_else(|| {
                WorkerError::StepProcessing(format!("Task not found: {}", step_message.task_id))
            })?;

        // Load full step data
        let step = WorkflowStep::find_by_uuid(&self.database_connection, step_message.step_id)
            .await
            .map_err(|e| WorkerError::Database(e))?
            .ok_or_else(|| {
                WorkerError::StepProcessing(format!("Step not found: {}", step_message.step_id))
            })?;

        // Build sequence using transitive dependencies approach
        let sequence = Sequence::build_for_step(&self.database_connection, &step)
            .await
            .map_err(|e| WorkerError::Database(e))?;

        debug!(
            "Hydration complete - task: {}, step: {}, sequence_length: {}",
            task.task_uuid,
            step.step_uuid,
            sequence.items.len()
        );

        Ok((task, sequence, step))
    }

    pub async fn is_idle(&self) -> bool {
        !self.currently_processing.load(Ordering::Relaxed)
    }

    pub async fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Relaxed)
    }

    pub async fn initiate_graceful_shutdown(&self) -> Result<()> {
        info!(
            "Initiating graceful shutdown - namespace: {}, executor_id: {}",
            self.namespace, self.executor_id
        );

        self.is_shutting_down.store(true, Ordering::Relaxed);
        Ok(())
    }
}