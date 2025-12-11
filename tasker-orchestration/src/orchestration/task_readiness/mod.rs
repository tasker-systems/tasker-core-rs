//! Simplified fallback task readiness system
//!
//! This module provides a fallback mechanism for catching any ready tasks
//! that may have been missed by the primary pgmq notification system.
//! It periodically runs TaskClaimStepEnqueuer::process_batch() to ensure
//! no tasks are left unprocessed.
//!
//! ## TAS-75 Phase 5b: Circuit Breaker Protection
//!
//! The fallback poller is protected by a circuit breaker to prevent cascading
//! failures when the database is unavailable. See `circuit_breaker` module.

pub mod circuit_breaker;
pub mod fallback_poller;

pub use circuit_breaker::{
    CircuitState, TaskReadinessCircuitBreaker, TaskReadinessCircuitBreakerConfig,
    TaskReadinessCircuitBreakerMetrics,
};
pub use fallback_poller::{FallbackPoller, FallbackPollerConfig};
