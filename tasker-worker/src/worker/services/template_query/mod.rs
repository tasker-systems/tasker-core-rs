//! # Template Query Service
//!
//! TAS-77: Extracted from web/handlers/templates.rs to enable FFI access to template data.
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
//! - **cache_stats**: Get cache statistics
//! - **clear_cache**: Clear template cache
//! - **maintain_cache**: Perform cache maintenance
//! - **refresh_template**: Refresh specific template in cache

mod service;

pub use service::{TemplateQueryError, TemplateQueryService};
