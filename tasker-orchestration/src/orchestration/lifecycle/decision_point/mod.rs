//! Decision Point Processing Module
//!
//! Handles dynamic workflow step creation based on decision point outcomes.
//!
//! ## Architecture
//!
//! This module coordinates the following responsibilities:
//!
//! - **OutcomeValidator**: Validates decision outcomes against task templates
//! - **StepCreationService**: Dynamically creates workflow steps using WorkflowStepCreator
//! - **DependencyWirer**: Creates edges between decision parent and new steps
//! - **DecisionPointService**: Main orchestration service
//!
//! The main `DecisionPointService` orchestrates these components to provide
//! a clean, transaction-safe decision point processing interface.

use tasker_shared::TaskerError;

mod service;
// Future components (to be created as needed):
// mod outcome_validator;
// mod dependency_wirer;

pub use service::{DecisionPointProcessingError, DecisionPointService};

impl From<DecisionPointProcessingError> for TaskerError {
    fn from(error: DecisionPointProcessingError) -> Self {
        TaskerError::OrchestrationError(format!("Decision point processing failed: {error}"))
    }
}
