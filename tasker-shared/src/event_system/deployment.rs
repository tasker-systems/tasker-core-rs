//! # Deployment Mode Management for Event-Driven Systems
//!
//! This module provides shared deployment mode concepts that can be used across
//! both orchestration and worker contexts for managing event-driven patterns.
//!
//! ## Deployment Modes
//!
//! - **PollingOnly**: Traditional polling-based approach (maximum reliability)
//! - **Hybrid**: Event-driven with polling fallback (optimal balance)
//! - **EventDrivenOnly**: Pure event-driven approach (maximum performance)
//!
//! ## Key Design Principles
//!
//! - **Permanent Fallback**: Polling is not a migration tool but a reliability feature
//! - **Configuration-Driven**: All behavior controlled via TOML configuration
//! - **Context Agnostic**: Same patterns work for orchestration and worker systems
//! - **Health Monitoring**: Built-in rollback detection and automatic fallback

use serde::{Deserialize, Serialize};
use std::fmt;

/// Deployment mode for event-driven systems
///
/// Defines the operational mode for event-driven coordination, supporting
/// gradual rollout and reliable fallback mechanisms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeploymentMode {
    /// Pure polling approach (maximum reliability)
    ///
    /// Uses traditional polling mechanisms without event-driven coordination.
    /// Recommended for:
    /// - Initial deployments where event infrastructure is not yet proven
    /// - Fallback mode when event-driven systems experience issues
    /// - Environments with strict reliability requirements
    PollingOnly,

    /// Event-driven with polling fallback (optimal balance)
    ///
    /// Primary event-driven coordination with polling as a safety net.
    /// Recommended for:
    /// - Production deployments (optimal reliability + performance)
    /// - Gradual rollout of event-driven features
    /// - Systems requiring zero missed events guarantee
    #[default]
    Hybrid,

    /// Pure event-driven approach (maximum performance)
    ///
    /// Event-driven coordination without polling fallback.
    /// Recommended for:
    /// - High-performance environments with proven event infrastructure
    /// - Systems optimizing for <10ms latency requirements
    /// - Mature deployments with comprehensive monitoring
    EventDrivenOnly,

    /// Disabled event-driven coordination (no polling fallback)
    ///
    /// Disabled coordination without polling fallback.
    /// Recommended for:
    /// - Environments where event-driven coordination is not required
    /// - Systems with no event infrastructure or monitoring
    Disabled,
}

impl fmt::Display for DeploymentMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeploymentMode::PollingOnly => write!(f, "PollingOnly"),
            DeploymentMode::Hybrid => write!(f, "Hybrid"),
            DeploymentMode::EventDrivenOnly => write!(f, "EventDrivenOnly"),
            DeploymentMode::Disabled => write!(f, "Disabled"),
        }
    }
}

impl DeploymentMode {
    /// Check if this mode includes event-driven coordination
    pub fn has_event_driven(&self) -> bool {
        matches!(
            self,
            DeploymentMode::Hybrid | DeploymentMode::EventDrivenOnly
        )
    }

    /// Check if this mode includes polling fallback
    pub fn has_polling(&self) -> bool {
        matches!(self, DeploymentMode::PollingOnly | DeploymentMode::Hybrid)
    }

    /// Check if this mode is event-driven only (no fallback)
    pub fn is_event_driven_only(&self) -> bool {
        matches!(self, DeploymentMode::EventDrivenOnly)
    }

    /// Check if this mode is disabled (no coordination)
    pub fn is_disabled(&self) -> bool {
        matches!(self, DeploymentMode::Disabled)
    }
}

/// Health status for deployment mode assessment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentModeHealthStatus {
    /// System is operating normally within expected parameters
    Healthy,
    /// System is experiencing minor issues but still functional
    Degraded,
    /// System is experiencing significant issues requiring attention
    Warning,
    /// System is failing and requires immediate rollback
    Critical,
}

impl fmt::Display for DeploymentModeHealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeploymentModeHealthStatus::Healthy => write!(f, "Healthy"),
            DeploymentModeHealthStatus::Degraded => write!(f, "Degraded"),
            DeploymentModeHealthStatus::Warning => write!(f, "Warning"),
            DeploymentModeHealthStatus::Critical => write!(f, "Critical"),
        }
    }
}

/// Error types for deployment mode operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum DeploymentModeError {
    #[error("Invalid deployment mode transition from {from} to {to}: {reason}")]
    InvalidTransition {
        from: DeploymentMode,
        to: DeploymentMode,
        reason: String,
    },

    #[error("Deployment mode configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Deployment mode health check failed: {details}")]
    HealthCheckFailed { details: String },
}

impl From<crate::TaskerError> for DeploymentModeError {
    fn from(error: crate::TaskerError) -> Self {
        DeploymentModeError::ConfigurationError {
            message: format!("TaskerError: {}", error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_mode_properties() {
        assert!(DeploymentMode::Hybrid.has_event_driven());
        assert!(DeploymentMode::Hybrid.has_polling());
        assert!(!DeploymentMode::Hybrid.is_event_driven_only());

        assert!(!DeploymentMode::PollingOnly.has_event_driven());
        assert!(DeploymentMode::PollingOnly.has_polling());

        assert!(DeploymentMode::EventDrivenOnly.has_event_driven());
        assert!(!DeploymentMode::EventDrivenOnly.has_polling());
        assert!(DeploymentMode::EventDrivenOnly.is_event_driven_only());
    }
}
