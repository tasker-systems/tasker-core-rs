//! # Worker Services
//!
//! TAS-69: Decomposed service layer for worker operations.
//!
//! This module contains focused service components extracted from the
//! command_processor.rs for better testability and maintainability.
//!
//! ## Services
//!
//! - **step_execution**: Step claiming, execution, and FFI handler invocation
//! - **ffi_completion**: Step completion processing and orchestration notification
//! - **worker_status**: Health checks, status reporting, and event status

pub mod ffi_completion;
pub mod step_execution;
pub mod worker_status;

// Re-export services for convenient access
pub use ffi_completion::FFICompletionService;
pub use step_execution::StepExecutorService;
pub use worker_status::WorkerStatusService;
