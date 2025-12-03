// State machine module for workflow orchestration
//
// This module provides Rust implementations of the Rails Statesman-based state machines
// with enhanced performance, type safety, and async support using the smlang library.

pub mod actions;
pub mod context;
pub mod errors;
pub mod events;
pub mod guards;
pub mod persistence;
pub mod states;
pub mod step_state_machine;
pub mod task_state_machine;

// Re-export main types for convenient access
pub use context::TransitionContext;
pub use errors::{ActionError, GuardError, PersistenceError, StateMachineError};
pub use events::{StepEvent, TaskEvent};
pub use states::{TaskState, WorkflowStepState};
pub use step_state_machine::StepStateMachine;
pub use task_state_machine::TaskStateMachine;

// Common traits and utilities
pub use actions::StateAction;
pub use guards::StateGuard;
pub use persistence::TransitionPersistence;
