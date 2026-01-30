//! Documentation Context Types (TAS-175)
//!
//! Unified context types for configuration documentation generation.
//! These types combine structural metadata from `TaskerConfig` defaults
//! with human-authored documentation from `_docs` TOML sections.
//!
//! Templates (Askama or otherwise) consume these types to produce
//! rendered documentation in various formats.

use serde::Serialize;
use std::fmt;

/// Unified documentation context for a configuration section.
///
/// Represents a TOML table node in the configuration hierarchy.
/// Contains both leaf parameters and nested subsections.
#[derive(Debug, Clone, Serialize)]
pub struct SectionContext {
    /// Section name (e.g., "database", "pool")
    pub name: String,
    /// Dotted path from root (e.g., "common.database.pool")
    pub path: String,
    /// Human description (from `_docs`, or auto-generated fallback)
    pub description: String,
    /// Config context this section belongs to
    pub config_context: ConfigContext,
    /// Parameters in this section (leaf values)
    pub parameters: Vec<ParameterContext>,
    /// Nested subsections
    pub subsections: Vec<SectionContext>,
}

impl SectionContext {
    /// Total parameter count including all nested subsections
    pub fn total_parameters(&self) -> usize {
        self.parameters.len()
            + self
                .subsections
                .iter()
                .map(|s| s.total_parameters())
                .sum::<usize>()
    }

    /// Count of documented parameters including all nested subsections
    pub fn documented_parameters(&self) -> usize {
        self.parameters.iter().filter(|p| p.is_documented).count()
            + self
                .subsections
                .iter()
                .map(|s| s.documented_parameters())
                .sum::<usize>()
    }
}

/// Unified documentation context for a single configuration parameter.
///
/// Merges structural metadata (type, default value) from `TaskerConfig::default()`
/// with human-authored content (description, system impact) from `_docs` sections.
#[derive(Debug, Clone, Serialize)]
pub struct ParameterContext {
    /// Field name (e.g., "max_connections")
    pub name: String,
    /// Dotted path from root (e.g., "common.database.pool.max_connections")
    pub path: String,
    /// Rust type name (e.g., "u32", "String", "bool") — from `_docs` or inferred from TOML value
    pub rust_type: String,
    /// Serialized default value from `TaskerConfig::default()`
    pub default_value: String,
    /// Validation range (e.g., "1-1000") — from `_docs`
    pub valid_range: String,
    /// Human description — from `_docs`, empty if undocumented
    pub description: String,
    /// System impact — from `_docs`
    pub system_impact: String,
    /// Related parameter paths — from `_docs`
    pub related: Vec<String>,
    /// Example TOML snippet — from `_docs`
    pub example: String,
    /// Per-environment recommendations — from `_docs`
    pub recommendations: Vec<RecommendationContext>,
    /// Whether this parameter has `_docs` coverage
    pub is_documented: bool,
}

/// Environment-specific recommendation for a parameter value.
#[derive(Debug, Clone, Serialize)]
pub struct RecommendationContext {
    /// Environment name (e.g., "test", "production")
    pub environment: String,
    /// Recommended value
    pub value: String,
    /// Rationale for the recommendation
    pub rationale: String,
}

/// Which configuration context a section belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConfigContext {
    Common,
    Orchestration,
    Worker,
}

impl fmt::Display for ConfigContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigContext::Common => write!(f, "common"),
            ConfigContext::Orchestration => write!(f, "orchestration"),
            ConfigContext::Worker => write!(f, "worker"),
        }
    }
}

impl ConfigContext {
    /// Parse from string (case-insensitive)
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "common" => Some(Self::Common),
            "orchestration" => Some(Self::Orchestration),
            "worker" => Some(Self::Worker),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_total_parameters() {
        let section = SectionContext {
            name: "database".to_string(),
            path: "common.database".to_string(),
            description: String::new(),
            config_context: ConfigContext::Common,
            parameters: vec![
                ParameterContext {
                    name: "url".to_string(),
                    path: "common.database.url".to_string(),
                    rust_type: "String".to_string(),
                    default_value: "\"localhost\"".to_string(),
                    valid_range: String::new(),
                    description: "Database URL".to_string(),
                    system_impact: String::new(),
                    related: vec![],
                    example: String::new(),
                    recommendations: vec![],
                    is_documented: true,
                },
            ],
            subsections: vec![
                SectionContext {
                    name: "pool".to_string(),
                    path: "common.database.pool".to_string(),
                    description: String::new(),
                    config_context: ConfigContext::Common,
                    parameters: vec![
                        ParameterContext {
                            name: "max_connections".to_string(),
                            path: "common.database.pool.max_connections".to_string(),
                            rust_type: "integer".to_string(),
                            default_value: "25".to_string(),
                            valid_range: "1-1000".to_string(),
                            description: "Max connections".to_string(),
                            system_impact: String::new(),
                            related: vec![],
                            example: String::new(),
                            recommendations: vec![],
                            is_documented: true,
                        },
                        ParameterContext {
                            name: "min_connections".to_string(),
                            path: "common.database.pool.min_connections".to_string(),
                            rust_type: "integer".to_string(),
                            default_value: "5".to_string(),
                            valid_range: String::new(),
                            description: String::new(),
                            system_impact: String::new(),
                            related: vec![],
                            example: String::new(),
                            recommendations: vec![],
                            is_documented: false,
                        },
                    ],
                    subsections: vec![],
                },
            ],
        };

        assert_eq!(section.total_parameters(), 3);
        assert_eq!(section.documented_parameters(), 2);
    }

    #[test]
    fn test_config_context_display() {
        assert_eq!(ConfigContext::Common.to_string(), "common");
        assert_eq!(ConfigContext::Orchestration.to_string(), "orchestration");
        assert_eq!(ConfigContext::Worker.to_string(), "worker");
    }

    #[test]
    fn test_config_context_from_str() {
        assert_eq!(
            ConfigContext::from_str_loose("common"),
            Some(ConfigContext::Common)
        );
        assert_eq!(
            ConfigContext::from_str_loose("ORCHESTRATION"),
            Some(ConfigContext::Orchestration)
        );
        assert_eq!(
            ConfigContext::from_str_loose("Worker"),
            Some(ConfigContext::Worker)
        );
        assert_eq!(ConfigContext::from_str_loose("unknown"), None);
    }
}
