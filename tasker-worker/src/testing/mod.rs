//! # Worker Testing Infrastructure
//!
//! Pure Rust testing utilities for the worker system, inspired by FFI shared
//! components but without cross-language dependencies.
//!
//! ## Modules
//!
//! - `database_utils` - Database setup and teardown for testing
//! - `factory` - Test data factories for creating worker test objects
//! - `environment` - Test environment validation and safety checks

pub mod database_utils;
pub mod environment;
pub mod factory;

// Re-export commonly used testing utilities
pub use database_utils::{WorkerDatabaseUtils, WorkerTestDatabase};
pub use environment::{TestSafetyValidator, WorkerTestEnvironment};
pub use factory::{WorkerTestData, WorkerTestFactory};
