//! # Worker Metrics Service
//!
//! TAS-77: Extracted from web/handlers/metrics.rs to enable FFI access to metrics data.
//!
//! This service provides metrics collection functionality independent of the HTTP layer,
//! allowing both the web API and FFI (Ruby/Python) consumers to access the same
//! metrics information.
//!
//! ## Metrics Formats
//!
//! - **Prometheus**: Text format for Prometheus scraping
//! - **JSON**: Structured format for programmatic access
//! - **Domain Events**: Statistics about event routing and delivery

mod service;

pub use service::MetricsService;
