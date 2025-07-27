//! Basic Command Infrastructure Tests
//! 
//! Simple validation tests for our Command pattern implementation to verify
//! core functionality works before running complex integration tests.

use std::collections::HashMap;
use tasker_core::execution::command::{Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities};

#[tokio::test]
async fn test_command_serialization_basic() {
    let capabilities = WorkerCapabilities {
        worker_id: "test_worker".to_string(),
        max_concurrent_steps: 5,
        supported_namespaces: vec!["test".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker { worker_capabilities: capabilities },
        CommandSource::RustOrchestrator { id: "test".to_string() },
    );

    // Test serialization/deserialization
    let json = serde_json::to_string(&command).unwrap();
    let deserialized: Command = serde_json::from_str(&json).unwrap();
    
    assert_eq!(command.command_type, deserialized.command_type);
    assert_eq!(command.command_id, deserialized.command_id);
}

#[tokio::test]
async fn test_worker_pool_basic() {
    use tasker_core::execution::worker_pool::WorkerPool;
    
    let pool = WorkerPool::new();
    
    let capabilities = WorkerCapabilities {
        worker_id: "test_worker".to_string(),
        max_concurrent_steps: 5,
        supported_namespaces: vec!["test".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };
    
    // Test worker registration
    pool.register_worker(capabilities).await.unwrap();
    
    // Test worker exists
    let worker = pool.get_worker("test_worker").await;
    assert!(worker.is_some());
    
    // Test stats
    let stats = pool.get_stats().await;
    assert_eq!(stats.total_workers, 1);
}

#[tokio::test]
async fn test_command_router_basic() {
    use tasker_core::execution::command_router::CommandRouter;
    use tasker_core::execution::command_handlers::WorkerManagementHandler;
    use tasker_core::execution::worker_pool::WorkerPool;
    use std::sync::Arc;
    
    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool));
    
    // Register handler
    router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();
    
    // Verify handler is registered
    assert!(router.has_handler(&CommandType::RegisterWorker).await);
}