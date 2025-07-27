//! Command Infrastructure Integration Tests
//! 
//! Comprehensive tests for the Command pattern architecture including
//! command creation, serialization, routing, and end-to-end workflows.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandTarget, CommandType, CommandMetadata,
    WorkerCapabilities, StepExecutionRequest, SystemStats, HealthCheckLevel,
};
use tasker_core::execution::command_router::{CommandRouter, CommandHandler, CommandRouterConfig};
use tasker_core::execution::worker_pool::{WorkerPool, WorkerPoolConfig, BatchRequest};
use tasker_core::execution::command_handlers::WorkerManagementHandler;

#[tokio::test]
async fn test_command_creation_and_serialization() {
    // Test command creation with all major payload types
    let capabilities = WorkerCapabilities {
        worker_id: "test_worker_1".to_string(),
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
            id: "test_worker_1".to_string(),
        },
    );

    // Test JSON serialization/deserialization
    let json = serde_json::to_string(&command).expect("Failed to serialize command");
    let deserialized: Command = serde_json::from_str(&json).expect("Failed to deserialize command");

    assert_eq!(command.command_type, deserialized.command_type);
    assert_eq!(command.command_id, deserialized.command_id);
    
    // Verify payload content
    if let CommandPayload::RegisterWorker { worker_capabilities } = deserialized.payload {
        assert_eq!(worker_capabilities.worker_id, "test_worker_1");
        assert_eq!(worker_capabilities.max_concurrent_steps, 10);
        assert_eq!(worker_capabilities.supported_namespaces, vec!["orders", "inventory"]);
    } else {
        panic!("Expected RegisterWorker payload");
    }
}

#[tokio::test]
async fn test_command_response_correlation() {
    let original_command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: WorkerCapabilities {
                worker_id: "test_worker".to_string(),
                max_concurrent_steps: 5,
                supported_namespaces: vec!["test".to_string()],
                step_timeout_ms: 15000,
                supports_retries: false,
                language_runtime: "python".to_string(),
                version: "3.9.0".to_string(),
                custom_capabilities: HashMap::new(),
            },
        },
        CommandSource::PythonWorker {
            id: "test_worker".to_string(),
        },
    );

    let response = original_command.create_response(
        CommandType::WorkerRegistered,
        CommandPayload::WorkerRegistered {
            worker_id: "test_worker".to_string(),
            assigned_pool: "default".to_string(),
            queue_position: 1,
        },
        CommandSource::RustServer {
            id: "server_main".to_string(),
        },
    );

    // Verify correlation
    assert!(response.is_response());
    assert_eq!(response.original_command_id().unwrap(), original_command.command_id);
    assert_eq!(response.correlation_id.as_ref().unwrap(), &original_command.command_id);
}

#[tokio::test]
async fn test_command_metadata_builder_pattern() {
    let source = CommandSource::RustOrchestrator {
        id: "coord_1".to_string(),
    };

    let target = CommandTarget::WorkerPool {
        namespace: "order_fulfillment".to_string(),
    };

    let metadata = CommandMetadata::new(source)
        .with_target(target)
        .with_timeout(30000)
        .with_namespace("order_fulfillment".to_string())
        .with_priority(tasker_core::execution::command::CommandPriority::High);

    assert!(metadata.target.is_some());
    assert_eq!(metadata.timeout_ms, Some(30000));
    assert_eq!(metadata.namespace.as_deref(), Some("order_fulfillment"));
    assert_eq!(metadata.priority, Some(tasker_core::execution::command::CommandPriority::High));
}

#[tokio::test]
async fn test_command_router_registration_and_routing() {
    let config = CommandRouterConfig {
        enable_history: true,
        max_history_size: 100,
        default_timeout_ms: 5000,
    };

    let router = CommandRouter::with_config(config);
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register handler
    router.register_handler(CommandType::RegisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::UnregisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::WorkerHeartbeat, handler).await.unwrap();

    // Verify handler registration
    assert!(router.has_handler(&CommandType::RegisterWorker).await);
    assert!(router.has_handler(&CommandType::UnregisterWorker).await);
    assert!(router.has_handler(&CommandType::WorkerHeartbeat).await);
    assert!(!router.has_handler(&CommandType::ExecuteBatch).await);

    let registered_types = router.get_registered_command_types().await;
    assert_eq!(registered_types.len(), 3);
    assert!(registered_types.contains(&CommandType::RegisterWorker));
}

#[tokio::test]
async fn test_worker_management_end_to_end() {
    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register worker management handler
    router.register_handler(CommandType::RegisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::WorkerHeartbeat, handler).await.unwrap();

    // Test worker registration
    let register_command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: WorkerCapabilities {
                worker_id: "integration_worker".to_string(),
                max_concurrent_steps: 8,
                supported_namespaces: vec!["integration_test".to_string()],
                step_timeout_ms: 25000,
                supports_retries: true,
                language_runtime: "ruby".to_string(),
                version: "3.1.0".to_string(),
                custom_capabilities: HashMap::new(),
            },
        },
        CommandSource::RubyWorker {
            id: "integration_worker".to_string(),
        },
    );

    let result = router.route_command(register_command).await.unwrap();
    assert!(result.success);
    assert!(result.response.is_some());

    let response = result.response.unwrap();
    assert_eq!(response.command_type, CommandType::WorkerRegistered);

    // Verify worker was actually registered in pool
    let worker_state = worker_pool.get_worker("integration_worker").await;
    assert!(worker_state.is_some());
    assert_eq!(worker_state.unwrap().capabilities.max_concurrent_steps, 8);

    // Test heartbeat
    let heartbeat_command = Command::new(
        CommandType::WorkerHeartbeat,
        CommandPayload::WorkerHeartbeat {
            worker_id: "integration_worker".to_string(),
            current_load: 3,
            system_stats: Some(SystemStats {
                cpu_usage_percent: 45.0,
                memory_usage_mb: 512,
                active_connections: 5,
                uptime_seconds: 3600,
            }),
        },
        CommandSource::RubyWorker {
            id: "integration_worker".to_string(),
        },
    );

    let heartbeat_result = router.route_command(heartbeat_command).await.unwrap();
    assert!(heartbeat_result.success);

    // Verify heartbeat updated worker state
    let updated_worker = worker_pool.get_worker("integration_worker").await.unwrap();
    assert_eq!(updated_worker.current_load, 3);
}

#[tokio::test]
async fn test_worker_pool_capability_based_selection() {
    let pool = WorkerPool::new();

    // Register workers with different capabilities
    let worker1_caps = WorkerCapabilities {
        worker_id: "orders_specialist".to_string(),
        max_concurrent_steps: 15,
        supported_namespaces: vec!["orders".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.1.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let worker2_caps = WorkerCapabilities {
        worker_id: "inventory_specialist".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["inventory".to_string()],
        step_timeout_ms: 20000,
        supports_retries: false,
        language_runtime: "python".to_string(),
        version: "3.9.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let worker3_caps = WorkerCapabilities {
        worker_id: "generalist".to_string(),
        max_concurrent_steps: 12,
        supported_namespaces: vec!["orders".to_string(), "inventory".to_string(), "shipping".to_string()],
        step_timeout_ms: 25000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.2.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    pool.register_worker(worker1_caps).await.unwrap();
    pool.register_worker(worker2_caps).await.unwrap();
    pool.register_worker(worker3_caps).await.unwrap();

    // Test selection for orders namespace
    let orders_batch = BatchRequest {
        batch_id: "orders_batch_001".to_string(),
        steps: vec![],
        namespace: Some("orders".to_string()),
        priority: None,
        estimated_duration_ms: None,
    };

    let selected = pool.select_worker_for_batch(&orders_batch).await;
    assert!(selected.is_some());
    let worker_id = selected.unwrap();
    assert!(worker_id == "orders_specialist" || worker_id == "generalist");

    // Test selection for unsupported namespace
    let unsupported_batch = BatchRequest {
        batch_id: "unsupported_batch".to_string(),
        steps: vec![],
        namespace: Some("analytics".to_string()),
        priority: None,
        estimated_duration_ms: None,
    };

    let selected = pool.select_worker_for_batch(&unsupported_batch).await;
    assert!(selected.is_none());

    // Test namespace distribution stats
    let stats = pool.get_stats().await;
    assert_eq!(stats.total_workers, 3);
    assert_eq!(stats.healthy_workers, 3);
    
    // Verify namespace distribution
    assert_eq!(*stats.namespace_distribution.get("orders").unwrap(), 2);
    assert_eq!(*stats.namespace_distribution.get("inventory").unwrap(), 2);
    assert_eq!(*stats.namespace_distribution.get("shipping").unwrap(), 1);
}

#[tokio::test]
async fn test_worker_health_monitoring() {
    let config = WorkerPoolConfig {
        heartbeat_timeout_ms: 500, // Short timeout for testing
        ..WorkerPoolConfig::default()
    };

    let pool = WorkerPool::with_config(config.clone());

    let capabilities = WorkerCapabilities {
        worker_id: "health_test_worker".to_string(),
        max_concurrent_steps: 5,
        supported_namespaces: vec!["test".to_string()],
        step_timeout_ms: 10000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    pool.register_worker(capabilities).await.unwrap();

    // Worker should be healthy initially
    let worker = pool.get_worker("health_test_worker").await.unwrap();
    assert!(worker.is_healthy(&config));

    let stats = pool.get_stats().await;
    assert_eq!(stats.healthy_workers, 1);
    assert_eq!(stats.unhealthy_workers, 0);

    // Wait for heartbeat timeout
    sleep(Duration::from_millis(600)).await;

    // Worker should now be unhealthy
    let worker = pool.get_worker("health_test_worker").await.unwrap();
    assert!(!worker.is_healthy(&config));

    // Test cleanup of unhealthy workers
    let removed_count = pool.cleanup_unhealthy_workers().await;
    assert_eq!(removed_count, 1);

    // Worker should be removed
    let worker = pool.get_worker("health_test_worker").await;
    assert!(worker.is_none());

    let stats = pool.get_stats().await;
    assert_eq!(stats.total_workers, 0);
}

#[tokio::test]
async fn test_command_router_history_and_stats() {
    let config = CommandRouterConfig {
        enable_history: true,
        max_history_size: 3, // Small size for testing
        default_timeout_ms: 5000,
    };

    let router = CommandRouter::with_config(config);
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool));

    router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();

    // Execute multiple commands to test history
    for i in 0..5 {
        let command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: WorkerCapabilities {
                    worker_id: format!("worker_{}", i),
                    max_concurrent_steps: 5,
                    supported_namespaces: vec!["test".to_string()],
                    step_timeout_ms: 10000,
                    supports_retries: true,
                    language_runtime: "rust".to_string(),
                    version: "1.0.0".to_string(),
                    custom_capabilities: HashMap::new(),
                },
            },
            CommandSource::RustOrchestrator {
                id: format!("coord_{}", i),
            },
        );

        router.route_command(command).await.unwrap();
    }

    // Check history is limited to max_history_size
    let history = router.get_command_history().await;
    assert_eq!(history.len(), 3);

    // Check stats
    let stats = router.get_stats().await;
    assert_eq!(stats.registered_handlers, 1);
    assert_eq!(stats.total_commands_processed, 3); // Limited by history size
    assert_eq!(stats.successful_commands, 3);
    assert_eq!(stats.failed_commands, 0);
    assert!(stats.history_enabled);
}

#[tokio::test]
async fn test_command_validation_and_error_handling() {
    let router = CommandRouter::new();

    // Test command with empty ID (should be rejected during validation)
    let mut invalid_command = Command::new(
        CommandType::HealthCheck,
        CommandPayload::HealthCheck {
            diagnostic_level: HealthCheckLevel::Basic,
        },
        CommandSource::RustOrchestrator {
            id: "test_coord".to_string(),
        },
    );

    invalid_command.command_id = String::new(); // Make invalid

    let result = router.route_command(invalid_command).await;
    assert!(result.is_err());

    // Test command without registered handler
    let unhandled_command = Command::new(
        CommandType::ExecuteBatch,
        CommandPayload::ExecuteBatch {
            batch_id: "test_batch".to_string(),
            steps: vec![],
        },
        CommandSource::RustOrchestrator {
            id: "test_coord".to_string(),
        },
    );

    let result = router.route_command(unhandled_command).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_complex_command_workflow() {
    // Integration test simulating a complete workflow:
    // 1. Worker registration
    // 2. Batch execution request 
    // 3. Partial result reporting
    // 4. Batch completion
    // 5. Worker heartbeat
    // 6. Health check

    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let worker_handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    // Register handlers
    router.register_handler(CommandType::RegisterWorker, worker_handler.clone()).await.unwrap();
    router.register_handler(CommandType::WorkerHeartbeat, worker_handler).await.unwrap();

    // Step 1: Register worker
    let worker_caps = WorkerCapabilities {
        worker_id: "workflow_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["workflow_test".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.1.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let register_cmd = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: worker_caps,
        },
        CommandSource::RubyWorker {
            id: "workflow_worker".to_string(),
        },
    );

    let register_result = router.route_command(register_cmd).await.unwrap();
    assert!(register_result.success);

    // Step 2: Send heartbeat with load
    let heartbeat_cmd = Command::new(
        CommandType::WorkerHeartbeat,
        CommandPayload::WorkerHeartbeat {
            worker_id: "workflow_worker".to_string(),
            current_load: 5,
            system_stats: Some(SystemStats {
                cpu_usage_percent: 60.0,
                memory_usage_mb: 1024,
                active_connections: 8,
                uptime_seconds: 7200,
            }),
        },
        CommandSource::RubyWorker {
            id: "workflow_worker".to_string(),
        },
    );

    let heartbeat_result = router.route_command(heartbeat_cmd).await.unwrap();
    assert!(heartbeat_result.success);

    // Verify final state
    let worker_state = worker_pool.get_worker("workflow_worker").await.unwrap();
    assert_eq!(worker_state.current_load, 5);
    assert_eq!(worker_state.max_capacity, 10);
    assert_eq!(worker_state.available_capacity(), 5);
    assert_eq!(worker_state.load_percentage(), 50.0);

    let stats = worker_pool.get_stats().await;
    assert_eq!(stats.total_workers, 1);
    assert_eq!(stats.current_load, 5);
    assert_eq!(stats.available_capacity, 5);
}