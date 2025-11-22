//! # FailNTimesHandler - Error Injection for Retry Testing (TAS-64)
//!
//! A step handler that fails the first N attempts, then succeeds.
//! Used for E2E testing of retry mechanics.
//!
//! ## Configuration (YAML)
//!
//! ```yaml
//! handler:
//!   callable: FailNTimesHandler
//!   initialization:
//!     fail_count: 2  # Fail first 2 attempts, succeed on 3rd
//! ```
//!
//! ## Behavior
//!
//! - Reads current attempt count from `workflow_step.attempts`
//! - If `attempts < fail_count`: returns retryable error
//! - If `attempts >= fail_count`: returns success
//!
//! This enables testing of:
//! - Retry mechanics actually re-execute handlers
//! - Attempt counting is accurate
//! - WaitingForRetry → Pending → InProgress transitions work

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use crate::step_handlers::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;

/// Step handler that fails first N attempts then succeeds
#[derive(Debug)]
pub struct FailNTimesHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for FailNTimesHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get configuration from step_definition.handler.initialization (runtime config from YAML)
        let fail_count = step_data
            .step_definition
            .handler
            .initialization
            .get("fail_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32;

        // Current attempt number (1-indexed: first attempt = 1)
        // Default to 1 if not set (first attempt)
        let current_attempt = step_data.workflow_step.attempts.unwrap_or(1);

        let elapsed_ms = start_time.elapsed().as_millis() as i64;

        tracing::info!(
            step_uuid = %step_uuid,
            current_attempt = current_attempt,
            fail_count = fail_count,
            "FailNTimesHandler executing"
        );

        // Fail if we haven't completed the required number of failures
        // With 1-indexed attempts: fail on attempts 1..=fail_count, succeed on attempt fail_count+1
        if current_attempt <= fail_count {
            tracing::warn!(
                step_uuid = %step_uuid,
                attempt = current_attempt,
                fail_until = fail_count,
                "FailNTimesHandler: Simulating failure"
            );

            return Ok(error_result(
                step_uuid,
                format!(
                    "Simulated failure on attempt {} (will succeed after {} failures)",
                    current_attempt, fail_count
                ),
                Some("SIMULATED_FAILURE".to_string()),
                Some("RetryableError".to_string()),
                true, // retryable
                elapsed_ms,
                Some(
                    [
                        ("attempt".to_string(), json!(current_attempt)),
                        ("fail_until".to_string(), json!(fail_count)),
                    ]
                    .into_iter()
                    .collect(),
                ),
            ));
        }

        // Success: we've failed enough times (current_attempt > fail_count)
        tracing::info!(
            step_uuid = %step_uuid,
            total_attempts = current_attempt,
            "FailNTimesHandler: Succeeded after simulated failures"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "attempts_before_success": current_attempt,
                "fail_count_configured": fail_count,
                "message": "Succeeded after simulated failures"
            }),
            elapsed_ms,
            None,
        ))
    }

    fn name(&self) -> &str {
        "FailNTimesHandler"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let config = StepHandlerConfig::empty();
        let handler = FailNTimesHandler::new(config);
        assert_eq!(handler.name(), "FailNTimesHandler");
    }

    #[test]
    fn test_config_parsing() {
        let config =
            StepHandlerConfig::new([("fail_count".to_string(), json!(3))].into_iter().collect());
        assert_eq!(config.get_i64("fail_count"), Some(3));
    }

    #[test]
    fn test_default_fail_count() {
        let config = StepHandlerConfig::empty();
        // Default should be 1 when not specified
        assert_eq!(config.get_i64("fail_count").unwrap_or(1), 1);
    }
}
