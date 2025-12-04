//! # Worker Status Service
//!
//! TAS-69: Extracted from command_processor.rs handle_get_worker_status(),
//! handle_health_check(), and handle_get_event_status().
//!
//! Handles health checks, status reporting, and event integration status.

mod service;

pub use service::WorkerStatusService;
