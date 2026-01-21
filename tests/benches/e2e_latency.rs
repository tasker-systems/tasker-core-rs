//! # End-to-End Task Completion Benchmarks (TAS-29 Phase 5.4)
//!
//! Measures complete workflow execution latency across the entire distributed system.
//!
//! ## What This Measures
//!
//! - **API Call → Task Complete**: Complete workflow execution time
//! - **All System Components**: API, Database, Message Queue, Worker, Events
//! - **Real Network Overhead**: Actual distributed system performance
//! - **Different Workflow Patterns**: Linear, Diamond complexity
//!
//! ## Why This Matters
//!
//! E2E latency represents the **user experience**:
//! - How long from "submit task" to "get result"
//! - Includes ALL system overhead (not just individual components)
//! - Reflects real production performance
//!
//! Worker-level breakdown (claim, execute, submit) comes from **OpenTelemetry traces**
//! via correlation IDs, not direct measurement.
//!
//! ## Expected Performance
//!
//! | Workflow Type | Steps | Target (p99) | Notes |
//! |--------------|-------|--------------|-------|
//! | Linear (Ruby) | 4 | < 800ms | Sequential execution via FFI |
//! | Diamond (Ruby) | 4 | < 1000ms | Parallel + join via FFI |
//! | Linear (Rust) | 4 | < 500ms | Native Rust (faster than Ruby) |
//! | Diamond (Rust) | 4 | < 800ms | Native Rust parallel execution |
//!
//! ## Prerequisites
//!
//! **All Docker Compose services must be running**:
//! ```bash
//! docker-compose -f docker/docker-compose.test.yml up --build -d
//!
//! # Verify all services healthy
//! curl http://localhost:8080/health  # Orchestration
//! curl http://localhost:8081/health  # Rust Worker
//! ```
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # This is a SLOW benchmark (measures actual workflow execution)
//! cargo bench --test e2e_latency
//! ```
//!
//! ## Important Notes
//!
//! - **Slow by design**: Measures actual workflow completion (hundreds of ms)
//! - **Fewer samples**: Uses sample_size=10 (vs 50 default) due to duration
//! - **Higher variance**: Network, worker scheduling, database state affect results
//! - **Regression detection**: Focus on trends, not absolute numbers

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
use tasker_shared::models::core::task_request::TaskRequest;

// ===================================================================================
// SETUP AND UTILITIES
// ===================================================================================

/// Verify orchestration service is healthy
fn ensure_services_ready() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("🔍 VERIFYING DOCKER SERVICES");
    eprintln!("{}", "═".repeat(80));

    let runtime = tokio::runtime::Runtime::new()?;
    let response =
        runtime.block_on(async { reqwest::get("http://localhost:8080/health").await })?;

    if !response.status().is_success() {
        eprintln!("❌ Orchestration service not healthy");
        eprintln!("\nPlease start Docker Compose services:");
        eprintln!("  docker-compose -f docker/docker-compose.test.yml up --build -d\n");
        return Err("Orchestration service not healthy".into());
    }

    eprintln!("✅ Orchestration service healthy (http://localhost:8080)");
    eprintln!("{}\n", "═".repeat(80));

    Ok(())
}

/// Create a TaskRequest
///
/// TAS-154: Injects a unique test_run_id into the context to ensure each benchmark
/// iteration produces a unique identity hash.
fn create_task_request(
    namespace: &str,
    name: &str,
    input_context: serde_json::Value,
) -> TaskRequest {
    // TAS-154: Inject unique test_run_id to ensure unique identity hash per iteration
    let mut context = match input_context {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    context.insert(
        "_test_run_id".to_string(),
        serde_json::Value::String(Uuid::now_v7().to_string()),
    );

    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context: serde_json::Value::Object(context),
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        initiator: "e2e-benchmark".to_string(),
        source_system: "criterion-benchmarks".to_string(),
        reason: "E2E latency measurement".to_string(),
        tags: vec!["benchmark".to_string(), "e2e".to_string()],

        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        idempotency_key: None,
    }
}

// ===================================================================================
// BENCHMARKS
// ===================================================================================

/// Benchmark complete workflow execution (API → Complete)
///
/// **Approach**:
/// 1. Create task via orchestration API
/// 2. Poll task status every 50ms
/// 3. Detect completion (execution_status = "AllComplete")
/// 4. Measure total elapsed time
/// 5. Timeout after 10s if stuck
///
/// **Note**: This naturally includes ALL system overhead:
/// - API processing
/// - Database writes
/// - Message queue latency
/// - Worker claim/execute/submit
/// - Event propagation
/// - Orchestration coordination
fn bench_complete_workflow(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("🔍 END-TO-END LATENCY BENCHMARK");
    eprintln!("{}", "═".repeat(80));
    eprintln!("\nMeasuring COMPLETE workflow execution:");
    eprintln!("  API → Database → Queue → Worker → Complete");
    eprintln!("\nComparing Ruby FFI vs Native Rust implementations:");
    eprintln!("  • Ruby: Via FFI (expected higher latency)");
    eprintln!("  • Rust: Native handlers (expected lower latency)");
    eprintln!("\nThis includes ALL system components naturally.");
    eprintln!("Worker breakdown available via OpenTelemetry traces (correlation ID).");
    eprintln!("{}\n", "═".repeat(80));

    // Verify services are running
    ensure_services_ready().expect("Docker services must be running");

    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 30000,
        max_retries: 1,
        auth: None,
    };

    let client = OrchestrationApiClient::new(config).expect("Failed to create client");

    let mut group = c.benchmark_group("e2e_task_completion");
    group.sample_size(10); // Very few samples - these are SLOW
    group.measurement_time(Duration::from_secs(30)); // Long measurement window

    // Test scenarios: (name, namespace, handler, context)
    // Note: Templates require "even_number" field (not "value")
    // Testing BOTH Ruby and Rust implementations for performance comparison
    let scenarios = vec![
        // Ruby FFI implementations - RE-ENABLED (TAS-29 Phase 5.4)
        // State transition race conditions FIXED:
        // 1. Worker command processor event firing (tasker-worker/src/worker/command_processor.rs)
        // 2. Orchestration PGMQ timing (tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs)
        (
            "linear_ruby",
            "linear_workflow",
            "mathematical_sequence",
            json!({"even_number": 6}),
        ),
        (
            "diamond_ruby",
            "diamond_workflow",
            "parallel_computation",
            json!({"even_number": 6}),
        ),
        // Native Rust implementations - WORKING CORRECTLY
        // Both scenarios complete reliably under high concurrency load
        (
            "linear_rust",
            "rust_e2e_linear",
            "mathematical_sequence",
            json!({"even_number": 6}),
        ),
        (
            "diamond_rust",
            "rust_e2e_diamond",
            "diamond_pattern",
            json!({"even_number": 6}),
        ),
    ];

    let runtime = tokio::runtime::Runtime::new().unwrap();

    for (scenario_name, namespace, handler, context) in scenarios {
        group.bench_function(BenchmarkId::new("complete_workflow", scenario_name), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    let client = client.clone();
                    let request = create_task_request(namespace, handler, context.clone());

                    // Create task
                    let response = client
                        .create_task(request)
                        .await
                        .expect("Task creation failed");

                    let task_uuid =
                        Uuid::parse_str(&response.task_uuid).expect("Invalid task UUID");

                    let start = std::time::Instant::now();

                    // Poll for completion
                    loop {
                        tokio::time::sleep(Duration::from_millis(50)).await;

                        let task = client
                            .get_task(task_uuid)
                            .await
                            .expect("Failed to get task");

                        // Debug: Print status during first 1.1 seconds
                        if start.elapsed().as_millis() < 1100 {
                            eprintln!(
                                "  [{:.1}s] exec_status: {} | state: {} | steps: {}/{} complete",
                                start.elapsed().as_secs_f64(),
                                task.execution_status,
                                task.status,
                                task.completed_steps,
                                task.total_steps
                            );
                        }

                        // Check execution status (note: snake_case, not PascalCase)
                        if task.execution_status == "all_complete" {
                            return start.elapsed();
                        }

                        // Check for failure states
                        if task.execution_status == "blocked_by_failures" {
                            panic!("Task failed: {}", task.execution_status);
                        }

                        // Timeout after 10s
                        if start.elapsed() > Duration::from_secs(10) {
                            eprintln!(
                                "  Final exec_status: {} | state: {} | steps: {}/{}",
                                task.execution_status,
                                task.status,
                                task.completed_steps,
                                task.total_steps
                            );
                            panic!("Task did not complete within 10s timeout");
                        }
                    }
                })
            });
        });
    }

    group.finish();

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 END-TO-END LATENCY BENCHMARKS COMPLETE");
    eprintln!("{}", "═".repeat(80));
    eprintln!("\nThis measures REAL user-facing latency across BOTH implementations:");
    eprintln!("  • Ruby FFI handlers (via magnus FFI bridge)");
    eprintln!("  • Native Rust handlers (compiled, zero-overhead)");
    eprintln!("\nFor worker-level breakdown, use OpenTelemetry traces:");
    eprintln!("  • Query traces by correlation_id");
    eprintln!("  • See step_claim, execute_handler, submit_result spans");
    eprintln!("  • Analyze timing across distributed workers");
    eprintln!("  • Compare FFI overhead vs native execution");
    eprintln!("{}\n", "═".repeat(80));
}

// ===================================================================================
// CRITERION CONFIGURATION
// ===================================================================================

criterion_group!(benches, bench_complete_workflow);
criterion_main!(benches);
