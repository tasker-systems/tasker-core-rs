//! Health Check Handler (pgmq-based)

use serde_json::json;

/// Simple health check handler for pgmq architecture
pub struct HealthCheckHandler;

impl HealthCheckHandler {
    pub fn new() -> Self {
        Self
    }

    /// Perform basic health check
    pub fn check_health(&self) -> serde_json::Value {
        json!({
            "status": "healthy",
            "architecture": "pgmq",
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
    }
}
