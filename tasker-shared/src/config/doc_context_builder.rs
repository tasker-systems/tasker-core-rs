//! Documentation Context Builder (TAS-175)
//!
//! Builds documentation contexts by combining:
//! 1. Structural metadata from base TOML files (parameter names, defaults, nesting)
//! 2. Human-authored documentation from `_docs` sections via `ConfigDocumentation`
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::doc_context_builder::DocContextBuilder;
//! use tasker_shared::config::doc_context::ConfigContext;
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let builder = DocContextBuilder::new(PathBuf::from("config/tasker/base"))?;
//! let sections = builder.build_sections(ConfigContext::Common);
//! for section in &sections {
//!     println!("{}: {} params", section.name, section.parameters.len());
//! }
//! # Ok(())
//! # }
//! ```

use super::doc_context::{ConfigContext, ParameterContext, RecommendationContext, SectionContext};
use super::documentation::ConfigDocumentation;
use super::error::{ConfigResult, ConfigurationError};
use std::path::PathBuf;
use tracing::{debug, info};

/// Builds `SectionContext` and `ParameterContext` trees from base TOML files.
///
/// The builder loads base configuration TOML files to discover all parameters
/// (including their default values and nesting structure), then cross-references
/// with `ConfigDocumentation` to enrich each parameter with human-authored
/// documentation from `_docs` sections.
#[derive(Debug)]
pub struct DocContextBuilder {
    /// Raw TOML value for the common context (under the "common" key)
    common_value: toml::Value,
    /// Raw TOML value for the orchestration context (under the "orchestration" key)
    orchestration_value: toml::Value,
    /// Raw TOML value for the worker context (under the "worker" key)
    worker_value: toml::Value,
    /// Parsed documentation from `_docs` sections
    documentation: ConfigDocumentation,
}

impl DocContextBuilder {
    /// Create a new builder from the base configuration directory.
    ///
    /// Loads all three base TOML files (common, orchestration, worker) and
    /// parses their `_docs` sections into `ConfigDocumentation`.
    ///
    /// # Arguments
    /// * `base_dir` - Path to base config directory (e.g., "config/tasker/base")
    pub fn new(base_dir: PathBuf) -> ConfigResult<Self> {
        info!(
            "Building documentation context from {}",
            base_dir.display()
        );

        // Load documentation from _docs sections
        let documentation = ConfigDocumentation::load(base_dir.clone())?;

        // Load raw TOML files and extract context sub-tables
        let common_value = Self::load_context_value(&base_dir, "common")?;
        let orchestration_value = Self::load_context_value(&base_dir, "orchestration")?;
        let worker_value = Self::load_context_value(&base_dir, "worker")?;

        info!(
            "Loaded {} documented parameters",
            documentation.parameter_count()
        );

        Ok(Self {
            common_value,
            orchestration_value,
            worker_value,
            documentation,
        })
    }

    /// Build section contexts for a given config context.
    ///
    /// Returns the top-level sections for the requested context. Each section
    /// contains its leaf parameters and nested subsections.
    pub fn build_sections(&self, context: ConfigContext) -> Vec<SectionContext> {
        let (root_value, context_name) = match context {
            ConfigContext::Common => (&self.common_value, "common"),
            ConfigContext::Orchestration => (&self.orchestration_value, "orchestration"),
            ConfigContext::Worker => (&self.worker_value, "worker"),
        };

        let table = match root_value {
            toml::Value::Table(t) => t,
            _ => return vec![],
        };

        let mut sections = Vec::new();
        let mut root_params = Vec::new();

        for (key, value) in table {
            // Skip _docs sections — they're documentation, not config structure
            if key == "_docs" || key.ends_with("_docs") {
                continue;
            }

            let child_path = format!("{}.{}", context_name, key);

            match value {
                toml::Value::Table(sub_table) => {
                    sections.push(self.build_section_from_table(
                        key,
                        &child_path,
                        sub_table,
                        context,
                    ));
                }
                _ => {
                    root_params.push(self.make_parameter_context(key, &child_path, value));
                }
            }
        }

        // If there are root-level scalar parameters, wrap them in a section
        if !root_params.is_empty() {
            sections.insert(
                0,
                SectionContext {
                    name: context_name.to_string(),
                    path: context_name.to_string(),
                    description: format!("Root-level {} parameters", context_name),
                    config_context: context,
                    parameters: root_params,
                    subsections: vec![],
                },
            );
        }

        sections
    }

    /// Build context for a single parameter by dotted path.
    ///
    /// The path should include the context prefix (e.g., "common.database.pool.max_connections").
    pub fn build_parameter(&self, path: &str) -> Option<ParameterContext> {
        let (root_value, context_name) = if path.starts_with("common.") {
            (&self.common_value, "common")
        } else if path.starts_with("orchestration.") {
            (&self.orchestration_value, "orchestration")
        } else if path.starts_with("worker.") {
            (&self.worker_value, "worker")
        } else {
            return None;
        };

        // Strip context prefix to get relative path
        let relative_path = path.strip_prefix(&format!("{}.", context_name))?;
        let parts: Vec<&str> = relative_path.split('.').collect();

        // Navigate to the parameter's parent table
        let mut current = root_value;
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // This is the leaf — extract it
                if let toml::Value::Table(table) = current {
                    if let Some(value) = table.get(*part) {
                        if !matches!(value, toml::Value::Table(_)) {
                            return Some(self.make_parameter_context(part, path, value));
                        }
                    }
                }
                return None;
            }
            // Navigate deeper
            if let toml::Value::Table(table) = current {
                current = table.get(*part)?;
            } else {
                return None;
            }
        }

        None
    }

    /// Get all parameters across all contexts (for coverage stats).
    pub fn all_parameters(&self) -> Vec<ParameterContext> {
        let mut params = Vec::new();
        for context in &[
            ConfigContext::Common,
            ConfigContext::Orchestration,
            ConfigContext::Worker,
        ] {
            for section in self.build_sections(*context) {
                Self::collect_parameters(&section, &mut params);
            }
        }
        params
    }

    /// Get documentation coverage statistics.
    pub fn coverage_stats(
        &self,
    ) -> (
        usize,
        usize,
        Vec<(ConfigContext, usize, usize)>,
    ) {
        let mut total = 0;
        let mut documented = 0;
        let mut per_context = Vec::new();

        for context in &[
            ConfigContext::Common,
            ConfigContext::Orchestration,
            ConfigContext::Worker,
        ] {
            let sections = self.build_sections(*context);
            let ctx_total: usize = sections.iter().map(|s| s.total_parameters()).sum();
            let ctx_documented: usize = sections.iter().map(|s| s.documented_parameters()).sum();
            total += ctx_total;
            documented += ctx_documented;
            per_context.push((*context, ctx_total, ctx_documented));
        }

        (total, documented, per_context)
    }

    /// Get the underlying documentation registry.
    pub fn documentation(&self) -> &ConfigDocumentation {
        &self.documentation
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Load a base TOML file and extract the context sub-table.
    ///
    /// For `common.toml`, this extracts the value under the "common" key.
    fn load_context_value(base_dir: &std::path::Path, context_name: &str) -> ConfigResult<toml::Value> {
        let file_path = base_dir.join(format!("{}.toml", context_name));
        if !file_path.exists() {
            debug!(
                "Base config file not found: {} — using empty table",
                file_path.display()
            );
            return Ok(toml::Value::Table(toml::map::Map::new()));
        }

        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            ConfigurationError::file_read_error(file_path.to_string_lossy(), e)
        })?;

        let root: toml::Value = toml::from_str(&content).map_err(|e| {
            ConfigurationError::invalid_toml(file_path.to_string_lossy(), e)
        })?;

        // Extract the context sub-table (e.g., "common" from common.toml)
        match root {
            toml::Value::Table(mut table) => {
                if let Some(context_value) = table.remove(context_name) {
                    Ok(context_value)
                } else {
                    debug!(
                        "No '{}' key found in {} — using root table",
                        context_name,
                        file_path.display()
                    );
                    Ok(toml::Value::Table(table))
                }
            }
            _ => Ok(toml::Value::Table(toml::map::Map::new())),
        }
    }

    /// Recursively build a SectionContext from a TOML table.
    fn build_section_from_table(
        &self,
        name: &str,
        path: &str,
        table: &toml::map::Map<String, toml::Value>,
        context: ConfigContext,
    ) -> SectionContext {
        let mut parameters = Vec::new();
        let mut subsections = Vec::new();

        for (key, value) in table {
            // Skip _docs sections
            if key == "_docs" || key.ends_with("_docs") {
                continue;
            }

            let child_path = format!("{}.{}", path, key);

            match value {
                toml::Value::Table(sub_table) => {
                    subsections.push(self.build_section_from_table(
                        key,
                        &child_path,
                        sub_table,
                        context,
                    ));
                }
                _ => {
                    parameters.push(self.make_parameter_context(key, &child_path, value));
                }
            }
        }

        SectionContext {
            name: name.to_string(),
            path: path.to_string(),
            description: self.get_section_description(path),
            config_context: context,
            parameters,
            subsections,
        }
    }

    /// Create a ParameterContext from a TOML value, enriched with _docs if available.
    fn make_parameter_context(
        &self,
        name: &str,
        path: &str,
        value: &toml::Value,
    ) -> ParameterContext {
        let default_value = toml_value_display(value);
        let docs = self.documentation.lookup(path);

        if let Some(doc) = docs {
            let recommendations: Vec<RecommendationContext> = doc
                .recommendations
                .iter()
                .map(|(env, rec)| RecommendationContext {
                    environment: env.clone(),
                    value: rec.value.clone(),
                    rationale: rec.rationale.clone(),
                })
                .collect();

            ParameterContext {
                name: name.to_string(),
                path: path.to_string(),
                rust_type: doc.param_type.clone(),
                default_value,
                valid_range: doc.valid_range.clone(),
                description: doc.description.clone(),
                system_impact: doc.system_impact.clone(),
                related: doc.related.clone(),
                example: doc.example.clone(),
                recommendations,
                is_documented: true,
            }
        } else {
            ParameterContext {
                name: name.to_string(),
                path: path.to_string(),
                rust_type: toml_type_name(value),
                default_value,
                valid_range: String::new(),
                description: String::new(),
                system_impact: String::new(),
                related: vec![],
                example: String::new(),
                recommendations: vec![],
                is_documented: false,
            }
        }
    }

    /// Look up a section-level description from _docs convention.
    ///
    /// Section descriptions aren't formally part of the `_docs` convention yet,
    /// so this returns an empty string for now. Can be extended in the future.
    fn get_section_description(&self, _path: &str) -> String {
        // Future: sections could have their own _docs entries
        String::new()
    }

    /// Recursively collect all parameters from a section tree.
    fn collect_parameters(section: &SectionContext, out: &mut Vec<ParameterContext>) {
        out.extend(section.parameters.clone());
        for sub in &section.subsections {
            Self::collect_parameters(sub, out);
        }
    }
}

// ── Utility functions ────────────────────────────────────────────────────

/// Map a TOML value type to a human-readable type name.
fn toml_type_name(value: &toml::Value) -> String {
    match value {
        toml::Value::String(_) => "String".to_string(),
        toml::Value::Integer(_) => "integer".to_string(),
        toml::Value::Float(_) => "float".to_string(),
        toml::Value::Boolean(_) => "bool".to_string(),
        toml::Value::Array(_) => "array".to_string(),
        toml::Value::Datetime(_) => "datetime".to_string(),
        toml::Value::Table(_) => "table".to_string(),
    }
}

/// Format a TOML value for display as a default.
fn toml_value_display(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => format!("\"{}\"", s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => {
            // Ensure float has decimal point for display
            let s = f.to_string();
            if s.contains('.') {
                s
            } else {
                format!("{}.0", s)
            }
        }
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let items: Vec<String> = arr.iter().map(toml_value_display).collect();
                format!("[{}]", items.join(", "))
            }
        }
        toml::Value::Datetime(dt) => dt.to_string(),
        toml::Value::Table(_) => "{...}".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_base_dir() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().to_path_buf();

        // Create a minimal common.toml
        let common_toml = r#"
[common.system]
version = "0.1.0"
max_recursion_depth = 10

[common.system._docs.version]
description = "System version identifier"
type = "String"
valid_range = "semver string"
system_impact = "Displayed in health checks and logs"

[common.database]
url = "postgresql://localhost/test"

[common.database.pool]
max_connections = 25
min_connections = 5

[common.database.pool._docs.max_connections]
description = "Maximum concurrent database connections"
type = "u32"
valid_range = "1-1000"
system_impact = "Controls DB connection concurrency"
related = ["common.database.pool.min_connections"]

[common.database.pool._docs.max_connections.recommendations]
test = { value = "5", rationale = "Minimal for tests" }
production = { value = "30-50", rationale = "Scale with workers" }
"#;

        // Create a minimal orchestration.toml
        let orchestration_toml = r#"
[orchestration]
mode = "standalone"
enable_performance_logging = true

[orchestration._docs.mode]
description = "Orchestration deployment mode"
type = "String"
valid_range = "standalone"
system_impact = "Only standalone is supported"
"#;

        // Create a minimal worker.toml
        let worker_toml = r#"
[worker]
worker_id = "worker-001"
worker_type = "general"
"#;

        fs::write(base_dir.join("common.toml"), common_toml).unwrap();
        fs::write(base_dir.join("orchestration.toml"), orchestration_toml).unwrap();
        fs::write(base_dir.join("worker.toml"), worker_toml).unwrap();

        (temp_dir, base_dir)
    }

    #[test]
    fn test_builder_creation() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir);
        assert!(builder.is_ok());
    }

    #[test]
    fn test_build_sections_common() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let sections = builder.build_sections(ConfigContext::Common);

        // Should have system and database sections
        let section_names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
        assert!(
            section_names.contains(&"system"),
            "Missing 'system' section, got: {:?}",
            section_names
        );
        assert!(
            section_names.contains(&"database"),
            "Missing 'database' section, got: {:?}",
            section_names
        );
    }

    #[test]
    fn test_build_sections_with_subsections() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let sections = builder.build_sections(ConfigContext::Common);
        let db_section = sections.iter().find(|s| s.name == "database").unwrap();

        // database should have a "pool" subsection
        let sub_names: Vec<&str> = db_section
            .subsections
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            sub_names.contains(&"pool"),
            "Missing 'pool' subsection, got: {:?}",
            sub_names
        );
    }

    #[test]
    fn test_documented_parameter_has_docs() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let param = builder
            .build_parameter("common.database.pool.max_connections")
            .expect("Parameter should exist");

        assert!(param.is_documented);
        assert_eq!(param.rust_type, "u32");
        assert_eq!(param.valid_range, "1-1000");
        assert!(!param.description.is_empty());
        assert!(!param.recommendations.is_empty());
    }

    #[test]
    fn test_undocumented_parameter_has_structural_info() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let param = builder
            .build_parameter("common.database.pool.min_connections")
            .expect("Parameter should exist");

        assert!(!param.is_documented);
        assert_eq!(param.rust_type, "integer"); // inferred from TOML value type
        assert_eq!(param.default_value, "5");
        assert!(param.description.is_empty());
    }

    #[test]
    fn test_parameter_not_found() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        assert!(builder.build_parameter("common.nonexistent.param").is_none());
    }

    #[test]
    fn test_orchestration_sections() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let sections = builder.build_sections(ConfigContext::Orchestration);

        // Should have root section with mode and enable_performance_logging
        assert!(!sections.is_empty());

        // Root section should contain the mode parameter
        let all_params = builder.all_parameters();
        let mode_param = all_params
            .iter()
            .find(|p| p.path == "orchestration.mode");
        assert!(mode_param.is_some());
        assert!(mode_param.unwrap().is_documented);
    }

    #[test]
    fn test_coverage_stats() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let (total, documented, per_context) = builder.coverage_stats();

        assert!(total > 0);
        assert!(documented > 0);
        assert!(documented <= total);
        assert_eq!(per_context.len(), 3);
    }

    #[test]
    fn test_recommendations_populated() {
        let (_temp, base_dir) = create_test_base_dir();
        let builder = DocContextBuilder::new(base_dir).unwrap();

        let param = builder
            .build_parameter("common.database.pool.max_connections")
            .unwrap();

        assert_eq!(param.recommendations.len(), 2);
        assert!(param
            .recommendations
            .iter()
            .any(|r| r.environment == "test"));
        assert!(param
            .recommendations
            .iter()
            .any(|r| r.environment == "production"));
    }

    #[test]
    fn test_toml_type_names() {
        assert_eq!(toml_type_name(&toml::Value::String("x".into())), "String");
        assert_eq!(toml_type_name(&toml::Value::Integer(42)), "integer");
        assert_eq!(toml_type_name(&toml::Value::Float(3.14)), "float");
        assert_eq!(toml_type_name(&toml::Value::Boolean(true)), "bool");
        assert_eq!(
            toml_type_name(&toml::Value::Array(vec![])),
            "array"
        );
    }

    #[test]
    fn test_toml_value_display() {
        assert_eq!(
            toml_value_display(&toml::Value::String("hello".into())),
            "\"hello\""
        );
        assert_eq!(toml_value_display(&toml::Value::Integer(42)), "42");
        assert_eq!(toml_value_display(&toml::Value::Boolean(true)), "true");
        assert_eq!(toml_value_display(&toml::Value::Array(vec![])), "[]");
    }
}
