//! # Unified Event System Configuration
//!
//! Provides a standardized configuration approach for all event systems across
//! orchestration, task readiness, and worker components. This eliminates configuration
//! drift and provides consistent naming and structure.

use crate::config::TaskerConfig;
use crate::event_system::DeploymentMode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Core event system configuration shared across all event systems
///
/// This struct contains the common configuration elements that all event systems
/// need, with system-specific details handled through the generic metadata parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemConfig<T = ()> {
    /// System identifier (e.g., "orchestration-event-system", "task-readiness-event-system")
    pub system_id: String,

    /// Deployment mode (EventDrivenOnly, PollingOnly, Hybrid)
    pub deployment_mode: DeploymentMode,

    /// Core timing configuration
    pub timing: EventSystemTimingConfig,

    /// Processing and concurrency configuration
    pub processing: EventSystemProcessingConfig,

    /// Health monitoring configuration
    pub health: EventSystemHealthConfig,

    /// System-specific metadata and configuration
    pub metadata: T,
}

/// Timing configuration for event systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemTimingConfig {
    /// Health check interval in seconds
    pub health_check_interval_seconds: u64,

    /// Fallback polling interval in seconds (for Hybrid and PollingOnly modes)
    pub fallback_polling_interval_seconds: u64,

    /// Message visibility timeout in seconds
    pub visibility_timeout_seconds: u64,

    /// Processing timeout for individual operations in seconds
    pub processing_timeout_seconds: u64,

    /// Claim timeout for claiming tasks/messages in seconds
    pub claim_timeout_seconds: u64,
}

/// Processing and concurrency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemProcessingConfig {
    /// Maximum concurrent operations/processors
    pub max_concurrent_operations: usize,

    /// Batch size for message processing
    pub batch_size: u32,

    /// Maximum retry attempts for operations
    pub max_retries: u32,

    /// Backoff configuration for retries
    pub backoff: BackoffConfig,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemHealthConfig {
    /// Enable health monitoring
    pub enabled: bool,

    /// Performance monitoring enabled
    pub performance_monitoring_enabled: bool,

    /// Maximum consecutive errors before triggering alerts/shutdown
    pub max_consecutive_errors: u32,

    /// Error rate threshold (errors per minute)
    pub error_rate_threshold_per_minute: u32,
}

/// Backoff configuration for retry operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Initial backoff delay in milliseconds
    pub initial_delay_ms: u64,

    /// Maximum backoff delay in milliseconds
    pub max_delay_ms: u64,

    /// Backoff multiplier for exponential backoff
    pub multiplier: f64,

    /// Maximum jitter percentage (0.0 to 1.0)
    pub jitter_percent: f64,
}

/// Orchestration-specific event system metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEventSystemMetadata {
    /// Note: Queue configuration is populated from main queues config, not TOML
    pub queues_populated_at_runtime: bool,
}

/// Task readiness-specific event system metadata
///
/// TAS-50: Metadata configuration consolidated to task_readiness.rs
/// This type kept for backward compatibility but no longer holds configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessEventSystemMetadata {
    /// Placeholder to maintain type compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _reserved: Option<()>,
}

/// Worker-specific event system metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerEventSystemMetadata {
    /// In-process event system configuration (no TCP endpoints - uses broadcast channels)
    pub in_process_events: InProcessEventConfig,

    /// Listener configuration (for PostgreSQL LISTEN/NOTIFY)
    pub listener: WorkerListenerConfig,

    /// Fallback poller configuration
    pub fallback_poller: WorkerFallbackPollerConfig,

    /// Resource limits
    pub resource_limits: WorkerResourceLimits,
}

// TAS-50: Task Readiness specific configurations removed
// These types have been consolidated into tasker-shared/src/config/task_readiness.rs
// which is the authoritative source for task readiness configuration

// Worker specific configurations
/// In-process event configuration
/// NOTE (TAS-51): broadcast_buffer_size has been MOVED to mpsc_channels.toml
/// Access via: config.mpsc_channels.worker.in_process_events.broadcast_buffer_size
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InProcessEventConfig {
    /// Whether to enable FFI event integration
    pub ffi_integration_enabled: bool,
    /// Deduplication cache size to prevent duplicate processing
    pub deduplication_cache_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerFallbackPollerConfig {
    pub enabled: bool,
    pub polling_interval_ms: u64,
    pub batch_size: u32,
    pub age_threshold_seconds: u64,
    pub max_age_hours: u64,
    pub visibility_timeout_seconds: u64,
    #[serde(default)]
    pub supported_namespaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerListenerConfig {
    pub retry_interval_seconds: u64,
    pub max_retry_attempts: u32,
    pub event_timeout_seconds: u64,
    pub batch_processing: bool,
    pub connection_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResourceLimits {
    pub max_memory_mb: u64,
    pub max_cpu_percent: f64,
    pub max_database_connections: u32,
    pub max_queue_connections: u32,
}

// Type aliases for convenience
pub type OrchestrationEventSystemConfig = EventSystemConfig<OrchestrationEventSystemMetadata>;
pub type TaskReadinessEventSystemConfig = EventSystemConfig<TaskReadinessEventSystemMetadata>;
pub type WorkerEventSystemConfig = EventSystemConfig<WorkerEventSystemMetadata>;

impl WorkerEventSystemConfig {
    /// Create WorkerEventSystemConfig from TaskerConfig using unified configuration
    pub fn from_tasker_config(config: &TaskerConfig) -> Self {
        // Use the unified worker event system config from the centralized loader
        config.event_systems.worker.clone()
    }

    /// Get fallback polling interval as Duration
    pub fn fallback_polling_interval(&self) -> Duration {
        self.timing.fallback_polling_interval()
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        self.timing.health_check_interval()
    }

    /// Get processing timeout as Duration
    pub fn processing_timeout(&self) -> Duration {
        Duration::from_secs(self.timing.processing_timeout_seconds)
    }
}

// Default implementations
impl Default for EventSystemTimingConfig {
    fn default() -> Self {
        Self {
            health_check_interval_seconds: 30,
            fallback_polling_interval_seconds: 1, // Reduced from 5 to 1 for debugging
            visibility_timeout_seconds: 30,
            processing_timeout_seconds: 30,
            claim_timeout_seconds: 300,
        }
    }
}

impl Default for EventSystemProcessingConfig {
    fn default() -> Self {
        Self {
            max_concurrent_operations: 10,
            batch_size: 10,
            max_retries: 3,
            backoff: BackoffConfig::default(),
        }
    }
}

impl Default for EventSystemHealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            performance_monitoring_enabled: true,
            max_consecutive_errors: 10,
            error_rate_threshold_per_minute: 5,
        }
    }
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            multiplier: 2.0,
            jitter_percent: 0.1,
        }
    }
}

impl Default for OrchestrationEventSystemMetadata {
    fn default() -> Self {
        Self {
            queues_populated_at_runtime: true,
        }
    }
}

impl Default for TaskReadinessEventSystemMetadata {
    fn default() -> Self {
        Self { _reserved: None }
    }
}

impl Default for WorkerEventSystemMetadata {
    fn default() -> Self {
        Self {
            in_process_events: InProcessEventConfig {
                ffi_integration_enabled: true,
                deduplication_cache_size: 1000,
            },
            listener: WorkerListenerConfig {
                retry_interval_seconds: 5,
                max_retry_attempts: 3,
                event_timeout_seconds: 30,
                batch_processing: true,
                connection_timeout_seconds: 10,
            },
            fallback_poller: WorkerFallbackPollerConfig {
                enabled: true,
                polling_interval_ms: 500,
                batch_size: 10,
                age_threshold_seconds: 2,
                max_age_hours: 12,
                visibility_timeout_seconds: 30,
                supported_namespaces: vec![],
            },
            resource_limits: WorkerResourceLimits {
                max_memory_mb: 2048,
                max_cpu_percent: 80.0,
                max_database_connections: 50,
                max_queue_connections: 20,
            },
        }
    }
}

impl Default for OrchestrationEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "orchestration-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: EventSystemTimingConfig::default(),
            processing: EventSystemProcessingConfig::default(),
            health: EventSystemHealthConfig::default(),
            metadata: OrchestrationEventSystemMetadata::default(),
        }
    }
}

impl Default for TaskReadinessEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "task-readiness-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: EventSystemTimingConfig::default(),
            processing: EventSystemProcessingConfig::default(),
            health: EventSystemHealthConfig::default(),
            metadata: TaskReadinessEventSystemMetadata::default(),
        }
    }
}

impl Default for WorkerEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "worker-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            timing: EventSystemTimingConfig::default(),
            processing: EventSystemProcessingConfig::default(),
            health: EventSystemHealthConfig::default(),
            metadata: WorkerEventSystemMetadata::default(),
        }
    }
}

// Convenience methods for Duration conversion
impl EventSystemTimingConfig {
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.health_check_interval_seconds)
    }

    pub fn fallback_polling_interval(&self) -> Duration {
        Duration::from_secs(self.fallback_polling_interval_seconds)
    }

    pub fn visibility_timeout(&self) -> Duration {
        Duration::from_secs(self.visibility_timeout_seconds)
    }

    pub fn processing_timeout(&self) -> Duration {
        Duration::from_secs(self.processing_timeout_seconds)
    }

    pub fn claim_timeout(&self) -> Duration {
        Duration::from_secs(self.claim_timeout_seconds)
    }
}

impl BackoffConfig {
    pub fn initial_delay(&self) -> Duration {
        Duration::from_millis(self.initial_delay_ms)
    }

    pub fn max_delay(&self) -> Duration {
        Duration::from_millis(self.max_delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_event_system_config_creation() {
        let config = OrchestrationEventSystemConfig::default();

        assert_eq!(config.system_id, "orchestration-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        assert_eq!(config.timing.health_check_interval_seconds, 30);
        assert_eq!(config.processing.batch_size, 10);
        assert!(config.health.enabled);
    }

    #[test]
    fn test_task_readiness_event_system_config_creation() {
        let config = TaskReadinessEventSystemConfig::default();

        assert_eq!(config.system_id, "task-readiness-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        // TAS-50: Metadata fields removed, configuration consolidated to task_readiness.rs
        assert!(config.metadata._reserved.is_none());
    }

    #[test]
    fn test_worker_event_system_config_creation() {
        let config = WorkerEventSystemConfig::default();

        assert_eq!(config.system_id, "worker-event-system");
        assert_eq!(config.deployment_mode, DeploymentMode::Hybrid);
        assert_eq!(config.metadata.resource_limits.max_memory_mb, 2048);
        // NOTE: broadcast_buffer_size migrated to mpsc_channels.toml (TAS-51)
        assert!(config.metadata.in_process_events.ffi_integration_enabled);
    }

    #[test]
    fn test_duration_conversion_methods() {
        let timing = EventSystemTimingConfig::default();

        assert_eq!(timing.health_check_interval(), Duration::from_secs(30));
        assert_eq!(timing.fallback_polling_interval(), Duration::from_secs(1));
        assert_eq!(timing.claim_timeout(), Duration::from_secs(300));
    }

    #[test]
    fn test_backoff_config_durations() {
        let backoff = BackoffConfig::default();

        assert_eq!(backoff.initial_delay(), Duration::from_millis(100));
        assert_eq!(backoff.max_delay(), Duration::from_millis(5000));
        assert_eq!(backoff.multiplier, 2.0);
    }

    #[test]
    fn test_serde_serialization() {
        let config = OrchestrationEventSystemConfig::default();

        // Test serialization
        let serialized = toml::to_string(&config).expect("Failed to serialize config");
        assert!(!serialized.is_empty());

        // Test deserialization
        let deserialized: OrchestrationEventSystemConfig =
            toml::from_str(&serialized).expect("Failed to deserialize config");

        assert_eq!(config.system_id, deserialized.system_id);
        assert_eq!(config.deployment_mode, deserialized.deployment_mode);
    }
}
