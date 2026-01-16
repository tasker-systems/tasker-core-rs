//! Worker queue event types and classifications for TAS-43
//!
//! This module implements structured event handling for worker namespace queue events,
//! providing type-safe handling of message events for worker coordination.
//!
//! ## Event Types
//!
//! - **StepMessage**: Step execution requests from orchestration to workers
//! - **HealthCheck**: Worker health monitoring events
//! - **ConfigurationUpdate**: Worker configuration change events
//!
//! ## Provider Abstraction (TAS-133)
//!
//! Event types use provider-agnostic `MessageEvent` for multi-backend support.

use serde::{Deserialize, Serialize};
use tasker_shared::messaging::service::MessageEvent;
use tasker_shared::messaging::service::QueuedMessage;

/// Worker queue event classification
///
/// Provides structured classification of messaging events for worker
/// namespace queue processing, focusing on step execution coordination.
///
/// Uses provider-agnostic `MessageEvent` to support multiple messaging backends.
#[derive(Debug, Clone)]
pub enum WorkerQueueEvent {
    /// Step message ready for processing in namespace queue
    StepMessage(MessageEvent),
    /// Worker health monitoring event
    HealthCheck(MessageEvent),
    /// Worker configuration update event
    ConfigurationUpdate(MessageEvent),
    /// Unknown queue event (for monitoring and debugging)
    Unknown { queue_name: String, payload: String },
}

/// Notification wrapper for worker queue events
///
/// Provides additional notification context for worker queue events,
/// including health updates and system notifications.
#[derive(Debug, Clone)]
pub enum WorkerNotification {
    /// Event-driven notification (signal-only, requires message fetch)
    /// Used by PGMQ which sends LISTEN/NOTIFY signals without payload
    Event(WorkerQueueEvent),
    /// Full step message with payload (no fetch required)
    /// Used by RabbitMQ which delivers complete messages via basic_consume
    /// TAS-133: Enables proper RabbitMQ push-based message processing
    StepMessageWithPayload(QueuedMessage<Vec<u8>>),
    /// System health notification
    Health(WorkerHealthUpdate),
    /// Configuration change notification
    Configuration(WorkerConfigUpdate),
}

/// Worker health update notification
///
/// Represents health status changes for worker components,
/// used for monitoring and operational awareness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerHealthUpdate {
    /// Worker instance identifier
    pub worker_id: String,
    /// Health status
    pub status: WorkerHealthStatus,
    /// Supported namespaces
    pub supported_namespaces: Vec<String>,
    /// Last activity timestamp
    pub last_activity: Option<String>,
    /// Additional health details
    pub details: Option<String>,
}

/// Worker health status enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerHealthStatus {
    /// Worker is healthy and processing normally
    Healthy,
    /// Worker is experiencing minor issues but still functional
    Degraded,
    /// Worker is experiencing significant issues
    Warning,
    /// Worker is failing and requires attention
    Critical,
    /// Worker is offline or unresponsive
    Offline,
}

/// Worker configuration update notification
///
/// Represents configuration changes affecting worker behavior,
/// such as namespace additions or processing parameter updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigUpdate {
    /// Worker instance identifier
    pub worker_id: String,
    /// Configuration update type
    pub update_type: ConfigUpdateType,
    /// Affected namespaces
    pub namespaces: Vec<String>,
    /// Update timestamp
    pub timestamp: String,
    /// Configuration details
    pub details: Option<String>,
}

/// Configuration update types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigUpdateType {
    /// Namespace support added
    NamespaceAdded,
    /// Namespace support removed
    NamespaceRemoved,
    /// Processing parameters updated
    ProcessingParametersUpdated,
    /// Template cache updated
    TemplateCacheUpdated,
    /// Connection configuration updated
    ConnectionConfigUpdated,
}

impl std::fmt::Display for WorkerHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerHealthStatus::Healthy => write!(f, "healthy"),
            WorkerHealthStatus::Degraded => write!(f, "degraded"),
            WorkerHealthStatus::Warning => write!(f, "warning"),
            WorkerHealthStatus::Critical => write!(f, "critical"),
            WorkerHealthStatus::Offline => write!(f, "offline"),
        }
    }
}

impl std::fmt::Display for ConfigUpdateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigUpdateType::NamespaceAdded => write!(f, "namespace_added"),
            ConfigUpdateType::NamespaceRemoved => write!(f, "namespace_removed"),
            ConfigUpdateType::ProcessingParametersUpdated => {
                write!(f, "processing_parameters_updated")
            }
            ConfigUpdateType::TemplateCacheUpdated => write!(f, "template_cache_updated"),
            ConfigUpdateType::ConnectionConfigUpdated => write!(f, "connection_config_updated"),
        }
    }
}

impl WorkerQueueEvent {
    /// Get the namespace associated with this event
    pub fn namespace(&self) -> Option<&str> {
        match self {
            Self::StepMessage(event) => Some(&event.namespace),
            Self::HealthCheck(event) => Some(&event.namespace),
            Self::ConfigurationUpdate(event) => Some(&event.namespace),
            Self::Unknown { .. } => None,
        }
    }

    /// Get the queue name for this event
    pub fn queue_name(&self) -> String {
        match self {
            Self::StepMessage(event) => event.queue_name.clone(),
            Self::HealthCheck(event) => event.queue_name.clone(),
            Self::ConfigurationUpdate(event) => event.queue_name.clone(),
            Self::Unknown { queue_name, .. } => queue_name.clone(),
        }
    }

    /// Check if this event represents a step message
    pub fn is_step_message(&self) -> bool {
        matches!(self, Self::StepMessage(_))
    }
}

impl WorkerNotification {
    /// Extract the inner queue event if this is an event notification
    pub fn as_queue_event(&self) -> Option<&WorkerQueueEvent> {
        match self {
            Self::Event(event) => Some(event),
            _ => None,
        }
    }

    /// Check if this notification is a step message event
    pub fn is_step_message(&self) -> bool {
        matches!(self, Self::Event(WorkerQueueEvent::StepMessage(_)))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests;
