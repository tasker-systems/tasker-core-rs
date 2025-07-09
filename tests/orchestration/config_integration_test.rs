//! Configuration Manager Integration Tests
//!
//! Tests for the complete configuration system with real YAML files.

use std::env;
use tasker_core::orchestration::config::ConfigurationManager;

#[tokio::test]
async fn test_load_system_configuration_from_file() {
    // Test loading the system configuration from YAML file
    let config_path = "config/tasker-config.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(config_path).exists() {
        eprintln!("Skipping test: {config_path} not found");
        return;
    }

    let config_manager = ConfigurationManager::load_from_file(config_path).await;

    match config_manager {
        Ok(manager) => {
            let system_config = manager.system_config();

            // Verify default values match Rails engine
            assert!(!system_config.auth.authentication_enabled);
            assert_eq!(system_config.auth.strategy, "none");
            assert!(!system_config.database.enable_secondary_database);
            assert_eq!(
                system_config.backoff.default_backoff_seconds,
                vec![1, 2, 4, 8, 16, 32]
            );
            assert_eq!(system_config.backoff.max_backoff_seconds, 300);
            assert!(system_config.backoff.jitter_enabled);
            assert_eq!(system_config.execution.max_concurrent_tasks, 100);
            assert_eq!(system_config.execution.max_concurrent_steps, 1000);

            // Verify reenqueue delays
            assert_eq!(
                system_config
                    .backoff
                    .reenqueue_delays
                    .get("has_ready_steps"),
                Some(&0)
            );
            assert_eq!(
                system_config
                    .backoff
                    .reenqueue_delays
                    .get("waiting_for_dependencies"),
                Some(&45)
            );
            assert_eq!(
                system_config.backoff.reenqueue_delays.get("processing"),
                Some(&10)
            );

            println!("✅ System configuration loaded successfully!");
        }
        Err(e) => {
            eprintln!("❌ Failed to load system configuration: {e}");
            panic!("Configuration loading failed");
        }
    }
}

#[tokio::test]
async fn test_load_task_template_from_file() {
    // Test loading a task template from YAML file
    let template_path = "config/tasks/payment_processing.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    let config_manager = ConfigurationManager::new();
    let template = config_manager.load_task_template(template_path).await;

    match template {
        Ok(task_template) => {
            // Verify task metadata
            assert_eq!(task_template.name, "payment_processing/credit_card_payment");
            assert_eq!(
                task_template.module_namespace,
                Some("PaymentProcessing".to_string())
            );
            assert_eq!(task_template.task_handler_class, "CreditCardPaymentHandler");
            assert_eq!(task_template.namespace_name, "payments");
            assert_eq!(task_template.version, "1.0.0");
            assert!(task_template.description.is_some());

            // Verify named steps
            assert_eq!(task_template.named_steps.len(), 5);
            assert_eq!(task_template.named_steps[0], "validate_payment");
            assert_eq!(task_template.named_steps[1], "check_fraud");
            assert_eq!(task_template.named_steps[2], "authorize_payment");
            assert_eq!(task_template.named_steps[3], "capture_payment");
            assert_eq!(task_template.named_steps[4], "send_confirmation");

            // Verify step templates
            assert_eq!(task_template.step_templates.len(), 5);

            // Check step dependencies
            let fraud_step = task_template
                .step_templates
                .iter()
                .find(|s| s.name == "check_fraud")
                .unwrap();
            assert_eq!(
                fraud_step.depends_on_step,
                Some("validate_payment".to_string())
            );

            let authorize_step = task_template
                .step_templates
                .iter()
                .find(|s| s.name == "authorize_payment")
                .unwrap();
            assert_eq!(
                authorize_step.depends_on_steps,
                Some(vec![
                    "validate_payment".to_string(),
                    "check_fraud".to_string()
                ])
            );

            // Verify environment configurations exist
            assert!(task_template.environments.is_some());
            let environments = task_template.environments.as_ref().unwrap();
            assert!(environments.contains_key("development"));
            assert!(environments.contains_key("staging"));
            assert!(environments.contains_key("production"));

            // Verify custom events
            assert!(task_template.custom_events.is_some());
            let custom_events = task_template.custom_events.as_ref().unwrap();
            assert_eq!(custom_events.len(), 3);
            assert_eq!(custom_events[0].name, "payment_authorized");
            assert_eq!(custom_events[1].name, "payment_captured");
            assert_eq!(custom_events[2].name, "fraud_detected");

            println!("✅ Task template loaded successfully!");
        }
        Err(e) => {
            eprintln!("❌ Failed to load task template: {e}");
            panic!("Task template loading failed");
        }
    }
}

#[tokio::test]
async fn test_environment_specific_overrides() {
    // Test environment-specific configuration overrides
    let template_path = "config/tasks/payment_processing.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    // Set environment to development
    env::set_var("TASKER_ENV", "development");

    let config_manager = ConfigurationManager::new();
    let template = config_manager
        .load_task_template(template_path)
        .await
        .unwrap();

    // Find the fraud check step
    let fraud_step = template
        .step_templates
        .iter()
        .find(|s| s.name == "check_fraud")
        .unwrap();

    // Verify development-specific configuration was applied
    if let Some(handler_config) = &fraud_step.handler_config {
        let fraud_service_url = handler_config.get("fraud_service_url").unwrap();
        assert_eq!(
            fraud_service_url.as_str().unwrap(),
            "http://localhost:8080/fraud-check"
        );

        let risk_threshold = handler_config.get("risk_threshold").unwrap();
        assert_eq!(risk_threshold.as_f64().unwrap(), 0.5);

        let debug_mode = handler_config.get("debug_mode").unwrap();
        assert!(debug_mode.as_bool().unwrap());
    }

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

    let config_manager = ConfigurationManager::new();
    let template = config_manager
        .load_task_template_from_yaml(yaml_content)
        .unwrap();

    let test_step = &template.step_templates[0];
    let handler_config = test_step.handler_config.as_ref().unwrap();

    assert_eq!(
        handler_config.get("gateway_url").unwrap().as_str().unwrap(),
        "https://test-gateway.example.com"
    );
    assert_eq!(
        handler_config.get("api_key").unwrap().as_str().unwrap(),
        "test_api_key_123"
    );

    println!("✅ Environment variable interpolation works correctly!");
}

#[tokio::test]
async fn test_task_template_validation() {
    let template_path = "config/tasks/payment_processing.yaml";

    // Check if file exists (skip test if not)
    if !std::path::Path::new(template_path).exists() {
        eprintln!("Skipping test: {template_path} not found");
        return;
    }

    let config_manager = ConfigurationManager::new();
    let template = config_manager
        .load_task_template(template_path)
        .await
        .unwrap();

    // Validate the template
    let validation_result = config_manager.validate_task_template(&template);

    match validation_result {
        Ok(()) => println!("✅ Task template validation passed!"),
        Err(e) => {
            eprintln!("❌ Task template validation failed: {e}");
            panic!("Template validation failed");
        }
    }
}

#[test]
fn test_configuration_builder_pattern() {
    // Test that we can build configuration programmatically
    let config_manager = ConfigurationManager::new();
    let system_config = config_manager.system_config();

    // Verify we can access all configuration sections
    assert!(!system_config.auth.authentication_enabled);
    assert!(!system_config.database.enable_secondary_database);
    assert!(system_config.telemetry.service_name.contains("tasker-core"));
    assert_eq!(system_config.engine.task_handler_directory, "tasks");
    assert!(system_config.health.enabled);
    assert_eq!(system_config.dependency_graph.max_depth, 50);
    assert_eq!(system_config.backoff.default_backoff_seconds.len(), 6);
    assert_eq!(system_config.execution.max_concurrent_tasks, 100);
    assert!(system_config.cache.enabled);

    println!("✅ Configuration builder pattern works correctly!");
}

#[test]
fn test_configuration_defaults_match_rails() {
    // Test that our defaults match the Rails engine defaults
    let config_manager = ConfigurationManager::new();
    let config = config_manager.system_config();

    // Auth defaults
    assert!(!config.auth.authentication_enabled);
    assert_eq!(config.auth.strategy, "none");
    assert_eq!(config.auth.current_user_method, "current_user");
    assert_eq!(config.auth.authenticate_user_method, "authenticate_user!");

    // Database defaults
    assert!(!config.database.enable_secondary_database);
    assert!(config.database.name.is_none());

    // Engine defaults
    assert_eq!(config.engine.task_handler_directory, "tasks");
    assert_eq!(config.engine.task_config_directory, "tasker/tasks");
    assert_eq!(config.engine.identity_strategy, "default");

    // Backoff defaults
    assert_eq!(
        config.backoff.default_backoff_seconds,
        vec![1, 2, 4, 8, 16, 32]
    );
    assert_eq!(config.backoff.max_backoff_seconds, 300);
    assert_eq!(config.backoff.backoff_multiplier, 2.0);
    assert!(config.backoff.jitter_enabled);
    assert_eq!(config.backoff.jitter_max_percentage, 0.1);
    assert_eq!(config.backoff.default_reenqueue_delay, 30);
    assert_eq!(config.backoff.buffer_seconds, 5);

    // Execution defaults
    assert_eq!(config.execution.max_concurrent_tasks, 100);
    assert_eq!(config.execution.max_concurrent_steps, 1000);
    assert_eq!(config.execution.default_timeout_seconds, 3600);
    assert_eq!(config.execution.step_execution_timeout_seconds, 300);

    println!("✅ Configuration defaults match Rails engine!");
}
