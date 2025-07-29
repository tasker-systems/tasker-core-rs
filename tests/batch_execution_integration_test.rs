//! Integration test for batch execution workflow
//!
//! Tests the complete batch execution flow:
//! 1. Worker registration
//! 2. Batch execution command
//! 3. Partial result reporting
//! 4. Batch completion reporting

use std::collections::HashMap;
use std::sync::Arc;

use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, StepError, StepExecutionRequest,
    StepResult, StepStatus, StepSummary, WorkerCapabilities,
};
use tasker_core::execution::command_handlers::{
    BatchExecutionHandler, ResultAggregationHandler, WorkerManagementHandler,
};
use tasker_core::execution::command_router::CommandRouter;
use tasker_core::execution::worker_pool::WorkerPool;

async fn setup_test_environment() -> (Arc<WorkerPool>, Arc<CommandRouter>) {
    let worker_pool = Arc::new(WorkerPool::new());
    let command_router = Arc::new(CommandRouter::new());

    // Set up handlers
    let worker_handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));
    let batch_handler = Arc::new(BatchExecutionHandler::new(worker_pool.clone()));
    let result_handler = Arc::new(ResultAggregationHandler::new(worker_pool.clone()));

    // Register handlers
    command_router
        .register_handler(CommandType::RegisterWorker, worker_handler.clone())
        .await
        .unwrap();
    command_router
        .register_handler(CommandType::ExecuteBatch, batch_handler)
        .await
        .unwrap();
    command_router
        .register_handler(CommandType::ReportPartialResult, result_handler.clone())
        .await
        .unwrap();
    command_router
        .register_handler(CommandType::ReportBatchCompletion, result_handler)
        .await
        .unwrap();

    (worker_pool, command_router)
}

fn create_test_worker_capabilities(worker_id: &str) -> WorkerCapabilities {
    WorkerCapabilities {
        worker_id: worker_id.to_string(),
        max_concurrent_steps: 5,
        supported_namespaces: vec!["test_namespace".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust_test".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    }
}

fn create_test_step_request(step_id: i64, step_name: &str) -> StepExecutionRequest {
    StepExecutionRequest {
        step_id,
        step_name: step_name.to_string(),
        step_context: HashMap::new(),
        dependencies: vec![],
        timeout_ms: Some(10000),
    }
}

#[tokio::test]
async fn test_complete_batch_execution_workflow() {
    let (worker_pool, command_router) = setup_test_environment().await;

    // Step 1: Register a worker
    let worker_capabilities = create_test_worker_capabilities("test_worker_1");
    let register_command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities,
        },
        CommandSource::RubyWorker {
            id: "test_worker_1".to_string(),
        },
    );

    let register_result = command_router
        .route_command(register_command)
        .await
        .unwrap();
    assert!(
        register_result.success,
        "Worker registration should succeed"
    );

    if let Some(register_response) = register_result.response {
        assert_eq!(
            register_response.command_type,
            CommandType::WorkerRegistered
        );
    } else {
        panic!("Expected response from worker registration");
    }

    // Verify worker is in pool
    let worker = worker_pool.get_worker("test_worker_1").await;
    assert!(worker.is_some(), "Worker should be registered in pool");

    // Step 2: Execute a batch
    let step_requests = vec![
        create_test_step_request(1, "validate_input"),
        create_test_step_request(2, "process_data"),
        create_test_step_request(3, "generate_output"),
    ];

    let execute_command = Command::new(
        CommandType::ExecuteBatch,
        CommandPayload::ExecuteBatch {
            batch_id: "test_batch_123".to_string(),
            steps: step_requests.clone(),
        },
        CommandSource::RustOrchestrator {
            id: "coordinator_1".to_string(),
        },
    );

    let execute_result = command_router.route_command(execute_command).await.unwrap();
    assert!(execute_result.success, "Batch execution should succeed");

    if let Some(execute_response) = execute_result.response {
        assert_eq!(execute_response.command_type, CommandType::BatchExecuted);

        // Extract batch execution details
        if let CommandPayload::BatchExecuted {
            batch_id,
            assigned_worker,
            ..
        } = &execute_response.payload
        {
            assert_eq!(batch_id, "test_batch_123");
            assert_eq!(assigned_worker, "test_worker_1");
        } else {
            panic!("Expected BatchExecuted payload");
        }
    } else {
        panic!("Expected response from batch execution");
    }

    // Verify worker load was incremented
    let worker = worker_pool.get_worker("test_worker_1").await.unwrap();
    assert_eq!(
        worker.current_load, 3,
        "Worker load should be incremented by number of steps"
    );

    // Step 3: Report partial results
    for (i, step_request) in step_requests.iter().enumerate() {
        let partial_result = StepResult {
            status: if i < 2 {
                StepStatus::Completed
            } else {
                StepStatus::Failed
            },
            output: Some(serde_json::json!({"step_output": format!("result_{}", i)})),
            error: if i >= 2 {
                Some(StepError {
                    error_type: "TestError".to_string(),
                    message: "Simulated failure".to_string(),
                    backtrace: None,
                    retryable: false,
                })
            } else {
                None
            },
            metadata: HashMap::new(),
        };

        let partial_command = Command::new(
            CommandType::ReportPartialResult,
            CommandPayload::ReportPartialResult {
                batch_id: "test_batch_123".to_string(),
                step_id: step_request.step_id,
                result: partial_result,
                execution_time_ms: 1000 + (i as u64 * 500),
                worker_id: "test_worker_1".to_string(),
            },
            CommandSource::RubyWorker {
                id: "test_worker_1".to_string(),
            },
        );

        let partial_result = command_router.route_command(partial_command).await.unwrap();
        assert!(partial_result.success, "Partial result should succeed");

        if let Some(partial_response) = partial_result.response {
            assert_eq!(partial_response.command_type, CommandType::Success);
        }
    }

    // Step 4: Report batch completion
    let step_summaries = vec![
        StepSummary {
            step_id: 1,
            final_status: StepStatus::Completed,
            execution_time_ms: 1000,
            worker_id: "test_worker_1".to_string(),
        },
        StepSummary {
            step_id: 2,
            final_status: StepStatus::Completed,
            execution_time_ms: 1500,
            worker_id: "test_worker_1".to_string(),
        },
        StepSummary {
            step_id: 3,
            final_status: StepStatus::Failed,
            execution_time_ms: 2000,
            worker_id: "test_worker_1".to_string(),
        },
    ];

    let completion_command = Command::new(
        CommandType::ReportBatchCompletion,
        CommandPayload::ReportBatchCompletion {
            batch_id: "test_batch_123".to_string(),
            step_summaries: step_summaries.clone(),
            total_execution_time_ms: 4500,
        },
        CommandSource::RubyWorker {
            id: "test_worker_1".to_string(),
        },
    );

    let completion_result = command_router
        .route_command(completion_command)
        .await
        .unwrap();
    assert!(completion_result.success, "Batch completion should succeed");

    if let Some(completion_response) = completion_result.response {
        assert_eq!(completion_response.command_type, CommandType::Success);

        // Verify batch completion data
        if let CommandPayload::Success { message, data } = &completion_response.payload {
            assert!(message.contains("test_batch_123"));
            assert!(message.contains("2/3 steps successful"));

            if let Some(data) = data {
                let batch_data = data.as_object().unwrap();
                assert_eq!(batch_data["total_steps"], 3);
                assert_eq!(batch_data["successful_steps"], 2);
                assert_eq!(batch_data["failed_steps"], 1);
                assert_eq!(batch_data["total_execution_time_ms"], 4500);
            }
        } else {
            panic!("Expected Success payload with batch completion data");
        }
    } else {
        panic!("Expected response from batch completion");
    }

    // Verify worker load was decremented after completion
    let worker = worker_pool.get_worker("test_worker_1").await.unwrap();
    assert_eq!(
        worker.current_load, 0,
        "Worker load should be decremented to 0 after batch completion"
    );

    println!("✅ Complete batch execution workflow test passed!");
}

#[tokio::test]
async fn test_batch_execution_with_no_available_workers() {
    let (_worker_pool, command_router) = setup_test_environment().await;

    // Try to execute a batch without any registered workers
    let execute_command = Command::new(
        CommandType::ExecuteBatch,
        CommandPayload::ExecuteBatch {
            batch_id: "no_workers_batch".to_string(),
            steps: vec![create_test_step_request(1, "test_step")],
        },
        CommandSource::RustOrchestrator {
            id: "coordinator_1".to_string(),
        },
    );

    let execute_result = command_router.route_command(execute_command).await.unwrap();
    assert!(
        execute_result.success,
        "Should succeed even with error response"
    );

    if let Some(execute_response) = execute_result.response {
        assert_eq!(execute_response.command_type, CommandType::Error);

        if let CommandPayload::Error {
            error_type,
            message,
            retryable,
            ..
        } = &execute_response.payload
        {
            assert_eq!(error_type, "NoAvailableWorkers");
            assert!(message.contains("no_workers_batch"));
            assert!(*retryable, "Should be retryable when no workers available");
        } else {
            panic!("Expected Error payload");
        }
    } else {
        panic!("Expected error response from batch execution");
    }

    println!("✅ No available workers test passed!");
}

#[tokio::test]
async fn test_worker_capacity_load_balancing() {
    let (worker_pool, command_router) = setup_test_environment().await;

    // Register two workers with different capacities
    let worker1_capabilities = WorkerCapabilities {
        worker_id: "high_capacity_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["test_namespace".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust_test".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let worker2_capabilities = WorkerCapabilities {
        worker_id: "low_capacity_worker".to_string(),
        max_concurrent_steps: 2,
        supported_namespaces: vec!["test_namespace".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust_test".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    // Register both workers
    let register1 = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: worker1_capabilities,
        },
        CommandSource::RubyWorker {
            id: "high_capacity_worker".to_string(),
        },
    );
    let _register1_result = command_router.route_command(register1).await.unwrap();

    let register2 = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: worker2_capabilities,
        },
        CommandSource::RubyWorker {
            id: "low_capacity_worker".to_string(),
        },
    );
    let _register2_result = command_router.route_command(register2).await.unwrap();

    // Fill up the low capacity worker first
    worker_pool
        .increment_worker_load("low_capacity_worker", 2)
        .await
        .unwrap();

    // Now execute a batch - should go to high capacity worker
    let execute_command = Command::new(
        CommandType::ExecuteBatch,
        CommandPayload::ExecuteBatch {
            batch_id: "capacity_test_batch".to_string(),
            steps: vec![
                create_test_step_request(1, "step1"),
                create_test_step_request(2, "step2"),
            ],
        },
        CommandSource::RustOrchestrator {
            id: "coordinator_1".to_string(),
        },
    );

    let execute_result = command_router.route_command(execute_command).await.unwrap();
    assert!(execute_result.success);

    if let Some(execute_response) = execute_result.response {
        if let CommandPayload::BatchExecuted {
            assigned_worker, ..
        } = &execute_response.payload
        {
            assert_eq!(
                assigned_worker, "high_capacity_worker",
                "Should select worker with available capacity"
            );
        } else {
            panic!("Expected BatchExecuted payload");
        }
    } else {
        panic!("Expected response from batch execution");
    }

    println!("✅ Worker capacity load balancing test passed!");
}
