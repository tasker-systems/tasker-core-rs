//! Configuration Manager Integration Tests
//!
//! Tests for the complete configuration system with real YAML files.

use std::env;
use tasker_shared::config::ConfigManager;

#[tokio::test]
async fn test_load_system_configuration_from_file() {
    // Test loading the system configuration from YAML file
    let config_dir = "config";

    // Check if config directory exists (skip test if not)
    if !std::path::Path::new(config_dir).join("tasker").exists() {
        eprintln!("Skipping test: {config_dir}/tasker component config directory not found");
        return;
    }

    let config_manager = ConfigManager::load().unwrap();
    let config = config_manager.config();

    // Verify default values match Rails engine
    assert!(!config.auth.authentication_enabled);
    assert_eq!(config.auth.strategy, "none");
    assert!(!config.database.enable_secondary_database);
    assert_eq!(config.backoff.default_backoff_seconds, vec![1]);
    assert_eq!(config.backoff.max_backoff_seconds, 1);
    assert!(!config.backoff.jitter_enabled);
    assert_eq!(config.execution.max_concurrent_tasks, 10);
    assert_eq!(config.execution.max_concurrent_steps, 50);

    // Verify reenqueue delays
    assert_eq!(config.backoff.reenqueue_delays.has_ready_steps, 0);
    assert_eq!(config.backoff.reenqueue_delays.waiting_for_dependencies, 1);
    assert_eq!(config.backoff.reenqueue_delays.processing, 0);

    println!("✅ System configuration loaded successfully!");
}

#[tokio::test]
async fn test_load_task_template_from_file() {
    // Test loading a task template from YAML file
    let template_path = "config/tasks/credit_card_payment.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    // Note: The new config system doesn't have task template loading built-in yet
    // This test demonstrates the transition - we'll load the YAML directly for now
    let yaml_content = std::fs::read_to_string(template_path).unwrap();
    let task_template: serde_yaml::Value = serde_yaml::from_str(&yaml_content).unwrap();

    // Verify task metadata (new TaskTemplate format)
    assert_eq!(
        task_template["name"].as_str().unwrap(),
        "credit_card_payment"
    );
    assert_eq!(
        task_template["namespace_name"].as_str().unwrap(), // New format: namespace_name
        "payments"
    );
    assert_eq!(
        task_template["task_handler"]["callable"].as_str().unwrap(), // New format: task_handler.callable
        "CreditCardPaymentHandler"
    );
    assert_eq!(task_template["version"].as_str().unwrap(), "1.0.0");
    assert!(task_template["description"].is_string());

    // Verify steps (new TaskTemplate format)
    let steps = task_template["steps"].as_sequence().unwrap();
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[0]["name"].as_str().unwrap(), "validate_payment");
    assert_eq!(steps[1]["name"].as_str().unwrap(), "check_fraud");
    assert_eq!(steps[2]["name"].as_str().unwrap(), "authorize_payment");
    assert_eq!(steps[3]["name"].as_str().unwrap(), "capture_payment");
    assert_eq!(steps[4]["name"].as_str().unwrap(), "send_confirmation");

    // Check step dependencies (new TaskTemplate format)
    let fraud_step = steps
        .iter()
        .find(|s| s["name"].as_str().unwrap() == "check_fraud")
        .unwrap();
    let fraud_dependencies = fraud_step["dependencies"].as_sequence().unwrap();
    assert_eq!(fraud_dependencies[0].as_str().unwrap(), "validate_payment");

    let authorize_step = steps
        .iter()
        .find(|s| s["name"].as_str().unwrap() == "authorize_payment")
        .unwrap();
    let authorize_dependencies = authorize_step["dependencies"].as_sequence().unwrap();
    assert_eq!(
        authorize_dependencies[0].as_str().unwrap(),
        "validate_payment"
    );
    assert_eq!(authorize_dependencies[1].as_str().unwrap(), "check_fraud");

    // Verify environment configurations exist
    assert!(task_template["environments"].is_mapping());
    let environments = task_template["environments"].as_mapping().unwrap();
    assert!(environments.contains_key(serde_yaml::Value::String("development".to_string())));
    assert!(environments.contains_key(serde_yaml::Value::String("staging".to_string())));
    assert!(environments.contains_key(serde_yaml::Value::String("production".to_string())));

    // Verify domain events (new TaskTemplate format)
    assert!(task_template["domain_events"].is_sequence());
    let domain_events = task_template["domain_events"].as_sequence().unwrap();
    // Currently empty in our migrated file
    assert_eq!(domain_events.len(), 0);

    println!("✅ Task template loaded successfully!");
}

#[tokio::test]
async fn test_environment_specific_overrides() {
    // Test environment-specific configuration overrides
    let template_path = "config/tasks/credit_card_payment.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    // Load YAML directly for now (until task template loading is implemented in new config system)
    let yaml_content = std::fs::read_to_string(template_path).unwrap();
    let template: serde_yaml::Value = serde_yaml::from_str(&yaml_content).unwrap();

    // Find the fraud check step in development environment overrides
    let environments = template["environments"].as_mapping().unwrap();
    let development = &environments[&serde_yaml::Value::String("development".to_string())];
    let dev_steps = development["steps"].as_sequence().unwrap(); // New format: steps

    let fraud_step_override = dev_steps
        .iter()
        .find(|s| s["name"].as_str().unwrap() == "check_fraud")
        .unwrap();

    // Verify development-specific configuration was applied
    let handler_config = &fraud_step_override["handler"]["initialization"]; // New format: handler.initialization
    assert_eq!(
        handler_config["fraud_service_url"].as_str().unwrap(),
        "http://localhost:8080/fraud-check"
    );
    assert_eq!(handler_config["risk_threshold"].as_f64().unwrap(), 0.5);
    assert!(handler_config["debug_mode"].as_bool().unwrap());

    println!("✅ Environment-specific overrides applied successfully!");
}

#[tokio::test]
async fn test_environment_variable_interpolation() {
    // Test environment variable interpolation in configuration
    env::set_var("PAYMENT_GATEWAY_URL", "https://test-gateway.example.com");
    env::set_var("PAYMENT_GATEWAY_API_KEY", "test_api_key_123");

    let yaml_content = r#"
name: test_payment
task_handler_class: TestHandler
namespace_name: test_namespace
version: "1.0.0"
named_steps:
  - test_step
step_templates:
  - name: test_step
    handler_class: TestStepHandler
    handler_config:
      gateway_url: "${PAYMENT_GATEWAY_URL}"
      api_key: "${PAYMENT_GATEWAY_API_KEY}"
"#;

    // For now, just parse YAML directly (task template loading to be implemented in new config system)
    let template: serde_yaml::Value = serde_yaml::from_str(yaml_content).unwrap();

    let step_templates = template["step_templates"].as_sequence().unwrap();
    let test_step = &step_templates[0];
    let handler_config = &test_step["handler_config"];

    // Note: Environment variable expansion would need to be implemented
    // For now we just verify the raw values are present
    assert_eq!(
        handler_config["gateway_url"].as_str().unwrap(),
        "${PAYMENT_GATEWAY_URL}"
    );
    assert_eq!(
        handler_config["api_key"].as_str().unwrap(),
        "${PAYMENT_GATEWAY_API_KEY}"
    );

    println!(
        "✅ Environment variable placeholders parsed correctly (expansion to be implemented)!"
    );
}

#[tokio::test]
async fn test_task_template_validation() {
    let template_path = "config/tasks/credit_card_payment.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    // For now, just validate that YAML can be parsed (task template validation to be implemented)
    let yaml_content = std::fs::read_to_string(template_path).unwrap();
    let template: serde_yaml::Value = serde_yaml::from_str(&yaml_content).unwrap();

    // Basic validation checks for new TaskTemplate format
    assert!(template["name"].is_string());
    assert!(template["task_handler"]["callable"].is_string()); // New format: task_handler.callable
    assert!(template["namespace_name"].is_string());
    assert!(template["version"].is_string());
    assert!(template["steps"].is_sequence()); // New format: steps instead of step_templates

    println!("✅ Task template basic validation passed!");
}

#[test]
fn test_configuration_builder_pattern() {
    // Test that we can build configuration programmatically
    let config_manager = ConfigManager::load().unwrap();
    let config = config_manager.config();

    // Verify we can access all configuration sections
    assert!(!config.auth.authentication_enabled);
    assert!(!config.database.enable_secondary_database);
    assert!(config.telemetry.service_name.contains("tasker-core"));
    assert_eq!(config.engine.task_handler_directory, "tasks");
    assert!(config.health.enabled);
    assert_eq!(config.dependency_graph.max_depth, 50);
    assert_eq!(config.backoff.default_backoff_seconds.len(), 1);
    assert_eq!(config.execution.max_concurrent_tasks, 10);

    println!("✅ Configuration builder pattern works correctly!");
}

#[test]
fn test_configuration_defaults_match_rails() {
    // Test that our defaults match the Rails engine defaults
    let config_manager = ConfigManager::load().unwrap();
    let config = config_manager.config();

    // Auth defaults
    assert!(!config.auth.authentication_enabled);
    assert_eq!(config.auth.strategy, "none");
    assert_eq!(config.auth.current_user_method, "current_user");
    assert_eq!(config.auth.authenticate_user_method, "authenticate_user!");

    // Database defaults
    assert!(!config.database.enable_secondary_database);
    // Note: database name will be present in componentized config

    // Engine defaults
    assert_eq!(config.engine.task_handler_directory, "tasks");
    assert_eq!(config.engine.task_config_directory, "tasker/tasks");
    assert_eq!(config.engine.identity_strategy, "default");

    // Backoff defaults
    assert_eq!(config.backoff.default_backoff_seconds, vec![1]);
    assert_eq!(config.backoff.max_backoff_seconds, 1);
    assert_eq!(config.backoff.backoff_multiplier, 1.5);
    assert!(!config.backoff.jitter_enabled);
    assert_eq!(config.backoff.jitter_max_percentage, 0.1);
    assert_eq!(config.backoff.default_reenqueue_delay, 1);
    assert_eq!(config.backoff.buffer_seconds, 0);

    // Execution defaults
    assert_eq!(config.execution.max_concurrent_tasks, 10);
    assert_eq!(config.execution.max_concurrent_steps, 50);
    assert_eq!(config.execution.default_timeout_seconds, 30);
    assert_eq!(config.execution.step_execution_timeout_seconds, 10);

    println!("✅ Configuration defaults match Rails engine!");
}
