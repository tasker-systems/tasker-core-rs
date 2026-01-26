//! # Web API Request Handlers
//!
//! Contains all HTTP request handlers organized by functional area.
//! Each module handles a specific aspect of the API.
//!
//! ## Authorization (TAS-176)
//!
//! Permission checks are performed at the route level via the `authorize()` wrapper,
//! which runs **before** body deserialization. This ensures unauthorized requests
//! are rejected with 403 before any request body parsing occurs.
//!
//! See `tasker-shared/src/web/authorize.rs` for the implementation.

pub mod analytics;
pub mod config;
pub mod dlq;
pub mod health;
pub mod steps;
pub mod tasks;
pub mod templates;
