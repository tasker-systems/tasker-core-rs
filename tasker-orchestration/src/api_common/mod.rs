//! # Common API Types
//!
//! Types shared between REST and gRPC APIs that don't depend on either transport.
//!
//! This module was extracted from the `web` module (TAS-177) to allow:
//! - gRPC-only deployments without requiring web-api feature
//! - Shared types without circular dependencies between transports

pub mod circuit_breaker;
pub mod operational_status;

// Re-export commonly used types
pub use circuit_breaker::{CircuitState, WebDatabaseCircuitBreaker};
pub use operational_status::{DatabasePoolUsageStats, OrchestrationStatus};
