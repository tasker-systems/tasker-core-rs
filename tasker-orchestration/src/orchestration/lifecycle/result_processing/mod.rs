//! Result Processing Module
//!
//! TAS-32: Orchestration Coordination Logic (No State Management)
//!
//! This module handles result processing for orchestration coordination.
//! It focuses on task-level coordination, metadata processing, and intelligent
//! backoff calculations without managing step state transitions (handled by Ruby workers).

use std::fmt;
use uuid::Uuid;

// Re-export service for public API
pub use service::OrchestrationResultProcessor;

// Internal modules
mod message_handler;
mod metadata_processor;
mod service;
mod state_transition_handler;
mod task_coordinator;

/// Error types for result processing operations
#[derive(Debug, Clone)]
pub enum ResultProcessingError {
    /// Failed to process message
    MessageProcessing {
        step_uuid: Uuid,
        reason: String,
    },
    /// Failed to process metadata
    MetadataProcessing {
        step_uuid: Uuid,
        reason: String,
    },
    /// Failed to handle state transition
    StateTransition {
        step_uuid: Uuid,
        reason: String,
    },
    /// Failed to coordinate task
    TaskCoordination {
        task_uuid: Uuid,
        reason: String,
    },
    /// Failed to find required entity
    EntityNotFound {
        entity_type: String,
        entity_uuid: Uuid,
    },
}

impl fmt::Display for ResultProcessingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageProcessing { step_uuid, reason } => {
                write!(f, "Message processing failed for step {step_uuid}: {reason}")
            }
            Self::MetadataProcessing { step_uuid, reason } => {
                write!(
                    f,
                    "Metadata processing failed for step {step_uuid}: {reason}"
                )
            }
            Self::StateTransition { step_uuid, reason } => {
                write!(f, "State transition failed for step {step_uuid}: {reason}")
            }
            Self::TaskCoordination { task_uuid, reason } => {
                write!(f, "Task coordination failed for task {task_uuid}: {reason}")
            }
            Self::EntityNotFound {
                entity_type,
                entity_uuid,
            } => {
                write!(f, "{entity_type} not found: {entity_uuid}")
            }
        }
    }
}

impl std::error::Error for ResultProcessingError {}

/// Step error information for partial results
#[derive(Debug, Clone)]
pub struct StepError {
    pub message: String,
    pub error_type: Option<String>,
    pub retryable: bool,
}
