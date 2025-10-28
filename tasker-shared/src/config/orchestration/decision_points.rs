//! Decision Points Configuration
//!
//! TAS-53: Configuration for dynamic workflow decision points that enable
//! runtime conditional branching and step creation.

use serde::{Deserialize, Serialize};

/// Configuration for decision point processing
///
/// Controls limits, thresholds, and behavior for dynamic workflow
/// decision points. See `config/tasker/base/decision_points.toml`
/// for detailed documentation of each parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionPointsConfig {
    /// Enable decision point processing
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Maximum number of steps that can be created from a single decision
    #[serde(default = "default_max_steps_per_decision")]
    pub max_steps_per_decision: usize,

    /// Maximum depth of nested decision points
    #[serde(default = "default_max_decision_depth")]
    pub max_decision_depth: usize,

    /// Warn when decision point creates more than this many steps
    #[serde(default = "default_warn_threshold_steps")]
    pub warn_threshold_steps: usize,

    /// Warn when decision depth exceeds this threshold
    #[serde(default = "default_warn_threshold_depth")]
    pub warn_threshold_depth: usize,

    /// Enable detailed decision point logging
    #[serde(default = "default_enable_detailed_logging")]
    pub enable_detailed_logging: bool,

    /// Enable decision point metrics collection
    #[serde(default = "default_enable_metrics")]
    pub enable_metrics: bool,
}

// Default value functions for serde
fn default_enabled() -> bool {
    true
}

fn default_max_steps_per_decision() -> usize {
    50
}

fn default_max_decision_depth() -> usize {
    10
}

fn default_warn_threshold_steps() -> usize {
    20
}

fn default_warn_threshold_depth() -> usize {
    5
}

fn default_enable_detailed_logging() -> bool {
    false
}

fn default_enable_metrics() -> bool {
    true
}

impl Default for DecisionPointsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_steps_per_decision: default_max_steps_per_decision(),
            max_decision_depth: default_max_decision_depth(),
            warn_threshold_steps: default_warn_threshold_steps(),
            warn_threshold_depth: default_warn_threshold_depth(),
            enable_detailed_logging: default_enable_detailed_logging(),
            enable_metrics: default_enable_metrics(),
        }
    }
}

impl DecisionPointsConfig {
    /// Check if decision points are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check if a step count exceeds the maximum
    pub fn exceeds_max_steps(&self, count: usize) -> bool {
        count > self.max_steps_per_decision
    }

    /// Check if a step count should trigger a warning
    pub fn should_warn_steps(&self, count: usize) -> bool {
        count > self.warn_threshold_steps
    }

    /// Check if a decision depth exceeds the maximum
    pub fn exceeds_max_depth(&self, depth: usize) -> bool {
        depth > self.max_decision_depth
    }

    /// Check if a decision depth should trigger a warning
    pub fn should_warn_depth(&self, depth: usize) -> bool {
        depth > self.warn_threshold_depth
    }

    /// Get the maximum steps per decision
    pub fn max_steps(&self) -> usize {
        self.max_steps_per_decision
    }

    /// Get the maximum decision depth
    pub fn max_depth(&self) -> usize {
        self.max_decision_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DecisionPointsConfig::default();
        assert!(config.is_enabled());
        assert_eq!(config.max_steps(), 50);
        assert_eq!(config.max_depth(), 10);
        assert_eq!(config.warn_threshold_steps, 20);
        assert_eq!(config.warn_threshold_depth, 5);
        assert!(!config.enable_detailed_logging);
        assert!(config.enable_metrics);
    }

    #[test]
    fn test_exceeds_max_steps() {
        let config = DecisionPointsConfig::default();
        assert!(!config.exceeds_max_steps(50));
        assert!(config.exceeds_max_steps(51));
        assert!(config.exceeds_max_steps(100));
    }

    #[test]
    fn test_should_warn_steps() {
        let config = DecisionPointsConfig::default();
        assert!(!config.should_warn_steps(20));
        assert!(config.should_warn_steps(21));
        assert!(config.should_warn_steps(30));
    }

    #[test]
    fn test_exceeds_max_depth() {
        let config = DecisionPointsConfig::default();
        assert!(!config.exceeds_max_depth(10));
        assert!(config.exceeds_max_depth(11));
        assert!(config.exceeds_max_depth(20));
    }

    #[test]
    fn test_should_warn_depth() {
        let config = DecisionPointsConfig::default();
        assert!(!config.should_warn_depth(5));
        assert!(config.should_warn_depth(6));
        assert!(config.should_warn_depth(10));
    }

    #[test]
    fn test_custom_config() {
        let config = DecisionPointsConfig {
            enabled: false,
            max_steps_per_decision: 100,
            max_decision_depth: 20,
            warn_threshold_steps: 50,
            warn_threshold_depth: 10,
            enable_detailed_logging: true,
            enable_metrics: false,
        };

        assert!(!config.is_enabled());
        assert_eq!(config.max_steps(), 100);
        assert_eq!(config.max_depth(), 20);
        assert!(config.enable_detailed_logging);
        assert!(!config.enable_metrics);
    }

    #[test]
    fn test_serde_serialization() {
        let config = DecisionPointsConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: DecisionPointsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(
            config.max_steps_per_decision,
            deserialized.max_steps_per_decision
        );
        assert_eq!(config.max_decision_depth, deserialized.max_decision_depth);
    }
}
