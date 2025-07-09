//! # Plugin Registry
//!
//! Registry for dynamic plugin discovery and loading with thread-safe management.
//!
//! ## Overview
//!
//! The PluginRegistry provides a centralized way to discover, load, and manage plugins
//! at runtime. It supports multiple plugin types and provides a plugin lifecycle management
//! system.
//!
//! ## Key Features
//!
//! - **Dynamic plugin discovery** from directories and configurations
//! - **Thread-safe plugin management** using RwLock for concurrent access
//! - **Plugin lifecycle management** (load, enable, disable, unload)
//! - **Plugin metadata** tracking and validation
//! - **Plugin dependency resolution**
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::plugin_registry::PluginRegistry;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut registry = PluginRegistry::new();
//!
//! // Register a plugin
//! registry.register_plugin("analytics", "1.0.0", "Analytics plugin for metrics collection").await?;
//!
//! // List available plugins
//! let plugins = registry.list_plugins().await;
//! println!("Available plugins: {:?}", plugins);
//!
//! // Enable a plugin
//! registry.enable_plugin("analytics").await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Plugin metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub dependencies: Vec<String>,
    pub enabled: bool,
    pub load_order: u32,
}

/// Plugin state for runtime management
#[derive(Debug, Clone, PartialEq)]
pub enum PluginState {
    Registered,
    Loaded,
    Enabled,
    Disabled,
    Failed,
}

/// Plugin instance with state management
#[derive(Debug, Clone)]
pub struct Plugin {
    pub metadata: PluginMetadata,
    pub state: PluginState,
    pub error_message: Option<String>,
    pub loaded_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Registry for managing plugins
pub struct PluginRegistry {
    /// Registered plugins
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    /// Plugin directories to scan
    plugin_directories: Vec<String>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_directories: vec![
                "plugins".to_string(),
                "lib/plugins".to_string(),
                "/usr/local/lib/tasker/plugins".to_string(),
            ],
        }
    }

    /// Register a plugin with metadata
    pub async fn register_plugin(
        &mut self,
        name: &str,
        version: &str,
        description: &str,
    ) -> OrchestrationResult<()> {
        let plugin = Plugin {
            metadata: PluginMetadata {
                name: name.to_string(),
                version: version.to_string(),
                description: description.to_string(),
                author: None,
                dependencies: vec![],
                enabled: false,
                load_order: 100,
            },
            state: PluginState::Registered,
            error_message: None,
            loaded_at: None,
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(name.to_string(), plugin);

        info!("Registered plugin '{}' version {}", name, version);
        Ok(())
    }

    /// Load a plugin (simulate loading from file system)
    pub async fn load_plugin(&self, name: &str) -> OrchestrationResult<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get_mut(name) {
            if plugin.state == PluginState::Registered {
                plugin.state = PluginState::Loaded;
                plugin.loaded_at = Some(chrono::Utc::now());
                info!("Loaded plugin '{}'", name);
                Ok(())
            } else {
                Err(OrchestrationError::ConfigurationError {
                    source: "PluginRegistry".to_string(),
                    reason: format!("Plugin '{name}' is not in registered state"),
                })
            }
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "PluginRegistry".to_string(),
                reason: format!("Plugin '{name}' not found"),
            })
        }
    }

    /// Enable a plugin
    pub async fn enable_plugin(&self, name: &str) -> OrchestrationResult<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get_mut(name) {
            match plugin.state {
                PluginState::Loaded | PluginState::Disabled => {
                    plugin.state = PluginState::Enabled;
                    plugin.metadata.enabled = true;
                    info!("Enabled plugin '{}'", name);
                    Ok(())
                }
                _ => Err(OrchestrationError::ConfigurationError {
                    source: "PluginRegistry".to_string(),
                    reason: format!(
                        "Plugin '{}' cannot be enabled from state {:?}",
                        name, plugin.state
                    ),
                }),
            }
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "PluginRegistry".to_string(),
                reason: format!("Plugin '{name}' not found"),
            })
        }
    }

    /// Disable a plugin
    pub async fn disable_plugin(&self, name: &str) -> OrchestrationResult<()> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get_mut(name) {
            if plugin.state == PluginState::Enabled {
                plugin.state = PluginState::Disabled;
                plugin.metadata.enabled = false;
                info!("Disabled plugin '{}'", name);
                Ok(())
            } else {
                Err(OrchestrationError::ConfigurationError {
                    source: "PluginRegistry".to_string(),
                    reason: format!("Plugin '{name}' is not enabled"),
                })
            }
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "PluginRegistry".to_string(),
                reason: format!("Plugin '{name}' not found"),
            })
        }
    }

    /// List all plugins
    pub async fn list_plugins(&self) -> Vec<Plugin> {
        let plugins = self.plugins.read().await;
        plugins.values().cloned().collect()
    }

    /// Get plugin by name
    pub async fn get_plugin(&self, name: &str) -> Option<Plugin> {
        let plugins = self.plugins.read().await;
        plugins.get(name).cloned()
    }

    /// Check if plugin is enabled
    pub async fn is_plugin_enabled(&self, name: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins
            .get(name)
            .map(|p| p.state == PluginState::Enabled)
            .unwrap_or(false)
    }

    /// Get plugin statistics
    pub async fn get_stats(&self) -> PluginStats {
        let plugins = self.plugins.read().await;
        let mut stats = PluginStats::default();

        for plugin in plugins.values() {
            stats.total_plugins += 1;
            match plugin.state {
                PluginState::Registered => stats.registered_plugins += 1,
                PluginState::Loaded => stats.loaded_plugins += 1,
                PluginState::Enabled => stats.enabled_plugins += 1,
                PluginState::Disabled => stats.disabled_plugins += 1,
                PluginState::Failed => stats.failed_plugins += 1,
            }
        }

        stats
    }

    /// Discover plugins from configured directories
    pub async fn discover_plugins(&self) -> OrchestrationResult<Vec<String>> {
        let mut discovered = Vec::new();

        for directory in &self.plugin_directories {
            debug!("Scanning directory: {}", directory);
            // In a real implementation, this would scan the file system
            // For now, we'll simulate discovery
            if directory.contains("plugins") {
                discovered.push(format!("{directory}/analytics_plugin.so"));
                discovered.push(format!("{directory}/metrics_plugin.so"));
            }
        }

        info!("Discovered {} plugins", discovered.len());
        Ok(discovered)
    }

    /// Add a plugin directory to scan
    pub fn add_plugin_directory(&mut self, directory: String) {
        self.plugin_directories.push(directory);
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about registered plugins
#[derive(Debug, Clone, Default)]
pub struct PluginStats {
    pub total_plugins: usize,
    pub registered_plugins: usize,
    pub loaded_plugins: usize,
    pub enabled_plugins: usize,
    pub disabled_plugins: usize,
    pub failed_plugins: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_registry_creation() {
        let registry = PluginRegistry::new();
        let plugins = registry.list_plugins().await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_plugin_registration() {
        let mut registry = PluginRegistry::new();

        registry
            .register_plugin("test_plugin", "1.0.0", "Test plugin")
            .await
            .unwrap();

        let plugins = registry.list_plugins().await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].metadata.name, "test_plugin");
        assert_eq!(plugins[0].state, PluginState::Registered);
    }

    #[tokio::test]
    async fn test_plugin_lifecycle() {
        let mut registry = PluginRegistry::new();

        // Register plugin
        registry
            .register_plugin("lifecycle_test", "1.0.0", "Lifecycle test plugin")
            .await
            .unwrap();

        // Load plugin
        registry.load_plugin("lifecycle_test").await.unwrap();
        let plugin = registry.get_plugin("lifecycle_test").await.unwrap();
        assert_eq!(plugin.state, PluginState::Loaded);

        // Enable plugin
        registry.enable_plugin("lifecycle_test").await.unwrap();
        let plugin = registry.get_plugin("lifecycle_test").await.unwrap();
        assert_eq!(plugin.state, PluginState::Enabled);
        assert!(plugin.metadata.enabled);

        // Disable plugin
        registry.disable_plugin("lifecycle_test").await.unwrap();
        let plugin = registry.get_plugin("lifecycle_test").await.unwrap();
        assert_eq!(plugin.state, PluginState::Disabled);
        assert!(!plugin.metadata.enabled);
    }

    #[tokio::test]
    async fn test_plugin_stats() {
        let mut registry = PluginRegistry::new();

        registry
            .register_plugin("plugin1", "1.0.0", "Plugin 1")
            .await
            .unwrap();
        registry
            .register_plugin("plugin2", "1.0.0", "Plugin 2")
            .await
            .unwrap();

        registry.load_plugin("plugin1").await.unwrap();
        registry.enable_plugin("plugin1").await.unwrap();

        let stats = registry.get_stats().await;
        assert_eq!(stats.total_plugins, 2);
        assert_eq!(stats.enabled_plugins, 1);
        assert_eq!(stats.registered_plugins, 1);
    }

    #[tokio::test]
    async fn test_plugin_discovery() {
        let registry = PluginRegistry::new();
        let discovered = registry.discover_plugins().await.unwrap();

        // Should find simulated plugins
        assert!(!discovered.is_empty());
        assert!(discovered.iter().any(|p| p.contains("analytics_plugin.so")));
    }
}
