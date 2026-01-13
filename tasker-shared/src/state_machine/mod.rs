//! # State Machine Module
//!
//! Type-safe, async state machines for task and workflow step orchestration.
//!
//! ## Overview
//!
//! This module provides Rust implementations of state machines that manage the lifecycle
//! of tasks and workflow steps. The design follows the pattern: **Events** trigger
//! **Transitions** which are validated by **Guards** and execute **Actions**.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
//! │   Events    │ ──► │    Guards    │ ──► │   Actions   │
//! │  (Triggers) │     │ (Validation) │     │ (Side Efx)  │
//! └─────────────┘     └──────────────┘     └─────────────┘
//!        │                   │                    │
//!        └───────────────────┴────────────────────┘
//!                            │
//!                   ┌────────▼────────┐
//!                   │   Persistence   │
//!                   │  (State Store)  │
//!                   └─────────────────┘
//! ```
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`states`] | Task (12 states) and Step (10 states) state enums |
//! | [`events`] | `TaskEvent` and `StepEvent` trigger enums |
//! | [`guards`] | `StateGuard` trait and validation implementations |
//! | [`actions`] | `StateAction` trait and side-effect implementations |
//! | [`persistence`] | `TransitionPersistence` for database state storage |
//! | [`context`] | `TransitionContext` for transition metadata |
//! | [`task_state_machine`] | Task lifecycle state machine |
//! | [`step_state_machine`] | Step execution state machine |
//! | [`errors`] | Error types for guards, actions, and persistence |
//!
//! ## Task State Lifecycle
//!
//! ```text
//! Pending → Initializing → EnqueuingSteps → StepsInProcess
//!                                                 ↓
//!                                         EvaluatingResults
//!                                           ↓         ↓
//!                                       Complete    Error
//! ```
//!
//! See [`states::TaskState`] for the complete 12-state lifecycle.
//!
//! ## Step State Lifecycle
//!
//! ```text
//! Pending → Enqueued → InProgress → EnqueuedForOrchestration → Complete
//!                          ↓
//!                     WaitingForRetry → (re-enqueue)
//! ```
//!
//! See [`states::WorkflowStepState`] for the complete 10-state lifecycle.

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
