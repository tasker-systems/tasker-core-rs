//! # Orchestration Health Service
//!
//! TAS-76: Health check logic extracted from web/handlers/health.rs.
//!
//! This module provides health check functionality independent of the HTTP layer,
//! making it available to both REST (Axum) and future gRPC (Tonic) endpoints.

mod service;

pub use service::HealthService;
