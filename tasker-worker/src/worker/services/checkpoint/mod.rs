//! # Checkpoint Service
//!
//! TAS-125: Checkpoint persistence for batch processing handlers.
//!
//! This module provides checkpoint management for handler-driven checkpoint yields.
//! Handlers call `checkpoint_yield()` to persist intermediate progress, enabling
//! resumability without re-processing already-completed items.

mod service;

pub use service::{CheckpointError, CheckpointService};
