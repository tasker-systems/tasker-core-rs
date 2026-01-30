//! Askama Template Definitions (TAS-175)
//!
//! Template structs that bind to Askama template files.
//! Each struct's fields become template variables, verified at compile time.

use askama::Template;
use tasker_shared::config::doc_context::{ParameterContext, SectionContext};

/// Full configuration reference document.
///
/// Renders a markdown document listing all sections and parameters for
/// a given configuration context, with documentation where available.
#[derive(Template, Debug)]
#[template(path = "config-reference.md")]
pub struct ConfigReferenceTemplate<'a> {
    /// Context name (e.g., "common", "orchestration", "worker")
    pub context_name: &'a str,
    /// Top-level sections for the context
    pub sections: &'a [SectionContext],
    /// Total parameter count across all sections
    pub total_parameters: usize,
    /// Count of parameters with `_docs` coverage
    pub documented_parameters: usize,
    /// ISO 8601 timestamp of generation
    pub generation_timestamp: &'a str,
}

/// Single section deep-dive document.
///
/// Renders a detailed view of one configuration section,
/// including all parameters with full documentation.
#[derive(Template, Debug)]
#[template(path = "section-detail.md")]
pub struct SectionDetailTemplate<'a> {
    /// The section to document
    pub section: &'a SectionContext,
    /// Context name (e.g., "common")
    pub context_name: &'a str,
    /// Target environment for recommendations (optional)
    pub environment: Option<&'a str>,
}

/// Annotated TOML configuration file with documentation comments.
///
/// Renders a TOML file where each parameter is preceded by
/// documentation comments showing description, type, and range.
#[derive(Template, Debug)]
#[template(path = "annotated-config.toml")]
pub struct AnnotatedConfigTemplate<'a> {
    /// Context name (e.g., "complete")
    pub context_name: &'a str,
    /// Target environment
    pub environment: &'a str,
    /// Sections to include
    pub sections: &'a [SectionContext],
}

/// Single parameter explanation for CLI output.
///
/// Renders a plain-text explanation of one parameter,
/// suitable for `tasker-cli config explain` output.
#[derive(Template, Debug)]
#[template(path = "parameter-explain.txt")]
pub struct ParameterExplainTemplate<'a> {
    /// The parameter to explain
    pub parameter: &'a ParameterContext,
    /// Target environment for filtering recommendations
    pub environment: Option<&'a str>,
}

/// Documentation index page listing all contexts and sections.
///
/// Renders a markdown index showing the full configuration structure
/// with coverage statistics.
#[derive(Template, Debug)]
#[template(path = "doc-index.md")]
pub struct DocIndexTemplate<'a> {
    /// Common configuration sections
    pub common_sections: &'a [SectionContext],
    /// Orchestration configuration sections
    pub orchestration_sections: &'a [SectionContext],
    /// Worker configuration sections
    pub worker_sections: &'a [SectionContext],
    /// Total parameter count across all contexts
    pub total_parameters: usize,
    /// Count of documented parameters
    pub documented_parameters: usize,
    /// Coverage percentage (pre-computed)
    pub coverage_percent: usize,
}
