//! Metadata Processor
//!
//! Handles orchestration metadata processing for backoff calculations and retry coordination.

use tracing::{debug, error, info};
use uuid::Uuid;

use crate::orchestration::{backoff_calculator::BackoffContext, BackoffCalculator};
use tasker_shared::messaging::message::OrchestrationMetadata;
use tasker_shared::errors::OrchestrationResult;

/// Processes orchestration metadata for intelligent retry decisions
#[derive(Clone)]
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
