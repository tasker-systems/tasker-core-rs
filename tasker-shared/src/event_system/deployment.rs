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

use derive_more::Display;
use serde::{Deserialize, Serialize};

/// Deployment mode for event systems
///
/// Enum deserialization will fail if TOML contains invalid value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Display)]
#[serde(rename_all = "PascalCase")]
pub enum DeploymentMode {
    /// Pure event-driven using PostgreSQL LISTEN/NOTIFY
    #[display("EventDrivenOnly")]
    EventDrivenOnly,
    /// Traditional polling-based coordination
    #[display("PollingOnly")]
    PollingOnly,
    /// Event-driven with polling fallback (recommended)
    #[default]
    #[display("Hybrid")]
    Hybrid,

    /// Disabled
    #[display("Disabled")]
    Disabled,
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

    /// Get the effective deployment mode for the given messaging provider.
    ///
    /// Different messaging backends have fundamentally different delivery models:
    ///
    /// - **PGMQ**: Uses PostgreSQL `pg_notify` for signaling, which is fire-and-forget.
    ///   If no listener is connected when a notification fires, it's lost. Messages
    ///   persist in the queue but no one knows to look for them without polling.
    ///   Fallback polling is essential for reliability.
    ///
    /// - **RabbitMQ**: Uses `basic_consume()` push-based delivery with broker-managed
    ///   acknowledgment tracking. The broker redelivers unacked messages when consumers
    ///   reconnect. Fallback polling is unnecessary and wasteful.
    ///
    /// For RabbitMQ, this always returns `EventDrivenOnly` regardless of configuration.
    /// Callers should compare the result with the original mode and log appropriately:
    /// - `Hybrid` → `EventDrivenOnly`: warn (reasonable intent, but unnecessary)
    /// - `PollingOnly` → `EventDrivenOnly`: error (indicates misunderstanding)
    pub fn effective_for_provider(&self, provider_name: &str) -> Self {
        match provider_name {
            "rabbitmq" => DeploymentMode::EventDrivenOnly,
            _ => *self, // PGMQ and others respect configured mode
        }
    }
}

/// Health status for deployment mode assessment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum DeploymentModeHealthStatus {
    /// System is operating normally within expected parameters
    #[display("Healthy")]
    Healthy,
    /// System is experiencing minor issues but still functional
    #[display("Degraded")]
    Degraded,
    /// System is experiencing significant issues requiring attention
    #[display("Warning")]
    Warning,
    /// System is failing and requires immediate rollback
    #[display("Critical")]
    Critical,
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

    #[test]
    fn test_effective_for_provider_pgmq() {
        // PGMQ respects all configured modes (pg_notify can miss messages)
        assert_eq!(
            DeploymentMode::PollingOnly.effective_for_provider("pgmq"),
            DeploymentMode::PollingOnly
        );
        assert_eq!(
            DeploymentMode::Hybrid.effective_for_provider("pgmq"),
            DeploymentMode::Hybrid
        );
        assert_eq!(
            DeploymentMode::EventDrivenOnly.effective_for_provider("pgmq"),
            DeploymentMode::EventDrivenOnly
        );
    }

    #[test]
    fn test_effective_for_provider_rabbitmq() {
        // RabbitMQ always uses EventDrivenOnly (broker handles redelivery)
        assert_eq!(
            DeploymentMode::PollingOnly.effective_for_provider("rabbitmq"),
            DeploymentMode::EventDrivenOnly
        );
        assert_eq!(
            DeploymentMode::Hybrid.effective_for_provider("rabbitmq"),
            DeploymentMode::EventDrivenOnly
        );
        assert_eq!(
            DeploymentMode::EventDrivenOnly.effective_for_provider("rabbitmq"),
            DeploymentMode::EventDrivenOnly
        );
    }
}
