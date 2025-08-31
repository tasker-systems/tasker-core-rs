//! # Tasker Worker Rust Demonstration
//!
//! Native Rust worker implementation demonstrating high-performance step handlers
//! using the tasker-worker foundation with TAS-40 command pattern and TAS-43 event-driven processing.
//!
//! ## Key Features
//!
//! - **Native Performance**: Pure Rust step handlers for maximum performance
//! - **Shared Infrastructure**: Uses same tasker-worker foundation as Ruby workers
//! - **Event-Driven Processing**: PostgreSQL LISTEN/NOTIFY with fallback polling
//! - **Workflow Compatibility**: Replicates all Ruby workflow patterns
//! - **Type Safety**: Compile-time guarantees with Rust's type system
//!
//! ## Architecture
//!
//! This demonstration proves that tasker-worker works excellently for native Rust development
//! by implementing all workflow patterns from workers/ruby/spec/handlers/examples/:
//! - Linear Workflow (mathematical sequence)
//! - Diamond Workflow (parallel branches + convergence)
//! - Tree Workflow (hierarchical branching)
//! - Mixed DAG Workflow (complex dependencies)
//! - Order Fulfillment (real-world business process)
//!
//! ## Usage
//!
//! ```rust
//! use tasker_worker_rust::{WorkerBootstrap, WorkerBootstrapConfig, RustStepHandlerRegistry};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Register all Rust step handlers
//!     RustStepHandlerRegistry::register_all_handlers().await?;
//!
//!     // Bootstrap worker with same configuration as Ruby workers
//!     let config = WorkerBootstrapConfig::default()
//!         .with_supported_namespaces(vec![
//!             "linear_workflow", "diamond_workflow", "tree_workflow",
//!             "mixed_dag_workflow", "order_fulfillment"
//!         ])
//!         .with_event_driven_processing(true);
//!
//!     let mut worker_handle = WorkerBootstrap::bootstrap(config).await?;
//!
//!     // Run until shutdown signal
//!     tokio::signal::ctrl_c().await?;
//!     worker_handle.stop()?;
//!
//!     Ok(())
//! }
//! ```

pub mod bootstrap;
pub mod event_handler;
pub mod global_event_system;
pub mod step_handlers;
pub mod test_helpers;

// Re-export tasker-worker types for easy access
pub use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};

// CORRECTED: Use actual production types from tasker-shared
pub use tasker_shared::messaging::StepExecutionResult;
pub use tasker_shared::types::TaskSequenceStep;

// Re-export event handler
pub use event_handler::RustEventHandler;

// Re-export step handler trait and utilities
pub use step_handlers::{
    error_result,
    registry::{GlobalRustStepHandlerRegistry, RustStepHandlerRegistry},
    success_result, RustStepHandler, RustStepHandlerError,
};
