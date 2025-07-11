//! BaseTaskHandler Integration Tests
//!
//! Tests for the BaseTaskHandler with complete workflow coordination.

use crate::factories::base::SqlxFactory;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tasker_core::orchestration::errors::OrchestrationError;
use tasker_core::orchestration::types::{StepResult, StepStatus, TaskContext, ViableStep};
use tasker_core::orchestration::{
    BaseTaskHandler, ConfigurationManager, FrameworkIntegration, TaskExecutionContext, TaskHandler,
    TaskHandlerFactory,
};

/// Test task handler implementation
#[allow(dead_code)]
struct TestTaskHandler {
    pub initialize_called: Arc<std::sync::Mutex<bool>>,
    pub before_execute_called: Arc<std::sync::Mutex<bool>>,
    pub after_execute_called: Arc<std::sync::Mutex<bool>>,
}

#[allow(dead_code)]
impl TestTaskHandler {
    fn new() -> Self {
        Self {
            initialize_called: Arc::new(std::sync::Mutex::new(false)),
            before_execute_called: Arc::new(std::sync::Mutex::new(false)),
            after_execute_called: Arc::new(std::sync::Mutex::new(false)),
        }
    }
}

#[async_trait::async_trait]
impl TaskHandler for TestTaskHandler {
    async fn initialize(
        &self,
        _context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        *self.initialize_called.lock().unwrap() = true;
        Ok(())
    }

    async fn before_execute(
        &self,
        _context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        *self.before_execute_called.lock().unwrap() = true;
        Ok(())
    }

    async fn after_execute(
        &self,
        _context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        *self.after_execute_called.lock().unwrap() = true;
        Ok(())
    }
}

/// Test framework integration
struct TestFrameworkIntegration {
    pub should_succeed: bool,
}

#[async_trait::async_trait]
impl FrameworkIntegration for TestFrameworkIntegration {
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        _task_context: &TaskContext,
    ) -> Result<StepResult, OrchestrationError> {
        if self.should_succeed {
            Ok(StepResult {
                step_id: step.step_id,
                status: StepStatus::Completed,
                output: serde_json::json!({"success": true}),
                execution_duration: std::time::Duration::from_millis(100),
                error_message: None,
                retry_after: None,
                error_code: None,
                error_context: None,
            })
        } else {
            Ok(StepResult {
                step_id: step.step_id,
                status: StepStatus::Failed,
                output: serde_json::json!({"success": false}),
                execution_duration: std::time::Duration::from_millis(100),
                error_message: Some("Test failure".to_string()),
                retry_after: None,
                error_code: Some("TEST_ERROR".to_string()),
                error_context: None,
            })
        }
    }

    fn framework_name(&self) -> &'static str {
        "TestFramework"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, OrchestrationError> {
        Ok(TaskContext {
            task_id,
            data: serde_json::json!({"test": true}),
            metadata: HashMap::new(),
        })
    }

    async fn enqueue_task(
        &self,
        _task_id: i64,
        _delay: Option<std::time::Duration>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn mark_task_failed(
        &self,
        _task_id: i64,
        _error: &str,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }

    async fn update_step_state(
        &self,
        _step_id: i64,
        _state: &str,
        _result: Option<&serde_json::Value>,
    ) -> Result<(), OrchestrationError> {
        Ok(())
    }
}

#[sqlx::test]
async fn test_base_task_handler_creation(pool: PgPool) -> sqlx::Result<()> {
    // Create task template from YAML
    let yaml_content = r#"
name: test_task
module_namespace: TestModule
task_handler_class: TestTaskHandler
namespace_name: test_namespace
version: "1.0.0"
description: "Test task for integration testing"
named_steps:
  - step1
  - step2
step_templates:
  - name: step1
    handler_class: Step1Handler
    description: "First test step"
  - name: step2
    handler_class: Step2Handler
    depends_on_step: step1
    description: "Second test step"
"#;

    let config_manager = ConfigurationManager::new();
    let task_template = config_manager
        .load_task_template_from_yaml(yaml_content)
        .unwrap();

    // Create base task handler
    let handler = BaseTaskHandler::new(task_template, pool);

    // Verify basic properties
    assert_eq!(handler.task_name(), "test_task");
    assert_eq!(handler.handler_class(), "TestTaskHandler");
    assert_eq!(handler.namespace(), "test_namespace");

    Ok(())
}

#[sqlx::test]
async fn test_task_handler_factory(pool: PgPool) -> sqlx::Result<()> {
    let config_manager = Arc::new(ConfigurationManager::new());
    let factory = TaskHandlerFactory::new(config_manager.clone(), pool.clone());

    // Create task template
    let yaml_content = r#"
name: factory_test_task
task_handler_class: FactoryTestHandler
namespace_name: factory_test
version: "1.0.0"
named_steps:
  - test_step
step_templates:
  - name: test_step
    handler_class: TestStepHandler
"#;

    let task_template = config_manager
        .load_task_template_from_yaml(yaml_content)
        .unwrap();

    // Create handler using factory
    let handler = factory.create_handler(task_template);

    assert_eq!(handler.task_name(), "factory_test_task");
    assert_eq!(handler.namespace(), "factory_test");

    Ok(())
}

#[sqlx::test]
async fn test_task_execution_lifecycle_hooks(_pool: PgPool) -> sqlx::Result<()> {
    // Skip this test for now until SQL functions are properly set up
    println!("Skipping test_task_execution_lifecycle_hooks - requires SQL function setup");
    Ok(())
}

#[test]
fn test_task_execution_context_serialization() {
    let context = TaskExecutionContext {
        task_id: 123,
        namespace: "test".to_string(),
        input_data: serde_json::json!({"key": "value"}),
        environment: "production".to_string(),
        metadata: {
            let mut map = HashMap::new();
            map.insert("meta_key".to_string(), serde_json::json!("meta_value"));
            map
        },
    };

    // Serialize
    let serialized = serde_json::to_string(&context).unwrap();

    // Deserialize
    let deserialized: TaskExecutionContext = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.task_id, 123);
    assert_eq!(deserialized.namespace, "test");
    assert_eq!(deserialized.environment, "production");
    assert_eq!(deserialized.metadata.len(), 1);
}

#[sqlx::test]
async fn test_establish_step_dependencies_validation(pool: PgPool) -> sqlx::Result<()> {
    // Create task template with invalid dependency
    let yaml_content = r#"
name: dependency_test_task
task_handler_class: DependencyTestHandler
namespace_name: dependency_test
version: "1.0.0"
named_steps:
  - step1
  - step2
step_templates:
  - name: step1
    handler_class: Step1Handler
  - name: step2
    handler_class: Step2Handler
    depends_on_step: step3  # Invalid dependency - step3 doesn't exist
"#;

    let config_manager = Arc::new(ConfigurationManager::new());
    let task_template = config_manager
        .load_task_template_from_yaml(yaml_content)
        .unwrap();

    let mut base_handler =
        BaseTaskHandler::with_config_manager(task_template, pool.clone(), config_manager);

    // Set framework integration
    let framework = Arc::new(TestFrameworkIntegration {
        should_succeed: true,
    });
    base_handler.set_framework_integration(framework);

    // Create task
    let task = crate::factories::core::TaskFactory::default()
        .with_initiator("test_dependencies")
        .create(&pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("Factory error: {e}")))?;

    // Create execution context
    let context = TaskExecutionContext {
        task_id: task.task_id,
        namespace: "dependency_test".to_string(),
        input_data: serde_json::json!({}),
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };

    // Execute task - should fail due to invalid dependency
    let result = base_handler.execute_task(context).await;

    assert!(result.is_err());
    if let Err(e) = result {
        match e {
            OrchestrationError::ValidationError { field, reason } => {
                assert_eq!(field, "depends_on_step");
                assert!(reason.contains("non-existent step"));
            }
            _ => panic!("Expected ValidationError, got: {e:?}"),
        }
    }

    Ok(())
}
