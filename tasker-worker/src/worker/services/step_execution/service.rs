//! # Step Executor Service
//!
//! TAS-69: Core step execution logic extracted from command_processor.rs.
//!
//! This service handles:
//! - Step claiming via StepClaim
//! - State verification with exponential backoff
//! - Message deletion after claiming
//! - FFI handler invocation via event publisher
//! - Metrics recording for observability
//!
//! ## Stateless Design
//!
//! The service is designed to be stateless during execution. All methods use `&self`
//! (not `&mut self`), enabling the actor to hold `Arc<StepExecutorService>` without
//! an `RwLock`. Dependencies are provided at construction time.

use std::sync::Arc;

use chrono::Utc;
use opentelemetry::KeyValue;
use pgmq::Message as PgmqMessage;
use tracing::{debug, error, event, info, span, warn, Instrument, Level};
use uuid::Uuid;

use tasker_shared::events::domain_events::EventMetadata;
use tasker_shared::messaging::message::StepMessage;
use tasker_shared::messaging::service::ReceiptHandle;
use tasker_shared::metrics::worker::*;
use tasker_shared::models::{
    orchestration::StepTransitiveDependenciesQuery, task::Task, workflow_step::WorkflowStepWithName,
};
use tasker_shared::state_machine::states::WorkflowStepState;
use tasker_shared::state_machine::step_state_machine::StepStateMachine;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::base::TaskSequenceStep;
use tasker_shared::{TaskerError, TaskerResult};

use crate::worker::domain_event_commands::DomainEventToPublish;
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;
use crate::worker::step_claim::StepClaim;
use crate::worker::task_template_manager::TaskTemplateManager;

/// Step Executor Service
///
/// TAS-69: Extracted from command_processor.rs to provide focused step execution logic.
///
/// This service encapsulates:
/// - Step claiming via state machine transitions
/// - State visibility verification with exponential backoff
/// - PGMQ message deletion after successful claim
/// - FFI handler invocation via event publisher
/// - Domain event dispatch after execution
/// - Metrics recording for observability
///
/// ## Stateless Execution
///
/// All execution methods use `&self` (not `&mut self`). The service has no mutable
/// state during step execution, enabling the actor to hold `Arc<StepExecutorService>`
/// without requiring an `RwLock`.
///
/// Dependencies are provided at construction time, eliminating two-phase initialization.
pub struct StepExecutorService {
    /// Worker identification
    worker_id: String,

    /// System context for dependencies and database access
    context: Arc<SystemContext>,

    /// Task template manager for metadata lookups
    task_template_manager: Arc<TaskTemplateManager>,

    /// Event publisher for sending execution events to FFI handlers
    event_publisher: WorkerEventPublisher,

    /// TAS-65/TAS-69: Domain event system handle for fire-and-forget event dispatch
    domain_event_handle: DomainEventSystemHandle,
}

impl std::fmt::Debug for StepExecutorService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepExecutorService")
            .field("worker_id", &self.worker_id)
            .field("context", &"Arc<SystemContext>")
            .field("task_template_manager", &"Arc<TaskTemplateManager>")
            .field("event_publisher", &"WorkerEventPublisher")
            .field("domain_event_handle", &"DomainEventSystemHandle")
            .finish()
    }
}

impl StepExecutorService {
    /// Create a new StepExecutorService with all dependencies
    ///
    /// All dependencies are required at construction time, eliminating
    /// two-phase initialization complexity.
    pub fn new(
        worker_id: String,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
    ) -> Self {
        Self {
            worker_id,
            context,
            task_template_manager,
            event_publisher,
            domain_event_handle,
        }
    }

    /// Execute a step from a PGMQ message
    ///
    /// This is the main entry point extracted from command_processor::handle_execute_step.
    ///
    /// # Flow
    /// 1. Create StepClaim and get TaskSequenceStep from message
    /// 2. Try to claim the step via state machine transition
    /// 3. Verify state visibility with exponential backoff
    /// 4. Delete the PGMQ message after claiming
    /// 5. Fire step execution event to FFI handler
    /// 6. Record metrics
    ///
    /// # Returns
    /// - `Ok(true)` if step was successfully claimed and executed
    /// - `Ok(false)` if step was not claimed (already claimed by another worker)
    /// - `Err` if an error occurred during execution
    ///
    /// # Stateless Design
    ///
    /// This method uses `&self` (not `&mut self`) as it has no mutable state.
    /// Domain event dispatch is handled separately via `dispatch_domain_events()`.
    pub async fn execute_step(
        &self,
        message: PgmqMessage<StepMessage>,
        queue_name: &str,
    ) -> TaskerResult<bool> {
        let start_time = std::time::Instant::now();
        let step_message = &message.message;

        debug!(
            worker_id = %self.worker_id,
            step_uuid = %step_message.step_uuid,
            task_uuid = %step_message.task_uuid,
            queue = %queue_name,
            "Starting step execution in StepExecutorService"
        );

        // TAS-29 Phase 3.3: Record step execution start
        if let Some(counter) = STEP_EXECUTIONS_TOTAL.get() {
            counter.add(
                1,
                &[KeyValue::new(
                    "correlation_id",
                    step_message.correlation_id.to_string(),
                )],
            );
        }

        // 1. Get TaskSequenceStep from message
        let step_claimer = StepClaim::new(
            step_message.task_uuid,
            step_message.step_uuid,
            self.context.clone(),
            self.task_template_manager.clone(),
        );

        let task_sequence_step = match step_claimer
            .get_task_sequence_step_from_step_message(step_message)
            .await
        {
            Ok(Some(step)) => step,
            Ok(None) => {
                self.record_step_failure(step_message, "unknown", "step_not_found");
                return Err(TaskerError::DatabaseError(format!(
                    "Step not found for {}",
                    step_message.step_uuid
                )));
            }
            Err(err) => {
                self.record_step_failure(step_message, "unknown", "database_error");
                return Err(err);
            }
        };

        let namespace = task_sequence_step.task.namespace_name.clone();

        // 2. Try to claim the step
        let claimed = match step_claimer
            .try_claim_step(&task_sequence_step, step_message.correlation_id)
            .await
        {
            Ok(claimed) => claimed,
            Err(err) => {
                self.record_step_failure(step_message, &namespace, "claim_failed");
                return Err(err);
            }
        };

        if !claimed {
            debug!(
                worker_id = %self.worker_id,
                step_uuid = %step_message.step_uuid,
                "Step not claimed - skipping execution"
            );
            return Ok(false);
        }

        // 3. Verify state visibility with exponential backoff
        self.verify_state_visibility(&task_sequence_step, step_message)
            .await?;

        // 4. Ack PGMQ message after claiming (TAS-133e: use provider-agnostic ack_message)
        let receipt_handle = ReceiptHandle::from(message.msg_id);
        if let Err(err) = self
            .context
            .message_client()
            .ack_message(queue_name, &receipt_handle)
            .await
        {
            self.record_step_failure(step_message, &namespace, "message_deletion_failed");
            return Err(TaskerError::MessagingError(format!(
                "Failed to ack message: {err}"
            )));
        }

        // 5. Execute FFI handler
        let execution_result = self
            .execute_ffi_handler(&task_sequence_step, step_message)
            .await;

        // 6. Record metrics and handle result
        let execution_time = start_time.elapsed().as_millis() as u64;

        match execution_result {
            Ok(()) => {
                self.record_step_success(step_message, &namespace, execution_time);
                Ok(true)
            }
            Err(e) => {
                self.record_step_failure(step_message, &namespace, "handler_execution_failed");
                self.record_execution_duration(step_message, &namespace, execution_time, "failure");
                Err(e)
            }
        }
    }

    /// Verify state transition visibility with exponential backoff
    async fn verify_state_visibility(
        &self,
        task_sequence_step: &TaskSequenceStep,
        step_message: &StepMessage,
    ) -> TaskerResult<()> {
        let state_machine = StepStateMachine::new(
            task_sequence_step.workflow_step.clone().into(),
            self.context.clone(),
        );

        for attempt in 0..5 {
            match state_machine.current_state().await {
                Ok(WorkflowStepState::InProgress) => {
                    debug!(
                        worker_id = %self.worker_id,
                        step_uuid = %step_message.step_uuid,
                        attempt = attempt,
                        "State transition verified as visible"
                    );
                    return Ok(());
                }
                Ok(other_state) if attempt < 4 => {
                    let delay_ms = 2_u64.pow(attempt);
                    warn!(
                        worker_id = %self.worker_id,
                        step_uuid = %step_message.step_uuid,
                        attempt = attempt,
                        current_state = %other_state,
                        delay_ms = delay_ms,
                        "State transition not yet visible, retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                Ok(other_state) => {
                    error!(
                        worker_id = %self.worker_id,
                        step_uuid = %step_message.step_uuid,
                        current_state = %other_state,
                        "State stuck after claiming - possible race condition"
                    );
                    return Err(TaskerError::WorkerError(format!(
                        "Step {} stuck in state {} after claiming (expected in_progress)",
                        step_message.step_uuid, other_state
                    )));
                }
                Err(e) => {
                    error!(
                        worker_id = %self.worker_id,
                        step_uuid = %step_message.step_uuid,
                        error = %e,
                        "Failed to verify state transition"
                    );
                    return Err(e.into());
                }
            }
        }

        Err(TaskerError::WorkerError(format!(
            "State transition not visible after 5 verification attempts for step {}",
            step_message.step_uuid
        )))
    }

    /// Execute the FFI handler for a step
    async fn execute_ffi_handler(
        &self,
        task_sequence_step: &TaskSequenceStep,
        step_message: &StepMessage,
    ) -> TaskerResult<()> {
        let step_span = span!(
            Level::INFO,
            "worker.step_execution",
            correlation_id = %step_message.correlation_id,
            task_uuid = %step_message.task_uuid,
            step_uuid = %step_message.step_uuid,
            step_name = %task_sequence_step.workflow_step.name,
            namespace = %task_sequence_step.task.namespace_name
        );

        async {
            event!(Level::INFO, "step.execution_started");

            let trace_id = Some(step_message.correlation_id.to_string());
            let span_id = Some(format!("span-{}", step_message.step_uuid));

            let fire_result = self
                .event_publisher
                .fire_step_execution_event_with_trace(task_sequence_step, trace_id, span_id)
                .await;

            if let Err(err) = fire_result {
                error!("Failed to fire step execution event: {err}");
                event!(Level::ERROR, "step.execution_failed");
                return Err(TaskerError::EventError(format!(
                    "Failed to fire step execution event: {err}"
                )));
            }

            event!(Level::INFO, "step.execution_completed");
            Ok(())
        }
        .instrument(step_span)
        .await
    }

    /// Fetch TaskSequenceStep by step_uuid only (for domain event dispatch)
    ///
    /// This queries the database to reconstruct the full context needed for
    /// domain event publishing. Used when we only have step_uuid available.
    async fn fetch_task_sequence_step_by_uuid(
        &self,
        step_uuid: Uuid,
    ) -> TaskerResult<Option<TaskSequenceStep>> {
        let db_pool = self.context.database_pool();

        // 1. Fetch workflow step to get task_id
        let workflow_step = match WorkflowStepWithName::find_by_id(db_pool, step_uuid).await {
            Ok(Some(step)) => step,
            Ok(None) => {
                debug!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_uuid,
                    "Workflow step not found for domain event dispatch"
                );
                return Ok(None);
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch workflow step: {e}"
                )));
            }
        };

        // 2. Fetch task using task_uuid from workflow step
        let task = match Task::find_by_id(db_pool, workflow_step.task_uuid).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                warn!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_uuid,
                    task_uuid = %workflow_step.task_uuid,
                    "Task not found for domain event dispatch"
                );
                return Ok(None);
            }
            Err(e) => {
                return Err(TaskerError::DatabaseError(format!(
                    "Failed to fetch task: {e}"
                )));
            }
        };

        let task_for_orchestration = task.for_orchestration(db_pool).await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to fetch task for orchestration: {e}"))
        })?;

        // 3. Get task template for step definition (needed for publishes_events)
        let task_template = self
            .context
            .task_handler_registry
            .get_task_template(
                &task_for_orchestration.namespace_name,
                &task_for_orchestration.task_name,
                &task_for_orchestration.task_version,
            )
            .await
            .map_err(|e| TaskerError::WorkerError(format!("Failed to get task template: {e}")))?;

        // Find step definition using template_step_name
        let step_definition = task_template
            .steps
            .iter()
            .find(|step| step.name == workflow_step.template_step_name)
            .ok_or_else(|| {
                TaskerError::WorkerError(format!(
                    "Step definition not found for domain event dispatch: '{}'",
                    workflow_step.template_step_name
                ))
            })?;

        // 4. Get dependency results (for completeness, though may not be needed for events)
        let deps_query = StepTransitiveDependenciesQuery::new(db_pool.clone());
        let dependency_results = deps_query.get_results_map(step_uuid).await.map_err(|e| {
            TaskerError::DatabaseError(format!("Failed to get dependency results: {e}"))
        })?;

        Ok(Some(TaskSequenceStep {
            task: task_for_orchestration,
            workflow_step,
            dependency_results,
            step_definition: step_definition.clone(),
        }))
    }

    /// Dispatch domain events after step completion (fire-and-forget)
    ///
    /// This method:
    /// 1. Fetches TaskSequenceStep context from database
    /// 2. Checks if step definition declares `publishes_events`
    /// 3. Builds DomainEventToPublish for each declared event
    /// 4. Dispatches via try_send() - never blocks
    ///
    /// # Stateless Design
    ///
    /// This method uses `&self` and fetches context from the database,
    /// eliminating the need for a cache that could grow unboundedly.
    pub async fn dispatch_domain_events(
        &self,
        step_uuid: Uuid,
        step_result: &tasker_shared::messaging::StepExecutionResult,
        correlation_id: Option<Uuid>,
    ) {
        // Fetch context from database (no cache)
        let task_sequence_step = match self.fetch_task_sequence_step_by_uuid(step_uuid).await {
            Ok(Some(ctx)) => ctx,
            Ok(None) => {
                warn!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_uuid,
                    "Could not fetch context for step - cannot dispatch domain events"
                );
                return;
            }
            Err(e) => {
                warn!(
                    worker_id = %self.worker_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Error fetching context for domain event dispatch"
                );
                return;
            }
        };

        let events_to_publish = &task_sequence_step.step_definition.publishes_events;
        if events_to_publish.is_empty() {
            return;
        }

        let correlation = correlation_id.unwrap_or_else(Uuid::new_v4);
        let mut domain_events = Vec::with_capacity(events_to_publish.len());

        for event_def in events_to_publish {
            if !event_def.should_publish(step_result.success) {
                continue;
            }

            let metadata = EventMetadata {
                task_uuid: task_sequence_step.task.task.task_uuid,
                step_uuid: Some(step_result.step_uuid),
                step_name: Some(task_sequence_step.workflow_step.name.clone()),
                namespace: task_sequence_step.task.namespace_name.clone(),
                correlation_id: correlation,
                fired_at: Utc::now(),
                fired_by: self.worker_id.clone(),
            };

            let event = DomainEventToPublish {
                event_name: event_def.name.clone(),
                delivery_mode: event_def.delivery_mode,
                business_payload: step_result.result.clone(),
                metadata,
                task_sequence_step: task_sequence_step.clone(),
                execution_result: step_result.clone(),
            };

            domain_events.push(event);
        }

        let event_count = domain_events.len();
        let publisher_name = task_sequence_step
            .step_definition
            .publishes_events
            .first()
            .and_then(|e| e.publisher.clone())
            .unwrap_or_else(|| "default".to_string());

        let dispatched =
            self.domain_event_handle
                .dispatch_events(domain_events, publisher_name, correlation);

        if dispatched {
            info!(
                worker_id = %self.worker_id,
                step_uuid = %step_uuid,
                event_count = event_count,
                "Domain events dispatched"
            );
        } else {
            warn!(
                worker_id = %self.worker_id,
                step_uuid = %step_uuid,
                event_count = event_count,
                "Domain event dispatch failed - channel full"
            );
        }
    }

    // Metrics helper methods

    fn record_step_failure(&self, step_message: &StepMessage, namespace: &str, error_type: &str) {
        if let Some(counter) = STEP_FAILURES_TOTAL.get() {
            counter.add(
                1,
                &[
                    KeyValue::new("correlation_id", step_message.correlation_id.to_string()),
                    KeyValue::new("namespace", namespace.to_string()),
                    KeyValue::new("error_type", error_type.to_string()),
                ],
            );
        }
    }

    fn record_step_success(
        &self,
        step_message: &StepMessage,
        namespace: &str,
        execution_time: u64,
    ) {
        if let Some(counter) = STEP_SUCCESSES_TOTAL.get() {
            counter.add(
                1,
                &[
                    KeyValue::new("correlation_id", step_message.correlation_id.to_string()),
                    KeyValue::new("namespace", namespace.to_string()),
                ],
            );
        }

        self.record_execution_duration(step_message, namespace, execution_time, "success");
    }

    fn record_execution_duration(
        &self,
        step_message: &StepMessage,
        namespace: &str,
        execution_time: u64,
        result: &str,
    ) {
        if let Some(histogram) = STEP_EXECUTION_DURATION.get() {
            histogram.record(
                execution_time as f64,
                &[
                    KeyValue::new("correlation_id", step_message.correlation_id.to_string()),
                    KeyValue::new("namespace", namespace.to_string()),
                    KeyValue::new("result", result.to_string()),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::event_router::EventRouter;
    use crate::worker::event_systems::domain_event_system::{
        DomainEventSystem, DomainEventSystemConfig,
    };
    use crate::worker::in_process_event_bus::{InProcessEventBus, InProcessEventBusConfig};
    use tasker_shared::events::domain_events::DomainEventPublisher;
    use tokio::sync::RwLock;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_step_executor_service_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        // Create required dependencies
        let event_publisher = WorkerEventPublisher::new("test_worker".to_string());
        let domain_publisher = Arc::new(DomainEventPublisher::new(context.message_client.clone()));
        let in_process_bus = Arc::new(RwLock::new(InProcessEventBus::new(
            InProcessEventBusConfig::default(),
        )));
        let event_router = Arc::new(EventRouter::new(domain_publisher, in_process_bus));
        let (_domain_event_system, domain_event_handle) =
            DomainEventSystem::new(event_router, DomainEventSystemConfig::default());

        let service = StepExecutorService::new(
            "test_worker".to_string(),
            context,
            task_template_manager,
            event_publisher,
            domain_event_handle,
        );

        assert_eq!(service.worker_id, "test_worker");
    }

    #[test]
    fn test_step_executor_service_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StepExecutorService>();
    }
}
