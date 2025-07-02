// State machine module for workflow orchestration
//
// This module provides Rust implementations of the Rails Statesman-based state machines
// with enhanced performance, type safety, and async support using the smlang library.

pub mod states;
pub mod events;
pub mod guards;
pub mod actions;
pub mod persistence;
pub mod task_state_machine;
pub mod step_state_machine;
pub mod errors;

// Re-export main types for convenient access
pub use states::{TaskState, WorkflowStepState};
pub use events::{TaskEvent, StepEvent};
pub use task_state_machine::TaskStateMachine;
pub use step_state_machine::StepStateMachine;
pub use errors::{StateMachineError, GuardError, ActionError, PersistenceError};

// Common traits and utilities
pub use guards::StateGuard;
pub use actions::StateAction;
pub use persistence::TransitionPersistence;