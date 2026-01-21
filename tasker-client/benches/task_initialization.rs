//! # Task Initialization API Latency Benchmarks (TAS-29 Phase 5.4)
//!
//! Measures end-to-end API latency for task creation across different workflow complexities.
//!
//! ## What This Measures
//!
//! This benchmark suite focuses on the **API request → task initialized** latency:
//! - HTTP request parsing and validation
//! - Task record creation in PostgreSQL
//! - Initial step discovery from template
//! - Response generation and serialization
//!
//! ## What This Does NOT Measure
//!
//! - Step execution time (business logic)
//! - Message queue latency
//! - Worker processing overhead
//! - Complete workflow execution time
//!
//! For those measurements, see other Phase 5.4 benchmarks.
//!
//! ## Prerequisites
//!
//! **Docker Compose services must be running**:
//! ```bash
//! docker-compose -f docker/docker-compose.test.yml up --build -d
//!
//! # Verify orchestration service is healthy
//! curl http://localhost:8080/health
//! ```
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Standard run (requires Docker services)
//! cargo bench --package tasker-client --features benchmarks
//!
//! # Run with verbose output
//! cargo bench --package tasker-client --features benchmarks -- --verbose
//!
//! # Save baseline for comparison
//! cargo bench --package tasker-client --features benchmarks -- --save-baseline main
//!
//! # Compare to baseline
//! cargo bench --package tasker-client --features benchmarks -- --baseline main
//! ```
//!
//! ## Expected Performance
//!
//! | Workflow Type | Steps | Target p99 | Notes |
//! |--------------|-------|------------|-------|
//! | Linear (3 steps) | 3 | < 50ms | Simple sequential workflow |
//! | Diamond (4 steps) | 4 | < 75ms | Parallel execution pattern |
//!
//! ## Troubleshooting
//!
//! **Error: "Services must be running"**
//! - Start Docker Compose: `docker-compose -f docker/docker-compose.test.yml up -d`
//! - Check service health: `curl http://localhost:8080/health`
//!
//! **Error: Connection refused**
//! - Verify orchestration service is bound to port 8080
//! - Check for port conflicts: `lsof -i :8080`
//!
//! **High latency (>100ms)**
//! - Check database connection pooling
//! - Verify no other benchmarks running concurrently
//! - Consider network overhead if Docker on remote host

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::time::Duration;

use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
use tasker_shared::models::core::task_request::TaskRequest;
use uuid::Uuid;

// ===================================================================================
// SETUP AND CONFIGURATION
// ===================================================================================

/// Verify orchestration service is healthy before running benchmarks
///
/// This prevents confusing benchmark failures when services aren't available.
fn ensure_services_ready() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("🔍 VERIFYING DOCKER SERVICES");
    eprintln!("{}", "═".repeat(80));

    // Use async reqwest with tokio runtime
    let runtime = tokio::runtime::Runtime::new()?;
    let response =
        runtime.block_on(async { reqwest::get("http://localhost:8080/health").await })?;

    if !response.status().is_success() {
        eprintln!(
            "❌ Orchestration service not healthy (status: {})",
            response.status()
        );
        eprintln!("\nPlease start Docker Compose services:");
        eprintln!("  docker-compose -f docker/docker-compose.test.yml up --build -d\n");
        return Err("Orchestration service not healthy".into());
    }

    eprintln!("✅ Orchestration service healthy (http://localhost:8080)");
    eprintln!("{}\n", "═".repeat(80));

    Ok(())
}

/// Create a TaskRequest matching the integration test pattern
///
/// This helper ensures consistency with integration tests and CLI usage.
fn create_task_request(
    namespace: &str,
    name: &str,
    input_context: serde_json::Value,
) -> TaskRequest {
    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context: input_context,
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        idempotency_key: None,
        initiator: "benchmark-suite".to_string(),
        source_system: "criterion-benchmarks".to_string(),
        reason: "API latency benchmark measurement".to_string(),
        tags: vec!["benchmark".to_string(), "phase-5.4".to_string()],

        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
    }
}

// ===================================================================================
// BENCHMARK: API TASK CREATION LATENCY
// ===================================================================================

/// Benchmark task creation API latency across different workflow complexities
///
/// **What this measures**:
/// - HTTP request/response overhead
/// - Task record creation in database
/// - Initial step discovery from template
/// - Response serialization
///
/// **Test scenarios**:
/// - Linear workflow (3 steps): Sequential execution pattern
/// - Diamond workflow (4 steps): Parallel execution with join
///
/// **Network considerations**:
/// - Tests run against localhost (minimal network latency)
/// - Variance expected due to network jitter
/// - Focus on regression detection, not absolute numbers
fn bench_task_creation_api(c: &mut Criterion) {
    // Verify services are running before starting benchmarks
    ensure_services_ready().expect("Docker services must be running - see prerequisites in docs");

    // Create client with appropriate timeouts for benchmark environment
    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 30000,
        max_retries: 1, // Minimal retries for benchmarks to avoid masking issues
        auth: None,
    };

    let client =
        OrchestrationApiClient::new(config).expect("Failed to create orchestration client");

    let mut group = c.benchmark_group("task_creation_api");

    // Configure for network tests (fewer samples, longer measurement time)
    group.sample_size(20); // Fewer samples than default (50) due to network overhead
    group.measurement_time(Duration::from_secs(15)); // Longer measurement for stable results

    // Test scenarios with different workflow complexities
    // Format: (scenario_name, namespace, handler_name, context)
    let scenarios = vec![
        (
            "linear_3_steps",
            "linear_workflow",
            "mathematical_sequence",
            json!({"value": 42}),
        ),
        (
            "diamond_4_steps",
            "diamond_workflow",
            "diamond_pattern",
            json!({"value": 100}),
        ),
    ];

    // Create runtime once for all benchmarks
    let runtime = tokio::runtime::Runtime::new().unwrap();

    for (scenario_name, namespace, template_name, context) in scenarios {
        group.bench_function(BenchmarkId::new("create_task", scenario_name), |b| {
            b.iter(|| {
                let client = client.clone();
                let request = create_task_request(namespace, template_name, context.clone());

                runtime.block_on(async {
                    client
                        .create_task(request)
                        .await
                        .expect("Task creation failed - check orchestration service logs")
                })
            });
        });
    }

    group.finish();

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TASK CREATION API BENCHMARKS COMPLETE");
    eprintln!("{}", "═".repeat(80));
    eprintln!("\nNote: These benchmarks measure API latency only.");
    eprintln!("For complete workflow execution time, see bench_e2e_latency.rs");
    eprintln!("{}\n", "═".repeat(80));
}

// ===================================================================================
// CRITERION CONFIGURATION
// ===================================================================================

criterion_group!(benches, bench_task_creation_api);
criterion_main!(benches);
