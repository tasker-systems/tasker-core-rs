//! BaseStepHandler Integration Tests
//!
//! Tests for the complete step handler system with configuration integration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tasker_core::orchestration::config::StepTemplate;
use tasker_core::orchestration::{
    BaseStepHandler, ConfigurationManager, OrchestrationResult, StepExecutionContext, StepHandler,
    StepHandlerExecutor, StepHandlerFactory, StepResult,
};

/// Custom step handler implementation for testing
struct TestStepHandler {
    should_fail: bool,
    output_data: serde_json::Value,
    execution_delay: Duration,
}

#[async_trait::async_trait]
impl StepHandler for TestStepHandler {
    async fn process(
        &self,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value> {
        // Simulate processing delay
        if !self.execution_delay.is_zero() {
            tokio::time::sleep(self.execution_delay).await;
        }

        if self.should_fail {
            Err(
                tasker_core::orchestration::OrchestrationError::StepExecutionFailed {
                    step_id: context.step_id,
                    task_id: context.task_id,
                    reason: "Test failure".to_string(),
                    error_code: Some("TEST_ERROR".to_string()),
                    retry_after: Some(Duration::from_secs(1)),
                },
            )
        } else {
            Ok(self.output_data.clone())
        }
    }

    async fn process_results(
        &self,
        context: &StepExecutionContext,
        result: &StepResult,
    ) -> OrchestrationResult<()> {
        println!(
            "process_results called for step: {} with success: {}",
            context.step_name, result.success
        );
        Ok(())
    }
}

#[tokio::test]
async fn test_step_handler_successful_execution() {
    let config_manager = Arc::new(ConfigurationManager::new());

    // Create a test step template
    let step_template = StepTemplate {
        name: "test_validation_step".to_string(),
        description: Some("Test validation step".to_string()),
        handler_class: "TestValidationHandler".to_string(),
        handler_config: Some({
            let mut config = HashMap::new();
            config.insert(
                "validation_rules".to_string(),
                serde_json::json!(["check_required_fields", "validate_format"]),
            );
            config
        }),
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(true),
        default_retry_limit: Some(3),
        timeout_seconds: Some(30),
        retry_backoff: None,
    };

    // Create step handler
    let mut step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    // Set custom handler implementation
    let custom_handler = TestStepHandler {
        should_fail: false,
        output_data: serde_json::json!({
            "validation_result": "passed",
            "validated_fields": ["email", "name", "amount"]
        }),
        execution_delay: Duration::from_millis(100),
    };
    step_handler.set_custom_handler(Box::new(custom_handler));

    // Create execution context
    let context = StepExecutionContext {
        step_id: 123,
        task_id: 456,
        step_name: "test_validation_step".to_string(),
        input_data: serde_json::json!({
            "email": "test@example.com",
            "name": "Test User",
            "amount": 100.0
        }),
        previous_steps: vec![],
        step_config: HashMap::new(),
        attempt_number: 1,
        max_retry_attempts: 3,
        timeout_seconds: 30,
        is_retryable: true,
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    // Execute step
    let result = step_handler.execute_step(context).await.unwrap();

    // Verify successful execution
    assert!(result.success);
    assert!(result.output_data.is_some());
    assert!(result.error.is_none());
    assert!(!result.should_retry);

    let output = result.output_data.unwrap();
    assert_eq!(output["validation_result"], "passed");
    assert!(output["validated_fields"].is_array());

    println!(
        "✅ Step executed successfully in {:?}",
        result.execution_duration
    );
}

#[tokio::test]
async fn test_step_handler_retry_logic() {
    let config_manager = Arc::new(ConfigurationManager::new());

    let step_template = StepTemplate {
        name: "test_retry_step".to_string(),
        description: Some("Test retry step".to_string()),
        handler_class: "TestRetryHandler".to_string(),
        handler_config: None,
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(true),
        default_retry_limit: Some(3),
        timeout_seconds: Some(10),
        retry_backoff: None,
    };

    let mut step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    // Set custom handler that fails
    let custom_handler = TestStepHandler {
        should_fail: true,
        output_data: serde_json::Value::Null,
        execution_delay: Duration::from_millis(50),
    };
    step_handler.set_custom_handler(Box::new(custom_handler));

    let context = StepExecutionContext {
        step_id: 124,
        task_id: 456,
        step_name: "test_retry_step".to_string(),
        input_data: serde_json::json!({"test": "data"}),
        previous_steps: vec![],
        step_config: HashMap::new(),
        attempt_number: 1,
        max_retry_attempts: 3,
        timeout_seconds: 10,
        is_retryable: true,
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    let result = step_handler.execute_step(context).await.unwrap();

    // Verify retry behavior
    assert!(!result.success);
    assert!(result.should_retry);
    assert!(result.retry_delay.is_some());
    assert!(result.error.is_some());

    println!(
        "✅ Step retry logic working correctly with delay: {:?}s",
        result.retry_delay
    );
}

#[tokio::test]
async fn test_step_handler_timeout() {
    let config_manager = Arc::new(ConfigurationManager::new());

    let step_template = StepTemplate {
        name: "test_timeout_step".to_string(),
        description: Some("Test timeout step".to_string()),
        handler_class: "TestTimeoutHandler".to_string(),
        handler_config: None,
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(true),
        default_retry_limit: Some(3),
        timeout_seconds: Some(1), // Very short timeout
        retry_backoff: None,
    };

    let mut step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    // Set custom handler with long delay
    let custom_handler = TestStepHandler {
        should_fail: false,
        output_data: serde_json::json!({"result": "should_timeout"}),
        execution_delay: Duration::from_secs(2), // Longer than timeout
    };
    step_handler.set_custom_handler(Box::new(custom_handler));

    let context = StepExecutionContext {
        step_id: 125,
        task_id: 456,
        step_name: "test_timeout_step".to_string(),
        input_data: serde_json::json!({"test": "data"}),
        previous_steps: vec![],
        step_config: HashMap::new(),
        attempt_number: 1,
        max_retry_attempts: 3,
        timeout_seconds: 1,
        is_retryable: true,
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    let result = step_handler.execute_step(context).await.unwrap();

    // Verify timeout behavior
    assert!(!result.success);
    assert!(result.should_retry);
    assert!(result.error.is_some());

    println!("✅ Step timeout handling working correctly");
}

#[tokio::test]
async fn test_step_handler_factory() {
    let config_manager = Arc::new(ConfigurationManager::new());
    let factory = StepHandlerFactory::new(Arc::clone(&config_manager));

    let step_template = StepTemplate {
        name: "factory_test_step".to_string(),
        description: Some("Factory test step".to_string()),
        handler_class: "FactoryTestHandler".to_string(),
        handler_config: Some({
            let mut config = HashMap::new();
            config.insert(
                "factory_config".to_string(),
                serde_json::json!("test_value"),
            );
            config
        }),
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(false),
        default_retry_limit: Some(1),
        timeout_seconds: Some(60),
        retry_backoff: None,
    };

    let step_handler = factory.create_handler(step_template);

    assert_eq!(step_handler.step_name(), "factory_test_step");
    assert_eq!(step_handler.handler_class(), "FactoryTestHandler");

    println!("✅ Step handler factory working correctly");
}

#[tokio::test]
async fn test_step_handler_configuration_integration() {
    // Test that step handler properly applies template configuration
    let config_manager = Arc::new(ConfigurationManager::new());

    let step_template = StepTemplate {
        name: "config_integration_step".to_string(),
        description: Some("Configuration integration test".to_string()),
        handler_class: "ConfigIntegrationHandler".to_string(),
        handler_config: Some({
            let mut config = HashMap::new();
            config.insert(
                "service_url".to_string(),
                serde_json::json!("http://localhost:8080"),
            );
            config.insert("timeout_ms".to_string(), serde_json::json!(5000));
            config.insert("retry_enabled".to_string(), serde_json::json!(true));
            config
        }),
        depends_on_step: None,
        depends_on_steps: None,
        default_retryable: Some(true),
        default_retry_limit: Some(5),
        timeout_seconds: Some(120),
        retry_backoff: Some("exponential".to_string()),
    };

    let step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    let context = StepExecutionContext {
        step_id: 126,
        task_id: 456,
        step_name: "config_integration_step".to_string(),
        input_data: serde_json::json!({"request": "test"}),
        previous_steps: vec![],
        step_config: HashMap::new(),
        attempt_number: 1,
        max_retry_attempts: 3, // Should be overridden by template
        timeout_seconds: 60,   // Should be overridden by template
        is_retryable: false,   // Should be overridden by template
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    // Execute with default implementation (no custom handler)
    let result = step_handler.execute_step(context).await.unwrap();

    // Should succeed with default implementation
    assert!(result.success);
    assert!(result.output_data.is_some());

    // Verify that input data is passed through by default implementation
    let output = result.output_data.unwrap();
    assert_eq!(output["request"], "test");

    println!("✅ Configuration integration working correctly");
}

#[test]
fn test_step_handler_component_access() {
    let config_manager = Arc::new(ConfigurationManager::new());

    let step_template = StepTemplate {
        name: "component_test_step".to_string(),
        description: Some("Component access test".to_string()),
        handler_class: "ComponentTestHandler".to_string(),
        handler_config: None,
        depends_on_step: Some("previous_step".to_string()),
        depends_on_steps: Some(vec!["step1".to_string(), "step2".to_string()]),
        default_retryable: Some(true),
        default_retry_limit: Some(3),
        timeout_seconds: Some(300),
        retry_backoff: None,
    };

    let step_handler =
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&config_manager));

    // Test component access methods
    assert_eq!(step_handler.step_name(), "component_test_step");
    assert_eq!(step_handler.handler_class(), "ComponentTestHandler");

    let template = step_handler.step_template();
    assert_eq!(template.name, "component_test_step");
    assert_eq!(template.depends_on_step, Some("previous_step".to_string()));
    assert_eq!(
        template.depends_on_steps,
        Some(vec!["step1".to_string(), "step2".to_string()])
    );
    assert_eq!(template.default_retry_limit, Some(3));
    assert_eq!(template.timeout_seconds, Some(300));

    println!("✅ Step handler component access working correctly");
}
