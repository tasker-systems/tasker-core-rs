//! # Worker Health Service
//!
//! TAS-77: Extracted from web/handlers/health.rs to enable FFI access to health data.
//!
//! This service provides health check functionality independent of the HTTP layer,
//! allowing both the web API and FFI (Ruby/Python) consumers to access the same
//! health information.
//!
//! ## Health Checks
//!
//! - **Database**: PostgreSQL connectivity check
//! - **Command Processor**: Worker core health status
//! - **Queue Processing**: PGMQ queue depth analysis
//! - **Event System**: Event system availability
//! - **Step Processing**: Error rate calculation
//! - **Circuit Breakers**: Circuit breaker states (TAS-75)

mod service;

pub use service::{HealthService, SharedCircuitBreakerProvider};
