//! # Tasker Client Library
//!
//! Comprehensive client library for interacting with Tasker orchestration and worker APIs.
//! Provides both library interfaces for programmatic use and CLI commands for manual operations.

pub mod api_clients;
pub mod config;
pub mod error;

// Re-export commonly used types for convenience
pub use api_clients::{
    OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient, WorkerApiConfig,
};
pub use config::ClientConfig;
pub use error::{ClientError, ClientResult};
