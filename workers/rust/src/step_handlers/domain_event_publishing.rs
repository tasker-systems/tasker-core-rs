//! # TAS-65: Domain Event Publishing Step Handlers
//!
//! Step handlers for the `domain_event_publishing` workflow template that demonstrates
//! domain event publication with various conditions and delivery modes.
//!
//! ## Workflow Steps
//!
//! 1. `validate_order` - Validates order data, publishes `order.validated` on success (fast)
//! 2. `process_payment` - Processes payment, publishes `payment.processed` on success (durable),
//!    `payment.failed` on failure (durable)
//! 3. `update_inventory` - Updates inventory, publishes `inventory.updated` always (fast)
//! 4. `send_notification` - Sends notification, publishes `notification.sent` on success (fast)
//!
//! ## Event Publishing Architecture (TAS-65)
//!
//! These handlers follow the TAS-65 post-execution publisher callback pattern:
//! - Handlers execute ONLY business logic (no direct event publishing)
//! - Return `StepExecutionResult` with relevant data
//! - Worker invokes registered `StepEventPublisher` after completion
//! - Publishers use YAML declarations to determine which events to publish

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::info;
use uuid::Uuid;

// =============================================================================
// ValidateOrderHandler - Step 1
// =============================================================================

/// Validates order data and prepares data for `order.validated` event
///
/// ## Event Declaration (YAML)
/// ```yaml
/// publishes_events:
///   - name: order.validated
///     condition: success
///     delivery_mode: fast
/// ```
#[derive(Debug)]
pub struct ValidateOrderHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ValidateOrderHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract order data from task context
        let order_id = step_data
            .get_context_field::<String>("order_id")
            .unwrap_or_else(|_| Uuid::new_v4().to_string());

        let customer_id = step_data
            .get_context_field::<String>("customer_id")
            .unwrap_or_else(|_| "unknown".to_string());

        let amount = step_data.get_context_field::<f64>("amount").unwrap_or(0.0);

        // Validate order data
        let validation_mode = self
            .config
            .get_string("validation_mode")
            .unwrap_or_else(|| "standard".to_string());

        info!(
            order_id = %order_id,
            customer_id = %customer_id,
            amount = %amount,
            mode = %validation_mode,
            "Validating order"
        );

        // Perform validation checks
        let mut validation_checks = vec!["order_id_present", "customer_id_present"];

        if amount <= 0.0 && validation_mode == "strict" {
            return Ok(error_result(
                step_uuid,
                "Amount must be positive in strict mode".to_string(),
                Some("VALIDATION_ERROR".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        if amount > 0.0 {
            validation_checks.push("amount_positive");
        }

        // Build result with data for event publishing
        let mut metadata = HashMap::new();
        metadata.insert("validation_mode".to_string(), json!(validation_mode));
        metadata.insert(
            "validation_timestamp".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "order_id": order_id,
                "validation_timestamp": chrono::Utc::now().to_rfc3339(),
                "validation_checks": validation_checks,
                "validated": true
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "domain_events_validate_order"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// ProcessPaymentHandler - Step 2
// =============================================================================

/// Processes payment and prepares data for `payment.processed` or `payment.failed` events
///
/// ## Event Declaration (YAML)
/// ```yaml
/// publishes_events:
///   - name: payment.processed
///     condition: success
///     delivery_mode: durable
///     publisher: PaymentEventPublisher  # Custom publisher example
///   - name: payment.failed
///     condition: failure
///     delivery_mode: durable
/// ```
#[derive(Debug)]
pub struct ProcessPaymentHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessPaymentHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract payment data from task context
        let amount = step_data.get_context_field::<f64>("amount").unwrap_or(0.0);
        let order_id = step_data
            .get_context_field::<String>("order_id")
            .unwrap_or_else(|_| "unknown".to_string());

        // Check if we should simulate failure
        let simulate_failure = step_data
            .get_context_field::<bool>("simulate_failure")
            .unwrap_or(false);

        let payment_provider = self
            .config
            .get_string("payment_provider")
            .unwrap_or_else(|| "mock".to_string());

        info!(
            order_id = %order_id,
            amount = %amount,
            provider = %payment_provider,
            simulate_failure = %simulate_failure,
            "Processing payment"
        );

        if simulate_failure {
            return Ok(error_result(
                step_uuid,
                "Simulated payment failure".to_string(),
                Some("PAYMENT_DECLINED".to_string()),
                Some("PaymentError".to_string()),
                true, // Retryable
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("order_id".to_string(), json!(order_id)),
                    (
                        "failed_at".to_string(),
                        json!(chrono::Utc::now().to_rfc3339()),
                    ),
                ])),
            ));
        }

        // Generate transaction data
        let transaction_id = format!("TXN-{}", Uuid::new_v4());

        let mut metadata = HashMap::new();
        metadata.insert("transaction_id".to_string(), json!(&transaction_id));
        metadata.insert(
            "processed_at".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "transaction_id": transaction_id,
                "amount": amount,
                "payment_method": "credit_card",
                "processed_at": chrono::Utc::now().to_rfc3339(),
                "status": "success"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "domain_events_process_payment"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// UpdateInventoryHandler - Step 3
// =============================================================================

/// Updates inventory and prepares data for `inventory.updated` event (published always)
///
/// ## Event Declaration (YAML)
/// ```yaml
/// publishes_events:
///   - name: inventory.updated
///     condition: always
///     delivery_mode: fast
/// ```
#[derive(Debug)]
pub struct UpdateInventoryHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for UpdateInventoryHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let order_id = step_data
            .get_context_field::<String>("order_id")
            .unwrap_or_else(|_| "unknown".to_string());

        let inventory_source = self
            .config
            .get_string("inventory_source")
            .unwrap_or_else(|| "mock".to_string());

        info!(
            order_id = %order_id,
            source = %inventory_source,
            "Updating inventory"
        );

        // Mock inventory items - in real scenarios this would come from order data
        let items = vec![
            json!({"sku": "ITEM-001", "quantity": 1}),
            json!({"sku": "ITEM-002", "quantity": 2}),
        ];

        let mut metadata = HashMap::new();
        metadata.insert("inventory_source".to_string(), json!(inventory_source));
        metadata.insert(
            "updated_at".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );

        // inventory.updated event is published with condition: always
        // So the result contains success status for the event payload
        Ok(success_result(
            step_uuid,
            json!({
                "order_id": order_id,
                "items": items,
                "success": true,
                "updated_at": chrono::Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "domain_events_update_inventory"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// SendNotificationHandler - Step 4
// =============================================================================

/// Sends customer notification and prepares data for `notification.sent` event
///
/// ## Event Declaration (YAML)
/// ```yaml
/// publishes_events:
///   - name: notification.sent
///     condition: success
///     delivery_mode: fast
/// ```
#[derive(Debug)]
pub struct SendNotificationHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for SendNotificationHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let customer_id = step_data
            .get_context_field::<String>("customer_id")
            .unwrap_or_else(|_| "unknown".to_string());

        let notification_type = self
            .config
            .get_string("notification_type")
            .unwrap_or_else(|| "email".to_string());

        info!(
            customer_id = %customer_id,
            notification_type = %notification_type,
            "Sending notification"
        );

        let notification_id = format!("NOTIF-{}", Uuid::new_v4());

        let mut metadata = HashMap::new();
        metadata.insert("notification_id".to_string(), json!(&notification_id));
        metadata.insert(
            "sent_at".to_string(),
            json!(chrono::Utc::now().to_rfc3339()),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "notification_id": notification_id,
                "channel": notification_type,
                "recipient": customer_id,
                "sent_at": chrono::Utc::now().to_rfc3339(),
                "status": "delivered"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &str {
        "domain_events_send_notification"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_shared::models::core::task::Task;
    use tasker_shared::models::core::task_template::HandlerDefinition;
    use tasker_shared::models::core::workflow_step::WorkflowStepWithName;
    use tasker_shared::models::task_template::StepDefinition;

    /// Helper to create test TaskSequenceStep with given context
    fn create_test_step_data(context: serde_json::Value) -> TaskSequenceStep {
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
            context: Some(context),
            identity_hash: "test".to_string(),
            priority: 0,
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            correlation_id: Uuid::new_v4(),
            parent_correlation_id: None,
        };

        let task_for_orch = tasker_shared::models::core::task::TaskForOrchestration {
            task,
            task_name: "domain_event_publishing".to_string(),
            task_version: "1.0.0".to_string(),
            namespace_name: "domain_events".to_string(),
        };

        let workflow_step = WorkflowStepWithName {
            workflow_step_uuid: Uuid::new_v4(),
            task_uuid: task_for_orch.task.task_uuid,
            named_step_uuid: Uuid::new_v4(),
            name: "test_step".to_string(),
            template_step_name: "test_step".to_string(),
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
            name: "test_step".to_string(),
            description: None,
            handler: HandlerDefinition {
                callable: "TestHandler".to_string(),
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
    async fn test_validate_order_success() {
        let config = StepHandlerConfig::empty();
        let handler = ValidateOrderHandler::new(config);

        let step_data = create_test_step_data(json!({
            "order_id": "order-123",
            "customer_id": "cust-456",
            "amount": 99.99
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.result["order_id"], "order-123");
        assert!(result.result["validated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_validate_order_strict_mode_failure() {
        let mut config_data = HashMap::new();
        config_data.insert("validation_mode".to_string(), json!("strict"));
        let config = StepHandlerConfig::new(config_data);
        let handler = ValidateOrderHandler::new(config);

        let step_data = create_test_step_data(json!({
            "order_id": "order-123",
            "customer_id": "cust-456",
            "amount": -10.0  // Invalid amount
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(!result.is_success());
        assert_eq!(
            result.metadata.error_code,
            Some("VALIDATION_ERROR".to_string())
        );
    }

    #[tokio::test]
    async fn test_process_payment_success() {
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        let step_data = create_test_step_data(json!({
            "order_id": "order-123",
            "amount": 99.99,
            "simulate_failure": false
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(result.is_success());
        assert!(result.result["transaction_id"]
            .as_str()
            .unwrap()
            .starts_with("TXN-"));
        assert_eq!(result.result["status"], "success");
    }

    #[tokio::test]
    async fn test_process_payment_simulated_failure() {
        let config = StepHandlerConfig::empty();
        let handler = ProcessPaymentHandler::new(config);

        let step_data = create_test_step_data(json!({
            "order_id": "order-123",
            "amount": 99.99,
            "simulate_failure": true
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(!result.is_success());
        assert_eq!(
            result.metadata.error_code,
            Some("PAYMENT_DECLINED".to_string())
        );
        assert!(result.is_retryable());
    }

    #[tokio::test]
    async fn test_update_inventory_success() {
        let config = StepHandlerConfig::empty();
        let handler = UpdateInventoryHandler::new(config);

        let step_data = create_test_step_data(json!({
            "order_id": "order-123"
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.result["order_id"], "order-123");
        assert!(result.result["success"].as_bool().unwrap());
        assert!(result.result["items"].is_array());
    }

    #[tokio::test]
    async fn test_send_notification_success() {
        let config = StepHandlerConfig::empty();
        let handler = SendNotificationHandler::new(config);

        let step_data = create_test_step_data(json!({
            "customer_id": "cust-456"
        }));

        let result = handler.call(&step_data).await.unwrap();
        assert!(result.is_success());
        assert!(result.result["notification_id"]
            .as_str()
            .unwrap()
            .starts_with("NOTIF-"));
        assert_eq!(result.result["recipient"], "cust-456");
        assert_eq!(result.result["status"], "delivered");
    }

    #[test]
    fn test_handler_names() {
        let config = StepHandlerConfig::empty();

        assert_eq!(
            ValidateOrderHandler::new(config.clone()).name(),
            "domain_events_validate_order"
        );
        assert_eq!(
            ProcessPaymentHandler::new(config.clone()).name(),
            "domain_events_process_payment"
        );
        assert_eq!(
            UpdateInventoryHandler::new(config.clone()).name(),
            "domain_events_update_inventory"
        );
        assert_eq!(
            SendNotificationHandler::new(config).name(),
            "domain_events_send_notification"
        );
    }
}
