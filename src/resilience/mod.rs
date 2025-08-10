//! # Resilience Module
//!
//! Provides fault tolerance and circuit breaker patterns for production deployment.
//! This module implements the circuit breaker patterns outlined in TAS-31 to prevent
//! cascade failures and provide graceful degradation under load.
//!
//! ## Architecture
//!
//! - **Circuit Breakers**: Prevent cascade failures by isolating failing components
//! - **Metrics Collection**: Track failure rates and circuit breaker state transitions
//! - **Configuration**: Environment-specific thresholds and behavior settings
//! - **Integration Points**: PGMQ operations, FFI calls, database connections
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::resilience::{CircuitBreaker, CircuitBreakerConfig};
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create circuit breaker for database operations
//! let config = CircuitBreakerConfig {
//!     failure_threshold: 5,
//!     timeout: Duration::from_secs(30),
//!     success_threshold: 2,
//! };
//!
//! let circuit_breaker = CircuitBreaker::new("database_operations".to_string(), config);
//!
//! // Use circuit breaker to protect operations
//! let result = circuit_breaker.call(|| async {
//!     // Database operation here
//!     Ok::<&str, Box<dyn std::error::Error>>("success")
//! }).await?;
//! # Ok(())
//! # }
//! ```

pub mod circuit_breaker;
pub mod config;
pub mod manager;
pub mod metrics;

#[cfg(test)]
mod yaml_config_test;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerError, CircuitState};
pub use config::{CircuitBreakerConfig, GlobalCircuitBreakerSettings};
pub use manager::CircuitBreakerManager;
pub use metrics::{CircuitBreakerMetrics, SystemCircuitBreakerMetrics};
