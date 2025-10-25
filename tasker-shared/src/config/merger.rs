//! Configuration Merger for CLI Tools
//!
//! Provides utilities for merging base and environment-specific configurations
//! into a single deployable TOML file. This supports the TAS-50 CLI goals of
//! generating single-file configurations for easy inspection and deployment.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::config::ConfigMerger;
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a config merger
//! let mut merger = ConfigMerger::new(
//!     PathBuf::from("config/tasker"),
//!     "development"
//! )?;
//!
//! // Generate merged configuration
//! let merged_toml = merger.merge_context("orchestration")?;
//!
//! // Write to file
//! std::fs::write("generated-config.toml", merged_toml)?;
//! # Ok(())
//! # }
//! ```

use super::error::{ConfigResult, ConfigurationError};
use super::unified_loader::UnifiedConfigLoader;
use std::path::PathBuf;
use tracing::{debug, info};

/// Configuration merger for generating single deployable config files
///
/// This wraps the UnifiedConfigLoader to provide CLI-specific functionality
/// for generating merged configuration files from base + environment TOMLs.
#[derive(Debug)]
pub struct ConfigMerger {
    /// Configuration loader
    loader: UnifiedConfigLoader,
    /// Source directory
    source_dir: PathBuf,
    /// Target environment
    environment: String,
}

impl ConfigMerger {
    /// Create a new configuration merger
    ///
    /// # Arguments
    /// * `source_dir` - Path to config directory (e.g., "config/tasker")
    /// * `environment` - Target environment (test, development, production)
    ///
    /// # Returns
    /// * `Result<Self, ConfigurationError>` - Merger instance or error
    ///
    /// # Errors
    /// * `ConfigurationError::ConfigFileNotFound` - If source directory doesn't exist
    /// * `ConfigurationError::ValidationError` - If directory structure is invalid
    pub fn new(source_dir: PathBuf, environment: &str) -> ConfigResult<Self> {
        debug!(
            "Creating ConfigMerger for source_dir: {}, environment: {}",
            source_dir.display(),
            environment
        );

        // Validate source directory exists
        if !source_dir.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![
                source_dir.clone()
            ]));
        }

        // Create unified loader for config generation (preserves env var placeholders)
        let loader = UnifiedConfigLoader::for_generation(source_dir.clone(), environment)?;

        Ok(Self {
            loader,
            source_dir,
            environment: environment.to_string(),
        })
    }

    /// Merge a specific context configuration
    ///
    /// This loads the base context configuration and applies environment-specific
    /// overrides, then returns the merged result as a TOML string.
    ///
    /// # Arguments
    /// * `context` - Context name (common, orchestration, worker, complete)
    ///
    /// # Returns
    /// * `Result<String, ConfigurationError>` - Merged TOML string or error
    ///
    /// # Example
    /// ```rust,no_run
    /// # use tasker_shared::config::ConfigMerger;
    /// # use std::path::PathBuf;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut merger = ConfigMerger::new(
    ///     PathBuf::from("config/tasker"),
    ///     "production"
    /// )?;
    ///
    /// let merged_config = merger.merge_context("orchestration")?;
    /// std::fs::write("orchestration-production.toml", merged_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
        info!(
            "Merging context '{}' for environment '{}' (including common config)",
            context, self.environment
        );

        // 1. Load common configuration as base (for orchestration and worker contexts)
        // For 'common' context itself, skip this step
        let mut merged = if context == "common" {
            // For common context, just load common.toml
            self.loader.load_context_toml("common")?
        } else if context == "complete" {
            // For complete context, merge common + orchestration + worker
            let mut base = self.loader.load_context_toml("common")?;
            debug!(
                "Loaded common config as base: {} bytes",
                base.to_string().len()
            );

            // 2. Load and merge orchestration configuration
            let orch_config = self.loader.load_context_toml("orchestration")?;
            debug!(
                "Loaded orchestration config: {} bytes",
                orch_config.to_string().len()
            );

            if let (toml::Value::Table(ref mut base_table), toml::Value::Table(orch_table)) =
                (&mut base, orch_config)
            {
                for (key, value) in orch_table {
                    base_table.insert(key, value);
                }
            }

            // 3. Load and merge worker configuration
            let worker_config = self.loader.load_context_toml("worker")?;
            debug!(
                "Loaded worker config: {} bytes",
                worker_config.to_string().len()
            );

            if let toml::Value::Table(ref mut base_table) = &mut base {
                if let toml::Value::Table(worker_table) = worker_config {
                    for (key, value) in worker_table {
                        base_table.insert(key, value);
                    }
                }
            }

            debug!(
                "Complete merged config total: {} bytes",
                base.to_string().len()
            );
            base
        } else {
            // For orchestration/worker, start with common as base
            let mut base = self.loader.load_context_toml("common")?;
            debug!(
                "Loaded common config as base: {} bytes",
                base.to_string().len()
            );

            // 2. Load context-specific configuration
            let context_config = self.loader.load_context_toml(context)?;
            debug!(
                "Loaded {} config: {} bytes",
                context,
                context_config.to_string().len()
            );

            // 3. Merge context on top of common (context wins in conflicts)
            if let (toml::Value::Table(ref mut base_table), toml::Value::Table(context_table)) =
                (&mut base, context_config)
            {
                for (key, value) in context_table {
                    base_table.insert(key, value);
                }
            }

            debug!("Merged config total: {} bytes", base.to_string().len());
            base
        };

        // 4. Strip documentation metadata (_docs sections) before serialization
        // This removes internal documentation from production config files
        Self::strip_docs_from_value(&mut merged);

        // 5. Convert to TOML string
        let merged_toml_string = toml::to_string_pretty(&merged).map_err(|e| {
            ConfigurationError::json_serialization_error(
                format!("Failed to serialize merged {} config", context),
                e,
            )
        })?;

        info!(
            "Successfully merged {} configuration with common ({} bytes)",
            context,
            merged_toml_string.len()
        );

        Ok(merged_toml_string)
    }

    /// Recursively strip _docs sections from TOML value
    ///
    /// Removes all keys ending with "_docs" to keep production configs clean
    /// and prevent internal documentation from being exposed.
    fn strip_docs_from_value(value: &mut toml::Value) {
        if let toml::Value::Table(table) = value {
            // Remove any keys ending with "_docs"
            table.retain(|key, _| !key.ends_with("_docs"));

            // Recursively strip from remaining values
            for (_, v) in table.iter_mut() {
                Self::strip_docs_from_value(v);
            }
        } else if let toml::Value::Array(array) = value {
            // Recursively strip from array elements
            for v in array.iter_mut() {
                Self::strip_docs_from_value(v);
            }
        }
    }

    /// Merge all context configurations
    ///
    /// This merges common, orchestration, and worker configurations and returns
    /// them as a map of context name to TOML string.
    ///
    /// # Returns
    /// * `Result<std::collections::HashMap<String, String>, ConfigurationError>`
    ///
    /// # Example
    /// ```rust,no_run
    /// # use tasker_shared::config::ConfigMerger;
    /// # use std::path::PathBuf;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut merger = ConfigMerger::new(
    ///     PathBuf::from("config/tasker"),
    ///     "production"
    /// )?;
    ///
    /// let all_configs = merger.merge_all_contexts()?;
    /// for (context, toml_content) in all_configs {
    ///     let filename = format!("{}-production.toml", context);
    ///     std::fs::write(&filename, toml_content)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn merge_all_contexts(
        &mut self,
    ) -> ConfigResult<std::collections::HashMap<String, String>> {
        let contexts = vec!["common", "orchestration", "worker"];
        let mut merged_configs = std::collections::HashMap::new();

        for context in contexts {
            // Check if context file exists before attempting to load
            let base_path = self
                .source_dir
                .join("base")
                .join(format!("{}.toml", context));
            if base_path.exists() {
                let merged = self.merge_context(context)?;
                merged_configs.insert(context.to_string(), merged);
            } else {
                debug!(
                    "Skipping context '{}' - base file not found at {}",
                    context,
                    base_path.display()
                );
            }
        }

        Ok(merged_configs)
    }

    /// Get the source directory path
    pub fn source_dir(&self) -> &PathBuf {
        &self.source_dir
    }

    /// Get the target environment
    pub fn environment(&self) -> &str {
        &self.environment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a test configuration structure
    fn create_test_config() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let config_root = temp_dir.path().join("tasker");

        // Create directory structure
        fs::create_dir_all(config_root.join("base")).unwrap();
        fs::create_dir_all(config_root.join("environments/test")).unwrap();
        fs::create_dir_all(config_root.join("environments/production")).unwrap();

        // Create base common.toml
        let base_common = r#"
[database]
url = "${DATABASE_URL:-postgresql://localhost/tasker}"
pool_size = 20

[engine]
max_retries = 3
"#;
        fs::write(config_root.join("base/common.toml"), base_common).unwrap();

        // Create test environment override
        let test_common = r#"
[database]
pool_size = 5

[engine]
max_retries = 1
"#;
        fs::write(
            config_root.join("environments/test/common.toml"),
            test_common,
        )
        .unwrap();

        (temp_dir, config_root)
    }

    #[test]
    fn test_config_merger_creation() {
        let (_temp_dir, config_root) = create_test_config();

        let merger = ConfigMerger::new(config_root, "test");
        assert!(merger.is_ok());

        let merger = merger.unwrap();
        assert_eq!(merger.environment(), "test");
    }

    #[test]
    fn test_merge_context() {
        let (_temp_dir, config_root) = create_test_config();

        let mut merger = ConfigMerger::new(config_root, "test").unwrap();
        let merged = merger.merge_context("common").unwrap();

        // Verify the merged config contains both base and override values
        assert!(merged.contains("database"));
        assert!(merged.contains("engine"));
        assert!(merged.contains("pool_size = 5")); // Override value
        assert!(merged.contains("max_retries = 1")); // Override value
    }

    #[test]
    fn test_merge_nonexistent_context() {
        let (_temp_dir, config_root) = create_test_config();

        let mut merger = ConfigMerger::new(config_root, "test").unwrap();
        let result = merger.merge_context("nonexistent");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("nonexistent.toml"));
    }

    #[test]
    fn test_invalid_source_directory() {
        let result = ConfigMerger::new(PathBuf::from("/nonexistent/path"), "test");
        assert!(result.is_err());
    }
}
