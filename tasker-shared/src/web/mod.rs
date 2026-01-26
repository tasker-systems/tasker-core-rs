//! # Web Utilities
//!
//! Protocol-agnostic web utilities for authorization and request handling.
//! Feature-gated behind `web-api` since it depends on Axum.

#[cfg(feature = "web-api")]
pub mod authorize;

#[cfg(feature = "web-api")]
pub use authorize::authorize;
