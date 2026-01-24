//! # Web API Request Handlers
//!
//! Contains all HTTP request handlers organized by functional area.
//! Each module handles a specific aspect of the API.
//!
//! ## Authorization ordering note
//!
//! Handlers that accept a request body (e.g. `Json<T>`) perform permission
//! checks (`require_permission`) in the handler body, which runs after Axum's
//! extractors. This means body deserialization occurs before authorization is
//! evaluated. An authenticated user without the required permission may receive
//! 422 (invalid body) instead of 403 (forbidden) if their payload is malformed.
//! This is acceptable because all requests are gated behind JWT/API key
//! authentication — the caller has a valid identity. A future improvement would
//! introduce route-level permission middleware so authorization is evaluated
//! before body extraction.

pub mod analytics;
pub mod config;
pub mod dlq;
pub mod health;
pub mod registry;
pub mod steps;
pub mod tasks;
