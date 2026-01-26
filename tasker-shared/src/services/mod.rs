//! # Services
//!
//! Application services providing higher-level business logic.
//! Services combine types, configuration, and infrastructure to provide
//! cohesive functionality.
//!
//! ## Available Services
//!
//! - [`SecurityService`]: Unified authentication (JWT + API key)
//! - [`require_permission`]: Permission checking for handlers (TAS-76)

pub mod permission_service;
pub mod security_service;

pub use permission_service::require_permission;
pub use security_service::SecurityService;
