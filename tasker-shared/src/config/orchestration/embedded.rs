use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Embedded orchestrator configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddedOrchestratorConfig {
    pub auto_start: bool,
    pub namespaces: Vec<String>,
    pub shutdown_timeout_seconds: u64,
}

impl EmbeddedOrchestratorConfig {
    /// Get shutdown timeout as Duration
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_seconds)
    }
}
