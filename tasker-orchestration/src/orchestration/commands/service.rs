//! # Command Processing Service
//!
//! Business logic service for orchestration command processing, following the
//! established actor/service separation pattern (TAS-46).
//!
//! ## Three Lifecycle Flows
//!
//! All command processing follows one of three message lifecycle flows:
//!
//! ```text
//! Flow 1 - Direct:      DomainObject ──────────────────────────────→ Actor
//! Flow 2 - FromMessage: QueuedMessage → Hydrator → DomainObject ──→ Actor → Ack
//! Flow 3 - FromEvent:   MessageEvent → PgmqResolver → QueuedMessage → (Flow 2)
//! ```
//!
//! - **Flow 1** is provider-agnostic: `initialize_task()`, `process_step_result()`,
//!   `finalize_task()`
//! - **Flow 2** is provider-agnostic: `*_from_message()` — hydrate, process, ack
//!   via `MessageClient`
//! - **Flow 3** is PGMQ-only: `*_from_message_event()` — resolve signal to full
//!   message via `PgmqMessageResolver`, then delegate to Flow 2

use std::sync::Arc;

use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::messaging::client::MessageClient;
use tasker_shared::messaging::service::{MessageEvent, MessageHandle, QueuedMessage};
use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};
use tasker_shared::system_context::SystemContext;
use tasker_shared::{TaskerError, TaskerResult};

use crate::actors::result_processor_actor::ProcessStepResultMessage;
use crate::actors::task_finalizer_actor::FinalizeTaskMessage;
use crate::actors::task_request_actor::ProcessTaskRequestMessage;
use crate::actors::{ActorRegistry, Handler, ProcessBatchMessage};
use crate::health::caches::HealthStatusCaches;
use crate::orchestration::commands::{
    StepProcessResult, SystemHealth, TaskFinalizationResult, TaskInitializeResult,
    TaskReadinessResult,
};
use crate::orchestration::hydration::{
    FinalizationHydrator, StepResultHydrator, TaskRequestHydrator,
};

use super::pgmq_message_resolver::PgmqMessageResolver;

/// Service containing all orchestration command business logic.
///
/// Follows the TAS-46 actor/service separation pattern: actors handle message
/// routing and instrumentation, services implement domain logic.
///
/// Methods are organized by lifecycle flow:
/// - Flow 1 (Direct): `initialize_task`, `process_step_result`, `finalize_task`
/// - Flow 2 (FromMessage): `*_from_message` — hydrate + process + ack
/// - Flow 3 (FromEvent): `*_from_message_event` — PGMQ resolve → Flow 2
#[derive(Debug)]
pub struct CommandProcessingService {
    #[expect(
        dead_code,
        reason = "SystemContext retained for future service operations (e.g., config access)"
    )]
    context: Arc<SystemContext>,
    actors: Arc<ActorRegistry>,
    message_client: Arc<MessageClient>,
    health_caches: HealthStatusCaches,
    pgmq_resolver: PgmqMessageResolver,
    step_result_hydrator: StepResultHydrator,
    task_request_hydrator: TaskRequestHydrator,
    finalization_hydrator: FinalizationHydrator,
}

impl CommandProcessingService {
    /// Create a new `CommandProcessingService` with all required dependencies.
    pub fn new(
        context: Arc<SystemContext>,
        actors: Arc<ActorRegistry>,
        message_client: Arc<MessageClient>,
        health_caches: HealthStatusCaches,
    ) -> Self {
        let step_result_hydrator = StepResultHydrator::new(context.clone());
        let task_request_hydrator = TaskRequestHydrator::new();
        let finalization_hydrator = FinalizationHydrator::new();
        let pgmq_resolver = PgmqMessageResolver::new(message_client.clone());

        Self {
            context,
            actors,
            message_client,
            health_caches,
            pgmq_resolver,
            step_result_hydrator,
            task_request_hydrator,
            finalization_hydrator,
        }
    }

    // =========================================================================
    // Flow 1 — Direct: DomainObject → Actor
    // =========================================================================

    /// Initialize a task using TaskRequestActor directly (TAS-46)
    pub async fn initialize_task(
        &self,
        request: TaskRequestMessage,
    ) -> TaskerResult<TaskInitializeResult> {
        let msg = ProcessTaskRequestMessage { request };
        let task_uuid = self.actors.task_request_actor.handle(msg).await?;

        Ok(TaskInitializeResult::Success {
            task_uuid,
            message: "Task initialized successfully".to_string(),
        })
    }

    /// Process a step result using ResultProcessorActor directly (TAS-46)
    pub async fn process_step_result(
        &self,
        step_result: StepExecutionResult,
    ) -> TaskerResult<StepProcessResult> {
        let msg = ProcessStepResultMessage {
            result: step_result.clone(),
        };

        match self.actors.result_processor_actor.handle(msg).await {
            Ok(()) => Ok(StepProcessResult::Success {
                message: format!(
                    "Step {} result processed successfully",
                    step_result.step_uuid
                ),
            }),
            Err(e) => match step_result.status.as_str() {
                "failed" => Ok(StepProcessResult::Failed {
                    error: format!("{e}"),
                }),
                "skipped" => Ok(StepProcessResult::Skipped {
                    reason: format!("{e}"),
                }),
                _ => Err(TaskerError::OrchestrationError(format!("{e}"))),
            },
        }
    }

    /// Finalize a task using TaskFinalizerActor (TAS-46)
    pub async fn finalize_task(&self, task_uuid: Uuid) -> TaskerResult<TaskFinalizationResult> {
        let msg = FinalizeTaskMessage { task_uuid };
        let result = self.actors.task_finalizer_actor.handle(msg).await?;

        Ok(TaskFinalizationResult::Success {
            task_uuid: result.task_uuid,
            final_status: format!("{:?}", result.action),
            completion_time: Some(chrono::Utc::now()),
        })
    }

    /// Process task readiness (TAS-43)
    ///
    /// Delegates to StepEnqueuerActor for atomic step enqueueing and returns
    /// processing metrics for observability.
    pub async fn process_task_readiness(
        &self,
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String,
    ) -> TaskerResult<TaskReadinessResult> {
        let start_time = std::time::Instant::now();

        debug!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            triggered_by = %triggered_by,
            "Processing task readiness event via command pattern"
        );

        let msg = ProcessBatchMessage;
        let process_result = match self.actors.step_enqueuer_actor.handle(msg).await {
            Ok(result) => result,
            Err(e) => {
                error!(
                    task_uuid = %task_uuid,
                    namespace = %namespace,
                    error = %e,
                    "Failed to process task readiness via StepEnqueuerActor"
                );
                return Err(e);
            }
        };

        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        info!(
            task_uuid = %task_uuid,
            namespace = %namespace,
            priority = priority,
            ready_steps = ready_steps,
            tasks_processed = process_result.tasks_processed,
            tasks_failed = process_result.tasks_failed,
            processing_time_ms = processing_time_ms,
            triggered_by = %triggered_by,
            "Task readiness processed successfully"
        );

        Ok(TaskReadinessResult {
            task_uuid,
            namespace,
            steps_enqueued: ready_steps as u32,
            steps_discovered: ready_steps as u32,
            processing_time_ms,
            triggered_by,
        })
    }

    /// Evaluate health check using cached health status (TAS-75)
    ///
    /// Delegates to `health_check_evaluator::evaluate_health_status` for testable
    /// pure-function evaluation of cached health data.
    pub async fn health_check(&self) -> TaskerResult<SystemHealth> {
        crate::orchestration::health_check_evaluator::evaluate_health_status(
            &self.health_caches,
            self.actors.actor_count(),
        )
        .await
    }

    // =========================================================================
    // Flow 2 — FromMessage: QueuedMessage → Hydrator → Actor → Ack
    // =========================================================================

    /// Initialize a task from a queued message.
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle.
    /// Hydrates the message into a TaskRequestMessage, delegates to Flow 1, then acks.
    pub async fn task_initialize_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<TaskInitializeResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "TASK_INIT_HANDLER: Processing task initialization via hydrator"
        );

        let task_request = self
            .task_request_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            handle = ?message.handle,
            namespace = %task_request.task_request.namespace,
            handler = %task_request.task_request.name,
            "TASK_INIT_HANDLER: Hydration complete, delegating to task initialization"
        );

        let result = self.initialize_task(task_request).await;

        match &result {
            Ok(_) => {
                debug!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    "Task initialization successful, acknowledging message"
                );
                if let Err(e) = self.ack_message_with_handle(&message.handle).await {
                    warn!(
                        handle = ?message.handle,
                        queue = %queue_name,
                        error = %e,
                        "Failed to acknowledge processed task request message"
                    );
                }
            }
            Err(e) => {
                error!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    error = %e,
                    "Task initialization failed, keeping message in queue"
                );
            }
        }

        result
    }

    /// Process a step result from a queued message.
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle.
    /// Hydrates the message into a StepExecutionResult, delegates to Flow 1, then acks.
    pub async fn step_result_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<StepProcessResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "STEP_RESULT_HANDLER: Processing step result message via hydrator"
        );

        let step_execution_result = self
            .step_result_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            step_uuid = %step_execution_result.step_uuid,
            status = %step_execution_result.status,
            "STEP_RESULT_HANDLER: Hydration complete, delegating to result processor"
        );

        let result = self
            .process_step_result(step_execution_result.clone())
            .await;

        match &result {
            Ok(step_result) => {
                debug!(
                    handle = ?message.handle,
                    step_uuid = %step_execution_result.step_uuid,
                    result = ?step_result,
                    "STEP_RESULT_HANDLER: Result processing succeeded"
                );

                debug!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    "STEP_RESULT_HANDLER: Acknowledging successfully processed message"
                );

                match self.ack_message_with_handle(&message.handle).await {
                    Ok(()) => {
                        debug!(
                            handle = ?message.handle,
                            queue = %queue_name,
                            "STEP_RESULT_HANDLER: Successfully acknowledged processed message"
                        );
                    }
                    Err(e) => {
                        warn!(
                            handle = ?message.handle,
                            queue = %queue_name,
                            error = %e,
                            "STEP_RESULT_HANDLER: Failed to acknowledge (will be reprocessed)"
                        );
                    }
                }
            }
            Err(err) => {
                error!(
                    handle = ?message.handle,
                    step_uuid = %step_execution_result.step_uuid,
                    error = %err,
                    "STEP_RESULT_HANDLER: Result processing failed (message will remain for retry)"
                );
            }
        }

        result
    }

    /// Finalize a task from a queued message.
    ///
    /// TAS-133: Accepts provider-agnostic QueuedMessage with explicit MessageHandle.
    /// Hydrates the message into a task UUID, delegates to Flow 1, then acks on success.
    pub async fn task_finalize_from_message(
        &self,
        message: QueuedMessage<serde_json::Value>,
    ) -> TaskerResult<TaskFinalizationResult> {
        let queue_name = message.queue_name();
        debug!(
            handle = ?message.handle,
            queue = %queue_name,
            "FINALIZATION_HANDLER: Processing finalization via hydrator"
        );

        let task_uuid = self
            .finalization_hydrator
            .hydrate_from_queued_message(&message)
            .await?;

        debug!(
            handle = ?message.handle,
            task_uuid = %task_uuid,
            "FINALIZATION_HANDLER: Hydration complete, delegating to task finalization"
        );

        let result = self.finalize_task(task_uuid).await;

        if matches!(result, Ok(TaskFinalizationResult::Success { .. })) {
            if let Err(e) = self.ack_message_with_handle(&message.handle).await {
                warn!(
                    handle = ?message.handle,
                    queue = %queue_name,
                    task_uuid = %task_uuid,
                    error = %e,
                    "Failed to acknowledge processed finalization message"
                );
            }
        } else {
            let error_msg = match &result {
                Ok(TaskFinalizationResult::NotClaimed { reason, .. }) => {
                    format!("Not claimed: {}", reason)
                }
                Ok(TaskFinalizationResult::Failed { error }) => format!("Failed: {}", error),
                Err(e) => format!("Error: {}", e),
                _ => "Unknown finalization result".to_string(),
            };

            warn!(
                handle = ?message.handle,
                queue = %queue_name,
                task_uuid = %task_uuid,
                result = %error_msg,
                "Task finalization was not successful - keeping message in queue"
            );
        }

        result
    }

    // =========================================================================
    // Flow 3 — FromEvent (PGMQ-only): MessageEvent → PgmqResolver → Flow 2
    // =========================================================================

    /// Initialize a task from a PGMQ signal-only message event.
    ///
    /// Resolves the signal into a full message via `PgmqMessageResolver`,
    /// then delegates to `task_initialize_from_message` (Flow 2).
    pub async fn task_initialize_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<TaskInitializeResult> {
        let queued_message = self
            .pgmq_resolver
            .resolve_message_event(&message_event, "task request")
            .await?;
        self.task_initialize_from_message(queued_message).await
    }

    /// Process a step result from a PGMQ signal-only message event.
    ///
    /// Resolves the signal into a full message via `PgmqMessageResolver`,
    /// then delegates to `step_result_from_message` (Flow 2).
    pub async fn step_result_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<StepProcessResult> {
        let queued_message = self
            .pgmq_resolver
            .resolve_message_event(&message_event, "step result")
            .await?;
        self.step_result_from_message(queued_message).await
    }

    /// Finalize a task from a PGMQ signal-only message event.
    ///
    /// Resolves the signal into a full message via `PgmqMessageResolver`,
    /// then delegates to `task_finalize_from_message` (Flow 2).
    pub async fn task_finalize_from_message_event(
        &self,
        message_event: MessageEvent,
    ) -> TaskerResult<TaskFinalizationResult> {
        let queued_message = self
            .pgmq_resolver
            .resolve_message_event(&message_event, "finalization")
            .await?;
        self.task_finalize_from_message(queued_message).await
    }

    // =========================================================================
    // Message Acknowledgment (provider-agnostic)
    // =========================================================================

    /// TAS-133: Ack a message using provider-agnostic MessageHandle
    async fn ack_message_with_handle(&self, handle: &MessageHandle) -> TaskerResult<()> {
        let queue_name = handle.queue_name();
        let receipt_handle = handle.as_receipt_handle();
        self.message_client
            .ack_message(queue_name, &receipt_handle)
            .await
    }
}
