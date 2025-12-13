//! # Config Query Service
//!
//! TAS-77: Extracted from web/handlers/config.rs to enable FFI access to configuration data.
//!
//! This service provides configuration query functionality independent of the HTTP layer,
//! allowing both the web API and FFI (Ruby/Python) consumers to access the same
//! configuration information.
//!
//! ## Features
//!
//! - Runtime configuration access
//! - Sensitive field redaction
//! - Unified view of common + worker-specific configuration

mod service;

pub use service::{ConfigQueryError, ConfigQueryService};
