//! Simplified fallback task readiness system
//!
//! This module provides a fallback mechanism for catching any ready tasks
//! that may have been missed by the primary pgmq notification system.
//! It periodically runs TaskClaimStepEnqueuer::process_batch() to ensure
//! no tasks are left unprocessed.

pub mod fallback_poller;

pub use fallback_poller::{FallbackPoller, FallbackPollerConfig};
