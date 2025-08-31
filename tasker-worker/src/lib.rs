//! # tasker-worker: TAS-40/TAS-43 Command Pattern with Event-Driven Processing
//!
//! This crate provides a comprehensive worker system that combines the TAS-40 command
//! pattern with TAS-43 event-driven processing architecture. Features multiple deployment
//! modes, robust reliability patterns, and seamless integration with orchestration systems.
//!
//! ## Key Features
//!
//! - **TAS-43 Event-Driven Processing**: Real-time PostgreSQL LISTEN/NOTIFY with fallback polling
//! - **Multiple Deployment Modes**: PollingOnly, EventDrivenOnly, Hybrid (recommended)
//! - **TAS-40 Command Pattern**: Unified command-driven architecture with tokio channels
//! - **Reliability-First Design**: Hybrid mode ensures no message loss with dual processing
//! - **Configuration-Driven**: TOML-based configuration with environment-specific overrides
//! - **FFI Integration**: Event-driven communication with Ruby/Python/WASM
//! - **Database-as-API**: Workers hydrate context from step message UUIDs
//! - **Orchestration Integration**: Seamless communication with orchestration systems
//!
//! ## TAS-43 Event-Driven Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────────┐    ┌─────────────────────┐
//! │ WorkerBootstrap │───▶│  WorkerEventSystem   │───▶│   WorkerProcessor   │
//! │  (Config-Driven)│    │   (TAS-43 Core)     │    │   (TAS-40 Commands) │
//! └─────────────────┘    └──────────────────────┘    └─────────────────────┘
//!                               │                               │
//!                               ▼                               ▼
//!                        ┌─────────────────┐            ┌──────────────────┐
//!                        │ PostgreSQL      │            │ Step Handlers    │
//!                        │ LISTEN/NOTIFY   │            │ (Ruby/Python)    │
//!                        │ + Fallback Poll │            │                  │
//!                        └─────────────────┘            └──────────────────┘
//! ```
//!
//! ## Usage Example
//!
//! ```rust
//! use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // TAS-43 Event-Driven Worker with Configuration
//!     let config = WorkerBootstrapConfig {
//!         worker_id: "worker-001".to_string(),
//!         supported_namespaces: vec!["payments".to_string(), "inventory".to_string()],
//!         enable_web_api: true,
//!         event_driven_enabled: true,  // TAS-43 Event-Driven Processing
//!         deployment_mode_hint: Some("Hybrid".to_string()), // Reliability mode
//!         ..Default::default()
//!     };
//!
//!     let mut worker_handle = WorkerBootstrap::bootstrap(config).await?;
//!
//!     // Worker system is now running with:
//!     // - Real-time PostgreSQL LISTEN/NOTIFY
//!     // - Fallback polling for reliability
//!     // - Command pattern step processing
//!     // - Configuration-driven deployment modes
//!
//!     // Graceful shutdown
//!     worker_handle.stop()?;
//!     Ok(())
//! }
//! ```

pub mod api_clients;
pub mod bootstrap;
pub mod config;
pub mod error;
pub mod health;
pub mod web;
pub mod worker;

// Testing infrastructure (only available in test builds or with test feature)
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use api_clients::OrchestrationApiClient;
pub use bootstrap::{
    WorkerBootstrap, WorkerBootstrapConfig, WorkerSystemHandle, WorkerSystemStatus,
};

pub use error::{Result, WorkerError};

pub use health::WorkerHealthStatus;
pub use tasker_shared::types::TaskSequenceStep;
pub use worker::{WorkerCore, WorkerCoreStatus};
