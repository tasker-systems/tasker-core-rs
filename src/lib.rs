#![allow(clippy::doc_markdown)] // Allow technical terms like PostgreSQL, SQLx in docs
#![allow(clippy::missing_errors_doc)] // Allow public functions without # Errors sections
#![allow(clippy::must_use_candidate)] // Allow methods without must_use when context is clear

//! # Tasker Core Rust
//!
//! High-performance Rust implementation of the core workflow orchestration engine.
//!
//! ## Overview
//!
//! Tasker Core Rust is designed to complement the existing Ruby on Rails **Tasker** engine,
//! leveraging Rust's memory safety, fearless parallelism, and performance characteristics
//! to handle computationally intensive workflow orchestration, dependency resolution,
//! and state management operations.
//!
//! ## Architecture
//!
//! The core implements a **step handler foundation** where Rust provides the complete
//! step handler base class that frameworks (Rails, Python, Node.js) extend through
//! subclassing with `process()` and `process_results()` hooks.
//!
//! ## Key Features
//!
//! - **Complete Model Layer**: All 18+ Rails models migrated with 100% schema parity
//! - **High-Performance Queries**: Rails-equivalent scopes with compile-time verification
//! - **SQL Function Integration**: Direct PostgreSQL function integration for complex operations
//! - **Memory Safety**: Zero memory leaks with Rust's ownership model
//! - **Type Safety**: Compile-time prevention of SQL injection and type mismatches
//! - **SQLx Native Testing**: Automatic database isolation per test (114+ tests)
//!
//! ## Module Organization
//!
//! - [`models`] - Complete data layer with all Rails models
//! - [`database`] - SQL function execution and database operations
//! - [`state_machine`] - Task and step state management
//! - [`config`] - Configuration management
//! - [`error`] - Structured error handling
//! - [`events`] - Event system foundation
//! - [`orchestration`] - Workflow orchestration logic
//! - [`registry`] - Component registration and discovery
//! - [`ffi`] - Multi-language FFI bindings
//!
//! ## Performance Targets
//!
//! - **10-100x faster** than Ruby/Rails equivalents
//! - **Sub-millisecond** atomic state changes  
//! - **Memory-safe parallelism** with better resource utilization
//! - **Zero-cost abstractions** where possible
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use tasker_core::config::TaskerConfig;
//! use tasker_core::models::core::task::Task;
//! use sqlx::PgPool;
//!
//! # async fn example(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize configuration
//! let config = TaskerConfig::default();
//!
//! // Work with models using SQLx
//! // Note: find_by_current_state would need to be implemented
//! // let tasks = Task::find_by_current_state(pool, "running").await?;
//! // for task in tasks {
//! //     println!("Task {} is running", task.task_id);
//! // }
//!
//! println!("Tasker core initialized with config: {:?}", config.database.max_connections);
//! # Ok(())
//! # }
//! ```
//!
//! ## Integration
//!
//! This Rust core serves as the foundational step handler that frameworks extend.
//! The Rails engine provides the web interface and developer ergonomics, while this
//! Rust core handles all performance and safety-critical workflow orchestration logic.
//!
//! ## Testing
//!
//! The project uses SQLx native testing with automatic database isolation:
//!
//! ```bash
//! cargo test --lib    # Unit tests
//! cargo test          # All tests (114+ tests)
//! ```

pub mod config;
pub mod constants;
pub mod database;
pub mod error;
pub mod events;
pub mod ffi;
pub mod models;
pub mod orchestration;
pub mod registry;
pub mod scopes;
pub mod sql_functions;
pub mod state_machine;
pub mod validation;

pub use config::{
    BackoffConfig, DatabaseConfig, EventConfig, ExecutionConfig, ReenqueueDelays, TaskerConfig,
    TelemetryConfig,
};
pub use constants::{
    status_groups, system, ExecutionStatus, HealthStatus, PendingReason, RecommendedAction,
    ReenqueueReason, TaskStatus, WorkflowEdgeType, WorkflowStepStatus,
};
// Re-export constants events with different name to avoid conflict
pub use constants::events as system_events;
pub use database::{
    AnalyticsMetrics, DependencyLevel, FunctionRegistry, SlowestStepAnalysis, SlowestTaskAnalysis,
    SqlFunctionExecutor, StepReadinessStatus, SystemHealthCounts, TaskExecutionContext,
};
pub use error::{Result, TaskerError};
