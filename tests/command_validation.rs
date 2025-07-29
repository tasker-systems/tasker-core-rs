//! Command Infrastructure Validation Tests
//!
//! Standalone tests to validate our Phase 1 Command pattern implementation.

use std::collections::HashMap;
use std::sync::Arc;

use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities,
};
use tasker_core::execution::command_handlers::WorkerManagementHandler;
use tasker_core::execution::command_router::CommandRouter;
use tasker_core::execution::worker_pool::WorkerPool;

#[tokio::test]
async fn test_phase1_command_serialization() {
    let capabilities = WorkerCapabilities {
        worker_id: "validation_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["orders".to_string(), "inventory".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.1.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: capabilities.clone(),
        },
        CommandSource::RubyWorker {
            id: "validation_worker".to_string(),
        },
    );

    // Test JSON serialization/deserialization
    let json = serde_json::to_string(&command).expect("Serialization failed");
    let deserialized: Command = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(command.command_type, deserialized.command_type);
    assert_eq!(command.command_id, deserialized.command_id);

    // Verify payload
    if let CommandPayload::RegisterWorker {
        worker_capabilities,
    } = deserialized.payload
    {
        assert_eq!(worker_capabilities.worker_id, "validation_worker");
        assert_eq!(worker_capabilities.max_concurrent_steps, 10);
        assert_eq!(
            worker_capabilities.supported_namespaces,
            vec!["orders", "inventory"]
        );
    } else {
        panic!("Expected RegisterWorker payload");
    }
}

#[tokio::test]
async fn test_phase1_worker_pool_operations() {
    let pool = WorkerPool::new();

    let capabilities = WorkerCapabilities {
        worker_id: "pool_test_worker".to_string(),
        max_concurrent_steps: 8,
        supported_namespaces: vec!["test_namespace".to_string()],
        step_timeout_ms: 25000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    // Test worker registration
    let result = pool.register_worker(capabilities).await;
    assert!(result.is_ok(), "Worker registration failed: {:?}", result);

    // Test worker retrieval
    let worker = pool.get_worker("pool_test_worker").await;
    assert!(worker.is_some(), "Worker not found after registration");

    let worker_state = worker.unwrap();
    assert_eq!(worker_state.capabilities.worker_id, "pool_test_worker");
    assert_eq!(worker_state.capabilities.max_concurrent_steps, 8);
    assert_eq!(worker_state.current_load, 0);
    assert_eq!(worker_state.max_capacity, 8);

    // Test pool statistics
    let stats = pool.get_stats().await;
    assert_eq!(stats.total_workers, 1);
    assert_eq!(stats.healthy_workers, 1);
    assert_eq!(stats.total_capacity, 8);
    assert_eq!(stats.current_load, 0);
    assert_eq!(stats.available_capacity, 8);
}

#[tokio::test]
async fn test_phase1_command_routing() {
    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register handler for worker registration commands
    let registration_result = router
        .register_handler(CommandType::RegisterWorker, handler.clone())
        .await;
    assert!(
        registration_result.is_ok(),
        "Handler registration failed: {:?}",
        registration_result
    );

    // Verify handler is registered
    assert!(router.has_handler(&CommandType::RegisterWorker).await);
    assert!(!router.has_handler(&CommandType::ExecuteBatch).await);

    // Test command routing
    let capabilities = WorkerCapabilities {
        worker_id: "routing_test_worker".to_string(),
        max_concurrent_steps: 5,
        supported_namespaces: vec!["routing_test".to_string()],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: "python".to_string(),
        version: "3.9.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: capabilities,
        },
        CommandSource::PythonWorker {
            id: "routing_test_worker".to_string(),
        },
    );

    let routing_result = router.route_command(command).await;
    assert!(
        routing_result.is_ok(),
        "Command routing failed: {:?}",
        routing_result
    );

    let result = routing_result.unwrap();
    assert!(result.success, "Command execution was not successful");
    assert!(result.response.is_some(), "No response received");

    // Verify worker was actually registered in the pool
    let registered_worker = worker_pool.get_worker("routing_test_worker").await;
    assert!(
        registered_worker.is_some(),
        "Worker was not registered in pool"
    );

    let worker_state = registered_worker.unwrap();
    assert_eq!(worker_state.capabilities.worker_id, "routing_test_worker");
    assert_eq!(worker_state.capabilities.max_concurrent_steps, 5);
}

#[tokio::test]
async fn test_phase1_end_to_end_workflow() {
    // Create complete system
    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register handlers
    router
        .register_handler(CommandType::RegisterWorker, handler.clone())
        .await
        .unwrap();
    router
        .register_handler(CommandType::WorkerHeartbeat, handler)
        .await
        .unwrap();

    // Step 1: Register worker
    let capabilities = WorkerCapabilities {
        worker_id: "e2e_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["e2e_test".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.1.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let register_command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: capabilities,
        },
        CommandSource::RubyWorker {
            id: "e2e_worker".to_string(),
        },
    );

    let register_result = router.route_command(register_command).await.unwrap();
    assert!(register_result.success);

    // Step 2: Send heartbeat
    let heartbeat_command = Command::new(
        CommandType::WorkerHeartbeat,
        CommandPayload::WorkerHeartbeat {
            worker_id: "e2e_worker".to_string(),
            current_load: 3,
            system_stats: None,
        },
        CommandSource::RubyWorker {
            id: "e2e_worker".to_string(),
        },
    );

    let heartbeat_result = router.route_command(heartbeat_command).await.unwrap();
    assert!(heartbeat_result.success);

    // Step 3: Verify final state
    let worker_state = worker_pool.get_worker("e2e_worker").await.unwrap();
    assert_eq!(worker_state.current_load, 3);
    assert_eq!(worker_state.max_capacity, 10);
    assert_eq!(worker_state.available_capacity(), 7);
    assert_eq!(worker_state.load_percentage(), 30.0);

    let stats = worker_pool.get_stats().await;
    assert_eq!(stats.total_workers, 1);
    assert_eq!(stats.current_load, 3);
    assert_eq!(stats.available_capacity, 7);
}
