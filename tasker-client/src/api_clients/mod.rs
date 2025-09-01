//! API Client Modules
//!
//! HTTP clients for communicating with external services, particularly
//! the orchestration system for task delegation and status coordination,
//! and worker services for monitoring and management.

pub mod orchestration_client;
pub mod worker_client;

pub use orchestration_client::{OrchestrationApiClient, OrchestrationApiConfig};
pub use worker_client::{WorkerApiClient, WorkerApiConfig};
