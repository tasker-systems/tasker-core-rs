//! # Error Injection Handlers for E2E Testing (TAS-64)
//!
//! This module provides step handlers designed to fail controllably for testing
//! retry mechanics and batch cursor resumption.
//!
//! ## Available Handlers
//!
//! - `FailNTimesHandler`: Fails first N attempts, then succeeds
//! - `ConditionalErrorHandler`: Fails based on task context conditions

mod fail_n_times_handler;

pub use fail_n_times_handler::FailNTimesHandler;
