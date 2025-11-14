//! # Diamond-Decision-Batch Combined Workflow Handlers
//!
//! Integration test demonstrating composition of three workflow patterns:
//! - Diamond pattern (parallel branches with convergence)
//! - Decision points (dynamic step creation)
//! - Batch processing (cursor-based parallel processing)
//!
//! ## Workflow Structure
//!
//! ```text
//! diamond_start
//!     ├─→ branch_evens (filter even numbers)
//!     └─→ branch_odds (filter odd numbers)
//!             └─→ routing_decision (decision point)
//!                     ├─→ even_batch_analyzer (batchable)
//!                     │       ├─→ process_even_batch_001 (batch_worker)
//!                     │       └─→ process_even_batch_N (batch_worker)
//!                     │               └─→ aggregate_even_results (deferred_convergence)
//!                     │
//!                     └─→ odd_batch_analyzer (batchable)
//!                             ├─→ process_odd_batch_001 (batch_worker)
//!                             └─→ process_odd_batch_N (batch_worker)
//!                                     └─→ aggregate_odd_results (deferred_convergence)
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use tasker_shared::messaging::{
    BatchProcessingOutcome, CursorConfig, DecisionPointOutcome, StepExecutionResult,
};
use tasker_shared::types::TaskSequenceStep;

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};

// ============================================================================
// DIAMOND PATTERN HANDLERS
// ============================================================================

/// DiamondStartHandler - Initial step that validates and extracts numbers
///
/// Expects task context:
/// ```json
/// {
///   "numbers": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
///   "batch_size": 3
/// }
/// ```
pub struct DiamondStartHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DiamondStartHandler {
    fn name(&self) -> &'static str {
        "ddb_diamond_start"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "🔷 DiamondStartHandler: Validating numbers array"
        );

        // Extract numbers array from task context
        let numbers = step_data
            .task
            .task
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'numbers' array in context"))?;

        let numbers_vec: Vec<i64> = numbers.iter().filter_map(|v| v.as_i64()).collect();

        if numbers_vec.is_empty() {
            return Ok(error_result(
                step_uuid,
                "Empty numbers array".to_string(),
                Some("VALIDATION_ERROR".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("field".to_string(), json!("numbers"))])),
            ));
        }

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("DiamondStartHandler"));
        metadata.insert("count".to_string(), json!(numbers_vec.len()));

        tracing::info!(
            step_uuid = %step_uuid,
            count = numbers_vec.len(),
            "✅ DiamondStartHandler: Validated {} numbers",
            numbers_vec.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "all_numbers": numbers_vec,
                "count": numbers_vec.len()
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// BranchEvensHandler - Filters and counts even numbers
pub struct BranchEvensHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for BranchEvensHandler {
    fn name(&self) -> &'static str {
        "branch_evens"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "🔷 BranchEvensHandler: Filtering even numbers"
        );

        // Get all numbers from ddb_diamond_start dependency
        let all_numbers: Vec<i64> = step_data
            .dependency_results
            .get("ddb_diamond_start")
            .and_then(|r| r.result.get("all_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing all_numbers from ddb_diamond_start"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        // Filter evens
        let even_numbers: Vec<i64> = all_numbers.into_iter().filter(|n| n % 2 == 0).collect();
        let count = even_numbers.len();

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("BranchEvensHandler"));
        metadata.insert("even_count".to_string(), json!(count));

        tracing::info!(
            step_uuid = %step_uuid,
            count = count,
            "✅ BranchEvensHandler: Found {} even numbers",
            count
        );

        Ok(success_result(
            step_uuid,
            json!({
                "even_numbers": even_numbers,
                "count": count
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// BranchOddsHandler - Filters and counts odd numbers
pub struct BranchOddsHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for BranchOddsHandler {
    fn name(&self) -> &'static str {
        "branch_odds"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "🔷 BranchOddsHandler: Filtering odd numbers"
        );

        // Get all numbers from ddb_diamond_start dependency
        let all_numbers: Vec<i64> = step_data
            .dependency_results
            .get("ddb_diamond_start")
            .and_then(|r| r.result.get("all_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing all_numbers from ddb_diamond_start"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        // Filter odds
        let odd_numbers: Vec<i64> = all_numbers.into_iter().filter(|n| n % 2 != 0).collect();
        let count = odd_numbers.len();

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("BranchOddsHandler"));
        metadata.insert("odd_count".to_string(), json!(count));

        tracing::info!(
            step_uuid = %step_uuid,
            count = count,
            "✅ BranchOddsHandler: Found {} odd numbers",
            count
        );

        Ok(success_result(
            step_uuid,
            json!({
                "odd_numbers": odd_numbers,
                "count": count
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

// ============================================================================
// DECISION POINT HANDLER
// ============================================================================

/// RoutingDecisionHandler - Decision point that creates batchable steps
///
/// Compares even vs odd counts and returns DecisionPointOutcome to create
/// the appropriate batchable step (even_batch_analyzer or odd_batch_analyzer).
///
/// Decision logic:
/// - If even_count >= odd_count: Create even_batch_analyzer
/// - If odd_count > even_count: Create odd_batch_analyzer
pub struct RoutingDecisionHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for RoutingDecisionHandler {
    fn name(&self) -> &'static str {
        "ddb_routing_decision"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "🔀 RoutingDecisionHandler: Determining batch routing"
        );

        // Get counts from both branches
        let even_count: i64 = step_data
            .dependency_results
            .get("branch_evens")
            .and_then(|r| r.result.get("count"))
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing count from branch_evens"))?;

        let odd_count: i64 = step_data
            .dependency_results
            .get("branch_odds")
            .and_then(|r| r.result.get("count"))
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing count from branch_odds"))?;

        tracing::info!(
            step_uuid = %step_uuid,
            even_count = even_count,
            odd_count = odd_count,
            "📊 Counts: {} evens, {} odds",
            even_count,
            odd_count
        );

        // Decision logic: prefer evens if equal
        let (route_type, step_name, reasoning) = if even_count >= odd_count {
            (
                "even_dominant",
                "even_batch_analyzer",
                format!(
                    "Even count ({}) >= odd count ({}), routing to even batch processing",
                    even_count, odd_count
                ),
            )
        } else {
            (
                "odd_dominant",
                "odd_batch_analyzer",
                format!(
                    "Odd count ({}) > even count ({}), routing to odd batch processing",
                    odd_count, even_count
                ),
            )
        };

        // Create decision point outcome
        let outcome = DecisionPointOutcome::create_steps(vec![step_name.to_string()]);

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("RoutingDecisionHandler"));
        metadata.insert("route_type".to_string(), json!(route_type));
        metadata.insert("created_step".to_string(), json!(step_name));

        tracing::info!(
            step_uuid = %step_uuid,
            route_type = route_type,
            created_step = step_name,
            "✅ RoutingDecisionHandler: Creating step '{}'",
            step_name
        );

        Ok(success_result(
            step_uuid,
            json!({
                "route_type": route_type,
                "even_count": even_count,
                "odd_count": odd_count,
                "reasoning": reasoning,
                "decision_point_outcome": outcome.to_value()
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

// ============================================================================
// BATCH PROCESSING HANDLERS - EVEN PATH
// ============================================================================

/// EvenBatchAnalyzerHandler - Batchable step for even numbers
///
/// Creates batch workers to process even numbers in parallel batches.
pub struct EvenBatchAnalyzerHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for EvenBatchAnalyzerHandler {
    fn name(&self) -> &'static str {
        "even_batch_analyzer"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📦 EvenBatchAnalyzerHandler: Analyzing even numbers for batching"
        );

        // Get even numbers from branch_evens dependency
        let even_numbers: Vec<i64> = step_data
            .dependency_results
            .get("branch_evens")
            .and_then(|r| r.result.get("even_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing even_numbers from branch_evens"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        let dataset_size = even_numbers.len() as u64;
        if dataset_size == 0 {
            // No batches needed
            let outcome = BatchProcessingOutcome::no_batches();
            return Ok(success_result(
                step_uuid,
                json!({
                    "batch_processing_outcome": outcome.to_value(),
                    "reason": "No even numbers to process"
                }),
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Get configuration
        let batch_size = self.config.get_u64("batch_size").unwrap_or(3);
        let max_workers = self.config.get_u64("max_workers").unwrap_or(10);

        // Calculate worker count
        let ideal_workers = ((dataset_size as f64) / (batch_size as f64)).ceil() as u64;
        let worker_count = ideal_workers.min(max_workers);
        let actual_batch_size = ((dataset_size as f64) / (worker_count as f64)).ceil() as u64;

        // Create cursor configs
        let mut cursor_configs = Vec::new();
        for i in 0..worker_count {
            let start_idx = i * actual_batch_size;
            let end_idx = ((i + 1) * actual_batch_size).min(dataset_size);
            let items_in_batch = end_idx - start_idx;

            cursor_configs.push(CursorConfig {
                batch_id: format!("{:03}", i + 1),
                start_cursor: json!(start_idx),
                end_cursor: json!(end_idx),
                batch_size: items_in_batch as u32,
            });
        }

        let outcome = BatchProcessingOutcome::create_batches(
            "process_even_batch".to_string(),
            worker_count as u32,
            cursor_configs,
            dataset_size,
        );

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("EvenBatchAnalyzerHandler"));
        metadata.insert("worker_count".to_string(), json!(worker_count));
        metadata.insert("dataset_size".to_string(), json!(dataset_size));

        tracing::info!(
            step_uuid = %step_uuid,
            dataset_size = dataset_size,
            worker_count = worker_count,
            batch_size = actual_batch_size,
            "✅ EvenBatchAnalyzerHandler: Creating {} workers for {} even numbers",
            worker_count,
            dataset_size
        );

        Ok(success_result(
            step_uuid,
            json!({
                "batch_processing_outcome": outcome.to_value(),
                "dataset_size": dataset_size,
                "worker_count": worker_count,
                "batch_size": actual_batch_size
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// ProcessEvenBatchHandler - Batch worker for processing even number batches
pub struct ProcessEvenBatchHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessEvenBatchHandler {
    fn name(&self) -> &'static str {
        "process_even_batch"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "⚙️ ProcessEvenBatchHandler: Processing even number batch"
        );

        // Extract cursor from initialization
        let cursor = step_data
            .step_definition
            .handler
            .initialization
            .get("cursor")
            .ok_or_else(|| anyhow::anyhow!("Missing cursor configuration"))?;

        let start_idx = cursor
            .get("start_cursor")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid start_cursor"))?;

        let end_idx = cursor
            .get("end_cursor")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid end_cursor"))?;

        // Get even numbers from branch_evens dependency
        let even_numbers: Vec<i64> = step_data
            .dependency_results
            .get("branch_evens")
            .and_then(|r| r.result.get("even_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing even_numbers from branch_evens"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        // Extract batch
        let batch: Vec<i64> = even_numbers[start_idx as usize..end_idx as usize].to_vec();
        let batch_sum: i64 = batch.iter().sum();
        let processed_count = batch.len();

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("ProcessEvenBatchHandler"));
        metadata.insert("batch_size".to_string(), json!(processed_count));
        metadata.insert("batch_sum".to_string(), json!(batch_sum));

        tracing::info!(
            step_uuid = %step_uuid,
            start_idx = start_idx,
            end_idx = end_idx,
            processed_count = processed_count,
            batch_sum = batch_sum,
            "✅ ProcessEvenBatchHandler: Processed {} evens (sum: {})",
            processed_count,
            batch_sum
        );

        Ok(success_result(
            step_uuid,
            json!({
                "processed_count": processed_count,
                "batch_sum": batch_sum,
                "start_idx": start_idx,
                "end_idx": end_idx,
                "batch_numbers": batch
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// AggregateEvenResultsHandler - Deferred convergence for even batch results
pub struct AggregateEvenResultsHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for AggregateEvenResultsHandler {
    fn name(&self) -> &'static str {
        "aggregate_even_results"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📊 AggregateEvenResultsHandler: Aggregating even batch results"
        );

        let mut total_processed = 0u64;
        let mut total_sum = 0i64;
        let mut batch_count = 0u32;

        // Aggregate results from all even batch workers
        for (step_name, result) in &step_data.dependency_results {
            if step_name.starts_with("process_even_batch_") {
                let count = result
                    .result
                    .get("processed_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let sum = result
                    .result
                    .get("batch_sum")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                total_processed += count;
                total_sum += sum;
                batch_count += 1;

                tracing::debug!(
                    step_name = %step_name,
                    count = count,
                    sum = sum,
                    "Aggregated batch: {} items, sum {}",
                    count,
                    sum
                );
            }
        }

        let average = if batch_count > 0 {
            total_sum / (batch_count as i64)
        } else {
            0
        };

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("AggregateEvenResultsHandler"));
        metadata.insert("batch_count".to_string(), json!(batch_count));
        metadata.insert("total_processed".to_string(), json!(total_processed));

        tracing::info!(
            step_uuid = %step_uuid,
            batch_count = batch_count,
            total_processed = total_processed,
            total_sum = total_sum,
            "✅ AggregateEvenResultsHandler: Aggregated {} batches ({} evens, sum: {})",
            batch_count,
            total_processed,
            total_sum
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": total_processed,
                "total_sum": total_sum,
                "batch_count": batch_count,
                "average": average
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

// ============================================================================
// BATCH PROCESSING HANDLERS - ODD PATH
// ============================================================================

/// OddBatchAnalyzerHandler - Batchable step for odd numbers
pub struct OddBatchAnalyzerHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for OddBatchAnalyzerHandler {
    fn name(&self) -> &'static str {
        "odd_batch_analyzer"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📦 OddBatchAnalyzerHandler: Analyzing odd numbers for batching"
        );

        // Get odd numbers from branch_odds dependency
        let odd_numbers: Vec<i64> = step_data
            .dependency_results
            .get("branch_odds")
            .and_then(|r| r.result.get("odd_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing odd_numbers from branch_odds"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        let dataset_size = odd_numbers.len() as u64;
        if dataset_size == 0 {
            // No batches needed
            let outcome = BatchProcessingOutcome::no_batches();
            return Ok(success_result(
                step_uuid,
                json!({
                    "batch_processing_outcome": outcome.to_value(),
                    "reason": "No odd numbers to process"
                }),
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Get configuration
        let batch_size = self.config.get_u64("batch_size").unwrap_or(3);
        let max_workers = self.config.get_u64("max_workers").unwrap_or(10);

        // Calculate worker count
        let ideal_workers = ((dataset_size as f64) / (batch_size as f64)).ceil() as u64;
        let worker_count = ideal_workers.min(max_workers);
        let actual_batch_size = ((dataset_size as f64) / (worker_count as f64)).ceil() as u64;

        // Create cursor configs
        let mut cursor_configs = Vec::new();
        for i in 0..worker_count {
            let start_idx = i * actual_batch_size;
            let end_idx = ((i + 1) * actual_batch_size).min(dataset_size);
            let items_in_batch = end_idx - start_idx;

            cursor_configs.push(CursorConfig {
                batch_id: format!("{:03}", i + 1),
                start_cursor: json!(start_idx),
                end_cursor: json!(end_idx),
                batch_size: items_in_batch as u32,
            });
        }

        let outcome = BatchProcessingOutcome::create_batches(
            "process_odd_batch".to_string(),
            worker_count as u32,
            cursor_configs,
            dataset_size,
        );

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("OddBatchAnalyzerHandler"));
        metadata.insert("worker_count".to_string(), json!(worker_count));
        metadata.insert("dataset_size".to_string(), json!(dataset_size));

        tracing::info!(
            step_uuid = %step_uuid,
            dataset_size = dataset_size,
            worker_count = worker_count,
            batch_size = actual_batch_size,
            "✅ OddBatchAnalyzerHandler: Creating {} workers for {} odd numbers",
            worker_count,
            dataset_size
        );

        Ok(success_result(
            step_uuid,
            json!({
                "batch_processing_outcome": outcome.to_value(),
                "dataset_size": dataset_size,
                "worker_count": worker_count,
                "batch_size": actual_batch_size
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// ProcessOddBatchHandler - Batch worker for processing odd number batches
pub struct ProcessOddBatchHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ProcessOddBatchHandler {
    fn name(&self) -> &'static str {
        "process_odd_batch"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "⚙️ ProcessOddBatchHandler: Processing odd number batch"
        );

        // Extract cursor from initialization
        let cursor = step_data
            .step_definition
            .handler
            .initialization
            .get("cursor")
            .ok_or_else(|| anyhow::anyhow!("Missing cursor configuration"))?;

        let start_idx = cursor
            .get("start_cursor")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid start_cursor"))?;

        let end_idx = cursor
            .get("end_cursor")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid end_cursor"))?;

        // Get odd numbers from branch_odds dependency
        let odd_numbers: Vec<i64> = step_data
            .dependency_results
            .get("branch_odds")
            .and_then(|r| r.result.get("odd_numbers"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing odd_numbers from branch_odds"))?
            .iter()
            .filter_map(|v| v.as_i64())
            .collect();

        // Extract batch
        let batch: Vec<i64> = odd_numbers[start_idx as usize..end_idx as usize].to_vec();
        let batch_sum: i64 = batch.iter().sum();
        let processed_count = batch.len();

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("ProcessOddBatchHandler"));
        metadata.insert("batch_size".to_string(), json!(processed_count));
        metadata.insert("batch_sum".to_string(), json!(batch_sum));

        tracing::info!(
            step_uuid = %step_uuid,
            start_idx = start_idx,
            end_idx = end_idx,
            processed_count = processed_count,
            batch_sum = batch_sum,
            "✅ ProcessOddBatchHandler: Processed {} odds (sum: {})",
            processed_count,
            batch_sum
        );

        Ok(success_result(
            step_uuid,
            json!({
                "processed_count": processed_count,
                "batch_sum": batch_sum,
                "start_idx": start_idx,
                "end_idx": end_idx,
                "batch_numbers": batch
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}

/// AggregateOddResultsHandler - Deferred convergence for odd batch results
pub struct AggregateOddResultsHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for AggregateOddResultsHandler {
    fn name(&self) -> &'static str {
        "aggregate_odd_results"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📊 AggregateOddResultsHandler: Aggregating odd batch results"
        );

        let mut total_processed = 0u64;
        let mut total_sum = 0i64;
        let mut batch_count = 0u32;

        // Aggregate results from all odd batch workers
        for (step_name, result) in &step_data.dependency_results {
            if step_name.starts_with("process_odd_batch_") {
                let count = result
                    .result
                    .get("processed_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let sum = result
                    .result
                    .get("batch_sum")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                total_processed += count;
                total_sum += sum;
                batch_count += 1;

                tracing::debug!(
                    step_name = %step_name,
                    count = count,
                    sum = sum,
                    "Aggregated batch: {} items, sum {}",
                    count,
                    sum
                );
            }
        }

        let average = if batch_count > 0 {
            total_sum / (batch_count as i64)
        } else {
            0
        };

        let elapsed_ms = start_time.elapsed().as_millis() as i64;
        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("AggregateOddResultsHandler"));
        metadata.insert("batch_count".to_string(), json!(batch_count));
        metadata.insert("total_processed".to_string(), json!(total_processed));

        tracing::info!(
            step_uuid = %step_uuid,
            batch_count = batch_count,
            total_processed = total_processed,
            total_sum = total_sum,
            "✅ AggregateOddResultsHandler: Aggregated {} batches ({} odds, sum: {})",
            batch_count,
            total_processed,
            total_sum
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": total_processed,
                "total_sum": total_sum,
                "batch_count": batch_count,
                "average": average
            }),
            elapsed_ms,
            Some(metadata),
        ))
    }
}
