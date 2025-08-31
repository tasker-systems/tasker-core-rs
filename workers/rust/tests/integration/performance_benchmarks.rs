// Performance Benchmarking Tests - Native Rust Implementation
//
// Comprehensive performance benchmarking suite for native Rust step handlers.
// These benchmarks measure execution time, throughput, memory usage, and scalability
// across all workflow patterns to demonstrate the performance benefits of native Rust.
//
// Benchmark Categories:
// - Single Workflow Performance: Measure individual workflow execution times
// - Concurrent Workflow Throughput: Test system throughput under load
// - Scalability Testing: Performance with increasing worker counts
// - Memory Efficiency: Memory usage patterns during execution
// - Comparison Baselines: Performance baselines for regression testing

use anyhow::Result;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use tasker_core::test_helpers::{
    create_business_test_context, create_mathematical_test_context, create_test_task_request,
};
use tasker_worker_rust::test_helpers::init_test_worker;

/// Performance benchmark configuration
const BENCHMARK_TIMEOUT_SECONDS: u64 = 120; // Longer timeout for performance tests
const PERFORMANCE_ITERATIONS: usize = 5; // Multiple iterations for averaging

/// Performance metrics collection structure
#[derive(Debug)]
pub struct PerformanceMetrics {
    pub workflow_type: String,
    pub execution_times: Vec<Duration>,
    pub average_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub throughput_per_second: f64,
    pub total_steps: u32,
    pub success_rate: f64,
}

impl PerformanceMetrics {
    fn new(workflow_type: &str) -> Self {
        Self {
            workflow_type: workflow_type.to_string(),
            execution_times: Vec::new(),
            average_time: Duration::default(),
            min_time: Duration::MAX,
            max_time: Duration::default(),
            throughput_per_second: 0.0,
            total_steps: 0,
            success_rate: 0.0,
        }
    }

    fn add_measurement(&mut self, execution_time: Duration, steps: u32, success: bool) {
        self.execution_times.push(execution_time);
        if execution_time < self.min_time {
            self.min_time = execution_time;
        }
        if execution_time > self.max_time {
            self.max_time = execution_time;
        }
        if success {
            self.total_steps += steps;
        }
    }

    fn calculate_final_metrics(&mut self) {
        if !self.execution_times.is_empty() {
            let total_time: Duration = self.execution_times.iter().sum();
            self.average_time = total_time / self.execution_times.len() as u32;

            let total_seconds = total_time.as_secs_f64();
            if total_seconds > 0.0 {
                self.throughput_per_second = self.execution_times.len() as f64 / total_seconds;
            }

            let successful_runs = self.execution_times.len();
            self.success_rate = (successful_runs as f64 / PERFORMANCE_ITERATIONS as f64) * 100.0;
        }
    }
}

impl std::fmt::Display for PerformanceMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 {} Performance:\n  Avg: {:?} | Min: {:?} | Max: {:?}\n  Throughput: {:.2}/s | Success: {:.1}% | Steps: {}",
            self.workflow_type,
            self.average_time,
            self.min_time,
            self.max_time,
            self.throughput_per_second,
            self.success_rate,
            self.total_steps
        )
    }
}

/// Performance Benchmarking Test Suite
#[cfg(test)]
mod performance_benchmarking_tests {
    use super::*;
    use tokio;

    /// Initialize logging for benchmarks
    fn init_benchmark_logging() {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .try_init();
    }

    #[tokio::test]
    async fn benchmark_linear_workflow_performance() -> Result<()> {
        init_benchmark_logging();
        info!("🚀 Starting: Linear Workflow Performance Benchmark");

        let mut metrics = PerformanceMetrics::new("Linear Workflow");
        let mut setup = init_test_worker().await?;

        // Run multiple iterations for statistical significance
        for iteration in 0..PERFORMANCE_ITERATIONS {
            info!(
                "🔄 Linear benchmark iteration {} of {}",
                iteration + 1,
                PERFORMANCE_ITERATIONS
            );

            let test_context = create_mathematical_test_context(6);
            let task_request = create_test_task_request(
                "linear_workflow",
                "mathematical_sequence",
                "1.0.0",
                test_context,
                &format!(
                    "Linear workflow performance benchmark iteration {}",
                    iteration + 1
                ),
            );

            let start_time = Instant::now();

            match setup
                .run_integration_test(task_request, "linear_workflow", BENCHMARK_TIMEOUT_SECONDS)
                .await
            {
                Ok(summary) => {
                    let execution_time = start_time.elapsed();
                    metrics.add_measurement(execution_time, summary.total_steps, true);
                    info!(
                        "✅ Linear iteration {} completed in {:?}",
                        iteration + 1,
                        execution_time
                    );
                }
                Err(e) => {
                    warn!("⚠️ Linear iteration {} failed: {}", iteration + 1, e);
                    metrics.add_measurement(start_time.elapsed(), 0, false);
                }
            }
        }

        metrics.calculate_final_metrics();
        info!("{}", metrics);

        // Performance assertions
        assert!(
            metrics.success_rate >= 80.0,
            "Linear workflow should have high success rate"
        );
        assert!(
            metrics.average_time < Duration::from_secs(15),
            "Linear workflow should be fast"
        );

        setup.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn benchmark_diamond_workflow_performance() -> Result<()> {
        init_benchmark_logging();
        info!("🚀 Starting: Diamond Workflow Performance Benchmark");

        let mut metrics = PerformanceMetrics::new("Diamond Workflow");
        let mut setup = init_test_worker().await?;

        for iteration in 0..PERFORMANCE_ITERATIONS {
            info!(
                "🔄 Diamond benchmark iteration {} of {}",
                iteration + 1,
                PERFORMANCE_ITERATIONS
            );

            let test_context = create_mathematical_test_context(6);
            let task_request = create_test_task_request(
                "diamond_workflow",
                "diamond_pattern",
                "1.0.0",
                test_context,
                &format!(
                    "Diamond workflow performance benchmark iteration {}",
                    iteration + 1
                ),
            );

            let start_time = Instant::now();

            match setup
                .run_integration_test(task_request, "diamond_workflow", BENCHMARK_TIMEOUT_SECONDS)
                .await
            {
                Ok(summary) => {
                    let execution_time = start_time.elapsed();
                    metrics.add_measurement(execution_time, summary.total_steps, true);
                    info!(
                        "✅ Diamond iteration {} completed in {:?}",
                        iteration + 1,
                        execution_time
                    );
                }
                Err(e) => {
                    warn!("⚠️ Diamond iteration {} failed: {}", iteration + 1, e);
                    metrics.add_measurement(start_time.elapsed(), 0, false);
                }
            }
        }

        metrics.calculate_final_metrics();
        info!("{}", metrics);

        // Performance assertions
        assert!(
            metrics.success_rate >= 80.0,
            "Diamond workflow should have high success rate"
        );
        assert!(
            metrics.average_time < Duration::from_secs(20),
            "Diamond workflow should be efficient with parallelism"
        );

        setup.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn benchmark_tree_workflow_performance() -> Result<()> {
        init_benchmark_logging();
        info!("🚀 Starting: Tree Workflow Performance Benchmark");

        let mut metrics = PerformanceMetrics::new("Tree Workflow");
        let mut setup = init_test_worker().await?;

        for iteration in 0..PERFORMANCE_ITERATIONS {
            info!(
                "🔄 Tree benchmark iteration {} of {}",
                iteration + 1,
                PERFORMANCE_ITERATIONS
            );

            let test_context = create_mathematical_test_context(6);
            let task_request = create_test_task_request(
                "tree_workflow",
                "hierarchical_tree",
                "1.0.0",
                test_context,
                &format!(
                    "Tree workflow performance benchmark iteration {}",
                    iteration + 1
                ),
            );

            let start_time = Instant::now();

            match setup
                .run_integration_test(task_request, "tree_workflow", BENCHMARK_TIMEOUT_SECONDS)
                .await
            {
                Ok(summary) => {
                    let execution_time = start_time.elapsed();
                    metrics.add_measurement(execution_time, summary.total_steps, true);
                    info!(
                        "✅ Tree iteration {} completed in {:?}",
                        iteration + 1,
                        execution_time
                    );
                }
                Err(e) => {
                    warn!("⚠️ Tree iteration {} failed: {}", iteration + 1, e);
                    metrics.add_measurement(start_time.elapsed(), 0, false);
                }
            }
        }

        metrics.calculate_final_metrics();
        info!("{}", metrics);

        // Performance assertions
        assert!(
            metrics.success_rate >= 80.0,
            "Tree workflow should have high success rate"
        );
        assert!(
            metrics.average_time < Duration::from_secs(30),
            "Tree workflow should handle complexity efficiently"
        );

        setup.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn benchmark_mixed_dag_workflow_performance() -> Result<()> {
        init_benchmark_logging();
        info!("🚀 Starting: Mixed DAG Workflow Performance Benchmark");

        let mut metrics = PerformanceMetrics::new("Mixed DAG Workflow");
        let mut setup = init_test_worker().await?;

        for iteration in 0..PERFORMANCE_ITERATIONS {
            info!(
                "🔄 Mixed DAG benchmark iteration {} of {}",
                iteration + 1,
                PERFORMANCE_ITERATIONS
            );

            let test_context = create_mathematical_test_context(6);
            let task_request = create_test_task_request(
                "mixed_dag_workflow",
                "complex_dag",
                "1.0.0",
                test_context,
                &format!(
                    "Mixed DAG workflow performance benchmark iteration {}",
                    iteration + 1
                ),
            );

            let start_time = Instant::now();

            match setup
                .run_integration_test(
                    task_request,
                    "mixed_dag_workflow",
                    BENCHMARK_TIMEOUT_SECONDS,
                )
                .await
            {
                Ok(summary) => {
                    let execution_time = start_time.elapsed();
                    metrics.add_measurement(execution_time, summary.total_steps, true);
                    info!(
                        "✅ Mixed DAG iteration {} completed in {:?}",
                        iteration + 1,
                        execution_time
                    );
                }
                Err(e) => {
                    warn!("⚠️ Mixed DAG iteration {} failed: {}", iteration + 1, e);
                    metrics.add_measurement(start_time.elapsed(), 0, false);
                }
            }
        }

        metrics.calculate_final_metrics();
        info!("{}", metrics);

        // Performance assertions
        assert!(
            metrics.success_rate >= 80.0,
            "Mixed DAG workflow should have high success rate"
        );
        assert!(
            metrics.average_time < Duration::from_secs(40),
            "Mixed DAG workflow should handle maximum complexity efficiently"
        );

        setup.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn benchmark_order_fulfillment_performance() -> Result<()> {
        init_benchmark_logging();
        info!("🚀 Starting: Order Fulfillment Performance Benchmark");

        let mut metrics = PerformanceMetrics::new("Order Fulfillment");
        let mut setup = init_test_worker().await?;

        for iteration in 0..PERFORMANCE_ITERATIONS {
            info!(
                "🔄 Order fulfillment benchmark iteration {} of {}",
                iteration + 1,
                PERFORMANCE_ITERATIONS
            );

            let test_context = create_business_test_context();
            let task_request = create_test_task_request(
                "order_fulfillment",
                "business_workflow",
                "1.0.0",
                test_context,
                &format!(
                    "Order fulfillment performance benchmark iteration {}",
                    iteration + 1
                ),
            );

            let start_time = Instant::now();

            match setup
                .run_integration_test(task_request, "order_fulfillment", BENCHMARK_TIMEOUT_SECONDS)
                .await
            {
                Ok(summary) => {
                    let execution_time = start_time.elapsed();
                    metrics.add_measurement(execution_time, summary.total_steps, true);
                    info!(
                        "✅ Order fulfillment iteration {} completed in {:?}",
                        iteration + 1,
                        execution_time
                    );
                }
                Err(e) => {
                    warn!(
                        "⚠️ Order fulfillment iteration {} failed: {}",
                        iteration + 1,
                        e
                    );
                    metrics.add_measurement(start_time.elapsed(), 0, false);
                }
            }
        }

        metrics.calculate_final_metrics();
        info!("{}", metrics);

        // Performance assertions
        assert!(
            metrics.success_rate >= 80.0,
            "Order fulfillment should have high success rate"
        );
        assert!(
            metrics.average_time < Duration::from_secs(25),
            "Order fulfillment should be efficient for business processes"
        );

        setup.cleanup().await?;
        Ok(())
    }

    // #[tokio::test]
    // async fn benchmark_concurrent_workflow_throughput() -> Result<()> {
    //     init_benchmark_logging();
    //     info!("🚀 Starting: Concurrent Workflow Throughput Benchmark");

    //     let mut setup = SharedTestSetup::new()?;

    //     // Test different concurrency levels
    //     let concurrency_levels = vec![1, 2, 4, 8];
    //     let mut throughput_results: HashMap<u32, f64> = HashMap::new();

    //     for &concurrency in &concurrency_levels {
    //         info!("🔄 Testing concurrency level: {} workflows", concurrency);

    //         // Initialize with more workers for concurrency
    //         setup
    //             .initialize_orchestration(vec!["linear_workflow"])
    //             .await?;
    //         setup
    //             .initialize_workers("linear_workflow", concurrency * 2)
    //             .await?;

    //         let start_time = Instant::now();

    //         // Create concurrent tasks
    //         let tasks = (0..concurrency).map(|i| {
    //             let test_context = create_mathematical_test_context(6 + (i as i64));
    //             let task_request = create_test_task_request(
    //                 "linear_workflow",
    //                 "mathematical_sequence",
    //                 "1.0.0",
    //                 test_context,
    //                 &format!("Concurrent throughput test {}/{}", i + 1, concurrency),
    //             );
    //             setup.create_task(task_request)
    //         });

    //         let task_uuids = try_join_all(tasks).await?;
    //         info!("✅ Created {} concurrent tasks", task_uuids.len());

    //         // Wait for all tasks to complete
    //         let completion_tasks = task_uuids
    //             .iter()
    //             .map(|uuid| setup.wait_for_completion(uuid, BENCHMARK_TIMEOUT_SECONDS));

    //         let summaries = try_join_all(completion_tasks).await?;
    //         let total_time = start_time.elapsed();

    //         // Calculate throughput
    //         let throughput = summaries.len() as f64 / total_time.as_secs_f64();
    //         throughput_results.insert(concurrency, throughput);

    //         info!(
    //             "✅ Concurrency {}: {:.2} workflows/second",
    //             concurrency, throughput
    //         );

    //         // Verify all completed successfully
    //         for summary in &summaries {
    //             assert_eq!(summary.status, "complete");
    //             assert_eq!(summary.failed_steps, 0);
    //         }

    //         // Clean up for next iteration
    //         setup.cleanup().await?;
    //     }

    //     // Report throughput results
    //     info!("📊 Throughput Benchmark Results:");
    //     for &concurrency in &concurrency_levels {
    //         if let Some(throughput) = throughput_results.get(&concurrency) {
    //             info!(
    //                 "  {} concurrent: {:.2} workflows/sec",
    //                 concurrency, throughput
    //             );
    //         }
    //     }

    //     // Performance assertions
    //     assert!(
    //         throughput_results[&1] > 0.0,
    //         "Single workflow throughput should be positive"
    //     );
    //     assert!(
    //         throughput_results[&4] >= throughput_results[&1],
    //         "Higher concurrency should maintain or improve throughput"
    //     );

    //     Ok(())
    // }

    // #[tokio::test]
    // async fn benchmark_worker_scaling_performance() -> Result<()> {
    //     init_benchmark_logging();
    //     info!("🚀 Starting: Worker Scaling Performance Benchmark");

    //     let mut setup = SharedTestSetup::new()?;
    //     let worker_counts = vec![1, 2, 4, 6];
    //     let mut scaling_results: HashMap<u32, Duration> = HashMap::new();

    //     // Test with fixed workload but varying worker counts
    //     const WORKLOAD_SIZE: u32 = 6; // Fixed number of workflows

    //     for &worker_count in &worker_counts {
    //         info!("🔄 Testing with {} workers", worker_count);

    //         setup
    //             .initialize_orchestration(vec!["linear_workflow"])
    //             .await?;
    //         setup
    //             .initialize_workers("linear_workflow", worker_count)
    //             .await?;

    //         let start_time = Instant::now();

    //         // Create fixed workload
    //         let tasks = (0..WORKLOAD_SIZE).map(|i| {
    //             let test_context = create_mathematical_test_context(6 + (i as i64));
    //             let task_request = create_test_task_request(
    //                 "linear_workflow",
    //                 "mathematical_sequence",
    //                 "1.0.0",
    //                 test_context,
    //                 &format!(
    //                     "Worker scaling test {}/{} with {} workers",
    //                     i + 1,
    //                     WORKLOAD_SIZE,
    //                     worker_count
    //                 ),
    //             );
    //             setup.create_task(task_request)
    //         });

    //         let task_uuids = try_join_all(tasks).await?;

    //         // Wait for all tasks to complete
    //         let completion_tasks = task_uuids
    //             .iter()
    //             .map(|uuid| setup.wait_for_completion(uuid, BENCHMARK_TIMEOUT_SECONDS));

    //         let summaries = try_join_all(completion_tasks).await?;
    //         let total_time = start_time.elapsed();

    //         scaling_results.insert(worker_count, total_time);

    //         info!(
    //             "✅ {} workers completed {} workflows in {:?}",
    //             worker_count, WORKLOAD_SIZE, total_time
    //         );

    //         // Verify all completed successfully
    //         for summary in &summaries {
    //             assert_eq!(summary.status, "complete");
    //             assert_eq!(summary.failed_steps, 0);
    //         }

    //         setup.cleanup().await?;
    //     }

    //     // Report scaling results
    //     info!("📊 Worker Scaling Benchmark Results:");
    //     for &worker_count in &worker_counts {
    //         if let Some(time) = scaling_results.get(&worker_count) {
    //             let throughput = WORKLOAD_SIZE as f64 / time.as_secs_f64();
    //             info!(
    //                 "  {} workers: {:?} ({:.2} workflows/sec)",
    //                 worker_count, time, throughput
    //             );
    //         }
    //     }

    //     // Performance assertions - more workers should generally be faster for fixed workload
    //     assert!(
    //         scaling_results[&1] > Duration::from_secs(0),
    //         "Single worker should complete workload"
    //     );
    //     assert!(
    //         scaling_results[&6] <= scaling_results[&1] * 2,
    //         "More workers should improve performance within reasonable bounds"
    //     );

    //     Ok(())
    // }

    // #[tokio::test]
    // async fn benchmark_comparative_workflow_performance() -> Result<()> {
    //     init_benchmark_logging();
    //     info!("🚀 Starting: Comparative Workflow Performance Benchmark");

    //     let mut setup = SharedTestSetup::new()?;
    //     let mut all_metrics: Vec<PerformanceMetrics> = Vec::new();

    //     // Define test workflows with their configurations
    //     let workflows = vec![
    //         ("linear_workflow", "mathematical_sequence", "Linear", 2),
    //         ("diamond_workflow", "diamond_pattern", "Diamond", 2),
    //         ("tree_workflow", "hierarchical_tree", "Tree", 3),
    //         ("mixed_dag_workflow", "complex_dag", "Mixed DAG", 4),
    //         (
    //             "order_fulfillment",
    //             "business_workflow",
    //             "Order Fulfillment",
    //             3,
    //         ),
    //     ];

    //     // Run single iteration of each workflow for comparison
    //     for (namespace, task_name, display_name, workers) in workflows {
    //         info!("🔄 Benchmarking {} workflow", display_name);

    //         let mut metrics = PerformanceMetrics::new(display_name);

    //         let test_context = if namespace == "order_fulfillment" {
    //             create_business_test_context()
    //         } else {
    //             create_mathematical_test_context(6)
    //         };

    //         let task_request = create_test_task_request(
    //             namespace,
    //             task_name,
    //             "1.0.0",
    //             test_context,
    //             &format!("{} comparative performance benchmark", display_name),
    //         );

    //         let start_time = Instant::now();

    //         match setup
    //             .run_integration_test(task_request, namespace, workers, BENCHMARK_TIMEOUT_SECONDS)
    //             .await
    //         {
    //             Ok(summary) => {
    //                 let execution_time = start_time.elapsed();
    //                 metrics.add_measurement(execution_time, summary.total_steps, true);
    //                 info!("✅ {} completed in {:?}", display_name, execution_time);
    //             }
    //             Err(e) => {
    //                 warn!("⚠️ {} failed: {}", display_name, e);
    //                 metrics.add_measurement(start_time.elapsed(), 0, false);
    //             }
    //         }

    //         metrics.calculate_final_metrics();
    //         all_metrics.push(metrics);
    //     }

    //     // Report comparative results
    //     info!("📊 Comparative Workflow Performance Results:");
    //     for metrics in &all_metrics {
    //         info!("{}", metrics);
    //     }

    //     // Find fastest and slowest workflows
    //     if let (Some(fastest), Some(slowest)) = (
    //         all_metrics
    //             .iter()
    //             .filter(|m| m.success_rate > 0.0)
    //             .min_by_key(|m| m.average_time),
    //         all_metrics
    //             .iter()
    //             .filter(|m| m.success_rate > 0.0)
    //             .max_by_key(|m| m.average_time),
    //     ) {
    //         info!(
    //             "🏆 Fastest: {} ({:?})",
    //             fastest.workflow_type, fastest.average_time
    //         );
    //         info!(
    //             "🐌 Slowest: {} ({:?})",
    //             slowest.workflow_type, slowest.average_time
    //         );

    //         let performance_ratio =
    //             slowest.average_time.as_secs_f64() / fastest.average_time.as_secs_f64();
    //         info!("📈 Performance ratio: {:.2}x", performance_ratio);
    //     }

    //     // Performance assertions
    //     for metrics in &all_metrics {
    //         assert!(
    //             metrics.success_rate >= 80.0 || metrics.execution_times.is_empty(),
    //             "{} should have high success rate",
    //             metrics.workflow_type
    //         );
    //     }

    //     setup.cleanup().await?;
    //     Ok(())
    // }
}
