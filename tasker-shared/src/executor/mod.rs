//! # Orchestration Executor Module
//!
//! This module provides the core executor traits and implementations for the
//! OrchestrationLoopCoordinator architecture. It replaces the naive tokio async
//! polling loops with a sophisticated, scalable, and monitorable executor pool system.
//!
//! ## Architecture Overview
//!
//! The executor system is built around the `OrchestrationExecutor` trait which provides
//! a standard interface for orchestration workers that can be pooled, monitored, and
//! dynamically scaled.
//!
//! ## Key Components
//!
//! - **OrchestrationExecutor**: Core trait for all executor implementations
//! - **BaseExecutor**: Base implementation with common functionality
//! - **ExecutorMetrics**: Metrics collection and aggregation
//! - **ExecutorHealth**: Health monitoring and state management
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::{OrchestrationExecutor, BaseExecutor, ExecutorType};
//! use sqlx::PgPool;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a database pool (example)
//!     let pool = PgPool::connect("postgresql://localhost/test").await?;
//!
//!     // Create a new executor
//!     let executor = Arc::new(BaseExecutor::new(ExecutorType::TaskRequestProcessor, pool));
//!
//!     // Start the executor
//!     executor.clone().start().await?;
//!
//!     // Monitor health
//!     let health = executor.health().await;
//!     println!("Executor health: {:?}", health);
//!
//!     Ok(())
//! }
//! ```

pub mod base;
pub mod health;
pub mod health_state_machine;
pub mod metrics;
pub mod process_batch_result;
pub mod processing_pool;
pub mod traits;

pub use crate::config::orchestration::{ExecutorConfig, ExecutorType};
pub use base::{BaseExecutor, ProcessingLoop, ProcessingState};
pub use health::{ExecutorHealth, HealthMonitor};
pub use health_state_machine::{
    ExecutorHealthStateMachine, HealthActions, HealthContext, HealthEvent, HealthGuards,
    HealthState,
};
pub use metrics::{ExecutorMetrics, MetricsCollector};
pub use process_batch_result::ProcessBatchResult;
pub use traits::{Executor, ExecutorProcessor};
