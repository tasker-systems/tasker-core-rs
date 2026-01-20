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
mod processing_context;
mod service;
mod state_transition_handler;
mod task_coordinator;

/// Error types for result processing operations
#[derive(Debug, Clone)]
pub enum ResultProcessingError {
    /// Failed to process message
    MessageProcessing { step_uuid: Uuid, reason: String },
    /// Failed to process metadata
    MetadataProcessing { step_uuid: Uuid, reason: String },
    /// Failed to handle state transition
    StateTransition { step_uuid: Uuid, reason: String },
    /// Failed to coordinate task
    TaskCoordination { task_uuid: Uuid, reason: String },
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
                write!(
                    f,
                    "Message processing failed for step {step_uuid}: {reason}"
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_processing_error_display() {
        let step_uuid = Uuid::new_v4();
        let task_uuid = Uuid::new_v4();

        let error = ResultProcessingError::MessageProcessing {
            step_uuid,
            reason: "Invalid format".to_string(),
        };
        assert!(error.to_string().contains("Message processing failed"));
        assert!(error.to_string().contains(&step_uuid.to_string()));

        let error = ResultProcessingError::MetadataProcessing {
            step_uuid,
            reason: "Missing metadata".to_string(),
        };
        assert!(error.to_string().contains("Metadata processing failed"));
        assert!(error.to_string().contains(&step_uuid.to_string()));

        let error = ResultProcessingError::StateTransition {
            step_uuid,
            reason: "Invalid state".to_string(),
        };
        assert!(error.to_string().contains("State transition failed"));
        assert!(error.to_string().contains(&step_uuid.to_string()));

        let error = ResultProcessingError::TaskCoordination {
            task_uuid,
            reason: "Task not found".to_string(),
        };
        assert!(error.to_string().contains("Task coordination failed"));
        assert!(error.to_string().contains(&task_uuid.to_string()));

        let error = ResultProcessingError::EntityNotFound {
            entity_type: "Task".to_string(),
            entity_uuid: task_uuid,
        };
        assert!(error.to_string().contains("Task not found"));
        assert!(error.to_string().contains(&task_uuid.to_string()));
    }

    #[test]
    fn test_result_processing_error_clone() {
        // Test that ResultProcessingError implements Clone
        let step_uuid = Uuid::new_v4();
        let error = ResultProcessingError::MessageProcessing {
            step_uuid,
            reason: "Test error".to_string(),
        };

        let cloned = error.clone();
        match cloned {
            ResultProcessingError::MessageProcessing {
                step_uuid: cloned_uuid,
                reason,
            } => {
                assert_eq!(step_uuid, cloned_uuid);
                assert_eq!(reason, "Test error");
            }
            _ => panic!("Expected MessageProcessing error"),
        }
    }

    #[test]
    fn test_step_error_structure() {
        // Test StepError structure
        let error = StepError {
            message: "Execution failed".to_string(),
            error_type: Some("RuntimeError".to_string()),
            retryable: true,
        };

        assert_eq!(error.message, "Execution failed");
        assert_eq!(error.error_type, Some("RuntimeError".to_string()));
        assert!(error.retryable);
    }

    #[test]
    fn test_step_error_clone() {
        // Test that StepError implements Clone
        let error = StepError {
            message: "Test error".to_string(),
            error_type: Some("TestError".to_string()),
            retryable: false,
        };

        let cloned = error.clone();
        assert_eq!(cloned.message, "Test error");
        assert_eq!(cloned.error_type, Some("TestError".to_string()));
        assert!(!cloned.retryable);
    }

    #[test]
    fn test_result_processing_error_is_error() {
        // Test that ResultProcessingError implements std::error::Error
        let error = ResultProcessingError::MessageProcessing {
            step_uuid: Uuid::new_v4(),
            reason: "Test".to_string(),
        };

        let _error_trait: &dyn std::error::Error = &error;
    }

    #[test]
    fn test_all_result_processing_error_variants() {
        // Verify all variants can be created
        let step_uuid = Uuid::new_v4();
        let task_uuid = Uuid::new_v4();

        let _msg = ResultProcessingError::MessageProcessing {
            step_uuid,
            reason: "test".to_string(),
        };
        let _meta = ResultProcessingError::MetadataProcessing {
            step_uuid,
            reason: "test".to_string(),
        };
        let _state = ResultProcessingError::StateTransition {
            step_uuid,
            reason: "test".to_string(),
        };
        let _task = ResultProcessingError::TaskCoordination {
            task_uuid,
            reason: "test".to_string(),
        };
        let _entity = ResultProcessingError::EntityNotFound {
            entity_type: "Test".to_string(),
            entity_uuid: task_uuid,
        };

        // All variants created successfully
    }
}
