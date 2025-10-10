//! # SQL Function Benchmarks (TAS-29 Phase 5.2)
//!
//! Comprehensive benchmark suite for critical SQL functions with realistic data volumes.
//!
//! ## Intelligent Sampling Strategy
//!
//! Benchmarks use diverse sampling to ensure representative results:
//! - **Task samples**: 5 tasks from different `named_task_uuid` types
//! - **Step samples**: 10 steps from different tasks (up to 2 per task)
//! - **Deterministic ordering**: Same UUIDs in same order for consistent comparisons
//! - **Variance detection**: Captures performance across different complexities
//!
//! This approach provides much more realistic measurements than using a single UUID.
//!
//! ## Benchmark Categories
//!
//! 1. **Task Discovery**: `get_next_ready_tasks()` performance (4 batch sizes)
//! 2. **Step Readiness**: `get_step_readiness_status()` with various DAG complexities (5 samples)
//! 3. **State Transitions**: `transition_task_state_atomic()` atomic operations (5 samples)
//! 4. **Task Context**: `get_task_execution_context()` for orchestration status (5 samples)
//! 5. **Transitive Dependencies**: `get_step_transitive_dependencies()` recursive CTE traversal (10 samples)
//! 6. **Query Analysis**: EXPLAIN ANALYZE (runs once per function, not repeated since plans don't change)
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Setup test database first
//! export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
//! cargo sqlx migrate run
//!
//! # Run all SQL benchmarks
//! cargo bench --package tasker-shared --features benchmarks
//!
//! # Run specific benchmark group
//! cargo bench --package tasker-shared --features benchmarks get_next_ready_tasks
//!
//! # Generate report with baseline comparison
//! cargo bench --package tasker-shared --features benchmarks -- --save-baseline main
//! cargo bench --package tasker-shared --features benchmarks -- --baseline main
//! ```
//!
//! ## Understanding Results
//!
//! Criterion provides detailed statistics:
//! - **Mean**: Average execution time
//! - **Std Dev**: Variation in measurements
//! - **Median**: Middle value (less affected by outliers)
//! - **P95/P99**: 95th/99th percentile latency
//!
//! Look for:
//! - Linear scaling with data volume (good)
//! - Exponential scaling (indicates missing indexes)
//! - High std dev (contention or cache effects)

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

/// Initialize runtime and database connection
fn setup_runtime() -> (tokio::runtime::Runtime, PgPool) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let pool = runtime.block_on(async {
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for benchmarks");
        PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database")
    });
    (runtime, pool)
}

/// Sample diverse task UUIDs for benchmarking
///
/// Attempts to get a representative sample by:
/// 1. Finding distinct named_task_uuid types
/// 2. Sampling tasks from each type
/// 3. Returning a deterministic ordered list for consistent benchmarks
fn sample_task_uuids(
    runtime: &tokio::runtime::Runtime,
    pool: &PgPool,
    target_count: usize,
) -> Vec<Uuid> {
    runtime.block_on(async {
        // Strategy: Get tasks from different named_task types for diversity
        let result = sqlx::query_scalar::<_, Uuid>(
            "WITH task_types AS (
                SELECT DISTINCT named_task_uuid
                FROM tasker_tasks
                WHERE named_task_uuid IS NOT NULL
                ORDER BY named_task_uuid
            ),
            samples_per_type AS (
                SELECT
                    t.task_uuid,
                    t.named_task_uuid,
                    ROW_NUMBER() OVER (PARTITION BY t.named_task_uuid ORDER BY t.task_uuid) as rn
                FROM tasker_tasks t
                INNER JOIN task_types tt ON t.named_task_uuid = tt.named_task_uuid
            )
            SELECT task_uuid
            FROM samples_per_type
            WHERE rn <= GREATEST(1, $1 / (SELECT COUNT(*) FROM task_types))
            ORDER BY named_task_uuid, task_uuid
            LIMIT $1",
        )
        .bind(target_count as i32)
        .fetch_all(pool)
        .await;

        // If stratified sampling fails, fall back to simple query
        if let Ok(tasks) = result {
            return tasks;
        }

        // Fallback: just grab any tasks
        sqlx::query_scalar("SELECT task_uuid FROM tasker_tasks ORDER BY task_uuid LIMIT $1")
            .bind(target_count as i32)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
    })
}

/// Sample diverse step UUIDs for benchmarking
///
/// Similar to task sampling, but for workflow steps
fn sample_step_uuids(
    runtime: &tokio::runtime::Runtime,
    pool: &PgPool,
    target_count: usize,
) -> Vec<Uuid> {
    runtime.block_on(async {
        // Strategy: Get steps from different tasks for diversity
        let result = sqlx::query_scalar::<_, Uuid>(
            "WITH step_samples AS (
                SELECT
                    workflow_step_uuid,
                    task_uuid,
                    ROW_NUMBER() OVER (PARTITION BY task_uuid ORDER BY workflow_step_uuid) as rn
                FROM tasker_workflow_steps
            )
            SELECT workflow_step_uuid
            FROM step_samples
            WHERE rn <= 2  -- Get up to 2 steps per task for diversity
            ORDER BY task_uuid, workflow_step_uuid
            LIMIT $1"
        )
        .bind(target_count as i32)
        .fetch_all(pool)
        .await;

        // If stratified sampling fails, fall back to simple query
        if let Ok(steps) = result {
            return steps;
        }

        // Fallback: just grab any steps
        sqlx::query_scalar("SELECT workflow_step_uuid FROM tasker_workflow_steps ORDER BY workflow_step_uuid LIMIT $1")
            .bind(target_count as i32)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
    })
}

/// Benchmark 1: get_next_ready_tasks() performance
///
/// Measures task discovery speed - this is the core orchestration hot path.
/// Tests with empty database to measure function overhead itself.
fn bench_get_next_ready_tasks(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    let mut group = c.benchmark_group("get_next_ready_tasks");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    // Benchmark with different batch sizes
    for batch_size in [1, 10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &size| {
                b.iter(|| {
                    runtime.block_on(async {
                        sqlx::query(
                            "SELECT task_uuid, task_name, priority, namespace_name,
                                    ready_steps_count, computed_priority, current_state
                             FROM get_next_ready_tasks($1)",
                        )
                        .bind(size)
                        .fetch_all(&pool)
                        .await
                        .expect("Query failed")
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 2: get_step_readiness_status() performance
///
/// Measures step readiness calculation across diverse task samples.
/// Benchmarks multiple tasks to capture performance variance across different DAG complexities.
fn bench_step_readiness_status(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    // Sample diverse tasks for representative benchmarking
    let task_sample = sample_task_uuids(&runtime, &pool, 5);

    if task_sample.is_empty() {
        eprintln!("⚠️  Skipping step_readiness_status benchmark - no test data found");
        eprintln!("    Run integration tests first to populate test data");
        return;
    }

    let mut group = c.benchmark_group("step_readiness_status");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for (idx, task_uuid) in task_sample.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("calculate_readiness", idx),
            task_uuid,
            |b, &uuid| {
                b.iter(|| {
                    runtime.block_on(async {
                        sqlx::query(
                            "SELECT workflow_step_uuid, task_uuid, named_step_uuid, name,
                                    current_state, dependencies_satisfied, retry_eligible,
                                    ready_for_execution
                             FROM get_step_readiness_status($1)",
                        )
                        .bind(uuid)
                        .fetch_all(&pool)
                        .await
                        .expect("Query failed")
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 3: transition_task_state_atomic() performance
///
/// Measures state transition speed with atomic compare-and-swap across diverse tasks.
/// Tests the atomic transition logic with multiple task samples.
fn bench_state_transitions(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    // Sample diverse tasks for representative benchmarking
    let task_sample = sample_task_uuids(&runtime, &pool, 5);

    if task_sample.is_empty() {
        eprintln!("⚠️  Skipping state_transitions benchmark - no test data found");
        eprintln!("    Run integration tests first to populate test data");
        return;
    }

    let mut group = c.benchmark_group("state_transitions");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for (idx, task_uuid) in task_sample.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("atomic_transition_attempt", idx),
            task_uuid,
            |b, &uuid| {
                b.iter(|| {
                    runtime.block_on(async {
                        let processor_uuid = Uuid::now_v7();
                        // Attempt a transition (will likely fail if task isn't in right state)
                        let _result: Result<bool, _> = sqlx::query_scalar(
                            "SELECT transition_task_state_atomic($1, $2, $3, $4)",
                        )
                        .bind(uuid)
                        .bind("Pending")
                        .bind("Initializing")
                        .bind(processor_uuid)
                        .fetch_one(&pool)
                        .await;
                        // Ignore failures - we're measuring the function call time
                    })
                });
            },
        );
    }

    group.finish();
}

// Benchmark 4: claim_task_for_finalization() - REMOVED
// This function was removed from the codebase and doesn't exist in current schema.
// Finalization is now handled through the standard state transition mechanism.

/// Benchmark 4: get_task_execution_context() performance
///
/// Measures task context retrieval across diverse task samples.
/// Used heavily in orchestration - benchmarks multiple tasks to capture variance.
fn bench_task_execution_context(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    // Sample diverse tasks for representative benchmarking
    let task_sample = sample_task_uuids(&runtime, &pool, 5);

    if task_sample.is_empty() {
        eprintln!("⚠️  Skipping task_execution_context benchmark - no test data found");
        eprintln!("    Run integration tests first to populate test data");
        return;
    }

    let mut group = c.benchmark_group("task_execution_context");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for (idx, task_uuid) in task_sample.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("get_context", idx),
            task_uuid,
            |b, &uuid| {
                b.iter(|| {
                    runtime.block_on(async {
                        sqlx::query(
                            "SELECT task_uuid, named_task_uuid, status, total_steps,
                                    pending_steps, in_progress_steps, completed_steps,
                                    failed_steps, ready_steps, execution_status,
                                    recommended_action, completion_percentage, health_status
                             FROM get_task_execution_context($1)",
                        )
                        .bind(uuid)
                        .fetch_one(&pool)
                        .await
                        .expect("Query failed")
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 5: get_step_transitive_dependencies() performance
///
/// Measures transitive dependency resolution across diverse step samples.
/// Uses recursive CTE to traverse full dependency tree - benchmarks multiple steps
/// to capture variance in DAG depth and complexity.
fn bench_step_transitive_dependencies(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    // Sample diverse steps for representative benchmarking
    let step_sample = sample_step_uuids(&runtime, &pool, 10);

    if step_sample.is_empty() {
        eprintln!("⚠️  Skipping step_transitive_dependencies benchmark - no test data found");
        eprintln!("    Run integration tests first to populate test data");
        return;
    }

    let mut group = c.benchmark_group("step_transitive_dependencies");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(10));

    for (idx, step_uuid) in step_sample.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("get_dependencies", idx),
            step_uuid,
            |b, &uuid| {
                b.iter(|| {
                    runtime.block_on(async {
                        sqlx::query(
                            "SELECT workflow_step_uuid, task_uuid, named_step_uuid, step_name,
                                    results, processed, distance
                             FROM get_step_transitive_dependencies($1)",
                        )
                        .bind(uuid)
                        .fetch_all(&pool)
                        .await
                        .expect("Query failed")
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 6: EXPLAIN ANALYZE query plan capture
///
/// Captures query plans once for analysis - no need to run multiple times
/// since query plans don't change between executions.
fn bench_explain_analyze(c: &mut Criterion) {
    let (runtime, pool) = setup_runtime();

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("📊 QUERY PLAN ANALYSIS");
    eprintln!("{}\n", "═".repeat(80));

    // Capture get_next_ready_tasks plan
    runtime.block_on(async {
        let explain_result = sqlx::query_scalar::<_, serde_json::Value>(
            "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
             SELECT task_uuid, task_name, priority, namespace_name,
                    ready_steps_count, computed_priority, current_state
             FROM get_next_ready_tasks($1)",
        )
        .bind(10)
        .fetch_one(&pool)
        .await;

        if let Ok(plan) = explain_result {
            log_query_plan("get_next_ready_tasks", &plan);
        } else {
            eprintln!("⚠️  Failed to capture get_next_ready_tasks plan");
        }
    });

    // Get a task UUID for task_execution_context plan
    let task_uuid: Option<Uuid> = runtime.block_on(async {
        sqlx::query_scalar("SELECT task_uuid FROM tasker_tasks LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
    });

    if let Some(task_uuid) = task_uuid {
        runtime.block_on(async {
            let explain_result = sqlx::query_scalar::<_, serde_json::Value>(
                "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
                 SELECT task_uuid, named_task_uuid, status, total_steps,
                        pending_steps, in_progress_steps, completed_steps,
                        failed_steps, ready_steps, execution_status
                 FROM get_task_execution_context($1)",
            )
            .bind(task_uuid)
            .fetch_one(&pool)
            .await;

            if let Ok(plan) = explain_result {
                log_query_plan("get_task_execution_context", &plan);
            } else {
                eprintln!("⚠️  Failed to capture get_task_execution_context plan");
            }
        });
    } else {
        eprintln!("⚠️  Skipping get_task_execution_context plan - no test data found");
    }

    // Get a step UUID for step_transitive_dependencies plan
    let step_uuid: Option<Uuid> = runtime.block_on(async {
        sqlx::query_scalar("SELECT workflow_step_uuid FROM tasker_workflow_steps LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
    });

    if let Some(step_uuid) = step_uuid {
        runtime.block_on(async {
            let explain_result = sqlx::query_scalar::<_, serde_json::Value>(
                "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
                 SELECT workflow_step_uuid, task_uuid, named_step_uuid, step_name,
                        results, processed, distance
                 FROM get_step_transitive_dependencies($1)",
            )
            .bind(step_uuid)
            .fetch_one(&pool)
            .await;

            if let Ok(plan) = explain_result {
                log_query_plan("get_step_transitive_dependencies", &plan);
            } else {
                eprintln!("⚠️  Failed to capture get_step_transitive_dependencies plan");
            }
        });
    } else {
        eprintln!("⚠️  Skipping get_step_transitive_dependencies plan - no test data found");
    }

    eprintln!("\n{}", "═".repeat(80));
    eprintln!("Query plan analysis complete. Use SAVE_QUERY_PLANS=1 to export JSON files.");
    eprintln!("{}\n", "═".repeat(80));

    // Create a dummy benchmark to satisfy Criterion's requirements
    let mut group = c.benchmark_group("explain_analyze");
    group.sample_size(10);
    group.bench_function("plan_capture_complete", |b| {
        b.iter(|| {
            // No-op - plans already captured above
        });
    });
    group.finish();
}

/// Helper function to log query plans in a readable format
fn log_query_plan(function_name: &str, plan: &serde_json::Value) {
    if let Some(plan_array) = plan.as_array() {
        if let Some(plan_obj) = plan_array.first() {
            eprintln!("🔍 Function: {}", function_name);
            eprintln!("{}", "─".repeat(80));

            // Extract key metrics
            if let Some(exec_time) = plan_obj.get("Execution Time").and_then(|v| v.as_f64()) {
                eprintln!("⏱️  Execution Time: {:.3} ms", exec_time);
            }

            if let Some(planning_time) = plan_obj.get("Planning Time").and_then(|v| v.as_f64()) {
                eprintln!("📋 Planning Time: {:.3} ms", planning_time);
            }

            // Extract plan details
            if let Some(plan_detail) = plan_obj.get("Plan") {
                if let Some(node_type) = plan_detail.get("Node Type").and_then(|v| v.as_str()) {
                    eprintln!("📦 Node Type: {}", node_type);
                }

                if let Some(total_cost) = plan_detail.get("Total Cost").and_then(|v| v.as_f64()) {
                    eprintln!("💰 Total Cost: {:.2}", total_cost);
                }

                // Check for sequential scans (potential performance issue)
                let plan_str = serde_json::to_string(plan_detail).unwrap_or_default();
                if plan_str.contains("Seq Scan") {
                    eprintln!("⚠️  WARNING: Sequential scan detected - consider adding index");
                }

                // Check buffer statistics
                if let Some(shared_hit) = plan_detail
                    .get("Shared Hit Blocks")
                    .and_then(|v| v.as_u64())
                {
                    if let Some(shared_read) = plan_detail
                        .get("Shared Read Blocks")
                        .and_then(|v| v.as_u64())
                    {
                        let total = shared_hit + shared_read;
                        if total > 0 {
                            let hit_ratio = (shared_hit as f64 / total as f64) * 100.0;
                            eprintln!(
                                "📊 Buffer Hit Ratio: {:.1}% ({}/{} blocks)",
                                hit_ratio, shared_hit, total
                            );
                        }
                    }
                }
            }

            eprintln!("{}\n", "─".repeat(80));

            // Save full plan to file for detailed analysis
            if std::env::var("SAVE_QUERY_PLANS").is_ok() {
                let filename = format!("target/query_plan_{}.json", function_name);
                if let Ok(plan_json) = serde_json::to_string_pretty(plan) {
                    let _ = std::fs::write(&filename, plan_json);
                    eprintln!("💾 Full plan saved to: {}\n", filename);
                }
            }
        }
    }
}

criterion_group!(
    benches,
    bench_get_next_ready_tasks,
    bench_step_readiness_status,
    bench_state_transitions,
    bench_task_execution_context,
    bench_step_transitive_dependencies,
    bench_explain_analyze
);
criterion_main!(benches);
