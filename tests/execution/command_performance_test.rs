//! Command Infrastructure Performance Tests
//! 
//! Performance and load testing for the Command pattern architecture,
//! measuring throughput, latency, and resource usage.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use tasker_core::execution::command::{
    Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities, SystemStats,
};
use tasker_core::execution::command_router::{CommandRouter, CommandRouterConfig};
use tasker_core::execution::worker_pool::{WorkerPool, WorkerPoolConfig, BatchRequest};
use tasker_core::execution::command_handlers::WorkerManagementHandler;
use tasker_core::execution::tokio_tcp_executor::{TokioTcpExecutor, TcpExecutorConfig};

#[tokio::test]
async fn test_command_serialization_performance() {
    let capabilities = WorkerCapabilities {
        worker_id: "perf_test_worker".to_string(),
        max_concurrent_steps: 20,
        supported_namespaces: vec![
            "orders".to_string(),
            "inventory".to_string(),
            "shipping".to_string(),
            "billing".to_string(),
        ],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "ruby".to_string(),
        version: "3.1.0".to_string(),
        custom_capabilities: {
            let mut caps = HashMap::new();
            caps.insert("feature_flags".to_string(), serde_json::json!(["flag1", "flag2", "flag3"]));
            caps.insert("max_memory_mb".to_string(), serde_json::json!(2048));
            caps.insert("supported_protocols".to_string(), serde_json::json!(["http", "grpc", "websocket"]));
            caps
        },
    };

    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: capabilities,
        },
        CommandSource::RubyWorker {
            id: "perf_test_worker".to_string(),
        },
    );

    // Benchmark serialization
    let iterations = 10_000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _json = serde_json::to_string(&command).expect("Serialization failed");
    }

    let serialization_time = start.elapsed();
    let per_operation = serialization_time.as_nanos() / iterations as u128;

    println!("Serialization: {} operations in {:?} ({} ns/op)", 
             iterations, serialization_time, per_operation);

    // Should be fast - under 10μs per operation
    assert!(per_operation < 10_000, "Serialization too slow: {} ns/op", per_operation);

    // Benchmark deserialization
    let json = serde_json::to_string(&command).unwrap();
    let start = Instant::now();

    for _ in 0..iterations {
        let _cmd: Command = serde_json::from_str(&json).expect("Deserialization failed");
    }

    let deserialization_time = start.elapsed();
    let per_operation = deserialization_time.as_nanos() / iterations as u128;

    println!("Deserialization: {} operations in {:?} ({} ns/op)", 
             iterations, deserialization_time, per_operation);

    // Should be fast - under 20μs per operation
    assert!(per_operation < 20_000, "Deserialization too slow: {} ns/op", per_operation);
}

#[tokio::test]
async fn test_command_router_throughput() {
    let config = CommandRouterConfig {
        enable_history: false, // Disable for max performance
        max_history_size: 0,
        default_timeout_ms: 30000,
    };

    let router = CommandRouter::with_config(config);
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    router.register_handler(CommandType::WorkerHeartbeat, handler).await.unwrap();

    // Pre-register a worker for heartbeat testing
    let capabilities = WorkerCapabilities {
        worker_id: "throughput_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["test".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
    };

    worker_pool.register_worker(capabilities).await.unwrap();

    // Benchmark command routing throughput
    let iterations = 1_000;
    let start = Instant::now();

    for i in 0..iterations {
        let command = Command::new(
            CommandType::WorkerHeartbeat,
            CommandPayload::WorkerHeartbeat {
                worker_id: "throughput_worker".to_string(),
                current_load: i % 10,
                system_stats: Some(SystemStats {
                    cpu_usage_percent: 50.0,
                    memory_usage_mb: 1024,
                    active_connections: 5,
                    uptime_seconds: 3600,
                }),
            },
            CommandSource::RubyWorker {
                id: "throughput_worker".to_string(),
            },
        );

        let result = router.route_command(command).await.unwrap();
        assert!(result.success);
    }

    let total_time = start.elapsed();
    let throughput = iterations as f64 / total_time.as_secs_f64();
    let per_operation = total_time.as_micros() / iterations as u128;

    println!("Command routing: {} ops/sec, {} μs/op", throughput, per_operation);

    // Should achieve at least 1000 commands/sec
    assert!(throughput > 1000.0, "Command routing too slow: {} ops/sec", throughput);
    
    // Each operation should take less than 1ms
    assert!(per_operation < 1000, "Command routing latency too high: {} μs/op", per_operation);
}

#[tokio::test]
async fn test_worker_pool_concurrent_operations() {
    let config = WorkerPoolConfig {
        max_workers: 1000,
        heartbeat_timeout_ms: 30000,
        cleanup_interval_ms: 60000,
        default_worker_timeout_ms: 30000,
    };

    let pool = WorkerPool::with_config(config);

    // Test concurrent worker registrations
    let concurrent_workers = 100;
    let start = Instant::now();

    let mut handles = Vec::new();
    for i in 0..concurrent_workers {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let capabilities = WorkerCapabilities {
                worker_id: format!("concurrent_worker_{}", i),
                max_concurrent_steps: 10,
                supported_namespaces: vec![format!("namespace_{}", i % 10)],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "rust".to_string(),
                version: "1.0.0".to_string(),
                custom_capabilities: HashMap::new(),
            };

            pool_clone.register_worker(capabilities).await
        });
        handles.push(handle);
    }

    // Wait for all registrations
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    let registration_time = start.elapsed();
    let registration_throughput = concurrent_workers as f64 / registration_time.as_secs_f64();

    println!("Worker registration: {} workers in {:?} ({} workers/sec)", 
             concurrent_workers, registration_time, registration_throughput);

    // Verify all workers registered
    let stats = pool.get_stats().await;
    assert_eq!(stats.total_workers, concurrent_workers);

    // Test concurrent worker selection
    let selection_iterations = 1000;
    let start = Instant::now();

    let mut selection_handles = Vec::new();
    for i in 0..selection_iterations {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let batch_request = BatchRequest {
                batch_id: format!("perf_batch_{}", i),
                steps: vec![],
                namespace: Some(format!("namespace_{}", i % 10)),
                priority: None,
                estimated_duration_ms: None,
            };

            pool_clone.select_worker_for_batch(&batch_request).await
        });
        selection_handles.push(handle);
    }

    let mut successful_selections = 0;
    for handle in selection_handles {
        if handle.await.unwrap().is_some() {
            successful_selections += 1;
        }
    }

    let selection_time = start.elapsed();
    let selection_throughput = selection_iterations as f64 / selection_time.as_secs_f64();

    println!("Worker selection: {} selections in {:?} ({} selections/sec, {}% success)", 
             selection_iterations, selection_time, selection_throughput,
             (successful_selections * 100) / selection_iterations);

    // Should achieve high throughput for worker selection
    assert!(selection_throughput > 5000.0, "Worker selection too slow: {} ops/sec", selection_throughput);
    
    // Most selections should succeed (workers available for most namespaces)
    assert!(successful_selections > selection_iterations * 80 / 100, 
            "Too many failed selections: {}%", (successful_selections * 100) / selection_iterations);
}

#[tokio::test]
async fn test_tcp_executor_connection_performance() {
    let config = TcpExecutorConfig {
        bind_address: "127.0.0.1:0".to_string(),
        command_queue_size: 10000,
        connection_timeout_ms: 5000,
        graceful_shutdown_timeout_ms: 2000,
        max_connections: 500,
    };

    let executor = TokioTcpExecutor::new(config).await.unwrap();
    
    // Register handlers for performance testing
    let router = executor.command_router();
    let worker_pool = executor.worker_pool();
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    router.register_handler(CommandType::RegisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::WorkerHeartbeat, handler).await.unwrap();

    executor.start().await.unwrap();

    // Test command routing performance through the executor
    let iterations = 500;
    let start = Instant::now();

    let mut routing_handles = Vec::new();
    for i in 0..iterations {
        let router_clone = router.clone();
        let handle = tokio::spawn(async move {
            let command = Command::new(
                CommandType::RegisterWorker,
                CommandPayload::RegisterWorker {
                    worker_capabilities: WorkerCapabilities {
                        worker_id: format!("perf_worker_{}", i),
                        max_concurrent_steps: 5,
                        supported_namespaces: vec!["performance_test".to_string()],
                        step_timeout_ms: 20000,
                        supports_retries: true,
                        language_runtime: "rust".to_string(),
                        version: "1.0.0".to_string(),
                        custom_capabilities: HashMap::new(),
                    },
                },
                CommandSource::RustOrchestrator {
                    id: format!("perf_coord_{}", i),
                },
            );

            router_clone.route_command(command).await
        });
        routing_handles.push(handle);
    }

    let mut successful_routes = 0;
    for handle in routing_handles {
        if handle.await.unwrap().unwrap().success {
            successful_routes += 1;
        }
    }

    let routing_time = start.elapsed();
    let routing_throughput = iterations as f64 / routing_time.as_secs_f64();

    println!("TCP Executor routing: {} routes in {:?} ({} routes/sec, {}% success)",
             iterations, routing_time, routing_throughput,
             (successful_routes * 100) / iterations);

    // Verify performance and success rate
    assert_eq!(successful_routes, iterations, "Some routes failed");
    assert!(routing_throughput > 200.0, "TCP routing too slow: {} ops/sec", routing_throughput);

    // Verify all workers were registered
    let stats = worker_pool.get_stats().await;
    assert_eq!(stats.total_workers, iterations);

    executor.stop().await.unwrap();
}

#[tokio::test]
async fn test_memory_usage_stability() {
    // Test that repeated operations don't cause memory leaks
    let router = CommandRouter::new();
    let worker_pool = Arc::new(WorkerPool::new());
    let handler = Arc::new(WorkerManagementHandler::new(worker_pool.clone()));

    router.register_handler(CommandType::RegisterWorker, handler.clone()).await.unwrap();
    router.register_handler(CommandType::UnregisterWorker, handler).await.unwrap();

    // Perform many register/unregister cycles
    let cycles = 1000;
    for i in 0..cycles {
        // Register worker
        let register_command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: WorkerCapabilities {
                    worker_id: format!("memory_test_worker_{}", i),
                    max_concurrent_steps: 5,
                    supported_namespaces: vec!["memory_test".to_string()],
                    step_timeout_ms: 10000,
                    supports_retries: true,
                    language_runtime: "rust".to_string(),
                    version: "1.0.0".to_string(),
                    custom_capabilities: HashMap::new(),
                },
            },
            CommandSource::RustOrchestrator {
                id: format!("memory_coord_{}", i),
            },
        );

        let result = router.route_command(register_command).await.unwrap();
        assert!(result.success);

        // Unregister worker
        let unregister_command = Command::new(
            CommandType::UnregisterWorker,
            CommandPayload::UnregisterWorker {
                worker_id: format!("memory_test_worker_{}", i),
                reason: "Memory test cleanup".to_string(),
            },
            CommandSource::RustOrchestrator {
                id: format!("memory_coord_{}", i),
            },
        );

        let result = router.route_command(unregister_command).await.unwrap();
        assert!(result.success);

        // Periodically check that workers are properly cleaned up
        if i % 100 == 99 {
            let stats = worker_pool.get_stats().await;
            assert_eq!(stats.total_workers, 0, "Workers not properly cleaned up at cycle {}", i);
        }
    }

    // Final verification - no workers should remain
    let final_stats = worker_pool.get_stats().await;
    assert_eq!(final_stats.total_workers, 0);
    assert_eq!(final_stats.total_capacity, 0);
    assert_eq!(final_stats.current_load, 0);
}

#[tokio::test]
async fn test_load_balancing_performance() {
    let pool = WorkerPool::new();

    // Register workers with different capacities
    for i in 0..50 {
        let capabilities = WorkerCapabilities {
            worker_id: format!("load_test_worker_{}", i),
            max_concurrent_steps: 5 + (i % 10), // Varying capacities
            supported_namespaces: vec![
                format!("namespace_{}", i % 5),  // 5 different namespaces
            ],
            step_timeout_ms: 30000,
            supports_retries: true,
            language_runtime: "rust".to_string(),
            version: "1.0.0".to_string(),
            custom_capabilities: HashMap::new(),
        };

        pool.register_worker(capabilities).await.unwrap();
    }

    // Test load balancing with concurrent batch requests
    let batch_count = 1000;
    let start = Instant::now();

    let mut selection_handles = Vec::new();
    for i in 0..batch_count {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let batch_request = BatchRequest {
                batch_id: format!("load_test_batch_{}", i),
                steps: vec![],
                namespace: Some(format!("namespace_{}", i % 5)),
                priority: None,
                estimated_duration_ms: None,
            };

            pool_clone.select_worker_for_batch(&batch_request).await
        });
        selection_handles.push(handle);
    }

    let mut worker_selection_counts = HashMap::new();
    for handle in selection_handles {
        if let Some(worker_id) = handle.await.unwrap() {
            *worker_selection_counts.entry(worker_id).or_insert(0) += 1;
        }
    }

    let selection_time = start.elapsed();
    let selection_throughput = batch_count as f64 / selection_time.as_secs_f64();

    println!("Load balancing: {} selections in {:?} ({} selections/sec)",
             batch_count, selection_time, selection_throughput);

    // Verify performance
    assert!(selection_throughput > 5000.0, "Load balancing too slow: {} selections/sec", selection_throughput);

    // Verify load distribution is reasonably balanced
    // With 50 workers and 1000 requests, average should be 20 per worker
    let total_selections: usize = worker_selection_counts.values().sum();
    let average_per_worker = total_selections as f64 / 50.0;
    
    println!("Load distribution: {} total selections, {:.1} average per worker", 
             total_selections, average_per_worker);

    // Most workers should have received some requests (within 50% of average)
    let workers_with_reasonable_load = worker_selection_counts
        .values()
        .filter(|&&count| {
            let ratio = count as f64 / average_per_worker;
            ratio >= 0.5 && ratio <= 2.0
        })
        .count();

    assert!(workers_with_reasonable_load >= 40, 
            "Load balancing too uneven: only {} workers with reasonable load", 
            workers_with_reasonable_load);
}