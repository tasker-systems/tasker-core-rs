//! # TAS-148: Orchestration Command Types Module
//!
//! This module contains command types and result structures for orchestration operations.
//! Separated from command processing logic for clarity and maintainability.

pub mod types;

pub use types::{
    CommandResponder, OrchestrationCommand, OrchestrationProcessingStats, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult, TaskReadinessResult,
};
