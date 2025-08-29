use super::queue::OrchestrationOwnedQueues;
use super::queues::{OrchestrationQueuesConfig, QueuesConfig};
use serde::{Deserialize, Serialize};

/// Configuration-driven queue classification for message routing and processing
///
/// This replaces hardcoded string matching with enum-based patterns grounded in
/// orchestration.toml configuration, ensuring exhaustive pattern matching and
/// consistency between orchestration and worker components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueType {
    /// Step result messages from workers to orchestration
    StepResults,
    /// Task request messages for orchestration processing
    TaskRequests,
    /// Task finalization messages for cleanup operations
    TaskFinalizations,
    /// Worker namespace queues (e.g., fulfillment_queue, inventory_queue)
    WorkerNamespace(String),
    /// Unknown/unclassified queue types
    Unknown,
}

/// Queue classification service that uses configuration to determine queue types
/// instead of hardcoded string matching patterns
pub struct QueueClassifier {
    orchestration_owned: OrchestrationOwnedQueues,
    orchestration_namespace: String,
    worker_namespace: String,
}

impl QueueClassifier {
    /// Create a new queue classifier from orchestration configuration
    pub fn new(
        orchestration_owned: OrchestrationOwnedQueues,
        orchestration_namespace: String,
        worker_namespace: String,
    ) -> Self {
        Self {
            orchestration_owned,
            orchestration_namespace,
            worker_namespace,
        }
    }

    /// Create a new queue classifier from the new centralized queues configuration
    pub fn from_queues_config(config: &QueuesConfig) -> Self {
        // Convert from new config to old structure for compatibility
        let orchestration_owned = OrchestrationOwnedQueues {
            step_results: config.orchestration_queues.step_results.clone(),
            task_requests: config.orchestration_queues.task_requests.clone(),
            task_finalizations: config.orchestration_queues.task_finalizations.clone(),
        };

        Self {
            orchestration_owned,
            orchestration_namespace: config.orchestration_namespace.clone(),
            worker_namespace: config.worker_namespace.clone(),
        }
    }

    /// Classify a queue name based on configuration instead of hardcoded patterns
    ///
    /// This method provides exhaustive pattern matching grounded in configuration
    /// to replace scattered string matching throughout the codebase.
    pub fn classify(&self, queue_name: &str) -> QueueType {
        // Check orchestration-owned queues first (most specific)
        if self.is_step_results_queue(queue_name) {
            return QueueType::StepResults;
        }

        if self.is_task_requests_queue(queue_name) {
            return QueueType::TaskRequests;
        }

        if self.is_task_finalizations_queue(queue_name) {
            return QueueType::TaskFinalizations;
        }

        // Check for worker namespace queues (pattern: {namespace}_{name}_queue)
        if let Some(namespace) = self.extract_worker_namespace(queue_name) {
            return QueueType::WorkerNamespace(namespace);
        }

        QueueType::Unknown
    }

    /// Check if queue name matches configured step results queue
    fn is_step_results_queue(&self, queue_name: &str) -> bool {
        queue_name == self.orchestration_owned.step_results
    }

    /// Check if queue name matches configured task requests queue
    fn is_task_requests_queue(&self, queue_name: &str) -> bool {
        queue_name == self.orchestration_owned.task_requests
    }

    /// Check if queue name matches configured task finalizations queue
    fn is_task_finalizations_queue(&self, queue_name: &str) -> bool {
        queue_name == self.orchestration_owned.task_finalizations
    }

    /// Extract namespace from worker queue name (e.g., "worker_fulfillment_queue" -> Some("fulfillment"))
    fn extract_worker_namespace(&self, queue_name: &str) -> Option<String> {
        if queue_name.ends_with("_queue") {
            let without_queue_suffix = &queue_name[..queue_name.len() - "_queue".len()];

            // Check if it's a worker namespace queue (pattern: worker_{name}_queue)
            if let Some(name) = without_queue_suffix.strip_prefix("worker_") {
                return Some(name.to_string());
            }

            // Legacy support: handle old pattern {namespace}_queue for backwards compatibility
            if !queue_name.starts_with(&self.orchestration_namespace) {
                return Some(without_queue_suffix.to_string());
            }
        }
        None
    }

    /// Get the configured step results queue name for sending messages
    /// This allows workers to send to the correct queue without hardcoding
    pub fn step_results_queue_name(&self) -> &str {
        &self.orchestration_owned.step_results
    }

    /// Get the configured task requests queue name
    pub fn task_requests_queue_name(&self) -> &str {
        &self.orchestration_owned.task_requests
    }

    /// Get the configured task finalizations queue name
    pub fn task_finalizations_queue_name(&self) -> &str {
        &self.orchestration_owned.task_finalizations
    }

    /// Ensure queue name has proper orchestration namespace prefix
    ///
    /// If the configured queue name doesn't start with the orchestration namespace,
    /// this method returns the properly prefixed name. This addresses the user's
    /// concern about enforcing namespace prefixing for orchestration-owned queues.
    pub fn ensure_queue_name_well_structured(
        &self,
        configured_queue_name: &str,
        context: &str,
    ) -> String {
        let expected_prefix = match context {
            "orchestration" => format!("{}_", self.orchestration_namespace),
            "worker" => format!("{}_", self.worker_namespace),
            _ => "".to_string(),
        };

        let expected_suffix = "_queue";

        let mut target_queue_name = configured_queue_name.to_string();

        if !configured_queue_name.starts_with(&expected_prefix) {
            target_queue_name = format!("{}{}", expected_prefix, configured_queue_name);
        }

        if !configured_queue_name.ends_with(expected_suffix) {
            target_queue_name = format!("{}{}", target_queue_name, expected_suffix);
        }

        target_queue_name
    }
}

/// Configuration-driven message classification for event handling
///
/// This enum replaces the hardcoded MessageReadyEventKind in event_driven_coordinator.rs
/// with a configuration-grounded approach that supports exhaustive pattern matching.
#[derive(Debug)]
pub enum ConfigDrivenMessageEvent<T> {
    StepResults(T),
    TaskRequests(T),
    TaskFinalizations(T),
    WorkerNamespace { namespace: String, event: T },
    Unknown(T),
}

impl<T> ConfigDrivenMessageEvent<T> {
    /// Create a config-driven message event using the queue classifier
    pub fn classify(event: T, queue_name: &str, classifier: &QueueClassifier) -> Self {
        match classifier.classify(queue_name) {
            QueueType::StepResults => Self::StepResults(event),
            QueueType::TaskRequests => Self::TaskRequests(event),
            QueueType::TaskFinalizations => Self::TaskFinalizations(event),
            QueueType::WorkerNamespace(namespace) => Self::WorkerNamespace { namespace, event },
            QueueType::Unknown => Self::Unknown(event),
        }
    }

    /// Get the inner event regardless of classification
    pub fn inner(&self) -> &T {
        match self {
            Self::StepResults(event)
            | Self::TaskRequests(event)
            | Self::TaskFinalizations(event)
            | Self::WorkerNamespace { event, .. }
            | Self::Unknown(event) => event,
        }
    }

    /// Consume self and return the inner event
    pub fn into_inner(self) -> T {
        match self {
            Self::StepResults(event)
            | Self::TaskRequests(event)
            | Self::TaskFinalizations(event)
            | Self::WorkerNamespace { event, .. }
            | Self::Unknown(event) => event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_classifier() -> QueueClassifier {
        let orchestration_owned = OrchestrationOwnedQueues {
            step_results: "orchestration_step_results_queue".to_string(),
            task_requests: "orchestration_task_requests_queue".to_string(),
            task_finalizations: "orchestration_task_finalizations_queue".to_string(),
        };

        QueueClassifier::new(
            orchestration_owned,
            "orchestration".to_string(),
            "worker".to_string(),
        )
    }

    #[test]
    fn test_step_results_classification() {
        let classifier = create_test_classifier();

        assert_eq!(
            classifier.classify("orchestration_step_results_queue"),
            QueueType::StepResults
        );
    }

    #[test]
    fn test_task_requests_classification() {
        let classifier = create_test_classifier();

        assert_eq!(
            classifier.classify("orchestration_task_requests_queue"),
            QueueType::TaskRequests
        );
    }

    #[test]
    fn test_worker_namespace_classification() {
        let classifier = create_test_classifier();

        // Test new worker namespace pattern: worker_{name}_queue
        assert_eq!(
            classifier.classify("worker_fulfillment_queue"),
            QueueType::WorkerNamespace("fulfillment".to_string())
        );

        assert_eq!(
            classifier.classify("worker_inventory_queue"),
            QueueType::WorkerNamespace("inventory".to_string())
        );

        // Test legacy support for backward compatibility
        assert_eq!(
            classifier.classify("fulfillment_queue"),
            QueueType::WorkerNamespace("fulfillment".to_string())
        );
    }

    #[test]
    fn test_unknown_classification() {
        let classifier = create_test_classifier();

        // "random_queue" should be classified as a worker namespace since it follows the pattern {namespace}_queue
        assert_eq!(
            classifier.classify("random_queue"),
            QueueType::WorkerNamespace("random".to_string())
        );

        // Test with a truly unknown pattern that doesn't match any rules
        assert_eq!(
            classifier.classify("truly_unknown_pattern"),
            QueueType::Unknown
        );
    }

    #[test]
    fn test_orchestration_prefix_enforcement() {
        let classifier = create_test_classifier();

        // Already has prefix - should return unchanged
        assert_eq!(
            classifier
                .ensure_queue_name_well_structured("orchestration_step_results", "orchestration"),
            "orchestration_step_results_queue"
        );

        // Missing prefix - should add it
        assert_eq!(
            classifier.ensure_queue_name_well_structured("step_results_queue", "orchestration"),
            "orchestration_step_results_queue"
        );

        // Missing prefix and suffix - should add it
        assert_eq!(
            classifier.ensure_queue_name_well_structured("step_results", "orchestration"),
            "orchestration_step_results_queue"
        );
    }

    #[test]
    fn test_config_driven_message_event() {
        let classifier = create_test_classifier();
        let test_event = "test_message";

        let classified = ConfigDrivenMessageEvent::classify(
            test_event,
            "orchestration_step_results_queue",
            &classifier,
        );

        match classified {
            ConfigDrivenMessageEvent::StepResults(event) => assert_eq!(event, test_event),
            _ => panic!("Expected StepResults classification"),
        }
    }
}
