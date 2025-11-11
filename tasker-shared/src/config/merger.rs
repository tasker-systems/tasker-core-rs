//! Configuration Merger for CLI Tools (TAS-61)
//!
//! Self-contained config merger for generating single deployable TOML files.
//! Reads base + environment files and merges them WITHOUT environment variable substitution
//! (preserves ${VAR:-default} placeholders for runtime).
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
//!     PathBuf::from("config/v2"),
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
use super::merge::deep_merge_toml;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Configuration merger for generating single deployable config files
#[derive(Debug)]
pub struct ConfigMerger {
    /// Source directory (e.g., config/v2/)
    source_dir: PathBuf,
    /// Target environment
    environment: String,
}

impl ConfigMerger {
    /// Create a new configuration merger
    ///
    /// # Arguments
    /// * `source_dir` - Path to config directory (e.g., "config/v2")
    /// * `environment` - Target environment (test, development, production)
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

        // Validate base directory exists
        let base_dir = source_dir.join("base");
        if !base_dir.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![base_dir]));
        }

        Ok(Self {
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
    pub fn merge_context(&mut self, context: &str) -> ConfigResult<String> {
        info!(
            "Merging context '{}' for environment '{}'",
            context, self.environment
        );

        let merged = if context == "common" {
            // For common context, just load common.toml
            self.load_context_toml("common")?
        } else if context == "complete" {
            // For complete context, merge common + orchestration + worker
            let mut base = self.load_context_toml("common")?;
            let orch_config = self.load_context_toml("orchestration")?;
            deep_merge_toml(&mut base, orch_config)?;

            let worker_config = self.load_context_toml("worker")?;
            deep_merge_toml(&mut base, worker_config)?;

            base
        } else {
            // For orchestration/worker, start with common as base
            let mut base = self.load_context_toml("common")?;
            let context_config = self.load_context_toml(context)?;
            deep_merge_toml(&mut base, context_config)?;

            base
        };

        // Convert to formatted TOML string
        let toml_string = toml::to_string_pretty(&merged)
            .map_err(|e| ConfigurationError::json_serialization_error("TOML serialization", e))?;

        info!(
            "Successfully merged context '{}' ({} bytes)",
            context,
            toml_string.len()
        );

        Ok(toml_string)
    }

    /// Load a context-specific configuration with environment overrides
    ///
    /// Reads base/{context}.toml and merges environments/{env}/{context}.toml if present.
    /// Does NOT perform environment variable substitution (preserves placeholders).
    fn load_context_toml(&self, context_name: &str) -> ConfigResult<toml::Value> {
        debug!(
            "Loading context '{}' for environment '{}'",
            context_name, self.environment
        );

        // 1. Load base context configuration - REQUIRED
        let base_path = self
            .source_dir
            .join("base")
            .join(format!("{}.toml", context_name));
        let mut config = self.load_toml_file(&base_path)?;

        // 2. Apply environment overrides if they exist
        let env_path = self
            .source_dir
            .join("environments")
            .join(&self.environment)
            .join(format!("{}.toml", context_name));

        let overrides = if env_path.exists() {
            debug!("Found environment overrides at: {}", env_path.display());
            self.load_toml_file(&env_path)?
        } else {
            debug!("No environment overrides found at: {}", env_path.display());
            // Use empty table for no overrides - this ensures _docs stripping happens
            toml::Value::Table(toml::value::Table::new())
        };

        // Always call deep_merge_toml to ensure _docs sections are stripped
        deep_merge_toml(&mut config, overrides)?;

        Ok(config)
    }

    /// Load a TOML file without environment variable substitution
    ///
    /// This preserves ${VAR:-default} placeholders for runtime substitution.
    fn load_toml_file(&self, path: &Path) -> ConfigResult<toml::Value> {
        if !path.exists() {
            return Err(ConfigurationError::config_file_not_found(vec![
                path.to_path_buf()
            ]));
        }

        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Failed to read {}: {}",
                path.display(),
                e
            ))
        })?;

        toml::from_str(&content).map_err(|e| {
            ConfigurationError::validation_error(format!(
                "Failed to parse TOML from {}: {}",
                path.display(),
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merger_creation() {
        let temp_dir = std::env::temp_dir().join("test_merger");
        std::fs::create_dir_all(temp_dir.join("base")).unwrap();

        let merger = ConfigMerger::new(temp_dir.clone(), "test");
        assert!(merger.is_ok());

        std::fs::remove_dir_all(temp_dir).ok();
    }
}
