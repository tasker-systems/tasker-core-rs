//! Batch Processing Module (TAS-59)
//!
//! Handles dynamic batch worker creation from batchable steps.
//!
//! ## Architecture
//!
//! This module coordinates the following responsibilities:
//!
//! - **BatchRequirementsAnalyzer**: Analyzes dataset size and calculates optimal batching
//! - **CursorInitializer**: Generates cursor configurations for batch workers
//! - **WorkerCreationService**: Dynamically creates worker instances using WorkflowStepCreator
//! - **BatchProcessingService**: Main orchestration service
//!
//! The main `BatchProcessingService` orchestrates these components to provide
//! a clean, transaction-safe batch worker creation interface.

use tasker_shared::TaskerError;

mod service;

pub use service::{BatchProcessingError, BatchProcessingService};

impl From<BatchProcessingError> for TaskerError {
    fn from(error: BatchProcessingError) -> Self {
        TaskerError::OrchestrationError(format!("Batch processing failed: {error}"))
    }
}
