//! # Configuration Management
//!
//! This module provides configuration management for the Tasker workflow orchestration system.
//! Configuration is distinct from constants - these are runtime settings that can be modified
//! through environment variables, config files, or programmatically.

use crate::error::{Result, TaskerError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration structure for the Tasker system
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskerConfig {
    /// Database connection settings
    pub database: DatabaseConfig,
    /// Execution and orchestration settings
    pub execution: ExecutionConfig,
    /// Retry and backoff settings
    pub backoff: BackoffConfig,
    /// Reenqueue timing settings
    pub reenqueue: ReenqueueDelays,
    /// Event system settings
    pub events: EventConfig,
    /// Telemetry and monitoring settings
    pub telemetry: TelemetryConfig,
    /// Custom application-specific settings
    pub custom_settings: HashMap<String, String>,
}

/// Database connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout_seconds: u32,
    pub statement_timeout_seconds: u32,
}

/// Execution configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Minimum concurrent steps to maintain for performance
    pub min_concurrent_steps: u32,
    /// Maximum allowed concurrent steps to prevent overload
    pub max_concurrent_steps_limit: u32,
    /// Cache duration for concurrency calculations (seconds)
    pub concurrency_cache_duration: u32,
    /// Base timeout for batch operations (seconds)
    pub batch_timeout_base_seconds: u32,
    /// Additional timeout per step in batch (seconds)
    pub batch_timeout_per_step_seconds: u32,
    /// Maximum total timeout for any batch (seconds)
    pub max_batch_timeout_seconds: u32,
    /// Wait time for future cleanup operations (seconds)
    pub future_cleanup_wait_seconds: u32,
    /// Batch size threshold for triggering garbage collection
    pub gc_trigger_batch_size_threshold: u32,
    /// Duration threshold for triggering garbage collection (seconds)
    pub gc_trigger_duration_threshold: u32,
}

/// Backoff and retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Default backoff progression in seconds
    pub default_backoff_seconds: Vec<u32>,
    /// Maximum backoff delay in seconds
    pub max_backoff_seconds: u32,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Whether to add jitter to backoff times
    pub jitter_enabled: bool,
    /// Maximum jitter as percentage of base delay
    pub jitter_max_percentage: f64,
    /// Buffer time added to calculated backoffs (seconds)
    pub buffer_seconds: u32,
    /// Default reenqueue delay when no specific reason (seconds)
    pub default_reenqueue_delay: u32,
}

/// Reenqueue delays based on task status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReenqueueDelays {
    /// Delay when task has steps ready for execution (seconds)
    pub has_ready_steps: u32,
    /// Delay when waiting for dependencies to complete (seconds)
    pub waiting_for_dependencies: u32,
    /// Delay when task is currently processing (seconds)
    pub processing: u32,
}

/// Event system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    /// Maximum number of events to batch together
    pub batch_size: usize,
    /// Whether event processing is enabled
    pub enabled: bool,
    /// Buffer timeout for event batching (milliseconds)
    pub batch_timeout_ms: u64,
}

/// Telemetry and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether telemetry collection is enabled
    pub enabled: bool,
    /// Sampling rate for telemetry (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Metrics export endpoint
    pub endpoint: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://localhost/tasker_rust_development".to_string(),
            max_connections: 10,
            connection_timeout_seconds: 30,
            statement_timeout_seconds: 60,
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            min_concurrent_steps: 3,
            max_concurrent_steps_limit: 12,
            concurrency_cache_duration: 30,
            batch_timeout_base_seconds: 30,
            batch_timeout_per_step_seconds: 5,
            max_batch_timeout_seconds: 120,
            future_cleanup_wait_seconds: 1,
            gc_trigger_batch_size_threshold: 6,
            gc_trigger_duration_threshold: 30,
        }
    }
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            default_backoff_seconds: vec![1, 2, 4, 8, 16, 32],
            max_backoff_seconds: 300,
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            jitter_max_percentage: 0.1,
            buffer_seconds: 5,
            default_reenqueue_delay: 30,
        }
    }
}

impl Default for ReenqueueDelays {
    fn default() -> Self {
        Self {
            has_ready_steps: 0,
            waiting_for_dependencies: 45,
            processing: 10,
        }
    }
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            enabled: true,
            batch_timeout_ms: 1000,
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_rate: 1.0,
            endpoint: None,
        }
    }
}

impl TaskerConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        // Database configuration
        if let Ok(db_url) = std::env::var("DATABASE_URL") {
            config.database.url = db_url;
        }

        if let Ok(max_connections) = std::env::var("TASKER_DB_MAX_CONNECTIONS") {
            config.database.max_connections = max_connections.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid db_max_connections: {e}"))
            })?;
        }

        // Execution configuration
        if let Ok(max_concurrent) = std::env::var("TASKER_MAX_CONCURRENT_STEPS") {
            config.execution.max_concurrent_steps_limit = max_concurrent.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid max_concurrent_steps: {e}"))
            })?;
        }

        if let Ok(min_concurrent) = std::env::var("TASKER_MIN_CONCURRENT_STEPS") {
            config.execution.min_concurrent_steps = min_concurrent.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid min_concurrent_steps: {e}"))
            })?;
        }

        // Backoff configuration
        if let Ok(max_backoff) = std::env::var("TASKER_MAX_BACKOFF_SECONDS") {
            config.backoff.max_backoff_seconds = max_backoff.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid max_backoff_seconds: {e}"))
            })?;
        }

        // Telemetry configuration
        if let Ok(telemetry_enabled) = std::env::var("TASKER_TELEMETRY_ENABLED") {
            config.telemetry.enabled = telemetry_enabled.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid telemetry_enabled: {e}"))
            })?;
        }

        Ok(config)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.execution.min_concurrent_steps > self.execution.max_concurrent_steps_limit {
            return Err(TaskerError::ConfigurationError(
                "min_concurrent_steps cannot be greater than max_concurrent_steps_limit"
                    .to_string(),
            ));
        }

        if self.backoff.backoff_multiplier <= 1.0 {
            return Err(TaskerError::ConfigurationError(
                "backoff_multiplier must be greater than 1.0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.telemetry.sampling_rate) {
            return Err(TaskerError::ConfigurationError(
                "telemetry sampling_rate must be between 0.0 and 1.0".to_string(),
            ));
        }

        Ok(())
    }
}
