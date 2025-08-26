//! tasker-orchestration: Orchestration system for workflow coordination
//! This crate contains the orchestration-specific functionality including
//! the orchestration core, coordinator, finalization system, and web API.

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
//! - [`orchestration`] - Core workflow orchestration logic and components
//! - [`web`] - REST API server (optional, requires `web-api` feature)
//!
//! Additional functionality provided by tasker-shared:
//! - Models, database operations, configuration, error handling
//! - PostgreSQL message queue (pgmq) integration
//! - Circuit breaker patterns and fault tolerance
//!
//! Testing utilities are available in tasker-worker/src/testing for pure Rust testing patterns.
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
//! use tasker_shared::config::TaskerConfig;
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

pub mod orchestration;
#[cfg(feature = "web-api")]
pub mod web;

// Re-export commonly used types from tasker-shared
pub use tasker_shared::{
    config::{ConfigManager, ConfigResult, ConfigurationError},
    TaskerError, TaskerResult,
};

#[cfg(feature = "web-api")]
pub use web::{
    create_app,
    response_types::{ApiError, ApiResult},
    state::{AppState, WebServerConfig},
};
