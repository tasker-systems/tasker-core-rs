//! Configuration Documentation Parser
//!
//! Parses structured documentation metadata from base configuration files
//! using the `_docs` convention. Documentation metadata is embedded in base
//! TOML files and stripped during config generation.
//!
//! ## Convention
//!
//! Documentation uses a `_docs` prefix to clearly separate metadata from
//! actual configuration:
//!
//! ```toml
//! [database.pool]
//! max_connections = 30
//!
//! [database.pool._docs.max_connections]
//! description = "Maximum number of concurrent database connections"
//! type = "u32"
//! valid_range = "1-1000"
//! system_impact = "Controls database connection concurrency"
//!
//! [database.pool._docs.max_connections.recommendations]
//! test = { value = "5", rationale = "Minimal connections" }
//! production = { value = "30-50", rationale = "Scale based on worker count" }
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::ConfigDocumentation;
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load documentation from base config directory
//! let docs = ConfigDocumentation::load(PathBuf::from("config/tasker/base"))?;
//!
//! // Look up parameter documentation
//! if let Some(param_docs) = docs.lookup("database.pool.max_connections") {
//!     println!("Description: {}", param_docs.description);
//!     println!("Valid range: {}", param_docs.valid_range);
//! }
//! # Ok(())
//! # }
//! ```

use super::error::{ConfigResult, ConfigurationError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

/// Documentation for a single configuration parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDocumentation {
    /// Parameter path (e.g., "database.pool.max_connections")
    #[serde(skip)]
    pub path: String,

    /// Human-readable description of the parameter's purpose
    pub description: String,

    /// Data type (e.g., "u32", "String", "bool")
    #[serde(rename = "type")]
    pub param_type: String,

    /// Valid value range or format
    pub valid_range: String,

    /// Default value (as string for display)
    #[serde(default)]
    pub default: String,

    /// What this parameter affects in the system
    pub system_impact: String,

    /// Related parameters that users should consider together
    #[serde(default)]
    pub related: Vec<String>,

    /// Example usage (TOML snippet)
    #[serde(default)]
    pub example: String,

    /// Environment-specific recommendations
    #[serde(default)]
    pub recommendations: HashMap<String, EnvironmentRecommendation>,
}

/// Environment-specific recommendation for a parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentRecommendation {
    /// Recommended value for this environment
    pub value: String,

    /// Rationale explaining why this value is recommended
    pub rationale: String,
}

/// Central documentation registry
///
/// Loads and caches parameter documentation from base TOML files.
/// Documentation is embedded using `_docs` prefix convention.
#[derive(Debug)]
pub struct ConfigDocumentation {
    /// Base configuration directory path
    base_dir: PathBuf,

    /// Cached parameter documentation by parameter path
    cache: HashMap<String, ParameterDocumentation>,
}

impl ConfigDocumentation {
    /// Load documentation from base configuration directory
    ///
    /// Parses all `_docs` sections from base TOML files (common, orchestration, worker)
    /// and builds an indexed lookup cache.
    ///
    /// # Arguments
    /// * `base_dir` - Path to base configuration directory (e.g., "config/tasker/base")
    ///
    /// # Returns
    /// * `ConfigResult<Self>` - Loaded documentation registry
    ///
    /// # Errors
    /// * If base directory doesn't exist
    /// * If TOML files cannot be parsed
    pub fn load(base_dir: PathBuf) -> ConfigResult<Self> {
        info!("Loading configuration documentation from {}", base_dir.display());

        if !base_dir.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![base_dir]));
        }

        let mut cache = HashMap::new();

        // Load documentation from all context files
        for context in &["common", "orchestration", "worker"] {
            let config_file = base_dir.join(format!("{}.toml", context));

            if !config_file.exists() {
                debug!("Skipping {} - file not found", context);
                continue;
            }

            info!("Parsing documentation from {}", config_file.display());

            let content = fs::read_to_string(&config_file).map_err(|e| {
                ConfigurationError::file_read_error(config_file.to_string_lossy(), e)
            })?;

            let toml_value: toml::Value = toml::from_str(&content).map_err(|e| {
                ConfigurationError::invalid_toml(config_file.to_string_lossy(), e)
            })?;

            // Extract all _docs sections
            Self::extract_docs_from_value(&toml_value, String::new(), &mut cache);
        }

        info!("Loaded documentation for {} parameters", cache.len());

        Ok(Self { base_dir, cache })
    }

    /// Recursively extract documentation from TOML value
    ///
    /// Searches for keys starting with `_docs` and parses them into ParameterDocumentation.
    fn extract_docs_from_value(
        value: &toml::Value,
        current_path: String,
        cache: &mut HashMap<String, ParameterDocumentation>,
    ) {
        if let toml::Value::Table(table) = value {
            for (key, val) in table {
                // Build the full path for this key
                let full_path = if current_path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", current_path, key)
                };

                // Check if this is a _docs section
                if key == "_docs" {
                    // This is a _docs container - recurse into it to find parameter docs
                    if let toml::Value::Table(docs_table) = val {
                        for (param_name, param_docs) in docs_table {
                            // Build the full parameter path (exclude _docs from path)
                            let param_path = if current_path.is_empty() {
                                param_name.clone()
                            } else {
                                format!("{}.{}", current_path, param_name)
                            };

                            // Try to deserialize the documentation
                            match toml::from_str::<ParameterDocumentation>(&toml::to_string(param_docs).unwrap()) {
                                Ok(mut docs) => {
                                    docs.path = param_path.clone();
                                    debug!("Loaded documentation for parameter: {}", param_path);
                                    cache.insert(param_path, docs);
                                }
                                Err(e) => {
                                    debug!("Failed to parse documentation for {}: {}", param_path, e);
                                }
                            }
                        }
                    }
                } else {
                    // Regular config section - recurse into it
                    Self::extract_docs_from_value(val, full_path, cache);
                }
            }
        }
    }

    /// Look up documentation for a parameter by path
    ///
    /// # Arguments
    /// * `path` - Parameter path (e.g., "database.pool.max_connections")
    ///
    /// # Returns
    /// * `Option<&ParameterDocumentation>` - Documentation if found
    pub fn lookup(&self, path: &str) -> Option<&ParameterDocumentation> {
        self.cache.get(path)
    }

    /// Get all documented parameters
    ///
    /// # Returns
    /// * Iterator over all parameter documentation entries
    pub fn all_parameters(&self) -> impl Iterator<Item = &ParameterDocumentation> {
        self.cache.values()
    }

    /// List all parameters for a specific context
    ///
    /// # Arguments
    /// * `context` - Context name ("common", "orchestration", "worker")
    ///
    /// # Returns
    /// * Vector of parameter documentation for matching context
    pub fn list_for_context(&self, context: &str) -> Vec<&ParameterDocumentation> {
        let prefixes: Vec<&str> = match context {
            "orchestration" => vec![
                "orchestration_system",
                "orchestration_events",
                "task_readiness_events",
                "mpsc_channels.command_processor",
                "mpsc_channels.event_systems",
                "mpsc_channels.event_listeners",
                "backoff",
            ],
            "worker" => vec![
                "worker_system",
                "worker_events",
                "mpsc_channels.event_subscribers",
            ],
            "common" => vec![
                "database",
                "queues",
                "circuit_breakers",
                "shared_channels",
            ],
            _ => vec![],
        };

        self.cache
            .values()
            .filter(|docs| {
                // Include if path starts with any of the context prefixes
                prefixes.iter().any(|prefix| docs.path.starts_with(prefix))
            })
            .collect()
    }

    /// Get the base directory path
    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    /// Get the total number of documented parameters
    pub fn parameter_count(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_docs() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("base");
        fs::create_dir_all(&base_dir).unwrap();

        // Create a test config with _docs sections
        let config_with_docs = r#"
[database.pool]
max_connections = 30
min_connections = 8

[database.pool._docs.max_connections]
description = "Maximum number of concurrent database connections in the pool"
type = "u32"
valid_range = "1-1000"
default = "30"
system_impact = "Controls database connection concurrency. Too few = query queuing, too many = DB resource exhaustion"
related = ["database.pool.min_connections", "database.checkout_timeout"]
example = """
[database.pool]
max_connections = 30  # Production: 30-50 recommended
"""

[database.pool._docs.max_connections.recommendations]
test = { value = "5", rationale = "Minimal connections for test isolation" }
development = { value = "10", rationale = "Small pool for local development" }
production = { value = "30-50", rationale = "Scale based on worker count and query patterns" }

[database.pool._docs.min_connections]
description = "Minimum number of idle connections to maintain"
type = "u32"
valid_range = "1-100"
default = "8"
system_impact = "Keeps connections warm to avoid cold start latency"
"#;

        fs::write(base_dir.join("common.toml"), config_with_docs).unwrap();

        (temp_dir, base_dir)
    }

    #[test]
    fn test_load_documentation() {
        let (_temp_dir, base_dir) = create_test_docs();

        let docs = ConfigDocumentation::load(base_dir).unwrap();

        assert_eq!(docs.parameter_count(), 2);
        assert!(docs.lookup("database.pool.max_connections").is_some());
        assert!(docs.lookup("database.pool.min_connections").is_some());
    }

    #[test]
    fn test_parameter_documentation_fields() {
        let (_temp_dir, base_dir) = create_test_docs();

        let docs = ConfigDocumentation::load(base_dir).unwrap();
        let param_docs = docs.lookup("database.pool.max_connections").unwrap();

        assert_eq!(param_docs.path, "database.pool.max_connections");
        assert_eq!(param_docs.description, "Maximum number of concurrent database connections in the pool");
        assert_eq!(param_docs.param_type, "u32");
        assert_eq!(param_docs.valid_range, "1-1000");
        assert_eq!(param_docs.default, "30");
        assert!(param_docs.system_impact.contains("connection concurrency"));
        assert_eq!(param_docs.related.len(), 2);
        assert!(param_docs.example.contains("max_connections = 30"));
    }

    #[test]
    fn test_environment_recommendations() {
        let (_temp_dir, base_dir) = create_test_docs();

        let docs = ConfigDocumentation::load(base_dir).unwrap();
        let param_docs = docs.lookup("database.pool.max_connections").unwrap();

        assert_eq!(param_docs.recommendations.len(), 3);

        let test_rec = param_docs.recommendations.get("test").unwrap();
        assert_eq!(test_rec.value, "5");
        assert_eq!(test_rec.rationale, "Minimal connections for test isolation");

        let prod_rec = param_docs.recommendations.get("production").unwrap();
        assert_eq!(prod_rec.value, "30-50");
        assert!(prod_rec.rationale.contains("Scale based on worker count"));
    }

    #[test]
    fn test_list_for_context() {
        let (_temp_dir, base_dir) = create_test_docs();

        let docs = ConfigDocumentation::load(base_dir).unwrap();

        let common_params = docs.list_for_context("common");
        assert_eq!(common_params.len(), 2); // Both database.pool params are in common context

        let orch_params = docs.list_for_context("orchestration");
        assert_eq!(orch_params.len(), 0); // No orchestration-specific params in this test
    }

    #[test]
    fn test_missing_documentation() {
        let (_temp_dir, base_dir) = create_test_docs();

        let docs = ConfigDocumentation::load(base_dir).unwrap();

        assert!(docs.lookup("nonexistent.parameter").is_none());
    }
}
