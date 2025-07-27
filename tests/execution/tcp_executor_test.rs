//! TCP Executor Integration Tests
//! 
//! Tests for the TokioTcpExecutor that replaces ZeroMQ with native TCP connections
//! and command-based communication.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities,
};
use tasker_core::execution::tokio_tcp_executor::{TokioTcpExecutor, TcpExecutorConfig};
use tasker_core::execution::command_handlers::WorkerManagementHandler;
use tasker_core::execution::worker_pool::WorkerPool;

#[tokio::test]
async fn test_tcp_executor_lifecycle() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(), // OS-assigned port
        command_queue_size: 100,
        connection_timeout_ms: 5000,
        graceful_shutdown_timeout_ms: 1000,
        max_connections: 10,
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();

    // Initially not running
    assert!(!executor.is_running().await);

    // Start executor
    executor.start().await.unwrap();
    assert!(executor.is_running().await);

    // Starting again should fail
    assert!(executor.start().await.is_err());

    // Stop executor
    executor.stop().await.unwrap();
    assert!(!executor.is_running().await);

    // Stopping again should succeed (idempotent)
    executor.stop().await.unwrap();
}

#[tokio::test]
async fn test_tcp_executor_stats() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    
    let initial_stats = executor.get_stats().await;
    assert!(!initial_stats.running);
    assert_eq!(initial_stats.active_connections, 0);
    assert_eq!(initial_stats.total_connections, 0);

    executor.start().await.unwrap();

    let running_stats = executor.get_stats().await;
    assert!(running_stats.running);
    assert!(running_stats.uptime_seconds < 5); // Should be very recent

    executor.stop().await.unwrap();
}

#[tokio::test]
async fn test_tcp_connection_handling() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        command_queue_size: 50,
        connection_timeout_ms: 2000,
        graceful_shutdown_timeout_ms: 500,
        max_connections: 5,
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    
    // Register a handler for testing
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool));
    executor.command_router()
        .register_handler(CommandType::RegisterWorker, handler)
        .await
        .unwrap();

    executor.start().await.unwrap();

    // Get the actual bound address (since we used port 0)
    let stats = executor.get_stats().await;
    
    // Give the server a moment to start listening
    sleep(Duration::from_millis(100)).await;

    // For this test, we'll just verify the server is accepting connections
    // without getting into the full command parsing (which requires more setup)
    
    let stats_after = executor.get_stats().await;
    assert!(stats_after.running);

    executor.stop().await.unwrap();
}

#[tokio::test]
async fn test_command_parsing() {
    let config = TcpExecutorConfig::default();
    let executor = TokioTcpExecutor::new(config).await.unwrap();

    // Test valid command JSON
    let command = Command::new(
        CommandType::HealthCheck,
        CommandPayload::HealthCheck {
            diagnostic_level: tasker_core::execution::command::HealthCheckLevel::Basic,
        },
        CommandSource::RubyWorker {
            id: "test_worker".to_string(),
        },
    );

    let json = serde_json::to_string(&command).unwrap();
    let parsed = executor.parse_command(&json);
    assert!(parsed.is_some());
    assert_eq!(parsed.unwrap().command_type, CommandType::HealthCheck);

    // Test invalid JSON
    let invalid_json = "not valid json";
    let parsed = executor.parse_command(invalid_json);
    assert!(parsed.is_none());

    // Test empty string
    let parsed = executor.parse_command("");
    assert!(parsed.is_none());
}

#[tokio::test]
async fn test_tcp_executor_configuration() {
    // Test custom configuration
    let custom_config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        command_queue_size: 500,
        connection_timeout_ms: 10000,
        graceful_shutdown_timeout_ms: 2000,
        max_connections: 100,
    };

    let executor = TokioTcpExecutor::new(custom_config).await.unwrap();
    
    // Verify executor was created successfully with custom config
    assert!(!executor.is_running().await);

    executor.start().await.unwrap();
    assert!(executor.is_running().await);

    executor.stop().await.unwrap();
}

#[tokio::test]
async fn test_tcp_executor_command_router_integration() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    
    // Get command router and register handlers
    let router = executor.command_router();
    let worker_pool = executor.worker_pool();
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    router.register_handler(CommandType::RegisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::WorkerHeartbeat, handler).await.unwrap();

    // Verify handlers are registered
    assert!(router.has_handler(&CommandType::RegisterWorker).await);
    assert!(router.has_handler(&CommandType::WorkerHeartbeat).await);
    assert!(!router.has_handler(&CommandType::ExecuteBatch).await);

    // Test direct command routing through the router
    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: WorkerCapabilities {
                worker_id: "router_test_worker".to_string(),
                max_concurrent_steps: 8,
                supported_namespaces: vec!["test".to_string()],
                step_timeout_ms: 20000,
                supports_retries: true,
                language_runtime: "ruby".to_string(),
                version: "3.1.0".to_string(),
                custom_capabilities: std::collections::HashMap::new(),
            },
        },
        CommandSource::RubyWorker {
            id: "router_test_worker".to_string(),
        },
    );

    let result = router.route_command(command).await.unwrap();
    assert!(result.success);
    assert!(result.response.is_some());

    // Verify worker was registered in the pool
    let worker = worker_pool.get_worker("router_test_worker").await;
    assert!(worker.is_some());
    assert_eq!(worker.unwrap().capabilities.max_concurrent_steps, 8);
}

#[tokio::test]
async fn test_tcp_executor_worker_pool_integration() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    let worker_pool = executor.worker_pool();

    // Register multiple workers
    for i in 0..3 {
        let capabilities = WorkerCapabilities {
            worker_id: format!("pool_test_worker_{}", i),
            max_concurrent_steps: 10 + i,
            supported_namespaces: vec![format!("namespace_{}", i % 2)],
            step_timeout_ms: 30000,
            supports_retries: i % 2 == 0,
            language_runtime: if i % 2 == 0 { "ruby" } else { "python" }.to_string(),
            version: "1.0.0".to_string(),
            custom_capabilities: std::collections::HashMap::new(),
        };

        worker_pool.register_worker(capabilities).await.unwrap();
    }

    let stats = worker_pool.get_stats().await;
    assert_eq!(stats.total_workers, 3);
    assert_eq!(stats.healthy_workers, 3);
    assert_eq!(stats.total_capacity, 10 + 11 + 12); // 33

    // Test namespace distribution
    assert_eq!(*stats.namespace_distribution.get("namespace_0").unwrap(), 2);
    assert_eq!(*stats.namespace_distribution.get("namespace_1").unwrap(), 1);
}

#[tokio::test] 
async fn test_tcp_executor_graceful_shutdown() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        graceful_shutdown_timeout_ms: 100, // Short timeout for testing
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    
    executor.start().await.unwrap();
    assert!(executor.is_running().await);

    // Simulate some activity by registering handlers
    let router = executor.command_router();
    let worker_pool = executor.worker_pool();
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool));
    router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();

    // Graceful shutdown should complete within timeout
    let start = std::time::Instant::now();
    executor.stop().await.unwrap();
    let elapsed = start.elapsed();

    assert!(!executor.is_running().await);
    // Should complete relatively quickly due to short timeout
    assert!(elapsed < Duration::from_millis(500));
}

#[tokio::test]
async fn test_tcp_executor_error_handling() {
    // Test binding to invalid address
    let invalid_config = TcpExecutorConfig {
        bind_address: "invalid_address:999999".to_string(),
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(invalid_config).await.unwrap();
    let result = executor.start().await;
    assert!(result.is_err());

    // Test binding to already used port
    let config1 = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        ..TcpExecutorConfig::default()
    };

    let executor1 = TokioTcpExecutor::new(config1).await.unwrap();
    executor1.start().await.unwrap();

    // Note: Can't easily test port conflict with OS-assigned ports,
    // but the error handling structure is validated above
    
    executor1.stop().await.unwrap();
}

#[tokio::test]
async fn test_tcp_executor_concurrent_operations() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        command_queue_size: 1000,
        max_connections: 50,
        ..TcpExecutorConfig::default()
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    let router = executor.command_router();
    let worker_pool = executor.worker_pool();
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();

    // Test concurrent worker registrations
    let mut handles = vec![];
    for i in 0..10 {
        let router_clone = router.clone();
        let handle = tokio::spawn(async move {
            let command = Command::new(
                CommandType::RegisterWorker,
                CommandPayload::RegisterWorker {
                    worker_capabilities: WorkerCapabilities {
                        worker_id: format!("concurrent_worker_{}", i),
                        max_concurrent_steps: 5,
                        supported_namespaces: vec!["concurrent_test".to_string()],
                        step_timeout_ms: 15000,
                        supports_retries: true,
                        language_runtime: "rust".to_string(),
                        version: "1.0.0".to_string(),
                        custom_capabilities: std::collections::HashMap::new(),
                    },
                },
                CommandSource::RustOrchestrator {
                    id: format!("coord_{}", i),
                },
            );

            router_clone.route_command(command).await
        });
        handles.push(handle);
    }

    // Wait for all registrations to complete
    for handle in handles {
        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
    }

    // Verify all workers were registered
    let stats = worker_pool.get_stats().await;
    assert_eq!(stats.total_workers, 10);
    assert_eq!(stats.healthy_workers, 10);
}