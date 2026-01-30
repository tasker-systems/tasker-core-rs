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
use std::sync::Arc;
// TAS-61 Phase 6C/6D: V2 configuration is canonical
use tasker_shared::config::tasker::TaskerConfig;
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

impl From<Arc<TaskerConfig>> for BackoffCalculatorConfig {
    fn from(config: Arc<TaskerConfig>) -> BackoffCalculatorConfig {
        // Use the first default backoff value as base delay, or fallback to 1 second
        let base_delay_seconds = config
            .common
            .backoff
            .default_backoff_seconds
            .first()
            .copied()
            .unwrap_or(1);

        BackoffCalculatorConfig {
            base_delay_seconds,
            max_delay_seconds: config.common.backoff.max_backoff_seconds,
            multiplier: config.common.backoff.backoff_multiplier,
            jitter_enabled: config.common.backoff.jitter_enabled,
            max_jitter: config.common.backoff.jitter_max_percentage,
        }
    }
}

impl From<&TaskerConfig> for BackoffCalculatorConfig {
    fn from(config: &TaskerConfig) -> BackoffCalculatorConfig {
        // Use the first default backoff value as base delay, or fallback to 1 second
        let base_delay_seconds = config
            .common
            .backoff
            .default_backoff_seconds
            .first()
            .copied()
            .unwrap_or(1);

        BackoffCalculatorConfig {
            base_delay_seconds,
            max_delay_seconds: config.common.backoff.max_backoff_seconds,
            multiplier: config.common.backoff.backoff_multiplier,
            jitter_enabled: config.common.backoff.jitter_enabled,
            max_jitter: config.common.backoff.jitter_max_percentage,
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
#[derive(Clone, Debug)]
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
        step_uuid: &Uuid,
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

    /// TAS-57: Atomically update backoff with row-level locking
    ///
    /// This method uses SELECT FOR UPDATE to acquire a row-level lock before updating,
    /// preventing race conditions when multiple orchestrators process the same step failure.
    /// Also updates last_attempted_at to ensure timing consistency with SQL calculations.
    async fn update_backoff_atomic(
        &self,
        step_uuid: &Uuid,
        delay_seconds: u32,
    ) -> Result<(), BackoffError> {
        // Begin transaction
        let mut tx = self.pool.begin().await.map_err(BackoffError::Database)?;

        // Acquire row-level lock with SELECT FOR UPDATE
        // This blocks other transactions from modifying this row until we commit
        sqlx::query!(
            "SELECT workflow_step_uuid
             FROM tasker.workflow_steps
             WHERE workflow_step_uuid = $1
             FOR UPDATE",
            step_uuid
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(BackoffError::Database)?;

        // Update both backoff and timestamp within locked transaction
        // The timestamp ensures SQL fallback calculations use consistent timing
        sqlx::query!(
            "UPDATE tasker.workflow_steps
             SET backoff_request_seconds = $1,
                 last_attempted_at = NOW(),
                 updated_at = NOW()
             WHERE workflow_step_uuid = $2",
            delay_seconds as i32,
            step_uuid
        )
        .execute(&mut *tx)
        .await
        .map_err(BackoffError::Database)?;

        // Commit transaction and release lock
        tx.commit().await.map_err(BackoffError::Database)?;

        Ok(())
    }

    /// Apply server-requested backoff from Retry-After header
    async fn apply_server_requested_backoff(
        &self,
        step_uuid: &Uuid,
        retry_after_seconds: u32,
    ) -> Result<BackoffResult, BackoffError> {
        // Cap the server-requested delay
        let delay_seconds = retry_after_seconds.min(self.config.max_delay_seconds);

        // TAS-57: Use atomic update instead of direct query
        self.update_backoff_atomic(step_uuid, delay_seconds).await?;

        Ok(BackoffResult {
            delay_seconds,
            backoff_type: BackoffType::ServerRequested,
            next_retry_at: Utc::now() + Duration::seconds(delay_seconds as i64),
        })
    }

    /// Apply exponential backoff based on attempt count
    async fn apply_exponential_backoff(
        &self,
        step_uuid: &Uuid,
        _context: &BackoffContext,
    ) -> Result<BackoffResult, BackoffError> {
        // Get current attempt count
        let step = sqlx::query!(
            "SELECT attempts FROM tasker.workflow_steps WHERE workflow_step_uuid = $1",
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

        // TAS-57: Use atomic update instead of direct query
        self.update_backoff_atomic(step_uuid, delay_seconds).await?;

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

        let mut rng = rand::rng();
        let jitter = rng.random_range(0..=jitter_range);

        // Add or subtract jitter randomly
        if rng.random_bool(0.5) {
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
            FROM tasker.workflow_steps
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
            "UPDATE tasker.workflow_steps SET backoff_request_seconds = NULL WHERE workflow_step_uuid = $1",
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

    #[test]
    fn test_backoff_context_default() {
        let context = BackoffContext::default();
        assert!(context.headers.is_empty());
        assert!(context.error_context.is_none());
        assert!(context.metadata.is_empty());
    }

    #[test]
    fn test_backoff_context_with_metadata() {
        let context = BackoffContext::new()
            .with_metadata("retry_count".to_string(), serde_json::json!(3))
            .with_metadata("source".to_string(), serde_json::json!("api_gateway"));

        assert_eq!(context.metadata.len(), 2);
        assert_eq!(context.metadata["retry_count"], serde_json::json!(3));
        assert_eq!(context.metadata["source"], serde_json::json!("api_gateway"));
    }

    #[test]
    fn test_backoff_context_full_builder_chain() {
        let context = BackoffContext::new()
            .with_header("Retry-After".to_string(), "60".to_string())
            .with_header("X-RateLimit-Reset".to_string(), "1700000000".to_string())
            .with_error("429 Too Many Requests".to_string())
            .with_metadata("endpoint".to_string(), serde_json::json!("/api/v1/tasks"));

        assert_eq!(context.headers.len(), 2);
        assert!(context.error_context.is_some());
        assert_eq!(context.metadata.len(), 1);
    }

    #[test]
    fn test_backoff_config_custom_values() {
        let config = BackoffCalculatorConfig {
            base_delay_seconds: 5,
            max_delay_seconds: 600,
            multiplier: 3.0,
            jitter_enabled: false,
            max_jitter: 0.2,
        };

        assert_eq!(config.base_delay_seconds, 5);
        assert_eq!(config.max_delay_seconds, 600);
        assert_eq!(config.multiplier, 3.0);
        assert!(!config.jitter_enabled);
        assert_eq!(config.max_jitter, 0.2);
    }

    #[test]
    fn test_backoff_config_serialization_roundtrip() {
        let config = BackoffCalculatorConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: BackoffCalculatorConfig =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.base_delay_seconds, config.base_delay_seconds);
        assert_eq!(deserialized.max_delay_seconds, config.max_delay_seconds);
        assert_eq!(deserialized.multiplier, config.multiplier);
        assert_eq!(deserialized.jitter_enabled, config.jitter_enabled);
        assert_eq!(deserialized.max_jitter, config.max_jitter);
    }

    #[test]
    fn test_backoff_result_construction() {
        let now = Utc::now();
        let result = BackoffResult {
            delay_seconds: 30,
            backoff_type: BackoffType::Exponential,
            next_retry_at: now + Duration::seconds(30),
        };

        assert_eq!(result.delay_seconds, 30);
        assert!(matches!(result.backoff_type, BackoffType::Exponential));
        assert!(result.next_retry_at > now);
    }

    #[test]
    fn test_backoff_result_server_requested_type() {
        let result = BackoffResult {
            delay_seconds: 120,
            backoff_type: BackoffType::ServerRequested,
            next_retry_at: Utc::now() + Duration::seconds(120),
        };

        assert_eq!(result.delay_seconds, 120);
        assert!(matches!(result.backoff_type, BackoffType::ServerRequested));
    }

    #[test]
    fn test_backoff_error_display_messages() {
        let db_err = BackoffError::Database(sqlx::Error::ColumnNotFound("col".to_string()));
        assert!(db_err.to_string().contains("Database error"));

        let config_err = BackoffError::InvalidConfig("negative delay".to_string());
        assert_eq!(
            config_err.to_string(),
            "Invalid configuration: negative delay"
        );

        let step_err = BackoffError::StepNotFound(99);
        assert_eq!(step_err.to_string(), "Step not found: 99");
    }

    #[test]
    fn test_backoff_result_serialization() {
        let result = BackoffResult {
            delay_seconds: 60,
            backoff_type: BackoffType::Exponential,
            next_retry_at: Utc::now(),
        };

        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["delay_seconds"], 60);
        assert_eq!(json["backoff_type"], "Exponential");
    }

    // --- Tests requiring a database pool (for BackoffCalculator methods) ---

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_extract_retry_after_case_insensitive(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);

        // Lowercase
        let ctx = BackoffContext::new().with_header("retry-after".to_string(), "30".to_string());
        assert_eq!(calculator.extract_retry_after_header(&ctx), Some(30));

        // Mixed case
        let ctx = BackoffContext::new().with_header("Retry-After".to_string(), "60".to_string());
        assert_eq!(calculator.extract_retry_after_header(&ctx), Some(60));

        // Uppercase
        let ctx = BackoffContext::new().with_header("RETRY-AFTER".to_string(), "90".to_string());
        assert_eq!(calculator.extract_retry_after_header(&ctx), Some(90));

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_extract_retry_after_missing_header(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);
        let ctx = BackoffContext::new().with_header("X-Custom".to_string(), "value".to_string());

        assert_eq!(calculator.extract_retry_after_header(&ctx), None);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_extract_retry_after_invalid_value(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);
        let ctx = BackoffContext::new()
            .with_header("retry-after".to_string(), "not-a-number".to_string());

        assert_eq!(calculator.extract_retry_after_header(&ctx), None);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_extract_retry_after_empty_headers(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);
        let ctx = BackoffContext::new();

        assert_eq!(calculator.extract_retry_after_header(&ctx), None);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_apply_jitter_within_bounds(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = BackoffCalculatorConfig {
            jitter_enabled: true,
            max_jitter: 0.1, // 10%
            ..Default::default()
        };
        let calculator = BackoffCalculator::new(config, pool);

        // Run multiple times to verify bounds
        for _ in 0..50 {
            let jittered = calculator.apply_jitter(100);
            // 10% jitter means 90-110 range
            assert!(jittered >= 90, "Jitter too low: {jittered}");
            assert!(jittered <= 110, "Jitter too high: {jittered}");
        }

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_apply_jitter_zero_delay(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);
        let jittered = calculator.apply_jitter(0);
        assert_eq!(jittered, 0, "Zero delay should remain zero");
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_apply_jitter_small_delay_no_underflow(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let calculator = BackoffCalculator::with_defaults(pool);
        // With small delays, jitter range rounds to 0 so delay is unchanged
        let jittered = calculator.apply_jitter(1);
        // max_jitter 0.1 * 1 = 0.1 → rounds to 0, so returns original
        assert_eq!(jittered, 1);
        Ok(())
    }
}
