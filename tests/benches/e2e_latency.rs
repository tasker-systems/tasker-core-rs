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
//! ## Environment Variables
//!
//! Configuration follows the same patterns as `tests/common/integration_test_manager.rs`
//! and `tests/common/orchestration_cluster.rs`:
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `TASKER_TEST_ORCHESTRATION_URL` | Single orchestration URL | `http://localhost:8080` |
//! | `TASKER_TEST_ORCHESTRATION_URLS` | Comma-separated cluster URLs | - |
//! | `TASKER_TEST_RUBY_WORKER_URL` | Ruby worker URL | `http://localhost:8082` |
//! | `TASKER_TEST_PYTHON_WORKER_URL` | Python worker URL | `http://localhost:8083` |
//! | `TASKER_TEST_TS_WORKER_URL` | TypeScript worker URL | `http://localhost:8084` |
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
use std::env;
use std::time::Duration;
use uuid::Uuid;

use tasker_client::{
    OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient, WorkerApiConfig,
};
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
// BENCHMARK CONFIGURATION
// ===================================================================================
//
// Follows patterns from:
// - tests/common/integration_test_manager.rs (IntegrationConfig)
// - tests/common/orchestration_cluster.rs (ClusterConfig)

/// Benchmark configuration loaded from environment variables
#[derive(Debug, Clone)]
struct BenchmarkConfig {
    /// Primary orchestration URL (single-instance mode)
    orchestration_url: String,
    /// Cluster orchestration URLs (multi-instance mode)
    cluster_urls: Vec<String>,
    /// Ruby worker URL (if available)
    ruby_worker_url: Option<String>,
    /// Python worker URL (if available)
    python_worker_url: Option<String>,
    /// TypeScript worker URL (if available)
    typescript_worker_url: Option<String>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        // Primary orchestration URL
        let orchestration_url = env::var("TASKER_TEST_ORCHESTRATION_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        // Cluster URLs (comma-separated)
        let cluster_urls = env::var("TASKER_TEST_ORCHESTRATION_URLS")
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        // FFI worker URLs - check env var then try default port
        let ruby_worker_url = env::var("TASKER_TEST_RUBY_WORKER_URL")
            .ok()
            .or_else(|| Some("http://localhost:8082".to_string()));

        let python_worker_url = env::var("TASKER_TEST_PYTHON_WORKER_URL")
            .ok()
            .or_else(|| Some("http://localhost:8083".to_string()));

        let typescript_worker_url = env::var("TASKER_TEST_TS_WORKER_URL")
            .ok()
            .or_else(|| Some("http://localhost:8084".to_string()));

        Self {
            orchestration_url,
            cluster_urls,
            ruby_worker_url,
            python_worker_url,
            typescript_worker_url,
        }
    }
}

impl BenchmarkConfig {
    /// Check if cluster mode is configured (multiple orchestration URLs)
    fn is_cluster_mode(&self) -> bool {
        self.cluster_urls.len() > 1
    }

    /// Get orchestration URLs for the current mode
    fn orchestration_urls(&self) -> Vec<String> {
        if self.is_cluster_mode() {
            self.cluster_urls.clone()
        } else {
            vec![self.orchestration_url.clone()]
        }
    }
}

// ===================================================================================
// HEALTH CHECK UTILITIES (using tasker-client)
// ===================================================================================

/// Create an orchestration client for the given URL
fn create_orchestration_client(
    url: &str,
) -> Result<OrchestrationApiClient, Box<dyn std::error::Error>> {
    let config = OrchestrationApiConfig {
        base_url: url.to_string(),
        timeout_ms: 10000,
        max_retries: 1,
        auth: None,
    };
    OrchestrationApiClient::new(config).map_err(|e| e.into())
}

/// Create a worker client for the given URL
fn create_worker_client(url: &str) -> Result<WorkerApiClient, Box<dyn std::error::Error>> {
    let config = WorkerApiConfig {
        base_url: url.to_string(),
        timeout_ms: 5000,
        max_retries: 1,
        auth: None,
    };
    WorkerApiClient::new(config).map_err(|e| e.into())
}

/// Check if an orchestration service is healthy at the given URL using tasker-client
fn check_orchestration_health(runtime: &tokio::runtime::Runtime, url: &str) -> bool {
    match create_orchestration_client(url) {
        Ok(client) => runtime.block_on(client.health_check()).is_ok(),
        Err(_) => false,
    }
}

/// Check if a worker service is healthy at the given URL using tasker-client
fn check_worker_health(runtime: &tokio::runtime::Runtime, url: &str) -> bool {
    match create_worker_client(url) {
        Ok(client) => runtime
            .block_on(client.health_check())
            .map(|h| h.status == "healthy")
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Check if cluster orchestration instances are healthy
fn check_cluster_health(runtime: &tokio::runtime::Runtime, config: &BenchmarkConfig) -> bool {
    if !config.is_cluster_mode() {
        return false;
    }
    // All configured cluster URLs must be healthy
    config
        .cluster_urls
        .iter()
        .all(|url| check_orchestration_health(runtime, url))
}

/// Verify orchestration service is healthy and ready
fn ensure_services_ready(
    runtime: &tokio::runtime::Runtime,
    config: &BenchmarkConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !check_orchestration_health(runtime, &config.orchestration_url) {
        eprintln!(
            "❌ Orchestration service not healthy at {}",
            config.orchestration_url
        );
        eprintln!("\nPlease start services with:");
        eprintln!("  cargo make services-start\n");
        return Err("Orchestration service not healthy".into());
    }
    Ok(())
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

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bench_config = BenchmarkConfig::default();
    ensure_services_ready(&runtime, &bench_config).expect("Services must be running");

    let client = create_orchestration_client(&bench_config.orchestration_url)
        .expect("Failed to create client");

    let mut group = c.benchmark_group("e2e_tier1_core");
    group.sample_size(50);
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

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bench_config = BenchmarkConfig::default();
    ensure_services_ready(&runtime, &bench_config).expect("Services must be running");

    let client = create_orchestration_client(&bench_config.orchestration_url)
        .expect("Failed to create client");

    let mut group = c.benchmark_group("e2e_tier2_complexity");
    group.sample_size(50);
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
/// Configure with `TASKER_TEST_ORCHESTRATION_URLS=http://localhost:8080,http://localhost:8081`
///
/// Tests concurrent task creation and completion across multiple orchestration
/// instances to measure coordination overhead.
fn bench_tier3_cluster(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 3: CLUSTER PERFORMANCE");
    eprintln!("{}", "═".repeat(80));

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bench_config = BenchmarkConfig::default();

    if !check_cluster_health(&runtime, &bench_config) {
        eprintln!("⚠️  Cluster not available - skipping Tier 3 benchmarks");
        eprintln!("   Start cluster with: cargo make cluster-start-all");
        eprintln!(
            "   Set TASKER_TEST_ORCHESTRATION_URLS=http://localhost:8080,http://localhost:8081"
        );
        return;
    }

    let urls = bench_config.orchestration_urls();
    eprintln!(
        "✅ Cluster healthy ({} orchestration instances)",
        urls.len()
    );
    for url in &urls {
        eprintln!("   - {}", url);
    }

    // Create clients for each orchestration instance
    let clients: Vec<_> = urls
        .iter()
        .map(|url| create_orchestration_client(url).expect("Failed to create client"))
        .collect();

    let mut group = c.benchmark_group("e2e_tier3_cluster");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(45));

    // Single task through cluster (tests basic cluster routing via round-robin)
    group.bench_function(BenchmarkId::new("cluster", "single_task_linear"), |b| {
        let url_index = std::cell::RefCell::new(0usize);
        b.iter(|| {
            runtime.block_on(async {
                let idx = {
                    let mut idx = url_index.borrow_mut();
                    let current = *idx;
                    *idx = (*idx + 1) % clients.len();
                    current
                };

                execute_benchmark_scenario(
                    &clients[idx],
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
    let urls_for_concurrent = urls.clone();
    group.bench_function(BenchmarkId::new("cluster", "concurrent_tasks_2x"), |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut handles = Vec::new();

                for (i, url) in urls_for_concurrent.iter().enumerate() {
                    let url = url.clone();
                    let handle = tokio::spawn(async move {
                        let client =
                            create_orchestration_client(&url).expect("Failed to create client");
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
/// - Ruby: Magnus FFI bridge
/// - Python: PyO3 FFI bridge
/// - TypeScript: Bun/Node FFI bridge
///
/// Each worker has different event mechanics for non-blocking handler invocation.
///
/// Configure with environment variables:
/// - `TASKER_TEST_RUBY_WORKER_URL` (default: http://localhost:8082)
/// - `TASKER_TEST_PYTHON_WORKER_URL` (default: http://localhost:8083)
/// - `TASKER_TEST_TS_WORKER_URL` (default: http://localhost:8084)
fn bench_tier4_languages(c: &mut Criterion) {
    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 TIER 4: FFI LANGUAGE COMPARISON");
    eprintln!("{}", "═".repeat(80));

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bench_config = BenchmarkConfig::default();
    ensure_services_ready(&runtime, &bench_config).expect("Orchestration must be running");

    let client = create_orchestration_client(&bench_config.orchestration_url)
        .expect("Failed to create client");

    let mut group = c.benchmark_group("e2e_tier4_languages");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(45));

    // Ruby Worker
    if let Some(ref ruby_url) = bench_config.ruby_worker_url {
        if check_worker_health(&runtime, ruby_url) {
            eprintln!("✅ Ruby worker available ({})", ruby_url);

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
            eprintln!("⚠️  Ruby worker not available at {} - skipping", ruby_url);
            eprintln!("   Start with: cargo make run-worker-ruby");
        }
    }

    // Python Worker
    if let Some(ref python_url) = bench_config.python_worker_url {
        if check_worker_health(&runtime, python_url) {
            eprintln!("✅ Python worker available ({})", python_url);

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
            eprintln!(
                "⚠️  Python worker not available at {} - skipping",
                python_url
            );
            eprintln!("   Start with: cargo make run-worker-python");
        }
    }

    // TypeScript Worker
    if let Some(ref ts_url) = bench_config.typescript_worker_url {
        if check_worker_health(&runtime, ts_url) {
            eprintln!("✅ TypeScript worker available ({})", ts_url);

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
            eprintln!(
                "⚠️  TypeScript worker not available at {} - skipping",
                ts_url
            );
            eprintln!("   Start with: cargo make run-worker-typescript");
        }
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

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let bench_config = BenchmarkConfig::default();
    ensure_services_ready(&runtime, &bench_config).expect("Services must be running");

    let client = create_orchestration_client(&bench_config.orchestration_url)
        .expect("Failed to create client");

    // Get fixture path from environment
    let fixture_base =
        std::env::var("TASKER_FIXTURE_PATH").unwrap_or_else(|_| "tests/fixtures".to_string());
    let csv_file_path = format!("{}/products.csv", fixture_base);

    eprintln!("   CSV file: {}", csv_file_path);

    let mut group = c.benchmark_group("e2e_tier5_batch");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(60));

    // CSV Batch Processing: 1000 rows, 5 workers, ~200 rows each
    group.bench_function(BenchmarkId::new("batch", "csv_products_1000_rows"), |b| {
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
    });

    group.finish();
}

// ===================================================================================
// CRITERION CONFIGURATION
// ===================================================================================

// TAS-159: Sample size 50 enables meaningful p50/p95 percentile capture
// p99 requires 100+ samples but 50 provides reasonable p95 accuracy
criterion_group!(
    name = tier1_core;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(120));
    targets = bench_tier1_core
);

criterion_group!(
    name = tier2_complexity;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(150));
    targets = bench_tier2_complexity
);

criterion_group!(
    name = tier3_cluster;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(150));
    targets = bench_tier3_cluster
);

criterion_group!(
    name = tier4_languages;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(180));
    targets = bench_tier4_languages
);

criterion_group!(
    name = tier5_batch;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(180));
    targets = bench_tier5_batch
);

criterion_main!(
    tier1_core,
    tier2_complexity,
    tier3_cluster,
    tier4_languages,
    tier5_batch
);
