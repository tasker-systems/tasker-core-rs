//! # Shared Template API Types
//!
//! Common types for template-related API endpoints shared between
//! orchestration and worker services.
//!
//! TAS-76: Extracted from worker-specific types to enable template
//! discovery endpoints in orchestration.

use serde::{Deserialize, Serialize};

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

// =============================================================================
// Request Types
// =============================================================================

/// Query parameters for template listing
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "web-api", derive(utoipa::IntoParams))]
pub struct TemplateQueryParams {
    /// Filter by namespace
    pub namespace: Option<String>,
}

/// Path parameters for template operations
#[derive(Debug, Clone, Deserialize)]
pub struct TemplatePathParams {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

// =============================================================================
// Response Types
// =============================================================================

/// Summary information about a template
///
/// Used in list responses to provide an overview of available templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TemplateSummary {
    /// Template name
    pub name: String,
    /// Namespace the template belongs to
    pub namespace: String,
    /// Template version
    pub version: String,
    /// Optional description
    pub description: Option<String>,
    /// Number of steps in the template
    pub step_count: usize,
}

/// Summary of a namespace with its templates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct NamespaceSummary {
    /// Namespace name
    pub name: String,
    /// Optional namespace description
    pub description: Option<String>,
    /// Number of templates in this namespace
    pub template_count: usize,
}

/// Response for template listing endpoint
///
/// Provides an overview of all available templates, optionally filtered by namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TemplateListResponse {
    /// Summary of available namespaces
    pub namespaces: Vec<NamespaceSummary>,
    /// List of templates (optionally filtered)
    pub templates: Vec<TemplateSummary>,
    /// Total number of templates (before filtering)
    pub total_count: usize,
}

/// Detailed information about a single template
///
/// Returned when fetching a specific template by namespace/name/version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct TemplateDetail {
    /// Template name
    pub name: String,
    /// Namespace the template belongs to
    pub namespace: String,
    /// Template version
    pub version: String,
    /// Optional description
    pub description: Option<String>,
    /// Template configuration (JSON)
    #[cfg_attr(feature = "web-api", schema(value_type = Object))]
    pub configuration: Option<serde_json::Value>,
    /// Step definitions for this template
    pub steps: Vec<StepDefinition>,
}

/// Definition of a step within a template
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct StepDefinition {
    /// Step name/handler identifier
    pub name: String,
    /// Step description
    pub description: Option<String>,
    /// Whether the step is retryable by default
    pub default_retryable: bool,
    /// Maximum retry attempts
    pub default_max_attempts: i32,
}
