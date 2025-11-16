//! Batch Processing Infrastructure
//!
//! Provides helper types and utilities for implementing batch processing workflows.
//!
//! # Overview
//!
//! Batch processing workflows involve three types of steps:
//! 1. **Batchable Step**: Analyzes dataset and decides whether to create batch workers
//! 2. **Batch Worker Steps**: Process data in parallel batches (dynamically created)
//! 3. **Convergence Step**: Aggregates results after all workers complete
//!
//! This module provides infrastructure to handle:
//! - **Worker helpers**: Extract cursor configuration and detect no-op workers
//! - **Convergence helpers**: Handle NoBatches vs WithBatches scenarios

mod convergence;
mod worker_helper;

pub use convergence::BatchAggregationScenario;
pub use worker_helper::BatchWorkerContext;
