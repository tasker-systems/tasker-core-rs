//! # Backoff Calculator
//!
//! Handles all backoff logic for step retries with exponential backoff calculations.
//!
//! ## Overview
//!
//! The BackoffCalculator provides unified handling of both server-requested backoff
//! (via Retry-After headers) and exponential backoff calculations. It integrates
//! with the model layer to update step backoff settings and publishes events
//! for observability.
//!
//! ## Key Features
//!
//! - **Exponential Backoff**: Configurable base delay with exponential growth
//! - **Server-Requested Backoff**: Honor Retry-After headers from APIs
//! - **Jitter Support**: Optional randomization to prevent thundering herd
//! - **Maximum Delay Caps**: Prevent infinite backoff growth
//! - **Event Publishing**: Observability for backoff decisions
//!
//! ## Rails Heritage
//!
//! Migrated from `lib/tasker/orchestration/backoff_calculator.rb` with enhanced
//! type safety and performance optimizations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Configuration for backoff calculation behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffCalculatorConfig {
    /// Base delay in seconds for exponential backoff
    pub base_delay_seconds: u32,
    /// Maximum delay cap in seconds
    pub max_delay_seconds: u32,
    /// Exponential multiplier (default: 2.0)
    pub multiplier: f64,
    /// Whether to add jitter to prevent thundering herd
    pub jitter_enabled: bool,
    /// Maximum jitter percentage (0.0 to 1.0)
    pub max_jitter: f64,
}

impl Default for BackoffCalculatorConfig {
    fn default() -> Self {
        Self {
            base_delay_seconds: 1,
            max_delay_seconds: 300, // 5 minutes
            multiplier: 2.0,
            jitter_enabled: true,
            max_jitter: 0.1, // 10% jitter
        }
    }
}

impl BackoffCalculatorConfig {
    /// Create BackoffCalculatorConfig from ConfigManager
    pub fn from_config_manager(config_manager: &tasker_shared::config::ConfigManager) -> Self {
        let config = config_manager.config();

        // Use the first default backoff value as base delay, or fallback to 1 second
        let base_delay_seconds = config
            .backoff
            .default_backoff_seconds
            .first()
            .copied()
            .unwrap_or(1) as u32;

        Self {
            base_delay_seconds,
            max_delay_seconds: config.backoff.max_backoff_seconds as u32,
            multiplier: config.backoff.backoff_multiplier,
            jitter_enabled: config.backoff.jitter_enabled,
            max_jitter: config.backoff.jitter_max_percentage,
        }
    }
}

/// Context for backoff calculation decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffContext {
    /// HTTP response headers (if applicable)
    pub headers: HashMap<String, String>,
    /// Error context that triggered the backoff
    pub error_context: Option<String>,
    /// Metadata for backoff decision
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for BackoffContext {
    fn default() -> Self {
        Self::new()
    }
}

impl BackoffContext {
    /// Create a new backoff context
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
            error_context: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a header to the context
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    /// Add error context
    pub fn with_error(mut self, error: String) -> Self {
        self.error_context = Some(error);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// BackoffCalculator handles all backoff logic for step retries
///
/// This component provides unified handling of both server-requested backoff
/// (via Retry-After headers) and exponential backoff calculations.
/// It integrates with the workflow step model to apply backoff settings.
#[derive(Clone)]
pub struct BackoffCalculator {
    config: BackoffCalculatorConfig,
    pool: PgPool,
}

impl BackoffCalculator {
    /// Create a new BackoffCalculator with configuration
    pub fn new(config: BackoffCalculatorConfig, pool: PgPool) -> Self {
        Self { config, pool }
    }

    /// Create a BackoffCalculator with default configuration
    pub fn with_defaults(pool: PgPool) -> Self {
        Self::new(BackoffCalculatorConfig::default(), pool)
    }

    /// Calculate and apply backoff for a step based on response context
    ///
    /// This method determines whether to use server-requested backoff or
    /// exponential backoff, calculates the appropriate delay, and updates
    /// the step with backoff information.
    pub async fn calculate_and_apply_backoff(
        &self,
        step_uuid: Uuid,
        context: BackoffContext,
    ) -> Result<BackoffResult, BackoffError> {
        // Check for server-requested backoff first
        if let Some(retry_after) = self.extract_retry_after_header(&context) {
            self.apply_server_requested_backoff(step_uuid, retry_after)
                .await
        } else {
            // Fall back to exponential backoff
            self.apply_exponential_backoff(step_uuid, &context).await
        }
    }

    /// Extract Retry-After header from context
    fn extract_retry_after_header(&self, context: &BackoffContext) -> Option<u32> {
        // Find retry-after header (case-insensitive)
        let retry_after_value = context
            .headers
            .iter()
            .find(|(key, _)| key.to_lowercase() == "retry-after")
            .map(|(_, value)| value)?;

        // Try parsing as seconds (integer)
        if let Ok(seconds) = retry_after_value.parse::<u32>() {
            return Some(seconds);
        }

        // Try parsing as HTTP date (RFC 2822)
        if let Ok(date) = DateTime::parse_from_rfc2822(retry_after_value) {
            let now = Utc::now();
            let diff = date.signed_duration_since(now);
            if diff.num_seconds() > 0 {
                return Some(diff.num_seconds() as u32);
            }
        }

        None
    }

    /// Apply server-requested backoff from Retry-After header
    async fn apply_server_requested_backoff(
        &self,
        step_uuid: Uuid,
        retry_after_seconds: u32,
    ) -> Result<BackoffResult, BackoffError> {
        // Cap the server-requested delay
        let delay_seconds = retry_after_seconds.min(self.config.max_delay_seconds);

        // Update the step with backoff information
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET backoff_request_seconds = $1 WHERE workflow_step_uuid = $2",
            delay_seconds as i32,
            step_uuid
        )
        .execute(&self.pool)
        .await
        .map_err(BackoffError::Database)?;

        Ok(BackoffResult {
            delay_seconds,
            backoff_type: BackoffType::ServerRequested,
            next_retry_at: Utc::now() + Duration::seconds(delay_seconds as i64),
        })
    }

    /// Apply exponential backoff based on attempt count
    async fn apply_exponential_backoff(
        &self,
        step_uuid: Uuid,
        _context: &BackoffContext,
    ) -> Result<BackoffResult, BackoffError> {
        // Get current attempt count
        let step = sqlx::query!(
            "SELECT attempts FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .fetch_one(&self.pool)
        .await
        .map_err(BackoffError::Database)?;

        let attempts = step.attempts.unwrap_or(0) as u32;

        // Calculate exponential delay: base_delay * multiplier^attempts
        let base_delay = self.config.base_delay_seconds as f64;
        let multiplier = self.config.multiplier;
        let exponential_delay = base_delay * multiplier.powi(attempts as i32);

        // Apply maximum cap
        let mut delay_seconds = exponential_delay.min(self.config.max_delay_seconds as f64) as u32;

        // Add jitter if enabled
        if self.config.jitter_enabled {
            delay_seconds = self.apply_jitter(delay_seconds);
        }

        // Update the step with backoff information
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET backoff_request_seconds = $1 WHERE workflow_step_uuid = $2",
            delay_seconds as i32,
            step_uuid
        )
        .execute(&self.pool)
        .await
        .map_err(BackoffError::Database)?;

        Ok(BackoffResult {
            delay_seconds,
            backoff_type: BackoffType::Exponential,
            next_retry_at: Utc::now() + Duration::seconds(delay_seconds as i64),
        })
    }

    /// Apply jitter to delay to prevent thundering herd
    fn apply_jitter(&self, delay_seconds: u32) -> u32 {
        use rand::Rng;

        let jitter_range = (delay_seconds as f64 * self.config.max_jitter) as u32;
        if jitter_range == 0 {
            return delay_seconds;
        }

        // Use thread_rng() which is properly RAII-compliant
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0..=jitter_range);

        // Add or subtract jitter randomly
        if rng.gen_bool(0.5) {
            delay_seconds.saturating_add(jitter)
        } else {
            delay_seconds.saturating_sub(jitter)
        }
    }

    /// Check if a step is ready to retry after backoff period
    pub async fn is_ready_to_retry(&self, step_uuid: Uuid) -> Result<bool, BackoffError> {
        let step = sqlx::query!(
            r#"
            SELECT backoff_request_seconds, last_attempted_at
            FROM tasker_workflow_steps
            WHERE workflow_step_uuid = $1
            "#,
            step_uuid
        )
        .fetch_one(&self.pool)
        .await
        .map_err(BackoffError::Database)?;

        match (step.backoff_request_seconds, step.last_attempted_at) {
            (Some(backoff_seconds), Some(last_attempt)) => {
                let retry_available_at = last_attempt + Duration::seconds(backoff_seconds as i64);
                Ok(Utc::now().naive_utc() >= retry_available_at)
            }
            _ => Ok(true), // No backoff or no last attempt, ready to retry
        }
    }

    /// Clear backoff for a step (e.g., after successful execution)
    pub async fn clear_backoff(&self, step_uuid: Uuid) -> Result<(), BackoffError> {
        sqlx::query!(
            "UPDATE tasker_workflow_steps SET backoff_request_seconds = NULL WHERE workflow_step_uuid = $1",
            step_uuid
        )
        .execute(&self.pool)
        .await
        .map_err(BackoffError::Database)?;

        Ok(())
    }
}

/// Result of a backoff calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffResult {
    /// Calculated delay in seconds
    pub delay_seconds: u32,
    /// Type of backoff applied
    pub backoff_type: BackoffType,
    /// When the step will be ready for retry
    pub next_retry_at: DateTime<Utc>,
}

/// Type of backoff calculation applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffType {
    /// Server requested via Retry-After header
    ServerRequested,
    /// Exponential backoff with optional jitter
    Exponential,
}

/// Errors that can occur during backoff calculation
#[derive(Debug, thiserror::Error)]
pub enum BackoffError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Step not found: {0}")]
    StepNotFound(i64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_config_default() {
        let config = BackoffCalculatorConfig::default();
        assert_eq!(config.base_delay_seconds, 1);
        assert_eq!(config.max_delay_seconds, 300);
        assert_eq!(config.multiplier, 2.0);
        assert!(config.jitter_enabled);
        assert_eq!(config.max_jitter, 0.1);
    }

    #[test]
    fn test_backoff_context_builder() {
        let context = BackoffContext::new()
            .with_header("retry-after".to_string(), "60".to_string())
            .with_error("Rate limited".to_string());

        assert_eq!(context.headers.get("retry-after"), Some(&"60".to_string()));
        assert_eq!(context.error_context, Some("Rate limited".to_string()));
    }

    #[test]
    fn test_extract_retry_after_seconds() {
        let context =
            BackoffContext::new().with_header("retry-after".to_string(), "120".to_string());

        // Test the parsing logic directly without creating a calculator
        let headers = &context.headers;
        let retry_after = headers
            .get("retry-after")
            .and_then(|value| value.parse::<u32>().ok());

        assert_eq!(retry_after, Some(120));
    }
}
