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
//! - [`messaging`] - PostgreSQL message queue (pgmq) integration
//! - [`registry`] - Component registration and discovery
//! - [`ffi`] - Multi-language FFI bindings
//! - [`resilience`] - Circuit breaker patterns and fault tolerance
//! - [`web`] - REST API server (optional, requires `web-api` feature)
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
//! ```rust
//! use tasker_shared::TaskerConfig;
//!
//! // Initialize configuration for tasker-core-rs
//! let config = TaskerConfig::default();
//!
//! // Configuration provides database settings
//! assert_eq!(config.database.enable_secondary_database, false);
//! assert_eq!(config.execution.max_concurrent_tasks, 100);
//!
//! // For complete database integration examples, see tests/models/ directory
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
pub mod errors;
pub mod events;
pub mod execution;
pub mod logging;
pub mod messaging;
pub mod models;
pub mod registry;
pub mod resilience;
pub mod scopes;
pub mod services;
pub mod sql_functions;
pub mod state_machine;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod types;
pub mod utils;
pub mod validation;

pub use constants::events as system_events;
pub use constants::{
    status_groups, system, ExecutionStatus, HealthStatus, PendingReason, RecommendedAction,
    ReenqueueReason, WorkflowEdgeType,
};
pub use database::{
    AnalyticsMetrics, DependencyLevel, FunctionRegistry, SlowestStepAnalysis, SlowestTaskAnalysis,
    SqlFunctionExecutor, StepReadinessStatus, SystemHealthCounts, TaskExecutionContext,
};

pub use errors::{
    DiscoveryError, EventError, ExecutionError, OrchestrationError, RegistryError, StateError,
    StepExecutionError, TaskerError, TaskerResult,
};
pub use messaging::{
    BatchMessage, BatchResultMessage, PgmqClient, PgmqStepMessage, PgmqStepMessageMetadata,
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult, StepMessage,
    StepMessageMetadata, TaskRequestMessage,
};

pub use registry::TaskHandlerRegistry;
