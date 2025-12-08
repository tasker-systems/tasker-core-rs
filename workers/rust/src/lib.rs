//! # Tasker Worker Rust Demonstration
//!
//! Native Rust worker implementation demonstrating high-performance step handlers
//! using the tasker-worker foundation with TAS-40 command pattern and TAS-43 event-driven processing.
//!
//! ## Key Features
//!
//! - **Native Performance**: Pure Rust step handlers for maximum performance
//! - **Shared Infrastructure**: Uses same tasker-worker foundation as Ruby workers
//! - **Event-Driven Processing**: `PostgreSQL` LISTEN/NOTIFY with fallback polling
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
//! ```ignore
//! use tasker_worker_rust::{WorkerBootstrap, WorkerBootstrapConfig, RustStepHandlerRegistry};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create registry (handlers are registered at construction time)
//!     let _registry = RustStepHandlerRegistry::new();
//!
//!     // Bootstrap worker with same configuration as Ruby workers
//!     let config = WorkerBootstrapConfig::default()
//!         .with_supported_namespaces(vec![
//!             "linear_workflow".to_string(), "diamond_workflow".to_string(),
//!             "tree_workflow".to_string(), "mixed_dag_workflow".to_string(),
//!             "order_fulfillment".to_string()
//!         ]);
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
pub mod event_subscribers;
pub mod global_event_system;
pub mod step_handlers;

// Re-export tasker-worker types for easy access
pub use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};

// TAS-67: Re-export bootstrap result type
pub use bootstrap::RustWorkerBootstrapResult;

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
