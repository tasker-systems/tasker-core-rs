//! # Task Configuration Finder Demo
//!
//! This example demonstrates how to use the TaskConfigFinder to discover task
//! configurations from both the registry and file system.

use std::sync::Arc;
use tasker_core::models::core::task_template::{StepTemplate, TaskTemplate};
use tasker_core::orchestration::config::ConfigurationManager;
use tasker_core::orchestration::task_config_finder::TaskConfigFinder;
use tasker_core::registry::TaskHandlerRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🔍 Task Configuration Finder Demo");
    println!("==================================\n");

    // Create the configuration manager and registry
    let config_manager = Arc::new(ConfigurationManager::new());
    let registry = Arc::new(TaskHandlerRegistry::new());

    // Create the task configuration finder
    let task_config_finder = TaskConfigFinder::new(config_manager, registry.clone());

    // Demo 1: Register a task template in the registry
    println!("📝 Demo 1: Registering task template in registry");
    let template = create_sample_task_template();

    registry
        .register_task_template("payments", "order_processing", "1.0.0", template.clone())
        .await?;

    println!("✅ Registered task template: {}", template.name);

    // Demo 2: Find the template from the registry
    println!("\n🔍 Demo 2: Finding template from registry");
    match task_config_finder
        .find_task_template("payments", "order_processing", "1.0.0")
        .await
    {
        Ok(found_template) => {
            println!("✅ Found template in registry: {}", found_template.name);
            println!("   - Handler class: {}", found_template.task_handler_class);
            println!(
                "   - Step templates: {}",
                found_template.step_templates.len()
            );
        }
        Err(e) => {
            println!("❌ Failed to find template: {e}");
        }
    }

    // Demo 3: Try to find a non-existent template (will check file system)
    println!("\n🔍 Demo 3: Searching for non-existent template (file system fallback)");
    match task_config_finder
        .find_task_template("ecommerce", "cart_processing", "2.0.0")
        .await
    {
        Ok(found_template) => {
            println!("✅ Found template in file system: {}", found_template.name);
        }
        Err(e) => {
            println!("ℹ️  Template not found (expected): {e}");
        }
    }

    // Demo 4: Show search paths
    println!("\n📂 Demo 4: Search paths used by TaskConfigFinder");
    println!("   The finder searches in this order:");
    println!("   1. Registry (fast lookup)");
    println!("   2. File system paths:");
    println!("      - config/tasker/tasks/{{namespace}}/{{name}}/{{version}}.yaml");
    println!("      - config/tasker/tasks/{{name}}/{{version}}.yaml");
    println!("      - config/tasker/tasks/{{name}}.yaml");

    // Demo 5: List templates in registry
    println!("\n📋 Demo 5: List templates in registry");
    match task_config_finder.list_registry_templates(Some("payments")) {
        Ok(templates) => {
            println!(
                "✅ Found {} templates in 'payments' namespace:",
                templates.len()
            );
            for template in templates {
                println!("   - {template}");
            }
        }
        Err(e) => {
            println!("❌ Failed to list templates: {e}");
        }
    }

    println!("\n🎉 Demo completed successfully!");
    Ok(())
}

fn create_sample_task_template() -> TaskTemplate {
    TaskTemplate {
        name: "payments/order_processing".to_string(),
        module_namespace: Some("Payments".to_string()),
        task_handler_class: "OrderProcessingHandler".to_string(),
        namespace_name: "payments".to_string(),
        version: "1.0.0".to_string(),
        default_dependent_system: Some("payment_gateway".to_string()),
        named_steps: vec![
            "validate_payment".to_string(),
            "process_payment".to_string(),
            "send_confirmation".to_string(),
        ],
        schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "amount": {"type": "number"},
                "currency": {"type": "string"},
                "payment_method": {"type": "string"}
            },
            "required": ["amount", "currency", "payment_method"]
        })),
        step_templates: vec![
            StepTemplate {
                name: "validate_payment".to_string(),
                description: Some("Validate payment details".to_string()),
                dependent_system: Some("payment_gateway".to_string()),
                default_retryable: Some(true),
                default_retry_limit: Some(3),
                skippable: Some(false),
                handler_class: "ValidatePaymentStepHandler".to_string(),
                handler_config: Some(serde_json::json!({
                    "timeout_seconds": 30,
                    "validation_strict": true
                })),
                depends_on_step: None,
                depends_on_steps: None,
                custom_events: None,
            },
            StepTemplate {
                name: "process_payment".to_string(),
                description: Some("Process the payment transaction".to_string()),
                dependent_system: Some("payment_gateway".to_string()),
                default_retryable: Some(true),
                default_retry_limit: Some(2),
                skippable: Some(false),
                handler_class: "ProcessPaymentStepHandler".to_string(),
                handler_config: Some(serde_json::json!({
                    "timeout_seconds": 60,
                    "max_amount": 10000
                })),
                depends_on_step: Some("validate_payment".to_string()),
                depends_on_steps: None,
                custom_events: None,
            },
            StepTemplate {
                name: "send_confirmation".to_string(),
                description: Some("Send payment confirmation to customer".to_string()),
                dependent_system: Some("notification_service".to_string()),
                default_retryable: Some(true),
                default_retry_limit: Some(5),
                skippable: Some(true),
                handler_class: "SendConfirmationStepHandler".to_string(),
                handler_config: Some(serde_json::json!({
                    "timeout_seconds": 15,
                    "notification_type": "email"
                })),
                depends_on_step: Some("process_payment".to_string()),
                depends_on_steps: None,
                custom_events: None,
            },
        ],
        environments: None,
        default_context: Some(serde_json::json!({
            "environment": "production",
            "region": "us-east-1"
        })),
        default_options: Some(std::collections::HashMap::from([
            ("max_retries".to_string(), serde_json::json!(3)),
            ("timeout_seconds".to_string(), serde_json::json!(300)),
        ])),
    }
}
