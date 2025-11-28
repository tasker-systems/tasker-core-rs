//! # TAS-65: Payment Handler Example
//!
//! Example step handler demonstrating pure business logic execution.
//!
//! ## Post-Execution Event Publishing (TAS-65 Architecture)
//!
//! This handler follows the TAS-65 post-execution publisher callback pattern:
//!
//! 1. Handler executes ONLY business logic (no event publishing)
//! 2. Returns `StepExecutionResult` with transaction data
//! 3. Worker invokes `PaymentEventPublisher` after completion
//! 4. `PaymentEventPublisher` publishes domain events based on YAML config
//!
//! This separation ensures:
//! - Pure separation of concerns (business logic vs observability)
//! - Event publishing failures never affect step execution
//! - Configuration-as-contract via YAML declarations
//! - Testability without event infrastructure mocks

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::info;
use uuid::Uuid;

/// Example handler that processes a payment
///
/// This handler demonstrates the TAS-65 architecture:
/// - Executes ONLY business logic (payment processing)
/// - Returns result data for downstream event publishing
/// - Does NOT publish events directly (that's handled by `PaymentEventPublisher`)
///
/// ## YAML Configuration
///
/// The step should declare events in the task template:
/// ```yaml
/// steps:
///   - name: process_payment
///     handler:
///       callable: ProcessPaymentHandler
///     publishes_events:
///       - name: payment.processed
///         condition: success
///         publisher: PaymentEventPublisher
///       - name: payment.failed
///         condition: failure
///         publisher: PaymentEventPublisher
/// ```
#[derive(Debug)]
pub struct ProcessPaymentHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessPaymentHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract payment data from task context
        let amount = match step_data.get_context_field::<f64>("amount") {
            Ok(value) => value,
            Err(e) => {
                return Ok(error_result(
                    step_uuid,
                    format!("Missing required field 'amount': {}", e),
                    Some("MISSING_CONTEXT_FIELD".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - data validation error
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("amount"))])),
                ));
            }
        };

        let currency = step_data
            .get_context_field::<String>("currency")
            .unwrap_or_else(|_| "USD".to_string());

        // Validate amount
        if amount <= 0.0 {
            return Ok(error_result(
                step_uuid,
                "Payment amount must be positive".to_string(),
                Some("INVALID_AMOUNT".to_string()),
                Some("ValidationError".to_string()),
                false, // Not retryable
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("amount".to_string(), json!(amount))])),
            ));
        }

        // Business logic: process payment
        info!("Processing payment: {} {}", amount, currency);

        let transaction_id = format!("TXN-{}", Uuid::new_v4());
        let timestamp = chrono::Utc::now();

        // Build the execution result
        // Note: The result data is used by PaymentEventPublisher to build domain events
        let mut metadata = HashMap::new();
        metadata.insert("transaction_id".to_string(), json!(transaction_id));
        metadata.insert("processed_at".to_string(), json!(timestamp.to_rfc3339()));

        // Return just the business result
        // Domain events are published by PaymentEventPublisher AFTER step completes
        Ok(success_result(
            step_uuid,
            json!({
                "transaction_id": transaction_id,
                "status": "success",
                "amount": amount,
                "currency": currency,
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        // Note: This name is used for handler lookup in the registry
        // Use "payment_example" to avoid conflicting with order_fulfillment's "process_payment"
        "payment_example"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::models::core::task::Task;
    use tasker_shared::models::core::task_template::HandlerDefinition;
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::models::task_template::StepDefinition;

    /// Helper to create test TaskSequenceStep
    fn create_test_step_data(amount: f64, currency: &str) -> TaskSequenceStep {
        // Create minimal task with context
        let task = Task {
            task_uuid: Uuid::new_v4(),
            named_task_uuid: Uuid::new_v4(),
            complete: false,
            requested_at: chrono::Utc::now().naive_utc(),
            initiator: Some("test".to_string()),
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: Some(json!({
                "amount": amount,
                "currency": currency,
            })),
            identity_hash: "test".to_string(),
            priority: 0,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            correlation_id: Uuid::new_v4(),
            parent_correlation_id: None,
        };

        let task_for_orch = tasker_shared::models::core::task::TaskForOrchestration {
            task,
            task_name: "payment_workflow".to_string(),
            task_version: "1.0.0".to_string(),
            namespace_name: "payments".to_string(),
        };

        let workflow_step = WorkflowStepWithName {
            workflow_step_uuid: Uuid::new_v4(),
            task_uuid: task_for_orch.task.task_uuid,
            named_step_uuid: Uuid::new_v4(),
            name: "process_payment".to_string(),
            template_step_name: "process_payment".to_string(),
            retryable: true,
            max_attempts: Some(3),
            in_process: false,
            processed: false,
            processed_at: None,
            attempts: Some(1),
            last_attempted_at: None,
            backoff_request_seconds: None,
            inputs: None,
            results: None,
            skippable: false,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        let step_definition = StepDefinition {
            name: "process_payment".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "ProcessPaymentHandler".to_string(),
                initialization: HashMap::new(),
            },
            step_type: tasker_shared::models::task_template::StepType::Standard,
            system_dependency: None,
            dependencies: vec![],
            retry: tasker_shared::models::task_template::RetryConfiguration::default(),
            timeout_seconds: None,
            publishes_events: vec![],
            batch_config: None,
        };

        TaskSequenceStep {
            task: task_for_orch,
            workflow_step,
            dependency_results: HashMap::new(),
            step_definition,
        }
    }

    #[tokio::test]
    async fn test_payment_handler_success() {
        // Handler executes business logic only - event publishing is external
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        let step_data = create_test_step_data(100.0, "USD");

        let result = handler.call(&step_data).await;
        assert!(result.is_ok());

        let execution_result = result.unwrap();
        assert!(execution_result.is_success());

        // Verify result contains transaction data
        assert_eq!(execution_result.result["status"], "success");
        assert_eq!(execution_result.result["amount"], 100.0);
        assert_eq!(execution_result.result["currency"], "USD");
        assert!(execution_result.result["transaction_id"].is_string());
    }

    #[tokio::test]
    async fn test_payment_handler_with_invalid_amount() {
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        let step_data = create_test_step_data(-50.0, "USD");

        let result = handler.call(&step_data).await.unwrap();
        assert!(!result.is_success());
        assert_eq!(
            result.metadata.error_code,
            Some("INVALID_AMOUNT".to_string())
        );
        assert!(!result.is_retryable());
    }

    #[tokio::test]
    async fn test_payment_handler_missing_amount() {
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        // Create step_data without amount field
        let mut step_data = create_test_step_data(100.0, "USD");
        step_data.task.task.context = Some(json!({
            "currency": "EUR"
            // Missing "amount"
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(!result.is_success());
        assert_eq!(
            result.metadata.error_code,
            Some("MISSING_CONTEXT_FIELD".to_string())
        );
        assert!(!result.is_retryable());
    }
}
