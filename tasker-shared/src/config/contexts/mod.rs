// TAS-50: Context-specific configuration structures
//
// This module contains the core configuration contexts for Tasker:
// - CommonConfig: Shared infrastructure configuration
// - OrchestrationConfig: Orchestration-specific configuration
// - WorkerConfig: Language-agnostic worker configuration
//
// Phase 1 implementation: Non-breaking addition of context-specific structs

pub mod common;
pub mod orchestration;
pub mod worker;

pub use common::CommonConfig;
pub use orchestration::OrchestrationConfig;
pub use worker::WorkerConfig;

use crate::config::error::ConfigurationError;
use serde::{Deserialize, Serialize};

/// Configuration loading context
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigContext {
    /// Load common + orchestration configuration
    Orchestration,

    /// Load common + worker configuration (language-agnostic)
    Worker,

    /// Load all configuration (combined deployment)
    Combined,

    /// Load legacy monolithic TaskerConfig (backward compatibility)
    Legacy,
}

impl ConfigContext {
    /// Get the name of this context for display/logging
    pub fn name(&self) -> &str {
        match self {
            ConfigContext::Orchestration => "orchestration",
            ConfigContext::Worker => "worker",
            ConfigContext::Combined => "combined",
            ConfigContext::Legacy => "legacy",
        }
    }
}

/// Trait for all configuration contexts
pub trait ConfigurationContext:
    Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync
{
    /// Validate this configuration context
    fn validate(&self) -> Result<(), Vec<ConfigurationError>>;

    /// Get the environment this configuration is for
    fn environment(&self) -> &str;

    /// Get configuration summary for logging
    fn summary(&self) -> String;
}
