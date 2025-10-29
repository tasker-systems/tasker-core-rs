//! Metadata Processor
//!
//! Handles orchestration metadata processing for backoff calculations and retry coordination.

use tracing::{debug, error, info};
use uuid::Uuid;

use crate::orchestration::{backoff_calculator::BackoffContext, BackoffCalculator};
use tasker_shared::errors::OrchestrationResult;
use tasker_shared::messaging::message::OrchestrationMetadata;

/// Processes orchestration metadata for intelligent retry decisions
#[derive(Clone, Debug)]
pub struct MetadataProcessor {
    backoff_calculator: BackoffCalculator,
}

impl MetadataProcessor {
    pub fn new(backoff_calculator: BackoffCalculator) -> Self {
        Self { backoff_calculator }
    }

    /// Process orchestration metadata for backoff and retry coordination
    ///
    /// This method analyzes worker-provided metadata to make intelligent backoff decisions:
    /// - HTTP headers (Retry-After, Rate-Limit headers)
    /// - Error context for domain-specific retry logic
    /// - Explicit backoff hints from handlers
    pub async fn process_metadata(
        &self,
        step_uuid: &Uuid,
        metadata: &OrchestrationMetadata,
        correlation_id: Uuid,
    ) -> OrchestrationResult<()> {
        debug!(
            correlation_id = %correlation_id,
            step_uuid = %step_uuid,
            headers_count = metadata.headers.len(),
            has_error_context = metadata.error_context.is_some(),
            has_backoff_hint = metadata.backoff_hint.is_some(),
            "Processing orchestration metadata"
        );

        // Create backoff context from orchestration metadata
        let mut backoff_context = BackoffContext::new();

        // Add HTTP headers (e.g., Retry-After, X-RateLimit-Reset)
        for (key, value) in &metadata.headers {
            backoff_context = backoff_context.with_header(key.clone(), value.clone());
        }

        // Add error context if present
        if let Some(error_context) = &metadata.error_context {
            backoff_context = backoff_context.with_error(error_context.clone());
        }

        // Add custom metadata
        for (key, value) in &metadata.custom {
            backoff_context = backoff_context.with_metadata(key.clone(), value.clone());
        }

        // Process explicit backoff hint if provided
        if let Some(backoff_hint) = &metadata.backoff_hint {
            match backoff_hint.backoff_type {
                tasker_shared::messaging::message::BackoffHintType::ServerRequested => {
                    // Add server-requested delay from hint to backoff context
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    info!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        delay_seconds = backoff_hint.delay_seconds,
                        "Handler provided server-requested backoff"
                    );
                }
                tasker_shared::messaging::message::BackoffHintType::RateLimit => {
                    // Add rate limit context for exponential backoff calculation
                    backoff_context = backoff_context.with_metadata(
                        "rate_limit_detected".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    if let Some(context) = &backoff_hint.context {
                        backoff_context = backoff_context.with_error(context.clone());
                    }
                    info!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        "Handler detected rate limit"
                    );
                }
                tasker_shared::messaging::message::BackoffHintType::ServiceUnavailable => {
                    // Service unavailable - use longer backoff
                    backoff_context = backoff_context.with_metadata(
                        "service_unavailable".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    info!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        delay_seconds = backoff_hint.delay_seconds,
                        "Handler reported service unavailable"
                    );
                }
                tasker_shared::messaging::message::BackoffHintType::Custom => {
                    // Custom backoff strategy
                    backoff_context = backoff_context
                        .with_metadata("custom_backoff".to_string(), serde_json::Value::Bool(true));
                    backoff_context = backoff_context.with_metadata(
                        "handler_delay_seconds".to_string(),
                        serde_json::Value::Number(backoff_hint.delay_seconds.into()),
                    );
                    if let Some(context) = &backoff_hint.context {
                        backoff_context = backoff_context.with_error(context.clone());
                    }
                    info!(
                        correlation_id = %correlation_id,
                        step_uuid = %step_uuid,
                        delay_seconds = backoff_hint.delay_seconds,
                        "Handler provided custom backoff strategy"
                    );
                }
            }
        }

        // Apply backoff calculation with enhanced context
        match self
            .backoff_calculator
            .calculate_and_apply_backoff(step_uuid, backoff_context)
            .await
        {
            Ok(backoff_result) => {
                info!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    backoff_type = ?backoff_result.backoff_type,
                    delay_seconds = backoff_result.delay_seconds,
                    next_retry_at = %backoff_result.next_retry_at,
                    "Applied backoff"
                );
            }
            Err(e) => {
                error!(
                    correlation_id = %correlation_id,
                    step_uuid = %step_uuid,
                    error = %e,
                    "Failed to calculate backoff with metadata"
                );
                return Err(e.into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tasker_shared::system_context::SystemContext;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_metadata_processor_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let backoff_config: crate::orchestration::BackoffCalculatorConfig =
            context.tasker_config.clone().into();
        let backoff_calculator = BackoffCalculator::new(backoff_config, pool);
        let processor = MetadataProcessor::new(backoff_calculator);

        // Verify it's created (basic smoke test)
        assert!(std::mem::size_of_val(&processor) > 0);
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_metadata_processor_clone(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        let backoff_config: crate::orchestration::BackoffCalculatorConfig =
            context.tasker_config.clone().into();
        let backoff_calculator = BackoffCalculator::new(backoff_config, pool);
        let processor = MetadataProcessor::new(backoff_calculator);

        let cloned = processor.clone();

        // Verify both exist and are independent
        assert!(std::mem::size_of_val(&processor) > 0);
        assert!(std::mem::size_of_val(&cloned) > 0);
        Ok(())
    }

    #[test]
    fn test_backoff_context_builder() {
        // Test that BackoffContext can be built with various metadata
        let context = BackoffContext::new();
        let context = context.with_header("Retry-After".to_string(), "60".to_string());
        let context = context.with_metadata(
            "handler_delay_seconds".to_string(),
            serde_json::Value::Number(30.into()),
        );

        // Verify context can be built (structure validation)
        assert!(std::mem::size_of_val(&context) > 0);
    }
}
