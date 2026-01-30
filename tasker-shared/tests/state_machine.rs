//! State Machine Tests Module
//!
//! This module contains all state machine tests using SQLx native testing
//! for automatic database isolation. These tests cover the core workflow
//! orchestration state management system.

// State machine component tests
#[path = "state_machine/errors.rs"]
pub mod errors;
#[path = "state_machine/events.rs"]
pub mod events;
#[path = "state_machine/guards.rs"]
pub mod guards;
#[path = "state_machine/persistence.rs"]
pub mod persistence;
#[path = "state_machine/states.rs"]
pub mod states;
#[path = "state_machine/step_state_machine.rs"]
pub mod step_state_machine;
#[path = "state_machine/task_state_machine.rs"]
pub mod task_state_machine;

// State machine action tests
#[path = "state_machine/actions.rs"]
pub mod actions;
