//! # End-to-End Task Completion Benchmarks (TAS-159)
//!
//! Comprehensive benchmark suite measuring workflow execution latency across multiple tiers.
//!
//! ## Benchmark Tiers
//!
//! | Tier | Focus | Requirements |
//! |------|-------|--------------|
//! | 1 | Core Performance (Rust native) | Services running |
//! | 2 | Complexity Scaling (DAG, Tree, Conditional) | Services running |
//! | 3 | Cluster Performance (multi-instance) | Cluster running |
//! | 4 | FFI Language Comparison (Ruby, Python, TS) | Language workers running |
//! | 5 | Batch Processing (CSV, parallel workers) | Services running |
//!
//! ## What This Measures
//!
//! - **API Call → Task Complete**: Complete workflow execution time
//! - **All System Components**: API, Database, Message Queue, Worker, Events
//! - **Real Network Overhead**: Actual distributed system performance
//! - **Different Workflow Patterns**: Linear, Diamond, DAG, Tree, Conditional, Batch
//!
//! ## Prerequisites
//!
//! **Tier 1-2, 5**: Single instance services
//! ```bash
//! cargo make services-start
//! ```
//!
//! **Tier 3**: Multi-instance cluster
//! ```bash
//! cargo make cluster-start-all
//! ```
//!
//! **Tier 4**: FFI language workers
//! ```bash
//! cargo make run-worker-ruby &
//! cargo make run-worker-python &
//! cargo make run-worker-typescript &
//! ```
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Tier 1 only (Rust baseline)
//! cargo make bench-e2e
//!
//! # Tier 1 + Tier 2 (+ complexity)
//! cargo make bench-e2e-full
//!
//! # Tier 3 (cluster - requires cluster running)
//! cargo make bench-e2e-cluster
//!
//! # Tier 4 (FFI languages - requires language workers)
//! cargo make bench-e2e-languages
//!
//! # Tier 5 (batch processing)
//! cargo make bench-e2e-batch
//!
//! # All tiers
//! cargo make bench-e2e-all
//! ```

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
use tasker_shared::models::core::task_request::TaskRequest;

// ===================================================================================
// TIMEOUT CONSTANTS (TAS-159)
// ===================================================================================

/// Timeout for simple 4-step patterns (linear, diamond)
const TIMEOUT_SIMPLE_MS: u64 = 10_000;

/// Timeout for complex patterns (7-step DAG, conditional routing)
const TIMEOUT_COMPLEX_MS: u64 = 15_000;

/// Timeout for deep tree patterns (8-step hierarchical)
const TIMEOUT_TREE_MS: u64 = 20_000;

/// Timeout for FFI workers (additional overhead buffer)
const TIMEOUT_FFI_MS: u64 = 15_000;

/// Timeout for batch processing (multiple workers + aggregation)
const TIMEOUT_BATCH_MS: u64 = 45_000;

/// Timeout for cluster operations (multi-instance coordination)
const TIMEOUT_CLUSTER_MS: u64 = 20_000;

// ===================================================================================
// SETUP AND UTILITIES
// ===================================================================================

/// Verify orchestration service is healthy
fn ensure_services_ready() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let response =
        runtime.block_on(async { reqwest::get("http://localhost:8080/health").await })?;

    if !response.status().is_success() {
        eprintln!("❌ Orchestration service not healthy");
        eprintln!("\nPlease start services with:");
        eprintln!("  cargo make services-start\n");
        return Err("Orchestration service not healthy".into());
    }

    Ok(())
}

/// Check if a worker is healthy on the given port
fn check_worker_health(port: u16) -> bool {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return false,
    };

    let url = format!("http://localhost:{}/health", port);
    runtime.block_on(async {
        match reqwest::get(&url).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    })
}

/// Check if cluster orchestration instances are healthy
fn check_cluster_health() -> bool {
    // Check both orchestration instances (8080, 8081)
    check_worker_health(8080) && check_worker_health(8081)
}

/// Create a TaskRequest with unique identity hash per iteration
///
/// TAS-154: Injects a unique test_run_id into the context to ensure each benchmark
/// iteration produces a unique identity hash.
fn create_task_request(
    namespace: &str,
    name: &str,
    input_context: serde_json::Value,
) -> TaskRequest {
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

/// Execute a benchmark scenario with the given timeout
async fn execute_benchmark_scenario(
    client: &OrchestrationApiClient,
    namespace: &str,
    name: &str,
    context: serde_json::Value,
    timeout_ms: u64,
) -> Duration {
    let request = create_task_request(namespace, name, context);

    // Create task
    let response = client
        .create_task(request)
        .await
        .expect("Task creation failed");

    let task_uuid = Uuid::parse_str(&response.task_uuid).expect("Invalid task UUID");
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    // Poll for completion
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;

        let task = client
            .get_task(task_uuid)
            .await
            .expect("Failed to get task");

        if task.execution_status == "all_complete" {
            return start.elapsed();
        }

        if task.execution_status == "blocked_by_failures" {
            panic!(
                "Task failed: {} (status: {})",
                task.execution_status, task.status
            );
        }

        if start.elapsed() > timeout {
            panic!(
                "Task {} did not complete within {:?} timeout. Status: {}, Execution: {}, Steps: {}/{}",
                task_uuid,
                timeout,
                task.status,
                task.execution_status,
                task.completed_steps,
                task.total_steps
            );
        }
    }
}

// ===================================================================================
// TIER 1: CORE PERFORMANCE (Rust Native Baseline)
// ===================================================================================

/// Benchmark Tier 1: Core Rust native performance
///
/// Scenarios:
/// - linear_rust: 4-step sequential workflow
/// - diamond_rust: 4-step parallel + convergence
fn bench_tier1_core(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 1: CORE PERFORMANCE (Rust Native)");
    eprintln!("{}", "═".repeat(80));

    ensure_services_ready().expect("Services must be running");

    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 30000,
        max_retries: 1,
        auth: None,
    };

    let client = OrchestrationApiClient::new(config).expect("Failed to create client");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("e2e_tier1_core");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    let scenarios = vec![
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

    for (scenario_name, namespace, handler, context) in scenarios {
        group.bench_function(BenchmarkId::new("workflow", scenario_name), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        namespace,
                        handler,
                        context.clone(),
                        TIMEOUT_SIMPLE_MS,
                    )
                    .await
                })
            });
        });
    }

    group.finish();
}

// ===================================================================================
// TIER 2: COMPLEXITY SCALING
// ===================================================================================

/// Benchmark Tier 2: Complexity scaling with different DAG patterns
///
/// Scenarios:
/// - complex_dag_rust: 7-step mixed DAG with multi-path convergence
/// - hierarchical_tree_rust: 8-step deep tree with 4-way convergence
/// - conditional_rust: 5-step dynamic routing based on decision points
fn bench_tier2_complexity(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 2: COMPLEXITY SCALING");
    eprintln!("{}", "═".repeat(80));

    ensure_services_ready().expect("Services must be running");

    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 30000,
        max_retries: 1,
        auth: None,
    };

    let client = OrchestrationApiClient::new(config).expect("Failed to create client");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("e2e_tier2_complexity");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(45));

    // Complex DAG: 7 steps, multi-path convergence
    group.bench_function(BenchmarkId::new("workflow", "complex_dag_rust"), |b| {
        b.iter(|| {
            runtime.block_on(async {
                execute_benchmark_scenario(
                    &client,
                    "rust_e2e_mixed_dag",
                    "complex_dag",
                    json!({"even_number": 6}),
                    TIMEOUT_COMPLEX_MS,
                )
                .await
            })
        });
    });

    // Hierarchical Tree: 8 steps, deep parallel fan-out
    group.bench_function(
        BenchmarkId::new("workflow", "hierarchical_tree_rust"),
        |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "rust_e2e_tree",
                        "hierarchical_tree",
                        json!({"even_number": 6}),
                        TIMEOUT_TREE_MS,
                    )
                    .await
                })
            });
        },
    );

    // Conditional: 5 steps (varies by decision), dynamic routing
    // Test with amount that triggers auto_approve path (amount < $1,000)
    group.bench_function(BenchmarkId::new("workflow", "conditional_rust"), |b| {
        b.iter(|| {
            runtime.block_on(async {
                execute_benchmark_scenario(
                    &client,
                    "conditional_approval_rust",
                    "approval_routing",
                    json!({"amount": 500, "requester": "benchmark", "purpose": "benchmark_test"}),
                    TIMEOUT_COMPLEX_MS,
                )
                .await
            })
        });
    });

    group.finish();
}

// ===================================================================================
// TIER 3: CLUSTER PERFORMANCE
// ===================================================================================

/// Benchmark Tier 3: Multi-instance cluster performance
///
/// Requires cluster to be running: `cargo make cluster-start-all`
///
/// Tests concurrent task creation and completion across multiple orchestration
/// instances to measure coordination overhead.
fn bench_tier3_cluster(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 3: CLUSTER PERFORMANCE");
    eprintln!("{}", "═".repeat(80));

    if !check_cluster_health() {
        eprintln!("⚠️  Cluster not available - skipping Tier 3 benchmarks");
        eprintln!("   Start cluster with: cargo make cluster-start-all");
        return;
    }

    eprintln!("✅ Cluster healthy (2x orchestration instances)");

    // Use round-robin across orchestration instances
    let urls = vec![
        "http://localhost:8080".to_string(),
        "http://localhost:8081".to_string(),
    ];

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("e2e_tier3_cluster");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(45));

    // Single task through cluster (tests basic cluster routing)
    group.bench_function(BenchmarkId::new("cluster", "single_task_linear"), |b| {
        let url_index = std::cell::RefCell::new(0usize);
        b.iter(|| {
            runtime.block_on(async {
                let idx = {
                    let mut idx = url_index.borrow_mut();
                    let current = *idx;
                    *idx = (*idx + 1) % urls.len();
                    current
                };

                let config = OrchestrationApiConfig {
                    base_url: urls[idx].clone(),
                    timeout_ms: 30000,
                    max_retries: 1,
                    auth: None,
                };
                let client = OrchestrationApiClient::new(config).expect("Failed to create client");

                execute_benchmark_scenario(
                    &client,
                    "rust_e2e_linear",
                    "mathematical_sequence",
                    json!({"even_number": 6}),
                    TIMEOUT_CLUSTER_MS,
                )
                .await
            })
        });
    });

    // Concurrent tasks through cluster (tests work distribution)
    group.bench_function(BenchmarkId::new("cluster", "concurrent_tasks_2x"), |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut handles = Vec::new();

                for (i, url) in urls.iter().enumerate() {
                    let url = url.clone();
                    let handle = tokio::spawn(async move {
                        let config = OrchestrationApiConfig {
                            base_url: url,
                            timeout_ms: 30000,
                            max_retries: 3, // Higher retry count for concurrent load
                            auth: None,
                        };
                        let client =
                            OrchestrationApiClient::new(config).expect("Failed to create client");

                        execute_benchmark_scenario(
                            &client,
                            "rust_e2e_linear",
                            "mathematical_sequence",
                            json!({"even_number": 6, "_instance": i}),
                            TIMEOUT_CLUSTER_MS,
                        )
                        .await
                    });
                    handles.push(handle);
                }

                let start = std::time::Instant::now();
                for handle in handles {
                    handle.await.expect("Task failed");
                }
                start.elapsed()
            })
        });
    });

    group.finish();
}

// ===================================================================================
// TIER 4: FFI LANGUAGE COMPARISON
// ===================================================================================

/// Benchmark Tier 4: FFI language worker comparison
///
/// Compares execution performance across different language workers:
/// - Ruby (port 8082): Magnus FFI bridge
/// - Python (port 8083): PyO3 FFI bridge
/// - TypeScript (port 8085): Bun/Node FFI bridge
///
/// Each worker has different event mechanics for non-blocking handler invocation.
fn bench_tier4_languages(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 4: FFI LANGUAGE COMPARISON");
    eprintln!("{}", "═".repeat(80));

    ensure_services_ready().expect("Orchestration must be running");

    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 30000,
        max_retries: 1,
        auth: None,
    };

    let client = OrchestrationApiClient::new(config).expect("Failed to create client");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("e2e_tier4_languages");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(45));

    // Ruby Worker (port 8082)
    if check_worker_health(8082) {
        eprintln!("✅ Ruby worker available (port 8082)");

        group.bench_function(BenchmarkId::new("ruby", "linear"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "linear_workflow",
                        "mathematical_sequence",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });

        group.bench_function(BenchmarkId::new("ruby", "diamond"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "diamond_workflow",
                        "parallel_computation",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });
    } else {
        eprintln!("⚠️  Ruby worker not available - skipping Ruby benchmarks");
        eprintln!("   Start with: cargo make run-worker-ruby");
    }

    // Python Worker (port 8083)
    if check_worker_health(8083) {
        eprintln!("✅ Python worker available (port 8083)");

        group.bench_function(BenchmarkId::new("python", "linear"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "linear_workflow_py",
                        "mathematical_sequence_py",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });

        group.bench_function(BenchmarkId::new("python", "diamond"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "diamond_workflow_py",
                        "parallel_computation_py",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });
    } else {
        eprintln!("⚠️  Python worker not available - skipping Python benchmarks");
        eprintln!("   Start with: cargo make run-worker-python");
    }

    // TypeScript Worker (port 8085)
    if check_worker_health(8085) {
        eprintln!("✅ TypeScript worker available (port 8085)");

        group.bench_function(BenchmarkId::new("typescript", "linear"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "linear_workflow_ts",
                        "mathematical_sequence_ts",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });

        group.bench_function(BenchmarkId::new("typescript", "diamond"), |b| {
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "diamond_workflow_ts",
                        "parallel_computation_ts",
                        json!({"even_number": 6}),
                        TIMEOUT_FFI_MS,
                    )
                    .await
                })
            });
        });
    } else {
        eprintln!("⚠️  TypeScript worker not available - skipping TypeScript benchmarks");
        eprintln!("   Start with: cargo make run-worker-typescript");
    }

    group.finish();
}

// ===================================================================================
// TIER 5: BATCH PROCESSING
// ===================================================================================

/// Benchmark Tier 5: Batch processing performance
///
/// Tests the batch processing pattern with:
/// - CSV file analysis (batchable step)
/// - Parallel batch workers (5 workers processing 200 rows each)
/// - Deferred convergence aggregation
fn bench_tier5_batch(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 5: BATCH PROCESSING");
    eprintln!("{}", "═".repeat(80));

    ensure_services_ready().expect("Services must be running");

    let config = OrchestrationApiConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_ms: 60000, // Longer timeout for batch operations
        max_retries: 1,
        auth: None,
    };

    let client = OrchestrationApiClient::new(config).expect("Failed to create client");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Get fixture path from environment
    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "tests/fixtures".to_string());
    let csv_file_path = format!("{}/products.csv", fixture_base);

    eprintln!("   CSV file: {}", csv_file_path);

    let mut group = c.benchmark_group("e2e_tier5_batch");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    // CSV Batch Processing: 1000 rows, 5 workers, ~200 rows each
    group.bench_function(
        BenchmarkId::new("batch", "csv_products_1000_rows"),
        |b| {
            let csv_path = csv_file_path.clone();
            b.iter(|| {
                runtime.block_on(async {
                    execute_benchmark_scenario(
                        &client,
                        "csv_processing_rust",
                        "csv_product_inventory_analyzer",
                        json!({
                            "csv_file_path": csv_path,
                            "analysis_mode": "inventory"
                        }),
                        TIMEOUT_BATCH_MS,
                    )
                    .await
                })
            });
        },
    );

    group.finish();
}

// ===================================================================================
// CRITERION CONFIGURATION
// ===================================================================================

criterion_group!(
    name = tier1_core;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30));
    targets = bench_tier1_core
);

criterion_group!(
    name = tier2_complexity;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(45));
    targets = bench_tier2_complexity
);

criterion_group!(
    name = tier3_cluster;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(45));
    targets = bench_tier3_cluster
);

criterion_group!(
    name = tier4_languages;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(45));
    targets = bench_tier4_languages
);

criterion_group!(
    name = tier5_batch;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(60));
    targets = bench_tier5_batch
);

criterion_main!(
    tier1_core,
    tier2_complexity,
    tier3_cluster,
    tier4_languages,
    tier5_batch
);
