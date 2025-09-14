use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Duration;
use tasker_shared::{config::ConfigManager, models::task_request::TaskRequest};

/// Benchmark task request creation (without database)
fn benchmark_task_request_creation(c: &mut Criterion) {
    c.bench_function("task_request_creation", |b| {
        b.iter(|| {
            let context = serde_json::from_str(
                r#"{
                "benchmark": true,
                "iteration": 42,
                "timestamp": "2024-01-01T00:00:00Z",
                "complex_data": {
                    "nested": {
                        "values": [1, 2, 3, 4, 5],
                        "flags": true
                    }
                }
            }"#,
            )
            .unwrap();

            let task_request = TaskRequest::new("test_task".to_string(), "benchmark".to_string())
                .with_context(context)
                .with_initiator("benchmark_test".to_string())
                .with_source_system("benchmarks".to_string())
                .with_reason("Performance testing".to_string())
                .with_priority(black_box(3));
            // Note: with_claim_timeout_seconds method doesn't exist

            black_box(task_request)
        })
    });
}

/// Benchmark JSON serialization/deserialization
fn benchmark_json_processing(c: &mut Criterion) {
    c.bench_function("json_processing", |b| {
        b.iter(|| {
            let data = r#"{
                "task_uuid": 12345,
                "workflow_steps": [
                    {"name": "step1", "status": "complete", "result": {"value": 100}},
                    {"name": "step2", "status": "pending", "inputs": {"dependency": "step1"}},
                    {"name": "step3", "status": "blocked", "dependencies": ["step1", "step2"]}
                ],
                "metadata": {
                    "created_at": "2024-01-01T00:00:00Z",
                    "priority": 5,
                    "namespace": "workflow_test"
                }
            }"#;

            // Parse JSON
            let parsed: serde_json::Value = serde_json::from_str(data).unwrap();

            // Serialize back to string
            let serialized = serde_json::to_string(&parsed).unwrap();

            black_box(serialized)
        })
    });
}

/// Benchmark configuration loading performance
fn benchmark_config_loading(c: &mut Criterion) {
    c.bench_function("config_loading", |b| {
        b.iter(|| {
            let config = ConfigManager::load();
            black_box(config)
        })
    });
}

/// Benchmark UUID generation performance
fn benchmark_uuid_generation(c: &mut Criterion) {
    c.bench_function("uuid_generation", |b| {
        b.iter(|| {
            let uuid = uuid::Uuid::new_v4();
            let uuid_string = uuid.to_string();
            let parsed_uuid = uuid::Uuid::parse_str(&uuid_string).unwrap();
            black_box(parsed_uuid)
        })
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(20);
    targets =
        benchmark_task_request_creation,
        benchmark_json_processing,
        benchmark_config_loading,
        benchmark_uuid_generation
);
criterion_main!(benches);
