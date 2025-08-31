use serde::{Deserialize, Serialize};

/// Queue configuration with backend abstraction
/// Prepares for RabbitMQ integration while maintaining PGMQ functionality
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueuesConfig {
    /// Backend selection (aligns with UnifiedMessageClient)
    /// Options: "pgmq", "rabbitmq", "redis", etc.
    pub backend: String,

    /// Orchestration namespace for orchestration-owned queues
    pub orchestration_namespace: String,

    /// Worker namespace for worker queues
    pub worker_namespace: String,

    /// Universal queue configuration (backend-agnostic)
    pub default_visibility_timeout_seconds: u32,
    pub default_batch_size: u32,
    pub max_batch_size: u32,
    pub naming_pattern: String,
    pub health_check_interval: u64,

    /// Default namespaces across all backends
    pub default_namespaces: Vec<String>,

    /// Queue type definitions for orchestration system
    pub orchestration_queues: OrchestrationQueuesConfig,

    /// Queue type definitions for worker system
    pub worker_queues: WorkerQueuesConfig,

    /// Backend-specific configuration for PGMQ
    pub pgmq: PgmqBackendConfig,

    /// Future RabbitMQ configuration (prepared for TAS-40+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rabbitmq: Option<RabbitMqBackendConfig>,
}

/// Orchestration queue definitions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationQueuesConfig {
    pub task_requests: String,
    pub task_finalizations: String,
    pub step_results: String,
}

/// Worker queue definitions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerQueuesConfig {
    pub default: String,
    pub fulfillment: String,
    pub inventory: String,
    pub notifications: String,
}

/// Backend-specific configuration for PGMQ
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PgmqBackendConfig {
    pub poll_interval_ms: u64,
    pub shutdown_timeout_seconds: u64,
    pub max_retries: u32,
}

/// Backend-specific configuration for RabbitMQ (future)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RabbitMqBackendConfig {
    pub connection_timeout_seconds: u64,
    pub heartbeat_interval_seconds: u64,
    pub channel_pool_size: u32,
}

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

    /// Get a worker queue name by type
    pub fn worker_queue_name(&self, queue_type: &str) -> String {
        let queue_name = match queue_type {
            "default" => &self.worker_queues.default,
            "fulfillment" => &self.worker_queues.fulfillment,
            "inventory" => &self.worker_queues.inventory,
            "notifications" => &self.worker_queues.notifications,
            _ => queue_type, // Fallback to queue_type itself
        };
        self.get_worker_queue_name(queue_name)
    }
}

impl Default for QueuesConfig {
    fn default() -> Self {
        Self {
            backend: "pgmq".to_string(),
            orchestration_namespace: "orchestration".to_string(),
            worker_namespace: "worker".to_string(),
            default_visibility_timeout_seconds: 30,
            default_batch_size: 10,
            max_batch_size: 100,
            health_check_interval: 60,
            naming_pattern: "{namespace}_{name}_queue".to_string(),
            default_namespaces: vec![
                "default".to_string(),
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
            ],
            orchestration_queues: OrchestrationQueuesConfig {
                task_requests: "task_requests".to_string(),
                task_finalizations: "task_finalizations".to_string(),
                step_results: "step_results".to_string(),
            },
            worker_queues: WorkerQueuesConfig {
                default: "default".to_string(),
                fulfillment: "fulfillment".to_string(),
                inventory: "inventory".to_string(),
                notifications: "notifications".to_string(),
            },
            pgmq: PgmqBackendConfig {
                poll_interval_ms: 250,
                shutdown_timeout_seconds: 5,
                max_retries: 3,
            },
            rabbitmq: Some(RabbitMqBackendConfig {
                connection_timeout_seconds: 10,
                heartbeat_interval_seconds: 60,
                channel_pool_size: 10,
            }),
        }
    }
}

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

        assert_eq!(
            config.worker_queue_name("fulfillment"),
            "worker_fulfillment_queue"
        );
    }
}
