//! Orchestration queue event types and classifications for TAS-43
//!
//! This module implements structured event handling for orchestration queue events,
//! providing type-safe handling of pgmq-notify events for orchestration coordination.
//!
//! ## Event Types
//!
//! - **StepResultEvent**: Step completion results from workers
//! - **TaskRequestEvent**: New task initialization requests
//! - **QueueManagementEvent**: Queue creation and administrative events

use pgmq_notify::MessageReadyEvent;
use serde::{Deserialize, Serialize};

/// Orchestration queue event classification
///
/// Provides structured classification of pgmq-notify events for orchestration
/// queue processing, similar to TaskReadinessEvent but for queue-level coordination.
#[derive(Debug, Clone)]
pub enum OrchestrationQueueEvent {
    /// Step result event from workers
    StepResult(MessageReadyEvent),
    /// Task request event for initialization
    TaskRequest(MessageReadyEvent),
    /// Queue management event (creation, etc.)
    TaskFinalization(MessageReadyEvent),
    /// Unknown queue event (for monitoring and debugging)
    Unknown { queue_name: String, payload: String },
}

/// Queue management event for administrative operations
///
/// Represents queue-level administrative events such as queue creation,
/// deletion, or configuration changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueManagementEvent {
    /// Queue name
    pub queue_name: String,
    /// Management operation type
    pub operation: QueueOperation,
    /// Namespace associated with queue
    pub namespace: Option<String>,
    /// Operation timestamp
    pub timestamp: Option<String>,
}

/// Queue operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueOperation {
    /// Queue was created
    Created,
    /// Queue was deleted
    Deleted,
    /// Queue configuration updated
    ConfigurationUpdated,
    /// Queue was purged
    Purged,
}

impl std::fmt::Display for QueueOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueOperation::Created => write!(f, "created"),
            QueueOperation::Deleted => write!(f, "deleted"),
            QueueOperation::ConfigurationUpdated => write!(f, "configuration_updated"),
            QueueOperation::Purged => write!(f, "purged"),
        }
    }
}

impl OrchestrationQueueEvent {
    /// Get the namespace associated with this event
    pub fn namespace(&self) -> Option<&str> {
        match self {
            Self::StepResult(event) => Some(&event.namespace),
            Self::TaskRequest(event) => Some(&event.namespace),
            Self::TaskFinalization(event) => Some(&event.namespace),
            Self::Unknown { .. } => None,
        }
    }
}
