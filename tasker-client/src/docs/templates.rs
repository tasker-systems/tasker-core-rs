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

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use tasker_shared::config::doc_context::{
        ConfigContext, ParameterContext, RecommendationContext, SectionContext,
    };

    fn test_parameter(documented: bool) -> ParameterContext {
        ParameterContext {
            name: "max_connections".to_string(),
            path: "common.database.pool.max_connections".to_string(),
            rust_type: "u32".to_string(),
            default_value: "25".to_string(),
            valid_range: "1-1000".to_string(),
            description: if documented {
                "Maximum concurrent database connections".to_string()
            } else {
                String::new()
            },
            system_impact: if documented {
                "Controls DB connection concurrency".to_string()
            } else {
                String::new()
            },
            related: if documented {
                vec!["common.database.pool.min_connections".to_string()]
            } else {
                vec![]
            },
            example: String::new(),
            recommendations: if documented {
                vec![
                    RecommendationContext {
                        environment: "test".to_string(),
                        value: "5".to_string(),
                        rationale: "Minimal for tests".to_string(),
                    },
                    RecommendationContext {
                        environment: "production".to_string(),
                        value: "30-50".to_string(),
                        rationale: "Scale with workers".to_string(),
                    },
                ]
            } else {
                vec![]
            },
            is_documented: documented,
        }
    }

    fn test_section() -> SectionContext {
        SectionContext {
            name: "pool".to_string(),
            path: "common.database.pool".to_string(),
            description: String::new(),
            config_context: ConfigContext::Common,
            parameters: vec![test_parameter(true), test_parameter(false)],
            subsections: vec![],
        }
    }

    fn test_sections() -> Vec<SectionContext> {
        vec![SectionContext {
            name: "database".to_string(),
            path: "common.database".to_string(),
            description: String::new(),
            config_context: ConfigContext::Common,
            parameters: vec![ParameterContext {
                name: "url".to_string(),
                path: "common.database.url".to_string(),
                rust_type: "String".to_string(),
                default_value: "\"postgresql://localhost/test\"".to_string(),
                valid_range: String::new(),
                description: "Database connection URL".to_string(),
                system_impact: String::new(),
                related: vec![],
                example: String::new(),
                recommendations: vec![],
                is_documented: true,
            }],
            subsections: vec![test_section()],
        }]
    }

    #[test]
    fn test_config_reference_renders() {
        let sections = test_sections();
        let template = ConfigReferenceTemplate {
            context_name: "common",
            sections: &sections,
            total_parameters: 3,
            documented_parameters: 2,
            generation_timestamp: "2026-01-30T00:00:00Z",
        };

        let output = template.render().expect("Template should render");
        assert!(
            output.contains("Configuration Reference: common"),
            "Missing title"
        );
        assert!(
            output.contains("2/3 parameters documented"),
            "Missing coverage line"
        );
        assert!(output.contains("## database"), "Missing section header");
        assert!(output.contains("### pool"), "Missing subsection header");
        assert!(
            output.contains("`max_connections`"),
            "Missing parameter name"
        );
        assert!(
            output.contains("Maximum concurrent database connections"),
            "Missing documented description"
        );
        // Base template footer
        assert!(
            output.contains("Generated by `tasker-cli docs`"),
            "Missing base template footer"
        );
    }

    #[test]
    fn test_config_reference_includes_recommendations() {
        let sections = test_sections();
        let template = ConfigReferenceTemplate {
            context_name: "common",
            sections: &sections,
            total_parameters: 3,
            documented_parameters: 2,
            generation_timestamp: "2026-01-30T00:00:00Z",
        };

        let output = template.render().unwrap();
        assert!(
            output.contains("Environment Recommendations"),
            "Missing recommendations header"
        );
        assert!(
            output.contains("Scale with workers"),
            "Missing production recommendation"
        );
    }

    #[test]
    fn test_section_detail_renders() {
        let section = test_section();
        let template = SectionDetailTemplate {
            section: &section,
            context_name: "common",
            environment: Some("production"),
        };

        let output = template.render().expect("Template should render");
        assert!(output.contains("# pool"), "Missing section title");
        assert!(output.contains("**Context:** common"), "Missing context");
        assert!(
            output.contains("**Environment:** production"),
            "Missing environment"
        );
        assert!(
            output.contains("`max_connections`"),
            "Missing parameter in table"
        );
    }

    #[test]
    fn test_section_detail_renders_without_environment() {
        let section = test_section();
        let template = SectionDetailTemplate {
            section: &section,
            context_name: "common",
            environment: None,
        };

        let output = template.render().expect("Template should render");
        assert!(output.contains("# pool"), "Missing section title");
        assert!(
            !output.contains("**Environment:**"),
            "Should not show environment when None"
        );
    }

    #[test]
    fn test_annotated_config_renders() {
        let sections = test_sections();
        let template = AnnotatedConfigTemplate {
            context_name: "common",
            environment: "production",
            sections: &sections,
        };

        let output = template.render().expect("Template should render");
        assert!(
            output.contains("# Tasker Configuration Reference — common"),
            "Missing header"
        );
        assert!(
            output.contains("Environment: production"),
            "Missing environment"
        );
        assert!(
            output.contains("[common.database]"),
            "Missing TOML section header"
        );
        assert!(
            output.contains("max_connections = 25"),
            "Missing parameter value"
        );
        assert!(
            output.contains("# Maximum concurrent database connections"),
            "Missing doc comment"
        );
    }

    #[test]
    fn test_parameter_explain_renders() {
        let param = test_parameter(true);
        let template = ParameterExplainTemplate {
            parameter: &param,
            environment: Some("production"),
        };

        let output = template.render().expect("Template should render");
        assert!(
            output.contains("Configuration Parameter: common.database.pool.max_connections"),
            "Missing parameter path"
        );
        assert!(output.contains("Type:        u32"), "Missing type");
        assert!(output.contains("Default:     25"), "Missing default");
        assert!(
            output.contains("Valid Range: 1-1000"),
            "Missing valid range"
        );
        assert!(
            output.contains("Controls DB connection concurrency"),
            "Missing system impact"
        );
        assert!(
            output.contains("common.database.pool.min_connections"),
            "Missing related parameter"
        );
    }

    #[test]
    fn test_parameter_explain_undocumented() {
        let param = test_parameter(false);
        let template = ParameterExplainTemplate {
            parameter: &param,
            environment: None,
        };

        let output = template.render().expect("Template should render");
        assert!(
            output.contains("Type:        u32"),
            "Missing type for undocumented param"
        );
        assert!(
            output.contains("Default:     25"),
            "Missing default for undocumented param"
        );
        // Should not contain recommendation or impact sections
        assert!(
            !output.contains("System Impact"),
            "Should not show empty system impact"
        );
        assert!(
            !output.contains("Environment Recommendations"),
            "Should not show empty recommendations"
        );
    }

    #[test]
    fn test_doc_index_renders() {
        let common_sections = test_sections();
        let orch_sections = vec![SectionContext {
            name: "dlq".to_string(),
            path: "orchestration.dlq".to_string(),
            description: String::new(),
            config_context: ConfigContext::Orchestration,
            parameters: vec![],
            subsections: vec![],
        }];
        let worker_sections = vec![];

        let template = DocIndexTemplate {
            common_sections: &common_sections,
            orchestration_sections: &orch_sections,
            worker_sections: &worker_sections,
            total_parameters: 10,
            documented_parameters: 3,
            coverage_percent: 30,
        };

        let output = template.render().expect("Template should render");
        assert!(
            output.contains("Documentation Index"),
            "Missing index title"
        );
        assert!(
            output.contains("3/10 parameters documented (30%)"),
            "Missing coverage stats"
        );
        assert!(
            output.contains("Common Configuration"),
            "Missing common section"
        );
        assert!(
            output.contains("Orchestration Configuration"),
            "Missing orchestration section"
        );
        assert!(
            output.contains("Worker Configuration"),
            "Missing worker section"
        );
    }

    #[test]
    fn test_annotated_config_no_extra_blank_lines() {
        let sections = test_sections();
        let template = AnnotatedConfigTemplate {
            context_name: "common",
            environment: "test",
            sections: &sections,
        };

        let output = template.render().unwrap();
        // Should not have triple blank lines
        assert!(
            !output.contains("\n\n\n\n"),
            "Output has quadruple blank lines — whitespace control issue"
        );
    }
}
