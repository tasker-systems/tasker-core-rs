//! # Error Injection Handlers for E2E Testing (TAS-64)
//!
//! This module provides step handlers designed to fail controllably for testing
//! retry mechanics and batch cursor resumption.
//!
//! ## Available Handlers
//!
//! - `FailNTimesHandler`: Fails first N attempts, then succeeds
//! - `CheckpointAndFailHandler`: Batch worker that checkpoints and fails for resumption testing

mod checkpoint_and_fail_handler;
mod fail_n_times_handler;

pub use checkpoint_and_fail_handler::CheckpointAndFailHandler;
pub use fail_n_times_handler::FailNTimesHandler;
