//! # Web API Integration Tests
//!
//! Comprehensive integration tests for the web API including:
//! - Unauthenticated endpoint testing
//! - JWT-based authentication testing
//! - TLS/HTTPS testing with self-signed certificates
//! - Database pool resource coordination validation
//! - Health monitoring integration testing

pub mod test_infrastructure;
pub mod unauthenticated_tests;
pub mod authenticated_tests;
pub mod tls_tests;
pub mod resource_coordination_tests;
pub mod test_analytics_endpoints;
pub mod test_openapi_documentation;

/// Re-export common test utilities
pub use test_infrastructure::*;