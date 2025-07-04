use crate::error::{Result, TaskerError};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TaskerConfig {
    pub database_url: String,
    pub max_concurrent_steps: usize,
    pub retry_limit: u32,
    pub backoff_base_ms: u64,
    pub backoff_max_ms: u64,
    pub event_batch_size: usize,
    pub telemetry_enabled: bool,
    pub custom_settings: HashMap<String, String>,
}

impl Default for TaskerConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/tasker_rust_development".to_string(),
            max_concurrent_steps: 10,
            retry_limit: 3,
            backoff_base_ms: 1000,
            backoff_max_ms: 60000,
            event_batch_size: 100,
            telemetry_enabled: true,
            custom_settings: HashMap::new(),
        }
    }
}

impl TaskerConfig {
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        if let Ok(db_url) = std::env::var("DATABASE_URL") {
            config.database_url = db_url;
        }

        if let Ok(max_concurrent) = std::env::var("TASKER_MAX_CONCURRENT_STEPS") {
            config.max_concurrent_steps = max_concurrent.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid max_concurrent_steps: {e}"))
            })?;
        }

        if let Ok(retry_limit) = std::env::var("TASKER_RETRY_LIMIT") {
            config.retry_limit = retry_limit.parse().map_err(|e| {
                TaskerError::ConfigurationError(format!("Invalid retry_limit: {e}"))
            })?;
        }

        Ok(config)
    }
}
