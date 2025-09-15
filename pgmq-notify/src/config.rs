//! # Configuration for pgmq-notify
//!
//! This module provides configuration structures for customizing PGMQ notification
//! behavior, including queue naming patterns, channel prefixes, and notification settings.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::{PgmqNotifyError, Result};

/// Configuration for PGMQ notification behavior
///
/// This struct controls how PGMQ notifications are generated, formatted, and delivered.
/// It allows customization of queue naming patterns, notification channels, and
/// automatic listening behavior.
///
/// # Examples
///
/// ```rust
/// use pgmq_notify::config::PgmqNotifyConfig;
/// use std::collections::HashSet;
///
/// // Basic configuration with defaults
/// let config = PgmqNotifyConfig::new();
/// assert_eq!(config.queue_naming_pattern, r"(?P<namespace>\w+)_queue");
/// assert!(!config.enable_triggers); // Triggers disabled by default
///
/// // Custom configuration with namespace listening
/// let mut namespaces = HashSet::new();
/// namespaces.insert("orders".to_string());
/// namespaces.insert("inventory".to_string());
///
/// let config = PgmqNotifyConfig {
///     queue_naming_pattern: r"(?P<namespace>\w+)_messages".to_string(),
///     channels_prefix: Some("prod".to_string()),
///     enable_triggers: true,
///     default_namespaces: namespaces,
///     max_payload_size: 4000,
///     include_metadata: false,
/// };
///
/// assert_eq!(config.channels_prefix, Some("prod".to_string()));
/// assert!(config.default_namespaces.contains("orders"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgmqNotifyConfig {
    /// Pattern for extracting namespace from queue names
    /// Should contain a named capture group "namespace"
    /// Default: `r"(?P<namespace>\w+)_queue"` matches "orders_queue" -> "orders"
    pub queue_naming_pattern: String,

    /// Optional prefix for all notification channels to avoid conflicts
    /// Example: "app1" results in channels like "app1.pgmq_queue_created"
    pub channels_prefix: Option<String>,

    /// Whether to enable database triggers for automatic notifications
    /// If false, relies on application-level emitters
    pub enable_triggers: bool,

    /// Default namespaces to auto-listen for message_ready events
    pub default_namespaces: HashSet<String>,

    /// Maximum payload size in bytes (pg_notify limit is 8000)
    pub max_payload_size: usize,

    /// Whether to include queue metadata in notifications
    pub include_metadata: bool,
}

impl Default for PgmqNotifyConfig {
    fn default() -> Self {
        Self {
            queue_naming_pattern: r"(?P<namespace>\w+)_queue".to_string(),
            channels_prefix: None,
            enable_triggers: false,
            default_namespaces: HashSet::new(),
            max_payload_size: 7800, // Leave buffer under 8KB limit
            include_metadata: true,
        }
    }
}

impl PgmqNotifyConfig {
    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the queue naming pattern for namespace extraction
    pub fn with_queue_naming_pattern<S: Into<String>>(mut self, pattern: S) -> Self {
        self.queue_naming_pattern = pattern.into();
        self
    }

    /// Set the channels prefix to avoid conflicts
    pub fn with_channels_prefix<S: Into<String>>(mut self, prefix: S) -> Self {
        self.channels_prefix = Some(prefix.into());
        self
    }

    /// Enable or disable database triggers
    pub fn with_triggers_enabled(mut self, enabled: bool) -> Self {
        self.enable_triggers = enabled;
        self
    }

    /// Add a default namespace to auto-listen
    pub fn with_default_namespace<S: Into<String>>(mut self, namespace: S) -> Self {
        self.default_namespaces.insert(namespace.into());
        self
    }

    /// Add multiple default namespaces
    pub fn with_default_namespaces<I, S>(mut self, namespaces: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for namespace in namespaces {
            self.default_namespaces.insert(namespace.into());
        }
        self
    }

    /// Set maximum payload size
    pub fn with_max_payload_size(mut self, size: usize) -> Self {
        self.max_payload_size = size.min(7800); // Enforce pg_notify limit
        self
    }

    /// Enable/disable metadata inclusion
    pub fn with_metadata_included(mut self, include: bool) -> Self {
        self.include_metadata = include;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Test regex compilation
        self.compiled_pattern()?;

        // Validate payload size
        if self.max_payload_size > 8000 {
            return Err(PgmqNotifyError::config(
                "max_payload_size cannot exceed 8000 bytes (pg_notify limit)",
            ));
        }

        // Validate channel prefix
        if let Some(ref prefix) = self.channels_prefix {
            if prefix.is_empty() || prefix.len() > 20 {
                return Err(PgmqNotifyError::config(
                    "channels_prefix must be 1-20 characters",
                ));
            }
        }

        Ok(())
    }

    /// Compile the queue naming pattern regex
    pub fn compiled_pattern(&self) -> Result<Regex> {
        Regex::new(&self.queue_naming_pattern)
            .map_err(|_| PgmqNotifyError::invalid_pattern(&self.queue_naming_pattern))
    }

    /// Extract namespace from queue name using the configured pattern
    pub fn extract_namespace(&self, queue_name: &str) -> Result<String> {
        let regex = self.compiled_pattern()?;

        if let Some(captures) = regex.captures(queue_name) {
            if let Some(namespace_match) = captures.name("namespace") {
                return Ok(namespace_match.as_str().to_string());
            }
        }

        Err(PgmqNotifyError::Configuration {
            message: format!("Invalid namespace: {queue_name}"),
        })
    }

    /// Build channel name with optional prefix
    pub fn build_channel_name(&self, base_channel: &str) -> String {
        match &self.channels_prefix {
            Some(prefix) => format!("{}.{}", prefix, base_channel),
            None => base_channel.to_string(),
        }
    }

    /// Build namespace-specific channel name
    pub fn build_namespace_channel(&self, base_channel: &str, namespace: &str) -> String {
        let channel = format!("{}.{}", base_channel, namespace);
        self.build_channel_name(&channel)
    }

    /// Get the queue created channel name
    pub fn queue_created_channel(&self) -> String {
        self.build_channel_name("pgmq_queue_created")
    }

    /// Get the message ready channel name for a namespace
    pub fn message_ready_channel(&self, namespace: &str) -> String {
        self.build_namespace_channel("pgmq_message_ready", namespace)
    }

    /// Get the global message ready channel name
    pub fn global_message_ready_channel(&self) -> String {
        self.build_channel_name("pgmq_message_ready")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PgmqNotifyConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.queue_naming_pattern, r"(?P<namespace>\w+)_queue");
        assert!(config.channels_prefix.is_none());
        assert!(!config.enable_triggers);
    }

    #[test]
    fn test_namespace_extraction() {
        let config = PgmqNotifyConfig::default();

        assert_eq!(config.extract_namespace("orders_queue").unwrap(), "orders");
        assert_eq!(
            config.extract_namespace("inventory_queue").unwrap(),
            "inventory"
        );
        assert!(
            config.extract_namespace("no_match").is_err(),
            "Should return error for non-matching pattern"
        );
    }

    #[test]
    fn test_channel_naming() {
        let config = PgmqNotifyConfig::new().with_channels_prefix("app1");

        assert_eq!(config.queue_created_channel(), "app1.pgmq_queue_created");
        assert_eq!(
            config.message_ready_channel("orders"),
            "app1.pgmq_message_ready.orders"
        );
    }

    #[test]
    fn test_validation() {
        // Invalid regex
        let config = PgmqNotifyConfig::new().with_queue_naming_pattern("[invalid");
        assert!(config.validate().is_err());

        // Payload size gets capped, so validation passes
        let config = PgmqNotifyConfig::new().with_max_payload_size(10000);
        assert!(config.validate().is_ok());
        assert_eq!(config.max_payload_size, 7800); // Should be capped

        // Valid config
        let config = PgmqNotifyConfig::new()
            .with_channels_prefix("test")
            .with_default_namespace("orders");
        assert!(config.validate().is_ok());
    }
}
