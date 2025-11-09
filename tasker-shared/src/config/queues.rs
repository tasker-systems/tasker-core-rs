use serde::{Deserialize, Serialize};

// Import queue config structs from V2
pub use crate::config::tasker::{
    OrchestrationQueuesConfig, PgmqConfig, QueuesConfig, RabbitmqConfig,
};

// Type aliases for backward compatibility (legacy names → V2 names)
pub type PgmqBackendConfig = PgmqConfig;
pub type RabbitMqBackendConfig = RabbitmqConfig;

/// Configuration for orchestration-owned queues used in message classification
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationOwnedQueues {
    pub step_results: String,
    pub task_requests: String,
    pub task_finalizations: String,
}

impl Default for OrchestrationOwnedQueues {
    fn default() -> Self {
        Self {
            step_results: "orchestration_step_results_queue".to_string(),
            task_requests: "orchestration_task_requests_queue".to_string(),
            task_finalizations: "orchestration_task_finalizations_queue".to_string(),
        }
    }
}

impl QueuesConfig {
    /// Get the full queue name using the configured naming pattern
    ///
    /// Examples:
    /// - get_queue_name("orchestration", "step_results") -> "orchestration_step_results_queue"
    /// - get_queue_name("worker", "fulfillment") -> "worker_fulfillment_queue"
    pub fn get_queue_name(&self, namespace: &str, name: &str) -> String {
        // Check if the name already follows the pattern to avoid double-applying

        // If the name already follows the pattern (starts with namespace and ends with _queue), use as-is
        if name.starts_with(&format!("{}_", namespace)) && name.ends_with("_queue") {
            return name.to_string();
        }

        // Otherwise, apply the naming pattern
        self.naming_pattern
            .replace("{namespace}", namespace)
            .replace("{name}", name)
    }

    /// Get orchestration queue name with proper namespace
    pub fn get_orchestration_queue_name(&self, queue_name: &str) -> String {
        self.get_queue_name(&self.orchestration_namespace, queue_name)
    }

    /// Get worker queue name with proper namespace
    pub fn get_worker_queue_name(&self, queue_name: &str) -> String {
        self.get_queue_name(&self.worker_namespace, queue_name)
    }

    /// Get the step results queue name (most commonly used)
    pub fn step_results_queue_name(&self) -> String {
        self.get_orchestration_queue_name(&self.orchestration_queues.step_results)
    }

    /// Get the task requests queue name
    pub fn task_requests_queue_name(&self) -> String {
        self.get_orchestration_queue_name(&self.orchestration_queues.task_requests)
    }

    /// Get the task finalizations queue name
    pub fn task_finalizations_queue_name(&self) -> String {
        self.get_orchestration_queue_name(&self.orchestration_queues.task_finalizations)
    }
}

// Default implementation now comes from V2 via impl_builder_default!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_name_generation() {
        let config = QueuesConfig::default();

        assert_eq!(
            config.get_queue_name("orchestration", "step_results"),
            "orchestration_step_results_queue"
        );

        assert_eq!(
            config.get_queue_name("worker", "fulfillment"),
            "worker_fulfillment_queue"
        );
    }

    #[test]
    fn test_convenience_methods() {
        let config = QueuesConfig::default();

        assert_eq!(
            config.step_results_queue_name(),
            "orchestration_step_results_queue"
        );
    }
}
