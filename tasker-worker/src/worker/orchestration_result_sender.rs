//! OrchestrationResultSender - Config-driven helper for sending step completion messages
//!
//! Implements the StepMessage approach for worker→orchestration communication
//! using configuration-driven queue names from orchestration.toml instead of hardcoded strings.
//!
//! TAS-133e: Updated to use MessageClient (provider-agnostic messaging)

use opentelemetry::KeyValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;
use uuid::Uuid;

use tasker_shared::config::QueueClassifier;
use tasker_shared::messaging::client::MessageClient;
use tasker_shared::messaging::message::StepMessage;
use tasker_shared::metrics::worker::*;
use tasker_shared::{TaskerError, TaskerResult};

/// Helper for sending step completion messages to orchestration with config-driven queue names
///
/// TAS-133e: Updated to use MessageClient for provider-agnostic messaging
pub(crate) struct OrchestrationResultSender {
    /// Message client for queue operations (TAS-133e: provider-agnostic)
    message_client: Arc<MessageClient>,
    /// Queue classifier for config-driven queue naming
    queue_classifier: QueueClassifier,
}

impl OrchestrationResultSender {
    /// Create new sender with MessageClient and queue configuration
    /// TAS-133e: Now uses MessageClient instead of UnifiedPgmqClient
    pub fn new(
        message_client: Arc<MessageClient>,
        queues_config: &tasker_shared::config::tasker::QueuesConfig,
    ) -> Self {
        // Create queue classifier for config-driven queue naming using V2 config
        let queue_classifier = QueueClassifier::from_queues_config(queues_config);

        Self {
            message_client,
            queue_classifier,
        }
    }

    /// Send step completion notification to orchestration using StepMessage approach
    ///
    /// This implements the database-as-API pattern where the worker persists full StepExecutionResult
    /// to the database via StepStateMachine, then sends only a lightweight StepMessage to
    /// notify orchestration that results are ready for processing.
    ///
    /// # Arguments
    /// * `task_uuid` - UUID of the task containing the completed step
    /// * `step_uuid` - UUID of the completed workflow step
    /// * `correlation_id` - TAS-29: Correlation ID for distributed tracing
    ///
    /// # Returns
    /// * `Ok(())` - Message sent successfully to orchestration queue
    /// * `Err(TaskerError)` - Queue communication or serialization error
    pub async fn send_completion(
        &self,
        task_uuid: Uuid,
        step_uuid: Uuid,
        correlation_id: Uuid,
    ) -> TaskerResult<()> {
        // TAS-29 Phase 3.3: Start timing result submission
        let start_time = Instant::now();

        let message = StepMessage {
            task_uuid,
            step_uuid,
            correlation_id,
        };

        // Use config-driven orchestration queue name (explicit, no naming pattern)
        let queue_name = self.queue_classifier.step_results_queue_name().to_string();

        // TAS-133e: Use MessageClient.send_message (provider-agnostic)
        self.message_client
            .send_message(&queue_name, &message)
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to send completion message: {e}"))
            })?;

        // TAS-29 Phase 3.3: Record successful result submission
        if let Some(counter) = STEP_RESULTS_SUBMITTED_TOTAL.get() {
            counter.add(
                1,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("result_type", "completion"),
                ],
            );
        }

        // TAS-29 Phase 3.3: Record submission duration
        let duration_ms = start_time.elapsed().as_millis() as f64;
        if let Some(histogram) = STEP_RESULT_SUBMISSION_DURATION.get() {
            histogram.record(
                duration_ms,
                &[
                    KeyValue::new("correlation_id", correlation_id.to_string()),
                    KeyValue::new("result_type", "completion"),
                ],
            );
        }

        debug!(
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            correlation_id = %correlation_id,
            queue_name = %queue_name,
            "Step completion sent to orchestration queue using config-driven naming"
        );

        Ok(())
    }

    /// Get the configured orchestration step results queue name (explicit, no naming pattern)
    #[expect(
        dead_code,
        reason = "Public API for retrieving configured step results queue name"
    )]
    pub fn step_results_queue(&self) -> String {
        self.queue_classifier.step_results_queue_name().to_string()
    }
}
