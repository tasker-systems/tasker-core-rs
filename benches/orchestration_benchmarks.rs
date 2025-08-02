use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::sync::Arc;
use std::time::Duration;
use tasker_core::{
    execution::{
        command::{Command, CommandPayload, CommandSource, CommandType, WorkerCapabilities},
        command_handlers::WorkerManagementHandler,
        tokio_tcp_executor::TokioTcpExecutor,
        worker_pool::WorkerPool,
    },
    ffi::shared::handles::SharedOrchestrationHandle,
    models::core::task_request::TaskRequest,
    TaskerConfig,
};
use tokio::runtime::Runtime;

/// Benchmark TCP command processing performance
fn benchmark_tcp_command_processing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("tcp_command_worker_registration", |b| {
        b.to_async(&rt).iter(|| async {
            // Create worker capabilities for registration
            let worker_capabilities = WorkerCapabilities {
                worker_id: format!("benchmark_worker_{}", uuid::Uuid::new_v4()),
                max_concurrent_steps: 10,
                supported_namespaces: vec!["benchmark".to_string()],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "rust".to_string(),
                version: "1.0.0".to_string(),
                custom_capabilities: std::collections::HashMap::new(),
                connection_info: None,
                runtime_info: None,
                supported_tasks: None,
            };

            // Create registration command
            let command = Command::new(
                CommandType::RegisterWorker,
                CommandPayload::RegisterWorker {
                    worker_capabilities,
                },
                CommandSource::RustServer {
                    id: "benchmark".to_string(),
                },
            );

            // Time command creation and serialization
            let _serialized = serde_json::to_string(&command).unwrap();
        });
    });
}

/// Benchmark database-backed handler resolution
fn benchmark_handler_resolution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("shared_handle_handler_resolution", |b| {
        b.to_async(&rt).iter(|| async {
            let handle = SharedOrchestrationHandle::get_global();

            // Benchmark handler lookup
            let _result = handle.find_handler("benchmark", "test_task", "1.0.0");
        });
    });
}

/// Benchmark worker pool operations
fn benchmark_worker_pool_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("worker_pool_registration", |b| {
        b.to_async(&rt).iter(|| async {
            let worker_pool = Arc::new(WorkerPool::new());

            let worker_capabilities = WorkerCapabilities {
                worker_id: format!("pool_worker_{}", uuid::Uuid::new_v4()),
                max_concurrent_steps: 5,
                supported_namespaces: vec!["test".to_string()],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "rust".to_string(),
                version: "1.0.0".to_string(),
                custom_capabilities: std::collections::HashMap::new(),
                connection_info: None,
                runtime_info: None,
                supported_tasks: None,
            };

            // Time worker registration
            let _ = worker_pool.register_worker(worker_capabilities).await;
        });
    });
}

/// Benchmark task initialization performance
fn benchmark_task_initialization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("task_initialization_performance", |b| {
        b.to_async(&rt).iter(|| async {
            let handle = SharedOrchestrationHandle::get_global();

            let task_request = TaskRequest {
                namespace: "benchmark".to_string(),
                name: "test_task".to_string(),
                version: "1.0.0".to_string(),
                context: serde_json::json!({
                    "test_data": "benchmark_value",
                    "item_count": 100
                }),
                options: serde_json::json!({
                    "priority": "normal",
                    "timeout": 30000
                }),
            };

            // Time task creation request preparation
            let _serialized = serde_json::to_string(&task_request).unwrap();
        });
    });
}

/// Benchmark varying workload sizes
fn benchmark_workload_scaling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("workload_scaling");

    for worker_count in [1, 5, 10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_workers", worker_count),
            worker_count,
            |b, &worker_count| {
                b.to_async(&rt).iter(|| async move {
                    let worker_pool = Arc::new(WorkerPool::new());

                    // Register multiple workers concurrently
                    let mut handles = Vec::new();
                    for i in 0..worker_count {
                        let pool = worker_pool.clone();
                        let handle = tokio::spawn(async move {
                            let worker_capabilities = WorkerCapabilities {
                                worker_id: format!("scale_worker_{}_{}", worker_count, i),
                                max_concurrent_steps: 3,
                                supported_namespaces: vec!["scaling".to_string()],
                                step_timeout_ms: 30000,
                                supports_retries: true,
                                language_runtime: "rust".to_string(),
                                version: "1.0.0".to_string(),
                                custom_capabilities: std::collections::HashMap::new(),
                                connection_info: None,
                                runtime_info: None,
                                supported_tasks: None,
                            };

                            pool.register_worker(worker_capabilities).await
                        });
                        handles.push(handle);
                    }

                    // Wait for all registrations
                    for handle in handles {
                        let _ = handle.await;
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark command serialization/deserialization performance
fn benchmark_command_serialization(c: &mut Criterion) {
    let worker_capabilities = WorkerCapabilities {
        worker_id: "serialization_test_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["serialization".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: std::collections::HashMap::new(),
        connection_info: None,
        runtime_info: None,
        supported_tasks: None,
    };

    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities,
        },
        CommandSource::RustServer {
            id: "benchmark".to_string(),
        },
    );

    c.bench_function("command_serialization", |b| {
        b.iter(|| {
            let _json = serde_json::to_string(&command).unwrap();
        });
    });

    let json = serde_json::to_string(&command).unwrap();

    c.bench_function("command_deserialization", |b| {
        b.iter(|| {
            let _cmd: Command = serde_json::from_str(&json).unwrap();
        });
    });
}

/// Benchmark handle validation and refresh performance
fn benchmark_handle_operations(c: &mut Criterion) {
    c.bench_function("handle_validation", |b| {
        b.iter(|| {
            let handle = SharedOrchestrationHandle::get_global();
            let _is_valid = handle.validate();
        });
    });

    c.bench_function("handle_info_retrieval", |b| {
        b.iter(|| {
            let handle = SharedOrchestrationHandle::get_global();
            let _info = handle.info();
        });
    });
}

criterion_group!(
    tcp_command_benches,
    benchmark_tcp_command_processing,
    benchmark_handler_resolution,
    benchmark_worker_pool_operations,
    benchmark_task_initialization,
    benchmark_workload_scaling,
    benchmark_command_serialization,
    benchmark_handle_operations
);

criterion_main!(tcp_command_benches);
