//! # Template Query Service
//!
//! TAS-77: Extracted from web/handlers/templates.rs to enable FFI access to template data.
//! TAS-169: Simplified API - cache operations removed from public API.
//!
//! This service provides template query functionality independent of the HTTP layer,
//! allowing both the web API and FFI (Ruby/Python) consumers to access the same
//! template information.
//!
//! ## Operations
//!
//! - **get_template**: Retrieve a specific template with metadata
//! - **list_templates**: List supported templates and namespaces
//! - **validate_template**: Validate template for worker execution
//! - **cache_stats**: Get cache statistics (used by list_templates)
//!
//! Cache operations (clear, maintain, refresh) are internal to TaskTemplateManager.
//! Distributed cache status is available via /health/detailed.

mod service;

pub use service::{TemplateQueryError, TemplateQueryService};
