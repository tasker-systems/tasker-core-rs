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
//! ## TAS-43/TAS-69 Actor-Based Event-Driven Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────────┐    ┌─────────────────────────┐
//! │ WorkerBootstrap │───▶│  WorkerEventSystem   │───▶│ ActorCommandProcessor   │
//! │  (Config-Driven)│    │   (TAS-43 Core)     │    │ (TAS-69 Actor Pattern)  │
//! └─────────────────┘    └──────────────────────┘    └─────────────────────────┘
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
//! ```rust,no_run
//! use tasker_worker::{WorkerBootstrap, WorkerBootstrapConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!     let mut worker_handle = WorkerBootstrap::bootstrap().await?;
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

pub mod batch_processing;
pub mod bootstrap;
pub mod config;
pub mod error;
pub mod health;
pub mod web;
pub mod worker;

// Testing infrastructure (only available in test builds or with test feature)
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use batch_processing::BatchAggregationScenario;
pub use bootstrap::{
    WorkerBootstrap, WorkerBootstrapConfig, WorkerSystemHandle, WorkerSystemStatus,
};

pub use error::{Result, WorkerError};

pub use health::WorkerHealthStatus;
pub use tasker_shared::types::TaskSequenceStep;
pub use worker::{WorkerCore, WorkerCoreStatus};
