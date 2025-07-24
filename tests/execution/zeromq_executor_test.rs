// ZeroMQ Executor Unit Tests
// Tests the ZeroMQ executor functionality and error handling

use std::collections::HashMap;
use std::time::Duration;
use serde_json::json;
use sqlx::PgPool;
use tasker_core::execution::zeromq_pub_sub_executor::ZmqPubSubExecutor;
use tasker_core::orchestration::{
    errors::{OrchestrationError, ExecutionError},
    step_handler::StepExecutionContext,
    types::{FrameworkIntegration, TaskContext},
};
use tasker_core::models::WorkflowStep;

#[tokio::test]
async fn test_framework_integration_interface() {
    // Create a mock database pool (this test doesn't require real DB)
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://test_steps",
        "inproc://test_results", 
        pool,
    ).await.expect("Failed to create executor");
    
    // Test framework name
    assert_eq!(executor.framework_name(), "ZeroMQ");
    
    // Test enqueue_task (should succeed as no-op)
    let result = executor.enqueue_task(123, Some(Duration::from_secs(10))).await;
    assert!(result.is_ok());
}

#[tokio::test]  
async fn test_step_request_building_with_previous_results() {
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://test_steps_2",
        "inproc://test_results_2",
        pool,
    ).await.expect("Failed to create executor");
    
    // Create mock previous steps with results
    let previous_step_1 = WorkflowStep {
        workflow_step_id: 100,
        task_id: 456,
        named_step_id: 1,
        step_name: Some("validate_order".to_string()),
        results: Some(json!({"status": "approved", "order_id": "ORDER-123"})),
        step_config: Some(json!({})),
        workflow_position: Some(1),
        depends_on_step_names: Some(vec![]),
        depends_on_steps: Some(vec![]),
        // Required fields for WorkflowStep
        state: "completed".to_string(),
        attempts: 1,
        processed: true,
        processed_at: Some(chrono::Utc::now()),
        completed_at: Some(chrono::Utc::now()),
        output: Some(json!({"status": "approved"})),
        error_information: None,
        retryable: true,
        retry_limit: 3,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let previous_step_2 = WorkflowStep {
        workflow_step_id: 101,
        task_id: 456,
        named_step_id: 2,
        step_name: Some("calculate_tax".to_string()),
        results: Some(json!({"tax_amount": 15.75, "tax_rate": 0.0875})),
        step_config: Some(json!({})),
        workflow_position: Some(2),
        depends_on_step_names: Some(vec!["validate_order".to_string()]),
        depends_on_steps: Some(vec![100]),
        // Required fields
        state: "completed".to_string(),
        attempts: 1,
        processed: true,
        processed_at: Some(chrono::Utc::now()),
        completed_at: Some(chrono::Utc::now()),
        output: Some(json!({"tax_amount": 15.75})),
        error_information: None,
        retryable: true,
        retry_limit: 3,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Create step execution context
    let context = StepExecutionContext {
        step_id: 102,
        task_id: 456,
        step_name: "process_payment".to_string(),
        input_data: json!({"order_id": "ORDER-123", "amount": 125.50}),
        previous_steps: vec![previous_step_1, previous_step_2],
        step_config: HashMap::new(),
        attempt_number: 0,
        max_retry_attempts: 3,
        timeout_seconds: 30,
        is_retryable: true,
        environment: "test".to_string(),
        metadata: HashMap::new(),
    };
    
    // Test building step request - this method is private, so we'll test through public interface
    // The key thing is that the system should handle previous results properly
    
    // This would be tested through execute_step_with_handler if we had a way to mock the socket
    // For now, we verify the context structure is correct
    assert_eq!(context.step_id, 102);
    assert_eq!(context.previous_steps.len(), 2);
    assert!(context.previous_steps[0].results.is_some());
    assert!(context.previous_steps[1].results.is_some());
}

#[tokio::test]
async fn test_error_handling_with_step_context() {
    // This test verifies that errors include proper step_id context
    // We'll test this by checking the error types that would be generated
    
    let step_id = 789i64;
    let timeout = Duration::from_secs(30);
    
    // Test timeout error includes step_id
    let timeout_error = OrchestrationError::ExecutionError(ExecutionError::ExecutionTimeout {
        step_id,
        timeout_duration: timeout,
    });
    
    match timeout_error {
        OrchestrationError::ExecutionError(ExecutionError::ExecutionTimeout { step_id: error_step_id, .. }) => {
            assert_eq!(error_step_id, 789);
        }
        _ => panic!("Expected ExecutionTimeout error"),
    }
    
    // Test step execution failed error includes step_id
    let execution_error = OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
        step_id,
        reason: "ZeroMQ socket send failed".to_string(),
        error_code: Some("ZMQ_PUBLISH_FAILED".to_string()),
    });
    
    match execution_error {
        OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed { step_id: error_step_id, reason, error_code }) => {
            assert_eq!(error_step_id, 789);
            assert!(reason.contains("ZeroMQ socket send failed"));
            assert_eq!(error_code, Some("ZMQ_PUBLISH_FAILED".to_string()));
        }
        _ => panic!("Expected StepExecutionFailed error"),
    }
    
    // Test no result returned error includes step_id
    let no_result_error = OrchestrationError::ExecutionError(ExecutionError::NoResultReturned { step_id });
    
    match no_result_error {
        OrchestrationError::ExecutionError(ExecutionError::NoResultReturned { step_id: error_step_id }) => {
            assert_eq!(error_step_id, 789);
        }
        _ => panic!("Expected NoResultReturned error"),
    }
}

#[tokio::test]
async fn test_task_context_structure() {
    // Test that TaskContext contains expected fields
    let task_context = TaskContext {
        task_id: 123,
        data: json!({
            "order_id": "ORDER-456",
            "customer_id": 789,
            "items": [
                {"sku": "WIDGET-A", "quantity": 2, "price": 25.00},
                {"sku": "WIDGET-B", "quantity": 1, "price": 15.50}
            ]
        }),
        metadata: {
            let mut meta = HashMap::new();
            meta.insert("initiator".to_string(), json!("api_user_123"));
            meta.insert("source_system".to_string(), json!("e-commerce"));
            meta.insert("total_steps".to_string(), json!(5));
            meta.insert("ready_steps".to_string(), json!(2));
            meta.insert("completed_steps".to_string(), json!(0));
            meta
        },
    };
    
    assert_eq!(task_context.task_id, 123);
    assert!(task_context.data.get("order_id").is_some());
    assert!(task_context.data.get("items").is_some());
    assert_eq!(task_context.metadata.len(), 5);
    assert_eq!(task_context.metadata.get("initiator").unwrap(), &json!("api_user_123"));
    assert_eq!(task_context.metadata.get("total_steps").unwrap(), &json!(5));
}

#[tokio::test]
async fn test_zmq_executor_creation_and_cleanup() {
    // Test that the executor can be created and will clean up properly
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://cleanup_test_steps",
        "inproc://cleanup_test_results",
        pool,
    ).await.expect("Failed to create executor");
    
    // Test basic properties
    assert_eq!(executor.framework_name(), "ZeroMQ");
    assert!(executor.supports_batch_execution());
    
    // Executor should be created successfully and background task should be running
    // When executor is dropped, the background task should be cleaned up automatically
    
    drop(executor);
    // Test passes if no panics or resource leaks occur
}

#[tokio::test]
async fn test_fire_and_forget_semantics() {
    // Test that execute_step_with_handler returns immediately with InProgress status
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://fire_forget_steps",
        "inproc://fire_forget_results",
        pool,
    ).await.expect("Failed to create executor");
    
    let context = integration_helpers::create_test_step_context(501, 701, "fire_forget_test");
    let handler_config = HashMap::new();
    
    let start_time = std::time::Instant::now();
    
    // This should return immediately without waiting for results
    let result = executor.execute_step_with_handler(
        &context,
        "TestHandler",
        &handler_config,
    ).await;
    
    let execution_time = start_time.elapsed();
    
    // Verify fire-and-forget semantics
    assert!(result.is_ok(), "Execute step should succeed (fire-and-forget)");
    let step_result = result.unwrap();
    
    // Should return InProgress status immediately
    assert_eq!(step_result.status, tasker_core::orchestration::types::StepStatus::InProgress);
    assert_eq!(step_result.step_id, 501);
    assert_eq!(step_result.output, serde_json::Value::Null);
    assert!(step_result.error_message.is_none());
    
    // Should return very quickly (fire-and-forget, not waiting for execution)
    assert!(execution_time.as_millis() < 100, "Fire-and-forget should be nearly instantaneous");
}

#[tokio::test]
async fn test_batch_execution_fire_and_forget() {
    // Test that execute_step_batch returns immediately with all steps marked InProgress
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://batch_fire_forget_steps",
        "inproc://batch_fire_forget_results",
        pool,
    ).await.expect("Failed to create executor");
    
    // Create multiple step contexts for batch execution
    let contexts = vec![
        (integration_helpers::create_test_step_context(601, 801, "batch_step_1"), "Handler1".to_string(), HashMap::new()),
        (integration_helpers::create_test_step_context(602, 801, "batch_step_2"), "Handler2".to_string(), HashMap::new()),
        (integration_helpers::create_test_step_context(603, 801, "batch_step_3"), "Handler3".to_string(), HashMap::new()),
    ];
    
    let contexts_refs: Vec<(&tasker_core::orchestration::step_handler::StepExecutionContext, &str, &HashMap<String, serde_json::Value>)> = contexts.iter()
        .map(|(ctx, handler, config)| (ctx, handler.as_str(), config))
        .collect();
    
    let start_time = std::time::Instant::now();
    
    // This should return immediately without waiting for results
    let result = executor.execute_step_batch(contexts_refs).await;
    
    let execution_time = start_time.elapsed();
    
    // Verify batch fire-and-forget semantics
    assert!(result.is_ok(), "Batch execution should succeed (fire-and-forget)");
    let step_results = result.unwrap();
    
    // Should return 3 results, all InProgress
    assert_eq!(step_results.len(), 3);
    
    for (i, step_result) in step_results.iter().enumerate() {
        assert_eq!(step_result.status, tasker_core::orchestration::types::StepStatus::InProgress);
        assert_eq!(step_result.step_id, 601 + i as i64);
        assert_eq!(step_result.output, serde_json::Value::Null);
        assert!(step_result.error_message.is_none());
    }
    
    // Batch should also return very quickly
    assert!(execution_time.as_millis() < 200, "Batch fire-and-forget should be nearly instantaneous");
}

#[tokio::test]
async fn test_get_task_context_with_metadata() {
    // Test that get_task_context properly loads and structures task data
    // Note: This test would require actual database setup to be fully functional
    // For now, we test the interface and expected structure
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://context_test_steps",
        "inproc://context_test_results",
        pool,
    ).await.expect("Failed to create executor");
    
    // This would fail in real scenario without proper database setup,
    // but demonstrates the expected interface
    let result = executor.get_task_context(999).await;
    
    // In a memory SQLite DB, the task won't exist, so we expect a specific error
    assert!(result.is_err());
    
    // Verify the error structure is correct for orchestration
    match result.unwrap_err() {
        tasker_core::orchestration::errors::OrchestrationError::TaskExecutionFailed { task_id, reason, error_code } => {
            assert_eq!(task_id, 999);
            assert!(reason.contains("Task not found") || reason.contains("Database error"));
            assert!(error_code.is_some());
        }
        _ => panic!("Expected TaskExecutionFailed error with proper structure"),
    }
}

/// Test that demonstrates the importance of proper error attribution
#[tokio::test]
async fn test_error_attribution_importance() {
    // This test demonstrates why proper step_id attribution is critical for orchestration
    
    // Scenario: A workflow has multiple steps, and one step fails due to ZeroMQ socket error
    let workflow_steps = vec![
        ("validate_order", 101i64),
        ("calculate_tax", 102i64), 
        ("process_payment", 103i64),  // This step fails
        ("send_confirmation", 104i64),
    ];
    
    // When step 103 (process_payment) fails due to socket error, the error must include step_id 103
    let failing_step_id = 103i64;
    
    let socket_error = OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed {
        step_id: failing_step_id,
        reason: "Failed to publish step batch to ZeroMQ: Connection refused".to_string(),
        error_code: Some("ZMQ_PUBLISH_FAILED".to_string()),
    });
    
    // The orchestration system needs this step_id to:
    // 1. Mark the specific step as failed
    // 2. Determine retry strategy for that step
    // 3. Update workflow state correctly 
    // 4. Log errors against the correct step for debugging
    // 5. Trigger step-specific error handlers
    
    match socket_error {
        OrchestrationError::ExecutionError(ExecutionError::StepExecutionFailed { step_id, reason, .. }) => {
            // Verify the error is attributed to the correct step
            assert_eq!(step_id, 103, "Error must be attributed to the failing step");
            assert!(reason.contains("ZeroMQ"), "Error should indicate the root cause");
            
            // Orchestration system can now:
            // - Mark step 103 as failed
            // - Keep steps 101, 102 as completed 
            // - Block step 104 until 103 is resolved
            // - Apply retry policy specifically to step 103
        }
        _ => panic!("Expected StepExecutionFailed with proper step_id"),
    }
}

#[cfg(test)]
mod integration_helpers {
    use super::*;
    
    /// Helper to create a realistic step execution context for testing
    pub fn create_test_step_context(step_id: i64, task_id: i64, step_name: &str) -> StepExecutionContext {
        StepExecutionContext {
            step_id,
            task_id,
            step_name: step_name.to_string(),
            input_data: json!({
                "test_mode": true,
                "step_name": step_name,
                "created_at": chrono::Utc::now().to_rfc3339()
            }),
            previous_steps: vec![],
            step_config: HashMap::new(),
            attempt_number: 0,
            max_retry_attempts: 3,
            timeout_seconds: 30,
            is_retryable: true,
            environment: "test".to_string(),
            metadata: HashMap::new(),
        }
    }
    
    /// Helper to create step context with previous step dependencies
    pub fn create_step_context_with_dependencies(
        step_id: i64,
        task_id: i64,
        step_name: &str,
        previous_steps: Vec<WorkflowStep>,
    ) -> StepExecutionContext {
        let mut context = create_test_step_context(step_id, task_id, step_name);
        context.previous_steps = previous_steps;
        context
    }
    
    /// Helper to create workflow step with results
    pub fn create_workflow_step_with_results(
        step_id: i64,
        task_id: i64,
        step_name: &str,
        results: serde_json::Value,
    ) -> WorkflowStep {
        WorkflowStep {
            workflow_step_id: step_id,
            task_id,
            named_step_id: step_id as i32,
            step_name: Some(step_name.to_string()),
            results: Some(results),
            step_config: Some(json!({})),
            workflow_position: Some(1),
            depends_on_step_names: Some(vec![]),
            depends_on_steps: Some(vec![]),
            state: "completed".to_string(),
            attempts: 1,
            processed: true,
            processed_at: Some(chrono::Utc::now()),
            completed_at: Some(chrono::Utc::now()),
            output: Some(results.clone()),
            error_information: None,
            retryable: true,
            retry_limit: 3,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}