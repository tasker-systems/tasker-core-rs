//! # Step Execution Service
//!
//! TAS-69: Extracted from command_processor.rs handle_execute_step().
//!
//! Handles step claiming, state verification, message deletion, and FFI handler invocation.

mod service;

pub use service::StepExecutorService;
