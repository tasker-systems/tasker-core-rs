//! # FFI Completion Service
//!
//! TAS-69: Extracted from command_processor.rs handle_send_step_result() and
//! handle_process_step_completion().
//!
//! Handles step completion processing and orchestration notification.

mod service;

pub use service::FFICompletionService;
