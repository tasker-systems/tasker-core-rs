//! State Machine Tests Module
//!
//! This module contains all state machine tests using SQLx native testing
//! for automatic database isolation. These tests cover the core workflow
//! orchestration state management system.

// State machine component tests
pub mod errors;
pub mod events;
pub mod guards;
pub mod persistence;
pub mod states;
pub mod step_state_machine;
pub mod task_state_machine;

// State machine action tests
pub mod actions;
