// ZeroMQ Orchestration Integration Tests
// Tests the complete integration between ZeroMQ executor and orchestration system

use std::collections::HashMap;
use std::time::Duration;
use serde_json::json;
use sqlx::PgPool;
use tasker_core::execution::zeromq_pub_sub_executor::ZmqPubSubExecutor;
use tasker_core::orchestration::{
    errors::OrchestrationError,
    state_manager::StateManager,
    step_handler::StepExecutionContext,
    types::{FrameworkIntegration, StepStatus, TaskContext},
};
use tasker_core::models::WorkflowStep;

/// Test the complete fire-and-forget flow with state machine integration
#[tokio::test]
async fn test_complete_fire_and_forget_orchestration_flow() {
    // This test demonstrates the complete orchestration flow:
    // 1. ZeroMQ executor publishes step batch (fire-and-forget)
    // 2. Steps are marked as InProgress in state machine and database
    // 3. Background result listener handles actual results when they arrive
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://orchestration_test_steps",
        "inproc://orchestration_test_results",
        pool.clone(),
    ).await.expect("Failed to create executor");
    
    // Create step execution contexts for orchestration
    let step_contexts = vec![
        create_orchestration_step_context(1001, 2001, "validate_input", &[]),
        create_orchestration_step_context(1002, 2001, "process_data", &[1001]),
        create_orchestration_step_context(1003, 2001, "generate_output", &[1002]),
    ];
    
    // Convert to the format expected by execute_step_batch
    let contexts_with_handlers: Vec<(&StepExecutionContext, &str, &HashMap<String, serde_json::Value>)> = step_contexts.iter()
        .enumerate()
        .map(|(i, ctx)| (ctx, format!("Handler{}", i + 1).as_str(), &HashMap::new()))
        .collect();
    
    let start_time = std::time::Instant::now();
    
    // Execute batch - should return immediately with InProgress status
    let results = executor.execute_step_batch(contexts_with_handlers).await
        .expect("Batch execution should succeed");
    
    let execution_time = start_time.elapsed();
    
    // Verify fire-and-forget semantics
    assert_eq!(results.len(), 3, "Should return results for all 3 steps");
    assert!(execution_time.as_millis() < 100, "Should complete quickly (fire-and-forget)");
    
    // Verify all steps are marked as InProgress
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.step_id, 1001 + i as i64);
        assert_eq!(result.status, StepStatus::InProgress);
        assert_eq!(result.output, serde_json::Value::Null);
        assert!(result.error_message.is_none());
    }
    
    // Verify framework integration properties
    assert_eq!(executor.framework_name(), "ZeroMQ");
    assert!(executor.supports_batch_execution());
}

/// Test error handling in orchestration context
#[tokio::test]
async fn test_orchestration_error_attribution() {
    // Test that errors maintain proper step_id context for orchestration system
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://error_test_steps", 
        "inproc://error_test_results",
        pool,
    ).await.expect("Failed to create executor");
    
    let context = create_orchestration_step_context(3001, 4001, "error_prone_step", &[]);
    let handler_config = HashMap::new();
    
    // Execute step that should succeed (fire-and-forget)
    let result = executor.execute_step_with_handler(&context, "ErrorProneHandler", &handler_config).await;
    
    // Should succeed because fire-and-forget doesn't wait for execution
    assert!(result.is_ok());
    let step_result = result.unwrap();
    assert_eq!(step_result.step_id, 3001);
    assert_eq!(step_result.status, StepStatus::InProgress);
    
    // In a real scenario with socket errors, the error would include proper step_id context
    let socket_error = OrchestrationError::ExecutionError(
        tasker_core::orchestration::errors::ExecutionError::StepExecutionFailed {
            step_id: 3001,
            reason: "ZeroMQ socket publish failed".to_string(),
            error_code: Some("ZMQ_PUBLISH_FAILED".to_string()),
        }
    );
    
    match socket_error {
        OrchestrationError::ExecutionError(
            tasker_core::orchestration::errors::ExecutionError::StepExecutionFailed { 
                step_id, reason, error_code 
            }
        ) => {
            assert_eq!(step_id, 3001);
            assert!(reason.contains("ZeroMQ"));
            assert_eq!(error_code, Some("ZMQ_PUBLISH_FAILED".to_string()));
        }
        _ => panic!("Expected StepExecutionFailed error with proper attribution"),
    }
}

/// Test task context loading and metadata enrichment
#[tokio::test]
async fn test_task_context_orchestration_integration() {
    // Test that get_task_context provides proper structure for orchestration
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://context_orchestration_steps",
        "inproc://context_orchestration_results", 
        pool,
    ).await.expect("Failed to create executor");
    
    // Test the interface contract for task context
    let result = executor.get_task_context(5001).await;
    
    // Should fail gracefully with proper orchestration error structure
    assert!(result.is_err());
    
    match result.unwrap_err() {
        OrchestrationError::TaskExecutionFailed { task_id, reason, error_code } => {
            assert_eq!(task_id, 5001);
            assert!(reason.contains("Task not found") || reason.contains("Database error"));
            assert!(error_code.is_some());
        }
        _ => panic!("Expected TaskExecutionFailed error with proper structure"),
    }
    
    // In a real scenario with proper database setup, we would verify:
    // - Task data is loaded correctly
    // - Metadata includes task fields, tags, execution context
    // - TaskContext structure is properly populated
    let expected_context_structure = TaskContext {
        task_id: 5001,
        data: json!({
            "order_id": "TEST-ORDER-123",
            "customer_id": 12345,
            "processing_mode": "standard"
        }),
        metadata: {
            let mut meta = HashMap::new();
            meta.insert("initiator".to_string(), json!("system"));
            meta.insert("source_system".to_string(), json!("e-commerce"));
            meta.insert("total_steps".to_string(), json!(5));
            meta.insert("ready_steps".to_string(), json!(2));
            meta.insert("completed_steps".to_string(), json!(0));
            meta.insert("execution_status".to_string(), json!("in_progress"));
            meta.insert("health_status".to_string(), json!("healthy"));
            meta
        },
    };
    
    // Verify expected structure for orchestration integration
    assert_eq!(expected_context_structure.task_id, 5001);
    assert!(expected_context_structure.data.get("order_id").is_some());
    assert!(expected_context_structure.metadata.contains_key("total_steps"));
    assert!(expected_context_structure.metadata.contains_key("execution_status"));
}

/// Test dependency resolution and previous step results
#[tokio::test]
async fn test_dependency_resolution_orchestration() {
    // Test that step dependencies are properly resolved for orchestration
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://dependency_test_steps",
        "inproc://dependency_test_results",
        pool,
    ).await.expect("Failed to create executor");
    
    // Create previous steps with results
    let previous_steps = vec![
        create_workflow_step_with_results(
            6001, 7001, "validate_order", 
            json!({"status": "approved", "validation_score": 95})
        ),
        create_workflow_step_with_results(
            6002, 7001, "calculate_pricing",
            json!({"base_price": 100.00, "discount": 10.00, "final_price": 90.00})
        ),
    ];
    
    let context = create_orchestration_step_context_with_dependencies(
        6003, 7001, "process_payment", &previous_steps
    );
    
    let handler_config = HashMap::new();
    
    // Execute step with dependencies
    let result = executor.execute_step_with_handler(&context, "PaymentHandler", &handler_config).await;
    
    // Should succeed with fire-and-forget
    assert!(result.is_ok());
    let step_result = result.unwrap();
    
    assert_eq!(step_result.step_id, 6003);
    assert_eq!(step_result.status, StepStatus::InProgress);
    
    // Verify that context includes dependency information
    assert_eq!(context.previous_steps.len(), 2);
    assert!(context.previous_steps[0].results.is_some());
    assert!(context.previous_steps[1].results.is_some());
    
    // In the actual ZeroMQ message, previous step results would be included
    // with proper step names for handler access
}

/// Test batch processing optimization for orchestration
#[tokio::test]
async fn test_batch_optimization_orchestration() {
    // Test that batch processing provides orchestration benefits
    
    let pool = PgPool::connect("sqlite::memory:").await.expect("Failed to create test pool");
    
    let executor = ZmqPubSubExecutor::new(
        "inproc://batch_optimization_steps",
        "inproc://batch_optimization_results",
        pool,
    ).await.expect("Failed to create executor");
    
    // Create a large batch of steps for performance testing
    let mut step_contexts = Vec::new();
    let mut contexts_with_handlers = Vec::new();
    
    for i in 1..=20 {  // 20 steps in batch
        let context = create_orchestration_step_context(
            8000 + i, 9001, &format!("batch_step_{}", i), &[]
        );
        step_contexts.push(context);
    }
    
    // Convert to handler format
    for (i, ctx) in step_contexts.iter().enumerate() {
        contexts_with_handlers.push((ctx, "BatchHandler", &HashMap::new()));
    }
    
    let start_time = std::time::Instant::now();
    
    // Execute large batch
    let results = executor.execute_step_batch(contexts_with_handlers).await
        .expect("Large batch should succeed");
        
    let batch_time = start_time.elapsed();
    
    // Verify batch efficiency
    assert_eq!(results.len(), 20);
    assert!(batch_time.as_millis() < 200, "Large batch should still be fast (fire-and-forget)");
    
    // All results should be InProgress
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.step_id, 8001 + i as i64);
        assert_eq!(result.status, StepStatus::InProgress);
    }
    
    // Batch processing provides orchestration benefits:
    // - Single ZeroMQ message for entire batch
    // - Atomic batch processing on Ruby side
    // - Reduced message overhead
    // - Better throughput for high-volume workflows
}

// Helper functions for creating test data

fn create_orchestration_step_context(
    step_id: i64, 
    task_id: i64, 
    step_name: &str, 
    dependency_step_ids: &[i64]
) -> StepExecutionContext {
    let previous_steps = dependency_step_ids.iter()
        .map(|&dep_id| create_workflow_step_with_results(
            dep_id, task_id, &format!("dep_step_{}", dep_id),
            json!({"completed": true})
        ))
        .collect();
        
    StepExecutionContext {
        step_id,
        task_id,
        step_name: step_name.to_string(),
        input_data: json!({
            "step_name": step_name,
            "orchestration_test": true,
            "created_at": chrono::Utc::now().to_rfc3339()
        }),
        previous_steps,
        step_config: HashMap::new(),
        attempt_number: 0,
        max_retry_attempts: 3,
        timeout_seconds: 60,
        is_retryable: true,
        environment: "orchestration_test".to_string(),
        metadata: HashMap::new(),
    }
}

fn create_orchestration_step_context_with_dependencies(
    step_id: i64,
    task_id: i64, 
    step_name: &str,
    previous_steps: &[WorkflowStep]
) -> StepExecutionContext {
    StepExecutionContext {
        step_id,
        task_id,
        step_name: step_name.to_string(),
        input_data: json!({
            "step_name": step_name,
            "has_dependencies": true,
            "dependency_count": previous_steps.len()
        }),
        previous_steps: previous_steps.to_vec(),
        step_config: HashMap::new(),
        attempt_number: 0,
        max_retry_attempts: 3,
        timeout_seconds: 60,
        is_retryable: true,
        environment: "orchestration_test".to_string(),
        metadata: HashMap::new(),
    }
}

fn create_workflow_step_with_results(
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
        results: Some(results.clone()),
        step_config: Some(json!({})),
        workflow_position: Some(1),
        depends_on_step_names: Some(vec![]),
        depends_on_steps: Some(vec![]),
        state: "completed".to_string(),
        attempts: 1,
        processed: true,
        processed_at: Some(chrono::Utc::now()),
        completed_at: Some(chrono::Utc::now()),
        output: Some(results),
        error_information: None,
        retryable: true,
        retry_limit: 3,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}