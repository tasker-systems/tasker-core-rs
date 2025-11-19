//! # Batch Processing Example Handlers
//!
//! Demonstrates TAS-59 batch processing pattern with cursor-based resumability.
//!
//! ## Architecture
//!
//! This module implements a complete batch processing workflow:
//!
//! ```text
//!    analyze_dataset (batchable)
//!           |
//!     [Creates N Workers]
//!           |
//!    +------+------+------+
//!    |      |      |      |
//! worker_1 worker_2  worker_N (batch_worker template instances)
//!    |      |      |      |
//!    +------+------+------+
//!           |
//!    aggregate_results (deferred convergence)
//! ```
//!
//! ## Handlers
//!
//! - **`DatasetAnalyzerHandler`**: Batchable step that analyzes a dataset and returns
//!   `BatchProcessingOutcome::CreateBatches` with cursor configurations
//! - **`BatchWorkerHandler`**: Batch worker template that processes a single batch
//!   with checkpoint support and resumability
//! - **`ResultsAggregatorHandler`**: Deferred convergence step that aggregates
//!   results from all batch workers
//!
//! ## Cursor-Based Resumability (TAS-59)
//!
//! Each batch worker receives a `CursorConfig` in its initialization defining:
//! - **start_position**: Starting offset in the dataset
//! - **end_position**: Ending offset (exclusive)
//! - **checkpoint_interval**: How often to save progress
//!
//! Workers update their progress through checkpoint_progress field, enabling:
//! - **Resume after failure**: Continue from last checkpoint
//! - **Progress tracking**: Real-time visibility into processing status
//! - **Staleness detection**: Identify stuck workers via TAS-49 integration
//!
//! ## Example Usage
//!
//! ```yaml
//! namespace: data_processing
//! name: large_dataset_processor
//! version: "1.0.0"
//! description: "Process large dataset in parallel batches"
//!
//! steps:
//!   - name: analyze_dataset
//!     type: batchable
//!     handler:
//!       callable: "DatasetAnalyzer"
//!       initialization:
//!         batch_size: 1000
//!         max_workers: 10
//!
//!   - name: process_batch
//!     type: batch_worker
//!     handler:
//!       callable: "BatchWorker"
//!       initialization:
//!         operation: "transform"
//!
//!   - name: aggregate_results
//!     type: deferred_convergence
//!     handler:
//!       callable: "ResultsAggregator"
//!       initialization:
//!         aggregation_type: "sum"
//!
//! edges:
//!   - from: analyze_dataset
//!     to: process_batch
//!   - from: process_batch
//!     to: aggregate_results
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use tasker_shared::messaging::{BatchProcessingOutcome, CursorConfig, StepExecutionResult};
use tasker_shared::types::TaskSequenceStep;
use tasker_worker::batch_processing::BatchWorkerContext;

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};

/// Dataset analyzer handler implementing the batchable step pattern
///
/// This handler analyzes a dataset and returns `BatchProcessingOutcome` to
/// orchestrate parallel batch processing with cursor-based resumability.
///
/// ## Batch Calculation Logic
///
/// ```text
/// total_items = task.context["dataset_size"]
/// batch_size = config.batch_size (default: 1000)
/// max_workers = config.max_workers (default: 10)
///
/// worker_count = min(
///     ceil(total_items / batch_size),
///     max_workers
/// )
///
/// actual_batch_size = ceil(total_items / worker_count)
/// ```
///
/// ## Cursor Configuration
///
/// Each worker receives a `CursorConfig`:
/// - **Worker 1**: start=0, end=1000, checkpoint_interval=100
/// - **Worker 2**: start=1000, end=2000, checkpoint_interval=100
/// - **Worker N**: start=(N-1)*1000, end=N*1000, checkpoint_interval=100
///
/// ## Return Value
///
/// Success result with `batch_processing_outcome` field:
/// ```json
/// {
///   "batch_processing_outcome": {
///     "type": "create_batches",
///     "worker_template_name": "process_batch",
///     "worker_count": 5,
///     "cursor_configs": [...],
///     "total_items": 5000
///   }
/// }
/// ```
#[derive(Debug)]
pub struct DatasetAnalyzerHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for DatasetAnalyzerHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "Starting dataset analysis for batch processing"
        );

        // Extract dataset size from task context
        let dataset_size = step_data
            .task
            .task
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("dataset_size"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required field: dataset_size"))?;

        // Get configuration parameters from step definition (YAML initialization section)
        let batch_size = step_data
            .step_definition
            .handler
            .initialization
            .get("batch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000);
        let max_workers = step_data
            .step_definition
            .handler
            .initialization
            .get("max_workers")
            .and_then(|v| v.as_u64())
            .unwrap_or(10);
        let checkpoint_interval = step_data
            .step_definition
            .handler
            .initialization
            .get("checkpoint_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);
        let worker_template_name = step_data
            .step_definition
            .handler
            .initialization
            .get("worker_template_name")
            .and_then(|v| v.as_str())
            .unwrap_or("process_batch")
            .to_string();

        tracing::debug!(
            dataset_size = dataset_size,
            batch_size = batch_size,
            max_workers = max_workers,
            checkpoint_interval = checkpoint_interval,
            "Batch processing configuration"
        );

        // Check if dataset is too small for batching
        if dataset_size < batch_size {
            tracing::info!(
                dataset_size = dataset_size,
                batch_size = batch_size,
                "Dataset too small for batching, returning NoBatches"
            );

            let outcome = BatchProcessingOutcome::no_batches();

            return Ok(success_result(
                step_uuid,
                json!({
                    "batch_processing_outcome": outcome.to_value(),
                    "reason": "dataset_too_small",
                    "dataset_size": dataset_size
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([(
                    "batching_decision".to_string(),
                    json!("no_batches"),
                )])),
            ));
        }

        // Calculate optimal worker count
        let ideal_workers = ((dataset_size as f64) / (batch_size as f64)).ceil() as u64;
        let worker_count = ideal_workers.min(max_workers);
        let actual_batch_size = ((dataset_size as f64) / (worker_count as f64)).ceil() as u64;

        tracing::info!(
            ideal_workers = ideal_workers,
            worker_count = worker_count,
            actual_batch_size = actual_batch_size,
            "Calculated batch worker configuration"
        );

        // Create cursor configurations for each worker
        let mut cursor_configs = Vec::new();
        for i in 0..worker_count {
            let start_position = i * actual_batch_size;
            let end_position = ((i + 1) * actual_batch_size).min(dataset_size);
            let items_in_batch = end_position - start_position;

            let cursor_config = CursorConfig {
                batch_id: format!("{:03}", i + 1),
                start_cursor: json!(start_position),
                end_cursor: json!(end_position),
                batch_size: items_in_batch as u32,
            };

            tracing::debug!(
                worker_index = i,
                batch_id = %cursor_config.batch_id,
                start_position = start_position,
                end_position = end_position,
                items_in_batch = items_in_batch,
                "Created cursor config for worker"
            );

            cursor_configs.push(cursor_config);
        }

        // Create batch processing outcome
        let outcome = BatchProcessingOutcome::create_batches(
            worker_template_name.clone(),
            worker_count as u32,
            cursor_configs,
            dataset_size,
        );

        tracing::info!(
            worker_count = worker_count,
            total_items = dataset_size,
            "Successfully created batch processing outcome"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "batch_processing_outcome": outcome.to_value(),
                "worker_count": worker_count,
                "actual_batch_size": actual_batch_size,
                "total_items": dataset_size
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("batching_decision".to_string(), json!("create_batches")),
                ("worker_template".to_string(), json!(worker_template_name)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "analyze_dataset"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }
}

/// Batch worker handler processing a single batch with cursor-based resumability
///
/// This handler implements the TAS-59 batch worker pattern with:
/// - **Cursor-based iteration**: Process items from `start_position` to `end_position`
/// - **Checkpoint support**: Report progress at `checkpoint_interval`
/// - **Resumability**: Resume from `checkpoint_progress` after failure
/// - **Progress tracking**: Update `checkpoint_progress` field during execution
///
/// ## Input Structure (from workflow_step.inputs)
///
/// Batch worker instances receive their configuration in `workflow_step.inputs`,
/// NOT in `step_definition.handler.initialization`. The step_definition contains
/// the template, while inputs contains instance-specific cursor configuration.
///
/// ```json
/// {
///   "cursor": {
///     "batch_id": "001",
///     "start_cursor": 0,
///     "end_cursor": 1000,
///     "batch_size": 1000,
///     "checkpoint_progress": 0
///   },
///   "batch_metadata": {
///     "checkpoint_interval": 100,
///     "cursor_field": "id",
///     "failure_strategy": "fail_fast"
///   },
///   "__template_step_name": "process_batch"
/// }
/// ```
///
/// ## Processing Logic
///
/// 1. Resume from `checkpoint_progress` (or `start_position` if first attempt)
/// 2. Process items in chunks of `checkpoint_interval`
/// 3. Update `checkpoint_progress` after each chunk
/// 4. Continue until reaching `end_position`
/// 5. Return success with processing summary
///
/// ## Checkpoint Updates (Future Enhancement)
///
/// Currently simulated with logging. In production, this would:
/// - Update `workflow_steps.initialization.cursor.checkpoint_progress`
/// - Emit checkpoint events for progress tracking
/// - Enable resume-from-checkpoint via TAS-49 staleness detection
#[derive(Debug)]
pub struct BatchWorkerHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for BatchWorkerHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "Starting batch worker execution"
        );

        // Extract batch worker context using helper
        let context = BatchWorkerContext::from_step_data(step_data)?;

        // Check if this is a no-op/placeholder worker (NoBatches scenario)
        if context.is_no_op() {
            tracing::info!(
                batch_id = %context.batch_id(),
                start_position = context.start_position(),
                end_position = context.end_position(),
                "Detected no-op worker (NoBatches scenario) - returning success immediately"
            );
            return Ok(success_result(
                step_uuid,
                json!({
                    "no_op": true,
                    "reason": "NoBatches scenario - no items to process",
                    "batch_id": context.batch_id(),
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("no_op".to_string(), json!(true))])),
            ));
        }

        tracing::debug!(
            batch_id = %context.batch_id(),
            start_position = context.start_position(),
            end_position = context.end_position(),
            checkpoint_interval = context.checkpoint_interval(),
            "Extracted cursor configuration"
        );

        // Simulate batch processing with checkpoints
        let mut current_position = context.start_position();
        let mut processed_count = 0;
        let mut checkpoint_count = 0;

        while current_position < context.end_position() {
            let chunk_end = (current_position + context.checkpoint_interval() as u64)
                .min(context.end_position());
            let chunk_size = chunk_end - current_position;

            // Simulate processing chunk
            tracing::debug!(
                current_position = current_position,
                chunk_end = chunk_end,
                chunk_size = chunk_size,
                "Processing chunk"
            );

            // In production, this would:
            // 1. Fetch items from dataset using cursor_field
            // 2. Apply transformation/validation logic
            // 3. Handle failures per failure_strategy
            // 4. Update checkpoint_progress in database

            processed_count += chunk_size;
            current_position = chunk_end;
            checkpoint_count += 1;

            // Simulate checkpoint update
            tracing::info!(
                checkpoint_progress = current_position,
                processed_count = processed_count,
                checkpoint_count = checkpoint_count,
                "Checkpoint reached"
            );
        }

        tracing::info!(
            processed_count = processed_count,
            checkpoint_count = checkpoint_count,
            "Batch worker completed successfully"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "processed_count": processed_count,
                "checkpoint_count": checkpoint_count,
                "start_position": context.start_position(),
                "end_position": context.end_position(),
                "final_position": current_position,
                "batch_id": context.batch_id()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                (
                    "batch_range".to_string(),
                    json!(format!(
                        "{}-{}",
                        context.start_position(),
                        context.end_position()
                    )),
                ),
                ("checkpoints".to_string(), json!(checkpoint_count)),
                ("batch_id".to_string(), json!(context.batch_id())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "process_batch"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }
}

/// Results aggregator handler implementing the deferred convergence pattern
///
/// This handler collects and aggregates results from all batch workers after
/// they complete. It demonstrates the deferred step type with intersection
/// semantics: waits for ALL parent batch workers to complete before executing.
///
/// ## Aggregation Logic
///
/// 1. Collect results from all parent batch workers via `dependency_results`
/// 2. Sum `processed_count` across all workers
/// 3. Validate total matches expected dataset size
/// 4. Return aggregated summary
///
/// ## Dependency Results Structure
///
/// ```json
/// {
///   "process_batch_001": {
///     "result": {
///       "processed_count": 1000,
///       "checkpoint_count": 10
///     }
///   },
///   "process_batch_002": {
///     "result": {
///       "processed_count": 1000,
///       "checkpoint_count": 10
///     }
///   }
/// }
/// ```
///
/// ## Return Value
///
/// Success result with aggregated statistics:
/// ```json
/// {
///   "total_processed": 5000,
///   "worker_count": 5,
///   "total_checkpoints": 50,
///   "aggregation_type": "sum"
/// }
/// ```
#[derive(Debug)]
pub struct ResultsAggregatorHandler {
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ResultsAggregatorHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "Starting results aggregation"
        );

        let aggregation_type = self
            .config
            .get_string("aggregation_type")
            .unwrap_or_else(|| "sum".to_string());

        // Extract expected dataset size from task context
        let expected_total = step_data
            .task
            .task
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("dataset_size"))
            .and_then(|v| v.as_u64());

        // Use centralized batch aggregation scenario detection
        let scenario = tasker_worker::BatchAggregationScenario::detect(
            &step_data.dependency_results,
            "analyze_dataset", // batchable step name
            "process_batch_",  // batch worker prefix
        )
        .map_err(|e| anyhow::anyhow!("Failed to detect aggregation scenario: {}", e))?;

        let (total_processed, total_checkpoints, worker_count) = match scenario {
            tasker_worker::BatchAggregationScenario::NoBatches { batchable_result } => {
                // NoBatches scenario: Use dataset_size from the batchable step result
                tracing::info!(
                    "Detected NoBatches scenario - using dataset size from batchable step"
                );

                let dataset_size = batchable_result
                    .result
                    .get("dataset_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                (dataset_size, 0, 0) // No batch workers, so 0 checkpoints and 0 workers
            }
            tasker_worker::BatchAggregationScenario::WithBatches {
                batch_results,
                worker_count,
            } => {
                // WithBatches scenario: Aggregate results from all batch workers
                let mut total_processed: u64 = 0;
                let mut total_checkpoints: u64 = 0;

                tracing::debug!(
                    worker_count = worker_count,
                    aggregation_type = %aggregation_type,
                    "Processing dependency results from batch workers"
                );

                for (step_name, result) in batch_results {
                    let processed = result
                        .result
                        .get("processed_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let checkpoints = result
                        .result
                        .get("checkpoint_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    total_processed += processed;
                    total_checkpoints += checkpoints;

                    tracing::debug!(
                        step_name = %step_name,
                        processed = processed,
                        checkpoints = checkpoints,
                        "Aggregated worker result"
                    );
                }

                (total_processed, total_checkpoints, worker_count)
            }
        };

        // Validate against expected total if available
        if let Some(expected) = expected_total {
            if total_processed != expected {
                tracing::warn!(
                    expected = expected,
                    actual = total_processed,
                    "Processed count does not match expected dataset size"
                );

                return Ok(error_result(
                    step_uuid,
                    format!(
                        "Processed count {} does not match expected {}",
                        total_processed, expected
                    ),
                    Some("AGGREGATION_MISMATCH".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - this is a data consistency issue
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([
                        ("expected".to_string(), json!(expected)),
                        ("actual".to_string(), json!(total_processed)),
                    ])),
                ));
            }
        }

        tracing::info!(
            total_processed = total_processed,
            worker_count = worker_count,
            total_checkpoints = total_checkpoints,
            "Results aggregation completed successfully"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": total_processed,
                "worker_count": worker_count,
                "total_checkpoints": total_checkpoints,
                "aggregation_type": aggregation_type,
                "validation_passed": expected_total.is_some()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("aggregation_type".to_string(), json!(aggregation_type)),
                ("worker_count".to_string(), json!(worker_count)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "aggregate_results"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// TODO: Add comprehensive unit tests in Phase 7
// For now, handler correctness will be validated through integration tests
