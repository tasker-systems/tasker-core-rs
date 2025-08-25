//! Configuration Bridge for Ruby FFI
//!
//! Provides Ruby bindings for the WorkerConfigManager, enabling Ruby workers
//! to use the unified TOML configuration system with the same validation
//! and error handling as the Rust system.

use crate::error_translation::translate_config_error;
use dotenvy::dotenv;
use magnus::{prelude::*, Error, IntoValue, RArray, RHash, RModule, Ruby, Value};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use tasker_orchestration::ffi::shared::WorkerConfigManager;

/// Ruby wrapper for WorkerConfigManager
///
/// This provides Ruby-friendly methods that wrap the Rust WorkerConfigManager
/// and handle conversion between Rust and Ruby types.
#[magnus::wrap(class = "TaskerCore::UnifiedConfig::Manager")]
pub struct RubyConfigManager {
    manager: WorkerConfigManager,
}

impl RubyConfigManager {
    /// Create a new configuration manager with automatic environment detection
    ///
    /// # Returns
    /// * Ruby ConfigManager instance or raises exception on error
    fn new() -> Result<Self, Error> {
        dotenv().ok();
        let manager = WorkerConfigManager::new().map_err(translate_config_error)?;

        Ok(Self { manager })
    }

    /// Create a new configuration manager with explicit environment
    ///
    /// # Arguments
    /// * `environment` - Environment name (development, test, production, etc.)
    ///
    /// # Returns
    /// * Ruby ConfigManager instance or raises exception on error
    fn new_with_environment(environment: String) -> Result<Self, Error> {
        // Load environment variables FIRST - this is critical for TOML configuration
        dotenv().ok();

        let manager = WorkerConfigManager::new_with_environment(&environment)
            .map_err(translate_config_error)?;

        Ok(Self { manager })
    }

    /// Create a new configuration manager with explicit config root
    ///
    /// # Arguments
    /// * `config_root` - Path to config/tasker directory
    /// * `environment` - Environment name
    ///
    /// # Returns
    /// * Ruby ConfigManager instance or raises exception on error
    fn new_with_config_root(config_root: String, environment: String) -> Result<Self, Error> {
        let config_path = PathBuf::from(config_root);
        let manager = WorkerConfigManager::new_with_config_root(config_path, &environment)
            .map_err(translate_config_error)?;

        Ok(Self { manager })
    }

    /// Get the complete configuration as Ruby Hash
    ///
    /// # Returns
    /// * Ruby Hash containing all configuration components
    fn config(&self) -> Result<Value, Error> {
        let tasker_config = self.manager.get_config();

        // Use serde_magnus for direct Rust-to-Ruby conversion - much more efficient!
        use serde_magnus::serialize;
        serialize(tasker_config).map_err(|e| {
            magnus::Error::new(
                magnus::exception::runtime_error(),
                format!("Failed to serialize TaskerConfig to Ruby: {e}"),
            )
        })
    }

    /// Get a specific component configuration as Ruby Hash
    ///
    /// # Arguments
    /// * `component_name` - Name of the component (database, orchestration, etc.)
    ///
    /// # Returns
    /// * Ruby Hash with component configuration or nil if not found
    fn component(&self, component_name: String) -> Result<Value, Error> {
        let json_config = self
            .manager
            .get_component_as_json(&component_name)
            .map_err(translate_config_error)?;

        match json_config {
            Some(json) => json_value_to_ruby(json),
            None => Ok(().into_value()), // Ruby nil
        }
    }

    /// Get available component names
    ///
    /// # Returns
    /// * Ruby Array of component names
    fn component_names(&self) -> Vec<String> {
        self.manager.get_component_names()
    }

    /// Get the current environment name
    ///
    /// # Returns
    /// * Ruby String with environment name
    fn environment(&self) -> String {
        self.manager.environment().to_string()
    }

    /// Get the configuration root path
    ///
    /// # Returns
    /// * Ruby String with configuration root path
    fn config_root(&self) -> String {
        self.manager.config_root().display().to_string()
    }

    /// Get configuration summary for debugging
    ///
    /// # Returns
    /// * Ruby Hash with configuration summary
    fn summary(&self) -> Result<Value, Error> {
        let json_summary = self
            .manager
            .get_config_summary()
            .map_err(translate_config_error)?;

        json_value_to_ruby(json_summary)
    }
}

/// Ruby class methods for WorkerConfigManager
impl RubyConfigManager {
    /// Check if unified TOML configuration is available
    ///
    /// # Arguments
    /// * `config_dir` - Base configuration directory to check
    ///
    /// # Returns
    /// * Ruby Boolean indicating if unified config is available
    fn is_unified_config_available(config_dir: String) -> bool {
        WorkerConfigManager::is_unified_config_available(&config_dir)
    }

    /// Detect current environment using the same logic as Rust
    ///
    /// # Returns
    /// * Ruby String with detected environment name
    fn detect_environment() -> String {
        tasker_shared::config::unified_loader::UnifiedConfigLoader::detect_environment()
    }
}

/// Convert JSON Value to Ruby Value
fn json_value_to_ruby(json: JsonValue) -> Result<Value, Error> {
    match json {
        JsonValue::Null => Ok(().into_value()),
        JsonValue::Bool(b) => Ok(b.into_value()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_value())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_value())
            } else {
                Ok(n.to_string().into_value())
            }
        }
        JsonValue::String(s) => Ok(s.into_value()),
        JsonValue::Array(arr) => {
            let ruby_array = RArray::new();
            for item in arr {
                let ruby_item = json_value_to_ruby(item)?;
                ruby_array.push(ruby_item)?;
            }
            Ok(ruby_array.into_value())
        }
        JsonValue::Object(obj) => {
            let ruby_hash = RHash::new();
            for (key, value) in obj {
                let ruby_key: Value = key.into_value();
                let ruby_value = json_value_to_ruby(value)?;
                ruby_hash.aset(ruby_key, ruby_value)?;
            }
            Ok(ruby_hash.into_value())
        }
    }
}

/// Register the configuration manager with Ruby
pub fn register_config_bridge(ruby: &Ruby, module: &RModule) -> Result<(), Error> {
    // Create UnifiedConfig module under TaskerCore to avoid collision with existing Config class
    let config_module = module.define_module("UnifiedConfig")?;

    // Register the Manager class
    let manager_class = config_module.define_class("Manager", ruby.class_object())?;

    // Instance methods
    manager_class.define_singleton_method("new", magnus::function!(RubyConfigManager::new, 0))?;
    manager_class.define_singleton_method(
        "new_with_environment",
        magnus::function!(RubyConfigManager::new_with_environment, 1),
    )?;
    manager_class.define_singleton_method(
        "new_with_config_root",
        magnus::function!(RubyConfigManager::new_with_config_root, 2),
    )?;

    manager_class.define_method("config", magnus::method!(RubyConfigManager::config, 0))?;
    manager_class.define_method(
        "component",
        magnus::method!(RubyConfigManager::component, 1),
    )?;
    manager_class.define_method(
        "component_names",
        magnus::method!(RubyConfigManager::component_names, 0),
    )?;
    manager_class.define_method(
        "environment",
        magnus::method!(RubyConfigManager::environment, 0),
    )?;
    manager_class.define_method(
        "config_root",
        magnus::method!(RubyConfigManager::config_root, 0),
    )?;
    manager_class.define_method("summary", magnus::method!(RubyConfigManager::summary, 0))?;

    // Class methods
    manager_class.define_singleton_method(
        "unified_config_available?",
        magnus::function!(RubyConfigManager::is_unified_config_available, 1),
    )?;
    manager_class.define_singleton_method(
        "detect_environment",
        magnus::function!(RubyConfigManager::detect_environment, 0),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests would require Magnus runtime setup
    // The module compiles correctly
}
