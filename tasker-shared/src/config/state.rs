use serde::{Deserialize, Serialize};
use std::time::Duration;

/// TAS-37 Supplemental: Operational state configuration for shutdown-aware monitoring
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperationalStateConfig {
    /// Enable shutdown-aware health monitoring
    pub enable_shutdown_aware_monitoring: bool,
    /// Suppress health alerts during graceful shutdown
    pub suppress_alerts_during_shutdown: bool,
    /// Health threshold multiplier during startup (0.0-1.0)
    pub startup_health_threshold_multiplier: f64,
    /// Health threshold multiplier during graceful shutdown (0.0-1.0)
    pub shutdown_health_threshold_multiplier: f64,
    /// Timeout for graceful shutdown operations in seconds
    pub graceful_shutdown_timeout_seconds: u64,
    /// Timeout for emergency shutdown operations in seconds
    pub emergency_shutdown_timeout_seconds: u64,
    /// Enable operational state transition logging
    pub enable_transition_logging: bool,
    /// Log level for operational state transitions ("DEBUG", "INFO", "WARN", "ERROR")
    pub transition_log_level: String,
}

impl OperationalStateConfig {
    /// Get graceful shutdown timeout as Duration
    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_timeout_seconds)
    }

    /// Get emergency shutdown timeout as Duration
    pub fn emergency_shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.emergency_shutdown_timeout_seconds)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.startup_health_threshold_multiplier < 0.0
            || self.startup_health_threshold_multiplier > 1.0
        {
            return Err(
                "startup_health_threshold_multiplier must be between 0.0 and 1.0".to_string(),
            );
        }

        if self.shutdown_health_threshold_multiplier < 0.0
            || self.shutdown_health_threshold_multiplier > 1.0
        {
            return Err(
                "shutdown_health_threshold_multiplier must be between 0.0 and 1.0".to_string(),
            );
        }

        if self.graceful_shutdown_timeout_seconds == 0 {
            return Err("graceful_shutdown_timeout_seconds must be greater than 0".to_string());
        }

        if self.emergency_shutdown_timeout_seconds == 0 {
            return Err("emergency_shutdown_timeout_seconds must be greater than 0".to_string());
        }

        match self.transition_log_level.as_str() {
            "DEBUG" | "INFO" | "WARN" | "ERROR" => Ok(()),
            _ => Err("transition_log_level must be one of: DEBUG, INFO, WARN, ERROR".to_string()),
        }
    }
}

impl Default for OperationalStateConfig {
    fn default() -> Self {
        Self {
            enable_shutdown_aware_monitoring: true,
            suppress_alerts_during_shutdown: true,
            startup_health_threshold_multiplier: 0.5, // Relaxed thresholds during startup
            shutdown_health_threshold_multiplier: 0.0, // No health requirements during shutdown
            graceful_shutdown_timeout_seconds: 30,
            emergency_shutdown_timeout_seconds: 5,
            enable_transition_logging: true,
            transition_log_level: "INFO".to_string(),
        }
    }
}
