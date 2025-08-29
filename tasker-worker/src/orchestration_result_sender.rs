//! OrchestrationResultSender - Config-driven helper for sending step completion messages
//!
//! Implements the SimpleStepMessage approach for worker→orchestration communication
//! using configuration-driven queue names from orchestration.toml instead of hardcoded strings.

use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use tasker_shared::config::{QueueClassifier, QueueConfig};
use tasker_shared::messaging::message::SimpleStepMessage;
use tasker_shared::messaging::{PgmqClientTrait, UnifiedPgmqClient};
use tasker_shared::{TaskerError, TaskerResult};

/// Helper for sending step completion messages to orchestration with config-driven queue names
pub struct OrchestrationResultSender {
    /// Unified PGMQ client for queue operations
    pgmq_client: Arc<UnifiedPgmqClient>,
    /// Queue classifier for config-driven queue naming
    queue_classifier: QueueClassifier,
}

impl OrchestrationResultSender {
    /// Create new sender with PGMQ client and queue configuration
    pub fn new(
        pgmq_client: Arc<UnifiedPgmqClient>,
        queue_config: QueueConfig,
        orchestration_namespace: String,
    ) -> Self {
        // Create queue classifier for config-driven queue naming
        let queue_classifier = QueueClassifier::new(
            queue_config.orchestration_owned.clone(),
            orchestration_namespace,
        );

        Self {
            pgmq_client,
            queue_classifier,
        }
    }

    /// Send step completion notification to orchestration using SimpleStepMessage approach
    ///
    /// This implements the database-as-API pattern where the worker persists full StepExecutionResult
    /// to the database via StepStateMachine, then sends only a lightweight SimpleStepMessage to
    /// notify orchestration that results are ready for processing.
    ///
    /// # Arguments
    /// * `task_uuid` - UUID of the task containing the completed step
    /// * `step_uuid` - UUID of the completed workflow step
    ///
    /// # Returns
    /// * `Ok(())` - Message sent successfully to orchestration queue
    /// * `Err(TaskerError)` - Queue communication or serialization error
    pub async fn send_completion(&self, task_uuid: Uuid, step_uuid: Uuid) -> TaskerResult<()> {
        let message = SimpleStepMessage {
            task_uuid,
            step_uuid,
        };

        // Use config-driven queue name with namespace prefixing
        let queue_name = self
            .queue_classifier
            .ensure_orchestration_prefix(self.queue_classifier.step_results_queue_name());

        self.pgmq_client
            .send_json_message(&queue_name, &message)
            .await
            .map_err(|e| {
                TaskerError::WorkerError(format!("Failed to send completion message: {e}"))
            })?;

        debug!(
            task_uuid = %task_uuid,
            step_uuid = %step_uuid,
            queue_name = %queue_name,
            "Step completion sent to orchestration queue using config-driven naming"
        );

        Ok(())
    }

    /// Get the configured orchestration step results queue name with proper prefixing
    pub fn step_results_queue(&self) -> String {
        self.queue_classifier
            .ensure_orchestration_prefix(self.queue_classifier.step_results_queue_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_result_sender_creation() {
        // This test verifies the OrchestrationResultSender can be created with proper config-driven queue classification
        let queue_config = QueueConfig {
            orchestration_owned: tasker_shared::config::OrchestrationOwnedQueues {
                step_results: "orchestration_step_results".to_string(),
                task_requests: "orchestration_task_requests".to_string(),
                task_finalizations: "orchestration_task_finalizations".to_string(),
            },
            worker_queues: std::collections::HashMap::new(),
            settings: tasker_shared::config::queue::QueueSettings {
                visibility_timeout_seconds: 30,
                message_retention_seconds: 604800,
                dead_letter_queue_enabled: true,
                max_receive_count: 3,
            },
            orchestration_namespace: "orchestration".to_string(),
            worker_namespace: "worker".to_string(),
        };

        // Test that queue classifier ensures proper orchestration namespace prefixing
        let queue_classifier = QueueClassifier::new(
            queue_config.orchestration_owned.clone(),
            "orchestration".to_string(),
        );

        // Verify that step_results queue gets proper prefix if needed
        let step_results_queue = queue_classifier
            .ensure_orchestration_prefix(&queue_config.orchestration_owned.step_results);
        assert_eq!(step_results_queue, "orchestration_step_results");

        // Test namespace prefix enforcement
        let non_prefixed_queue = queue_classifier.ensure_orchestration_prefix("step_results");
        assert_eq!(non_prefixed_queue, "orchestration_step_results");

        // Note: Would need a mock UnifiedPgmqClient for full unit testing
        // Integration tests will use real PGMQ client with test database
    }
}
