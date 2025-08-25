//! API Client Modules
//!
//! HTTP clients for communicating with external services, particularly
//! the orchestration system for task delegation and status coordination.

pub mod orchestration_client;

pub use orchestration_client::OrchestrationApiClient;