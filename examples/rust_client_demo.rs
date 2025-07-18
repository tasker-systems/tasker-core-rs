//! # Rust Client Foundation Demo
//!
//! This example demonstrates how to use the Rust client foundation
//! to create custom task and step handlers.

use std::collections::HashMap;
use std::sync::Arc;
use tasker_core::client::{BaseTaskHandler, RustStepHandler, StepContext};
use tasker_core::orchestration::handler_config::{HandlerConfiguration, StepTemplate};
use tasker_core::{Result, TaskerError};

/// Example step handler that processes payment transactions
#[derive(Clone)]
struct PaymentProcessorStep;

#[async_trait::async_trait]
impl RustStepHandler for PaymentProcessorStep {
    async fn process(&self, context: &StepContext) -> Result<serde_json::Value> {
        println!("Processing payment for step: {}", context.step_name);
        println!("Input data: {}", context.input_data);

        // Extract payment amount from input
        let amount = context
            .input_data
            .get("amount")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| TaskerError::ValidationError("Missing amount field".to_string()))?;

        // Simulate payment processing
        if amount <= 0.0 {
            return Err(TaskerError::ValidationError(
                "Amount must be positive".to_string(),
            ));
        }

        println!("Processing payment of ${:.2}", amount);

        // Simulate some processing time
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Return success result
        Ok(serde_json::json!({
            "status": "processed",
            "amount": amount,
            "transaction_id": format!("txn_{}", context.step_id),
            "processed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn process_results(
        &self,
        context: &StepContext,
        process_output: &serde_json::Value,
        _initial_results: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        println!(
            "Processing results for payment step {}: {}",
            context.step_name, process_output
        );

        // Add additional metadata to the result
        let mut enhanced_result = process_output.clone();
        if let Some(obj) = enhanced_result.as_object_mut() {
            obj.insert(
                "result_processed_at".to_string(),
                serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
            );
            obj.insert(
                "step_handler".to_string(),
                serde_json::Value::String(self.handler_name().to_string()),
            );
        }

        Ok(enhanced_result)
    }

    fn validate_config(&self, config: &HashMap<String, serde_json::Value>) -> Result<()> {
        // Validate that required configuration is present
        if config.get("payment_gateway").is_none() {
            return Err(TaskerError::ValidationError(
                "payment_gateway configuration is required".to_string(),
            ));
        }
        Ok(())
    }

    fn handler_name(&self) -> &'static str {
        "PaymentProcessorStep"
    }
}

/// Example step handler that sends notifications
#[derive(Clone)]
struct NotificationStep;

#[async_trait::async_trait]
impl RustStepHandler for NotificationStep {
    async fn process(&self, context: &StepContext) -> Result<serde_json::Value> {
        println!("Sending notification for step: {}", context.step_name);

        // Get the payment result from previous step
        let default_result = serde_json::json!({});
        let payment_result = context
            .previous_results
            .get("process_payment")
            .unwrap_or(&default_result);

        let transaction_id = payment_result
            .get("transaction_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        println!("Sending notification for transaction: {}", transaction_id);

        // Simulate notification sending
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Ok(serde_json::json!({
            "status": "sent",
            "transaction_id": transaction_id,
            "notification_id": format!("notif_{}", context.step_id),
            "sent_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    fn handler_name(&self) -> &'static str {
        "NotificationStep"
    }
}

fn create_demo_handler_config() -> HandlerConfiguration {
    HandlerConfiguration {
        name: "payment_workflow".to_string(),
        module_namespace: Some("PaymentModule".to_string()),
        task_handler_class: "PaymentWorkflowHandler".to_string(),
        namespace_name: "payments".to_string(),
        version: "1.0.0".to_string(),
        default_dependent_system: Some("payment_service".to_string()),
        named_steps: vec![
            "process_payment".to_string(),
            "send_notification".to_string(),
        ],
        schema: None,
        step_templates: vec![
            StepTemplate {
                name: "process_payment".to_string(),
                description: Some("Process the payment transaction".to_string()),
                dependent_system: Some("payment_gateway".to_string()),
                default_retryable: Some(true),
                default_retry_limit: Some(3),
                skippable: Some(false),
                handler_class: "PaymentProcessorStep".to_string(),
                handler_config: Some(serde_json::json!({
                    "payment_gateway": "stripe",
                    "timeout_seconds": 30
                })),
                depends_on_step: None,
                depends_on_steps: None,
                custom_events: None,
            },
            StepTemplate {
                name: "send_notification".to_string(),
                description: Some("Send payment confirmation notification".to_string()),
                dependent_system: Some("notification_service".to_string()),
                default_retryable: Some(true),
                default_retry_limit: Some(2),
                skippable: Some(true),
                handler_class: "NotificationStep".to_string(),
                handler_config: Some(serde_json::json!({
                    "notification_type": "email",
                    "template": "payment_confirmation"
                })),
                depends_on_step: Some("process_payment".to_string()),
                depends_on_steps: None,
                custom_events: None,
            },
        ],
        environments: None,
        default_context: Some(serde_json::json!({
            "currency": "USD",
            "merchant_id": "demo_merchant"
        })),
        default_options: None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Rust Client Foundation Demo");
    println!("===============================");

    // Create the handler configuration
    let handler_config = create_demo_handler_config();
    println!(
        "✅ Created handler configuration with {} steps",
        handler_config.step_templates.len()
    );

    // Create the base task handler
    let mut task_handler = BaseTaskHandler::from_config(handler_config, "development")?;
    println!("✅ Created base task handler");

    // Add custom step handlers
    task_handler = task_handler
        .with_step_handler(
            "process_payment".to_string(),
            Arc::new(PaymentProcessorStep),
        )?
        .with_step_handler("send_notification".to_string(), Arc::new(NotificationStep))?;
    println!("✅ Registered custom step handlers");

    // Create a task context
    let task_context = tasker_core::client::TaskContext::new(
        12345,
        "payment_workflow".to_string(),
        "payments".to_string(),
        serde_json::json!({
            "amount": 99.99,
            "customer_id": "cust_123",
            "payment_method": "card_456"
        }),
    );
    println!(
        "✅ Created task context for task ID: {}",
        task_context.task_id
    );

    // Execute the task
    println!("\n🔄 Executing payment workflow...");
    println!("================================");

    let result = task_handler.execute_task(task_context).await?;

    // Display results
    println!("\n📊 Execution Results:");
    println!("====================");
    println!("✅ Success: {}", result.success);
    println!("📝 Steps executed: {}", result.steps_executed);
    println!("⏱️  Duration: {:?}", result.duration);

    if let Some(output) = &result.output_data {
        println!("📤 Final output:");
        println!(
            "{}",
            serde_json::to_string_pretty(output)
                .unwrap_or_else(|_| "Failed to serialize output".to_string())
        );
    }

    println!("\n📋 Step Results:");
    for (step_name, step_result) in &result.step_results {
        println!(
            "  {} - Success: {}, Duration: {:?}",
            step_name, step_result.success, step_result.duration
        );
        if let Some(output) = &step_result.output_data {
            println!("    Output: {output}");
        }
    }

    println!("\n🎉 Demo completed successfully!");
    Ok(())
}
