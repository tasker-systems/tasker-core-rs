//! # Worker Configuration
//!
//! Configuration for worker processes implementing the command pattern architecture.
//! Replaces the old polling-based executor pools with event-driven step processing.

use serde::{Deserialize, Serialize};

/// Worker configuration for TAS-40 command pattern architecture
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerConfig {
    /// Worker system identification
    pub worker_id: String,
    pub worker_type: String,

    /// Namespaces this worker will process
    pub namespaces: Vec<String>,

    /// Step processing configuration (command pattern)
    pub step_processing: StepProcessingConfig,

    /// Event system configuration for command pattern
    pub event_system: EventSystemConfig,

    /// Worker health monitoring
    pub health_monitoring: HealthMonitoringConfig,

    /// Worker resource limits
    pub resource_limits: ResourceLimitsConfig,

    /// Queue configuration for message consumption
    pub queue_config: QueueConfig,
}

/// Step processing configuration for command pattern architecture
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepProcessingConfig {
    /// Timeout for claiming a step (seconds)
    pub claim_timeout_seconds: u64,

    /// Maximum retries for failed steps
    pub max_retries: u32,

    /// Retry backoff multiplier
    pub retry_backoff_multiplier: f64,

    /// Heartbeat interval for step processing (seconds)
    pub heartbeat_interval_seconds: u64,

    /// Maximum concurrent steps this worker can process
    pub max_concurrent_steps: usize,
}

/// Event system configuration for command pattern
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventSystemConfig {
    /// Event publisher configuration
    pub publisher: EventPublisherConfig,

    /// Event subscriber configuration
    pub subscriber: EventSubscriberConfig,

    /// Event processing configuration
    pub processing: EventProcessingConfig,
}

/// Event publisher configuration for sending results back to orchestration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventPublisherConfig {
    /// Publisher endpoint or connection string
    pub endpoint: String,

    /// Batch size for publishing events
    pub batch_size: usize,

    /// Timeout for publishing events (ms)
    pub timeout_ms: u64,

    /// Maximum retries for publishing
    pub max_retries: u32,
}

/// Event subscriber configuration for receiving step execution requests
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventSubscriberConfig {
    /// Subscriber endpoint or connection string
    pub endpoint: String,

    /// Queue names to subscribe to
    pub queue_names: Vec<String>,

    /// Message prefetch count
    pub prefetch_count: usize,

    /// Acknowledgment timeout (seconds)
    pub ack_timeout_seconds: u64,
}

/// Event processing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventProcessingConfig {
    /// Number of event processor threads
    pub processor_threads: usize,

    /// Event processing timeout (ms)
    pub processing_timeout_ms: u64,

    /// Enable event deduplication
    pub deduplication_enabled: bool,

    /// Deduplication cache size
    pub deduplication_cache_size: usize,
}

/// Worker health monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthMonitoringConfig {
    /// Health check interval (seconds)
    pub health_check_interval_seconds: u64,

    /// Enable metrics collection
    pub metrics_collection_enabled: bool,

    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,

    /// Step processing rate threshold (steps/second)
    pub step_processing_rate_threshold: f64,

    /// Error rate threshold (0.0-1.0)
    pub error_rate_threshold: f64,

    /// Memory usage threshold (MB)
    pub memory_usage_threshold_mb: usize,
}

/// Worker resource limits configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResourceLimitsConfig {
    /// Maximum memory usage (MB)
    pub max_memory_mb: usize,

    /// Maximum CPU usage percentage
    pub max_cpu_percent: u8,

    /// Maximum database connections
    pub max_database_connections: usize,

    /// Maximum queue connections
    pub max_queue_connections: usize,
}

/// Queue configuration for message consumption
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueueConfig {
    /// Message visibility timeout (seconds)
    pub visibility_timeout_seconds: u64,

    /// Batch size for message consumption
    pub batch_size: usize,

    /// Polling interval (ms)
    pub polling_interval_ms: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: "worker-001".to_string(),
            worker_type: "general".to_string(),
            namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
            ],
            step_processing: StepProcessingConfig::default(),
            event_system: EventSystemConfig::default(),
            health_monitoring: HealthMonitoringConfig::default(),
            resource_limits: ResourceLimitsConfig::default(),
            queue_config: QueueConfig::default(),
        }
    }
}

impl Default for StepProcessingConfig {
    fn default() -> Self {
        Self {
            claim_timeout_seconds: 300,
            max_retries: 3,
            retry_backoff_multiplier: 2.0,
            heartbeat_interval_seconds: 30,
            max_concurrent_steps: 100,
        }
    }
}

impl Default for EventSystemConfig {
    fn default() -> Self {
        Self {
            publisher: EventPublisherConfig::default(),
            subscriber: EventSubscriberConfig::default(),
            processing: EventProcessingConfig::default(),
        }
    }
}

impl Default for EventPublisherConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp://localhost:5555".to_string(),
            batch_size: 10,
            timeout_ms: 5000,
            max_retries: 3,
        }
    }
}

impl Default for EventSubscriberConfig {
    fn default() -> Self {
        Self {
            endpoint: "tcp://localhost:5556".to_string(),
            queue_names: vec!["worker_queue".to_string()],
            prefetch_count: 10,
            ack_timeout_seconds: 30,
        }
    }
}

impl Default for EventProcessingConfig {
    fn default() -> Self {
        Self {
            processor_threads: 4,
            processing_timeout_ms: 30000,
            deduplication_enabled: true,
            deduplication_cache_size: 1000,
        }
    }
}

impl Default for HealthMonitoringConfig {
    fn default() -> Self {
        Self {
            health_check_interval_seconds: 10,
            metrics_collection_enabled: true,
            performance_monitoring_enabled: true,
            step_processing_rate_threshold: 10.0,
            error_rate_threshold: 0.05,
            memory_usage_threshold_mb: 1024,
        }
    }
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 2048,
            max_cpu_percent: 80,
            max_database_connections: 50,
            max_queue_connections: 20,
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            visibility_timeout_seconds: 30,
            batch_size: 10,
            polling_interval_ms: 100,
        }
    }
}

impl WorkerConfig {
    /// Validate worker configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.worker_id.is_empty() {
            return Err("worker_id cannot be empty".to_string());
        }

        if self.worker_type.is_empty() {
            return Err("worker_type cannot be empty".to_string());
        }

        if self.namespaces.is_empty() {
            return Err("worker must have at least one namespace".to_string());
        }

        if self.step_processing.max_concurrent_steps == 0 {
            return Err("max_concurrent_steps must be greater than 0".to_string());
        }

        if self.resource_limits.max_memory_mb == 0 {
            return Err("max_memory_mb must be greater than 0".to_string());
        }

        Ok(())
    }

    /// Check if worker can process the given namespace
    pub fn can_process_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains(&namespace.to_string())
    }

    /// Get step processing concurrency limit
    pub fn max_concurrent_steps(&self) -> usize {
        self.step_processing.max_concurrent_steps
    }
}
