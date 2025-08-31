//! # TAS-43 Task Readiness Configuration Types
//!
//! Configuration types for the TAS-43 Event-Driven Task Readiness system,
//! providing structured configuration for all task readiness components.

use crate::event_system::DeploymentMode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// TAS-43 Task Readiness configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessConfig {
    /// Enable task readiness event-driven coordination
    pub enabled: bool,

    /// Deployment mode for event coordination behavior
    pub event_system: TaskReadinessEventSystemConfig,

    /// Enhanced coordinator specific settings
    pub enhanced_settings: EnhancedCoordinatorSettings,

    /// PostgreSQL LISTEN/NOTIFY configuration
    pub notification: TaskReadinessNotificationConfig,

    /// Fallback polling configuration for hybrid reliability
    pub fallback_polling: ReadinessFallbackConfig,

    /// Event channel configuration
    pub event_channel: EventChannelConfig,

    /// Task readiness coordinator configuration
    pub coordinator: TaskReadinessCoordinatorConfig,

    /// Error handling configuration
    pub error_handling: ErrorHandlingConfig,
}

/// Enhanced coordinator specific settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnhancedCoordinatorSettings {
    /// Coordinator startup timeout in seconds
    pub startup_timeout_seconds: u64,

    /// Coordinator shutdown timeout in seconds
    pub shutdown_timeout_seconds: u64,

    /// Enable metrics collection for TAS-43 vs TAS-37 comparison
    pub metrics_enabled: bool,

    /// Rollback threshold - error rate percentage that triggers rollback consideration
    pub rollback_threshold_percent: f64,
}

/// PostgreSQL LISTEN/NOTIFY configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessNotificationConfig {
    /// Global channels for all task readiness events
    pub global_channels: Vec<String>,

    /// Namespace-specific channel patterns
    pub namespace_patterns: NamespacePatterns,

    /// Event classification configuration
    pub event_classification: EventClassificationConfig,

    /// Connection and reliability settings
    pub connection: ConnectionConfig,
}

/// Namespace-specific channel patterns
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NamespacePatterns {
    /// Task ready channel pattern: "task_ready.{namespace}"
    pub task_ready: String,

    /// Task state change channel pattern: "task_state_change.{namespace}"
    pub task_state_change: String,
}

/// Event classification configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventClassificationConfig {
    /// Maximum payload size for PostgreSQL NOTIFY (8KB limit)
    pub max_payload_size_bytes: usize,

    /// Timeout for parsing notification payloads in milliseconds
    pub parse_timeout_ms: u64,
}

/// Connection and reliability settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    /// Connection retry attempts before failing
    pub max_connection_retries: u32,

    /// Delay between connection retry attempts in seconds
    pub connection_retry_delay_seconds: u64,

    /// Connection health check interval in seconds
    pub health_check_interval_seconds: u64,

    /// Automatic reconnection on connection loss
    pub auto_reconnect: bool,
}

/// Fallback polling configuration for hybrid reliability
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReadinessFallbackConfig {
    /// Enable fallback polling as safety net
    pub enabled: bool,

    /// Polling interval in milliseconds for checking missed events
    pub polling_interval_ms: u64,

    /// Age threshold in seconds - only check tasks older than this
    pub age_threshold_seconds: u64,

    /// Maximum age in seconds - don't check tasks older than this
    pub max_age_seconds: u64,

    /// Batch size for each polling query
    pub batch_size: u32,

    /// Maximum concurrent polling operations
    pub max_concurrent_pollers: u32,
}

/// Event channel configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventChannelConfig {
    /// Buffer size for task readiness event channels
    pub buffer_size: usize,

    /// Channel timeout in milliseconds when sending events
    pub send_timeout_ms: u64,

    /// Maximum number of event processing retries
    pub max_retries: u32,

    /// Backoff configuration for event processing failures
    pub backoff: BackoffConfig,
}

/// Backoff configuration for event processing failures
#[derive(Debug, Clone, Deserialize, Serialize)]
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

/// Task readiness coordinator configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessCoordinatorConfig {
    /// Coordinator instance identifier prefix
    pub instance_id_prefix: String,

    /// Maximum number of concurrent task processing operations
    pub max_concurrent_operations: u32,

    /// Operation timeout in milliseconds
    pub operation_timeout_ms: u64,

    /// Statistics collection interval in seconds
    pub stats_interval_seconds: u64,
}

/// Error handling configuration
///
/// TAS-43: Circuit breaker configuration has been consolidated to circuit_breakers.toml
/// and is accessed through the centralized CircuitBreakerConfig in TaskerConfig
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorHandlingConfig {
    /// Maximum consecutive errors before coordinator shutdown
    pub max_consecutive_errors: u32,

    /// Error rate threshold for triggering alerts (errors per minute)
    pub error_rate_threshold_per_minute: u32,
    // NOTE: Circuit breaker configuration moved to centralized circuit_breakers.toml
    // Access via TaskerConfig.circuit_breakers.component_configs["task_readiness"]
}

/// Configuration for task readiness event system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessEventSystemConfig {
    /// System identifier
    pub system_id: String,
    /// Deployment mode
    pub deployment_mode: DeploymentMode,
    /// Namespaces to coordinate (empty = all)
    pub namespaces: Vec<String>,
    /// Event-driven coordination enabled
    pub event_driven_enabled: bool,
    /// Maximum concurrent task processors
    pub max_concurrent_tasks: usize,
    /// Task claiming timeout
    pub claim_timeout: Duration,
    /// Backoff delay when no tasks ready
    pub backoff_delay: Duration,
    /// Fallback polling interval for hybrid mode
    pub fallback_polling_interval: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,
}

impl Default for TaskReadinessEventSystemConfig {
    fn default() -> Self {
        Self {
            system_id: "task-readiness-event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            namespaces: vec![], // Empty = all namespaces
            event_driven_enabled: true,
            max_concurrent_tasks: 10,
            claim_timeout: Duration::from_secs(30),
            backoff_delay: Duration::from_millis(100),
            fallback_polling_interval: Duration::from_secs(5),
            health_check_interval: Duration::from_secs(30),
            performance_monitoring_enabled: true,
        }
    }
}

impl Default for TaskReadinessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            event_system: TaskReadinessEventSystemConfig::default(),
            enhanced_settings: EnhancedCoordinatorSettings::default(),
            notification: TaskReadinessNotificationConfig::default(),
            fallback_polling: ReadinessFallbackConfig::default(),
            event_channel: EventChannelConfig::default(),
            coordinator: TaskReadinessCoordinatorConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        }
    }
}

impl Default for EnhancedCoordinatorSettings {
    fn default() -> Self {
        Self {
            startup_timeout_seconds: 30,
            shutdown_timeout_seconds: 10,
            metrics_enabled: true,
            rollback_threshold_percent: 5.0,
        }
    }
}

impl Default for TaskReadinessNotificationConfig {
    fn default() -> Self {
        Self {
            global_channels: vec![
                "task_ready".to_string(),
                "task_state_change".to_string(),
                "namespace_created".to_string(),
            ],
            namespace_patterns: NamespacePatterns::default(),
            event_classification: EventClassificationConfig::default(),
            connection: ConnectionConfig::default(),
        }
    }
}

impl Default for NamespacePatterns {
    fn default() -> Self {
        Self {
            task_ready: "task_ready.{namespace}".to_string(),
            task_state_change: "task_state_change.{namespace}".to_string(),
        }
    }
}

impl Default for EventClassificationConfig {
    fn default() -> Self {
        Self {
            max_payload_size_bytes: 8000,
            parse_timeout_ms: 100,
        }
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connection_retries: 3,
            connection_retry_delay_seconds: 2,
            health_check_interval_seconds: 30,
            auto_reconnect: true,
        }
    }
}

impl Default for ReadinessFallbackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            polling_interval_ms: 2000,
            age_threshold_seconds: 5,
            max_age_seconds: 3600,
            batch_size: 50,
            max_concurrent_pollers: 2,
        }
    }
}

impl Default for EventChannelConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            send_timeout_ms: 1000,
            max_retries: 3,
            backoff: BackoffConfig::default(),
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

impl Default for TaskReadinessCoordinatorConfig {
    fn default() -> Self {
        Self {
            instance_id_prefix: "task_readiness".to_string(),
            max_concurrent_operations: 100,
            operation_timeout_ms: 5000,
            stats_interval_seconds: 60,
        }
    }
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            max_consecutive_errors: 10,
            error_rate_threshold_per_minute: 5,
            // TAS-43: Circuit breaker configuration now handled centrally via TaskerConfig.circuit_breakers
        }
    }
}

impl TaskReadinessConfig {
    /// Get startup timeout as Duration
    pub fn startup_timeout(&self) -> Duration {
        Duration::from_secs(self.enhanced_settings.startup_timeout_seconds)
    }

    /// Get shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.enhanced_settings.shutdown_timeout_seconds)
    }

    /// Get polling interval as Duration
    pub fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.fallback_polling.polling_interval_ms)
    }

    /// Get age threshold as Duration
    pub fn age_threshold(&self) -> Duration {
        Duration::from_secs(self.fallback_polling.age_threshold_seconds)
    }

    /// Get max age as Duration
    pub fn max_age(&self) -> Duration {
        Duration::from_secs(self.fallback_polling.max_age_seconds)
    }

    /// Get event send timeout as Duration
    pub fn event_send_timeout(&self) -> Duration {
        Duration::from_millis(self.event_channel.send_timeout_ms)
    }

    /// Get operation timeout as Duration
    pub fn operation_timeout(&self) -> Duration {
        Duration::from_millis(self.coordinator.operation_timeout_ms)
    }

    /// Get connection retry delay as Duration
    pub fn connection_retry_delay(&self) -> Duration {
        Duration::from_secs(self.notification.connection.connection_retry_delay_seconds)
    }

    /// Get health check interval as Duration
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.notification.connection.health_check_interval_seconds)
    }

    /// Generate namespace-specific channel name
    pub fn namespace_channel(&self, base_channel: &str, namespace: &str) -> String {
        match base_channel {
            "task_ready" => self
                .notification
                .namespace_patterns
                .task_ready
                .replace("{namespace}", namespace),
            "task_state_change" => self
                .notification
                .namespace_patterns
                .task_state_change
                .replace("{namespace}", namespace),
            _ => format!("{}.{}", base_channel, namespace),
        }
    }

    /// Check if task readiness is enabled for the deployment mode
    pub fn is_event_driven_enabled(&self) -> bool {
        self.enabled && self.event_system.deployment_mode != DeploymentMode::PollingOnly
    }

    /// Check if fallback polling should be enabled
    pub fn is_fallback_polling_enabled(&self) -> bool {
        self.fallback_polling.enabled && self.event_system.deployment_mode == DeploymentMode::Hybrid
    }

    /// Validate configuration for consistency
    pub fn validate(&self) -> Result<(), String> {
        // Validate buffer sizes
        if self.event_channel.buffer_size == 0 {
            return Err("event_channel.buffer_size must be greater than 0".to_string());
        }

        if self.fallback_polling.batch_size == 0 {
            return Err("fallback_polling.batch_size must be greater than 0".to_string());
        }

        // Validate timeout values
        if self.enhanced_settings.startup_timeout_seconds == 0 {
            return Err(
                "enhanced_settings.startup_timeout_seconds must be greater than 0".to_string(),
            );
        }

        if self.coordinator.operation_timeout_ms == 0 {
            return Err("coordinator.operation_timeout_ms must be greater than 0".to_string());
        }

        // Validate percentages
        if self.enhanced_settings.rollback_threshold_percent < 0.0
            || self.enhanced_settings.rollback_threshold_percent > 100.0
        {
            return Err(
                "enhanced_settings.rollback_threshold_percent must be between 0.0 and 100.0"
                    .to_string(),
            );
        }

        if self.event_channel.backoff.jitter_percent < 0.0
            || self.event_channel.backoff.jitter_percent > 1.0
        {
            return Err(
                "event_channel.backoff.jitter_percent must be between 0.0 and 1.0".to_string(),
            );
        }

        // Validate backoff configuration
        if self.event_channel.backoff.initial_delay_ms > self.event_channel.backoff.max_delay_ms {
            return Err(
                "event_channel.backoff.initial_delay_ms cannot be greater than max_delay_ms"
                    .to_string(),
            );
        }

        if self.event_channel.backoff.multiplier <= 1.0 {
            return Err("event_channel.backoff.multiplier must be greater than 1.0".to_string());
        }

        // Validate age thresholds
        if self.fallback_polling.age_threshold_seconds > self.fallback_polling.max_age_seconds {
            return Err(
                "fallback_polling.age_threshold_seconds cannot be greater than max_age_seconds"
                    .to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configuration() {
        let config = TaskReadinessConfig::default();

        assert!(config.enabled);
        assert!(config.enhanced_settings.metrics_enabled);
        assert_eq!(config.enhanced_settings.rollback_threshold_percent, 5.0);
        assert!(config.notification.connection.auto_reconnect);
        assert!(config.fallback_polling.enabled);
    }

    #[test]
    fn test_duration_conversions() {
        let config = TaskReadinessConfig::default();

        assert_eq!(config.startup_timeout(), Duration::from_secs(30));
        assert_eq!(config.polling_interval(), Duration::from_millis(2000));
        assert_eq!(config.age_threshold(), Duration::from_secs(5));
    }

    #[test]
    fn test_namespace_channel_generation() {
        let config = TaskReadinessConfig::default();

        assert_eq!(
            config.namespace_channel("task_ready", "fulfillment"),
            "task_ready.fulfillment"
        );
        assert_eq!(
            config.namespace_channel("task_state_change", "inventory"),
            "task_state_change.inventory"
        );
    }

    #[test]
    fn test_configuration_validation() {
        let mut config = TaskReadinessConfig::default();

        // Valid configuration should pass
        assert!(config.validate().is_ok());

        // Invalid buffer size
        config.event_channel.buffer_size = 0;
        assert!(config.validate().is_err());
        config.event_channel.buffer_size = 1000;

        // Invalid rollback threshold
        config.enhanced_settings.rollback_threshold_percent = -1.0;
        assert!(config.validate().is_err());
        config.enhanced_settings.rollback_threshold_percent = 101.0;
        assert!(config.validate().is_err());
        config.enhanced_settings.rollback_threshold_percent = 5.0;

        // Invalid backoff configuration
        config.event_channel.backoff.initial_delay_ms = 5000;
        config.event_channel.backoff.max_delay_ms = 1000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serde_serialization() {
        let config = TaskReadinessConfig::default();

        // Test serialization
        let serialized = toml::to_string(&config).expect("Failed to serialize config");
        assert!(!serialized.is_empty());

        // Test deserialization
        let deserialized: TaskReadinessConfig =
            toml::from_str(&serialized).expect("Failed to deserialize config");

        assert_eq!(
            config.enhanced_settings.metrics_enabled,
            deserialized.enhanced_settings.metrics_enabled
        );
    }
}
