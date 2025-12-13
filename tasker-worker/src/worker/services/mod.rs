//! # Worker Services
//!
//! TAS-69: Decomposed service layer for worker operations.
//!
//! This module contains focused service components extracted from the
//! command_processor.rs for better testability and maintainability.
//!
//! ## Services
//!
//! - **step_execution**: Step claiming, execution, and FFI handler invocation
//! - **ffi_completion**: Step completion processing and orchestration notification
//! - **worker_status**: Health checks, status reporting, and event status
//! - **health**: TAS-77 - Health check service for web API and FFI consumers
//! - **metrics**: TAS-77 - Metrics collection service for web API and FFI consumers
//! - **template_query**: TAS-77 - Template query service for web API and FFI consumers
//! - **config_query**: TAS-77 - Config query service for web API and FFI consumers

pub mod config_query;
pub mod ffi_completion;
pub mod health;
pub mod metrics;
pub mod step_execution;
pub mod template_query;
pub mod worker_status;

// Re-export services for convenient access
pub use config_query::{ConfigQueryError, ConfigQueryService};
pub use ffi_completion::FFICompletionService;
pub use health::{HealthService, SharedCircuitBreakerProvider};
pub use metrics::MetricsService;
pub use step_execution::StepExecutorService;
pub use template_query::{TemplateQueryError, TemplateQueryService};
pub use worker_status::WorkerStatusService;
