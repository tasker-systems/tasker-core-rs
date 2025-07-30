//! TCP Command Performance Benchmarks
//!
//! Focused benchmarks for measuring the performance of our TCP command architecture
//! compared to baseline operations.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::collections::HashMap;
use tasker_core::execution::command::{
    Command, CommandType, CommandPayload, CommandSource, WorkerCapabilities,
};

/// Benchmark command creation and serialization
fn benchmark_command_operations(c: &mut Criterion) {
    // Test data for benchmarks
    let worker_capabilities = WorkerCapabilities {
        worker_id: "benchmark_worker".to_string(),
        max_concurrent_steps: 10,
        supported_namespaces: vec!["benchmark".to_string()],
        step_timeout_ms: 30000,
        supports_retries: true,
        language_runtime: "rust".to_string(),
        version: "1.0.0".to_string(),
        custom_capabilities: HashMap::new(),
        connection_info: None,
        runtime_info: None,
        supported_tasks: None,
    };

    // Benchmark command creation
    c.bench_function("command_creation", |b| {
        b.iter(|| {
            Command::new(
                CommandType::RegisterWorker,
                CommandPayload::RegisterWorker {
                    worker_capabilities: worker_capabilities.clone(),
                },
                CommandSource::RustServer {
                    id: "benchmark".to_string(),
                },
            )
        });
    });

    // Create command for serialization benchmarks
    let command = Command::new(
        CommandType::RegisterWorker,
        CommandPayload::RegisterWorker {
            worker_capabilities: worker_capabilities.clone(),
        },
        CommandSource::RustServer {
            id: "benchmark".to_string(),
        },
    );

    // Benchmark JSON serialization
    c.bench_function("json_serialization", |b| {
        b.iter(|| serde_json::to_string(&command).unwrap());
    });

    // Benchmark JSON deserialization
    let json_data = serde_json::to_string(&command).unwrap();
    c.bench_function("json_deserialization", |b| {
        b.iter(|| {
            let _: Command = serde_json::from_str(&json_data).unwrap();
        });
    });
}

/// Benchmark command payload size impact
fn benchmark_payload_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("payload_size_impact");

    for namespace_count in [1, 5, 10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("namespaces", namespace_count),
            namespace_count,
            |b, &count| {
                b.iter(|| {
                    let namespaces: Vec<String> = (0..count)
                        .map(|i| format!("namespace_{}", i))
                        .collect();

                    let worker_capabilities = WorkerCapabilities {
                        worker_id: format!("worker_with_{}_namespaces", count),
                        max_concurrent_steps: 10,
                        supported_namespaces: namespaces,
                        step_timeout_ms: 30000,
                        supports_retries: true,
                        language_runtime: "rust".to_string(),
                        version: "1.0.0".to_string(),
                        custom_capabilities: HashMap::new(),
                        connection_info: None,
                        runtime_info: None,
                        supported_tasks: None,
                    };

                    let command = Command::new(
                        CommandType::RegisterWorker,
                        CommandPayload::RegisterWorker { worker_capabilities },
                        CommandSource::RustServer {
                            id: "benchmark".to_string(),
                        },
                    );

                    // Measure serialization time for varying payload sizes
                    let _json = serde_json::to_string(&command).unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark various command types
fn benchmark_command_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("command_types");

    // Worker Registration
    group.bench_function("worker_registration", |b| {
        b.iter(|| {
            let worker_capabilities = WorkerCapabilities {
                worker_id: "test_worker".to_string(),
                max_concurrent_steps: 5,
                supported_namespaces: vec!["test".to_string()],
                step_timeout_ms: 30000,
                supports_retries: true,
                language_runtime: "rust".to_string(),
                version: "1.0.0".to_string(),
                custom_capabilities: HashMap::new(),
                connection_info: None,
                runtime_info: None,
                supported_tasks: None,
            };

            let command = Command::new(
                CommandType::RegisterWorker,
                CommandPayload::RegisterWorker { worker_capabilities },
                CommandSource::RustServer {
                    id: "test".to_string(),
                },
            );

            let _json = serde_json::to_string(&command).unwrap();
        });
    });

    // Worker Heartbeat
    group.bench_function("worker_heartbeat", |b| {
        b.iter(|| {
            let command = Command::new(
                CommandType::WorkerHeartbeat,
                CommandPayload::WorkerHeartbeat {
                    worker_id: "test_worker".to_string(),
                    current_load: 3,
                    system_stats: None,
                },
                CommandSource::RustServer {
                    id: "test".to_string(),
                },
            );

            let _json = serde_json::to_string(&command).unwrap();
        });
    });

    // Worker Unregistration
    group.bench_function("worker_unregistration", |b| {
        b.iter(|| {
            let command = Command::new(
                CommandType::UnregisterWorker,
                CommandPayload::UnregisterWorker {
                    worker_id: "test_worker".to_string(),
                    reason: "shutdown".to_string(),
                },
                CommandSource::RustServer {
                    id: "test".to_string(),
                },
            );

            let _json = serde_json::to_string(&command).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    tcp_command_performance,
    benchmark_command_operations,
    benchmark_payload_sizes,
    benchmark_command_types
);

criterion_main!(tcp_command_performance);