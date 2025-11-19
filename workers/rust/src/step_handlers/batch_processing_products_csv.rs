//! # CSV Batch Processing Example - Product Inventory Analysis
//!
//! Demonstrates real-world batch processing with actual file I/O using cursor-based row selection.
//!
//! ## Workflow Structure
//!
//! ```text
//!    analyze_csv (batchable)
//!         |
//!   [Creates 5 Workers]
//!         |
//!    +----+----+----+----+
//!    |    |    |    |    |
//! batch_1  ...  batch_5  (process_csv_batch instances)
//!    |    |    |    |    |
//!    +----+----+----+----+
//!         |
//!    aggregate_csv_results (deferred convergence)
//! ```
//!
//! ## Processing Logic
//!
//! **Batchable Step**: Counts CSV rows (excluding header), creates 5 workers with 200 rows each
//! **Batch Workers**: Read assigned CSV rows, calculate inventory metrics per batch
//! **Convergence**: Aggregates results across all batches
//!
//! ## CSV Structure
//!
//! File: `tests/fixtures/products.csv` (1001 lines: 1 header + 1000 data rows)
//! Columns: id, title, description, category, price, discountPercentage, rating, stock, brand, sku, weight
//!
//! ## Example Task Context
//!
//! ```json
//! {
//!   "csv_file_path": "tests/fixtures/products.csv",
//!   "batch_size": 200,
//!   "max_workers": 5
//! }
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use tasker_shared::messaging::{BatchProcessingOutcome, CursorConfig, StepExecutionResult};
use tasker_shared::types::TaskSequenceStep;
use tasker_worker::batch_processing::BatchWorkerContext;

use super::{success_result, RustStepHandler, StepHandlerConfig};

// ============================================================================
// CSV PRODUCT STRUCTURE
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Product {
    id: u32,
    title: String,
    #[allow(dead_code)]
    description: String,
    category: String,
    price: f64,
    #[allow(dead_code)]
    #[serde(rename = "discountPercentage")]
    discount_percentage: f64,
    rating: f64,
    stock: u32,
    #[allow(dead_code)]
    brand: String,
    #[allow(dead_code)]
    sku: String,
    #[allow(dead_code)]
    weight: u32,
}

// ============================================================================
// BATCHABLE STEP - CSV ANALYZER
// ============================================================================

/// CSV Analyzer Handler - Batchable step that analyzes CSV file and creates batch workers
///
/// This handler counts total data rows in the CSV file (excluding header) and returns
/// `BatchProcessingOutcome::CreateBatches` with cursor configurations for parallel processing.
///
/// ## Cursor Strategy
///
/// Cursors represent CSV row numbers (1-indexed after header):
/// - Row 1: Header (skipped)
/// - Rows 2-1001: Data rows (1000 total)
/// - Worker 1: rows 2-201 (cursor 1-200)
/// - Worker 2: rows 202-401 (cursor 201-400)
/// - Worker 5: rows 802-1001 (cursor 801-1000)
#[derive(Debug)]
pub struct CsvAnalyzerHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CsvAnalyzerHandler {
    fn name(&self) -> &str {
        "analyze_csv"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📊 CsvAnalyzerHandler: Analyzing CSV file for batch processing"
        );

        // Extract CSV file path from task context
        let csv_file_path = step_data
            .task
            .task
            .context
            .as_ref()
            .and_then(|ctx| ctx.get("csv_file_path"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required field: csv_file_path"))?;

        // Get configuration parameters
        let batch_size = step_data
            .step_definition
            .handler
            .initialization
            .get("batch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(200);
        let max_workers = step_data
            .step_definition
            .handler
            .initialization
            .get("max_workers")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        // Count total data rows (excluding header)
        let file = File::open(csv_file_path)?;
        let reader = BufReader::new(file);
        let total_rows = reader.lines().count().saturating_sub(1) as u64; // Subtract header row

        tracing::info!(
            csv_file_path = %csv_file_path,
            total_rows = total_rows,
            "Counted CSV rows (excluding header)"
        );

        // Check if dataset is too small for batching
        if total_rows < batch_size {
            tracing::info!(
                total_rows = total_rows,
                batch_size = batch_size,
                "Dataset too small for batching, returning NoBatches"
            );

            let outcome = BatchProcessingOutcome::no_batches();
            return Ok(success_result(
                step_uuid,
                json!({
                    "batch_processing_outcome": outcome.to_value(),
                    "reason": "dataset_too_small",
                    "total_rows": total_rows,
                    "csv_file_path": csv_file_path
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([(
                    "batching_decision".to_string(),
                    json!("no_batches"),
                )])),
            ));
        }

        // Calculate optimal worker count
        let ideal_workers = ((total_rows as f64) / (batch_size as f64)).ceil() as u64;
        let worker_count = ideal_workers.min(max_workers);
        let actual_batch_size = ((total_rows as f64) / (worker_count as f64)).ceil() as u64;

        tracing::info!(
            ideal_workers = ideal_workers,
            worker_count = worker_count,
            actual_batch_size = actual_batch_size,
            "Calculated batch worker configuration"
        );

        // Create cursor configurations
        // Cursors represent row indices (1-based after header)
        let mut cursor_configs = Vec::new();
        for i in 0..worker_count {
            let start_row = (i * actual_batch_size) + 1; // 1-indexed (row 1 is first data row after header)
            let end_row = ((i + 1) * actual_batch_size).min(total_rows) + 1; // Inclusive end
            let items_in_batch = end_row - start_row;

            cursor_configs.push(CursorConfig {
                batch_id: format!("{:03}", i + 1),
                start_cursor: json!(start_row),
                end_cursor: json!(end_row),
                batch_size: items_in_batch as u32,
            });

            tracing::debug!(
                worker_index = i,
                batch_id = format!("{:03}", i + 1),
                start_row = start_row,
                end_row = end_row,
                items = items_in_batch,
                "Created cursor config for CSV batch worker"
            );
        }

        let outcome = BatchProcessingOutcome::create_batches(
            "process_csv_batch".to_string(),
            worker_count as u32,
            cursor_configs,
            total_rows,
        );

        tracing::info!(
            worker_count = worker_count,
            total_rows = total_rows,
            "Successfully created CSV batch processing outcome"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "batch_processing_outcome": outcome.to_value(),
                "worker_count": worker_count,
                "actual_batch_size": actual_batch_size,
                "total_rows": total_rows,
                "csv_file_path": csv_file_path
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("batching_decision".to_string(), json!("create_batches")),
                ("csv_file".to_string(), json!(csv_file_path)),
            ])),
        ))
    }
}

// ============================================================================
// BATCH WORKER - CSV BATCH PROCESSOR
// ============================================================================

/// CSV Batch Processor Handler - Processes a single batch of CSV rows
///
/// This handler reads assigned CSV rows based on cursor configuration and calculates:
/// - Total inventory value (price × stock)
/// - Product count by category
/// - Most expensive product in batch
/// - Average rating for batch
///
/// ## Cursor-Based Row Selection
///
/// The cursor defines which CSV rows this worker processes:
/// - `start_cursor`: First data row index (1-based, after header)
/// - `end_cursor`: Last data row index (inclusive)
///
/// Example: Worker with start=1, end=200 processes CSV rows 2-201 (row 1 is header)
#[derive(Debug)]
pub struct CsvBatchProcessorHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CsvBatchProcessorHandler {
    fn name(&self) -> &str {
        "process_csv_batch"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "⚙️ CsvBatchProcessorHandler: Processing CSV batch"
        );

        // Extract batch worker context using helper
        let context = BatchWorkerContext::from_step_data(step_data)?;

        // Check if this is a no-op worker
        if context.is_no_op() {
            tracing::info!(
                batch_id = %context.batch_id(),
                "Detected no-op worker - returning success immediately"
            );
            return Ok(success_result(
                step_uuid,
                json!({
                    "no_op": true,
                    "reason": "NoBatches scenario",
                    "batch_id": context.batch_id(),
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("no_op".to_string(), json!(true))])),
            ));
        }

        // Get CSV file path from analyze_csv dependency
        let csv_file_path = step_data
            .dependency_results
            .get("analyze_csv")
            .and_then(|r| r.result.get("csv_file_path"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing csv_file_path from analyze_csv"))?;

        let start_row = context.start_position() as usize;
        let end_row = context.end_position() as usize;

        tracing::debug!(
            batch_id = %context.batch_id(),
            start_row = start_row,
            end_row = end_row,
            csv_file = %csv_file_path,
            "Processing CSV batch"
        );

        // Read and process CSV rows
        let file = File::open(Path::new(csv_file_path))?;
        let reader = BufReader::new(file);

        let mut csv_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(reader);

        let mut total_inventory_value = 0.0;
        let mut category_counts: HashMap<String, u32> = HashMap::new();
        let mut max_price = 0.0;
        let mut max_price_product: Option<String> = None;
        let mut total_rating = 0.0;
        let mut processed_count = 0;

        // Process rows in cursor range
        // CSV row index 0 is header, so data rows start at index 1
        // Our cursor is 1-based after header, so we need to read rows at indices start_row..end_row
        for (row_idx, result) in csv_reader.deserialize::<Product>().enumerate() {
            // row_idx starts at 0 for first data row (after header)
            // We want rows where (row_idx + 1) is in range [start_row, end_row)
            let data_row_num = row_idx + 1;

            if data_row_num < start_row {
                continue; // Skip rows before our range
            }
            if data_row_num >= end_row {
                break; // We've processed all our rows
            }

            let product: Product = result?;

            // Calculate metrics
            let inventory_value = product.price * (product.stock as f64);
            total_inventory_value += inventory_value;

            *category_counts.entry(product.category.clone()).or_insert(0) += 1;

            if product.price > max_price {
                max_price = product.price;
                max_price_product = Some(product.title.clone());
            }

            total_rating += product.rating;
            processed_count += 1;
        }

        let average_rating = if processed_count > 0 {
            total_rating / (processed_count as f64)
        } else {
            0.0
        };

        tracing::info!(
            batch_id = %context.batch_id(),
            processed_count = processed_count,
            total_inventory_value = format!("${:.2}", total_inventory_value),
            max_price = format!("${:.2}", max_price),
            average_rating = format!("{:.2}", average_rating),
            "CSV batch processing complete"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "processed_count": processed_count,
                "total_inventory_value": total_inventory_value,
                "category_counts": category_counts,
                "max_price": max_price,
                "max_price_product": max_price_product,
                "average_rating": average_rating,
                "batch_id": context.batch_id(),
                "start_row": start_row,
                "end_row": end_row
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("batch_id".to_string(), json!(context.batch_id())),
                ("processed_count".to_string(), json!(processed_count)),
            ])),
        ))
    }
}

// ============================================================================
// CONVERGENCE STEP - CSV RESULTS AGGREGATOR
// ============================================================================

/// CSV Results Aggregator Handler - Aggregates results from all CSV batch workers
///
/// This handler collects and combines metrics from all batch workers:
/// - Sums total inventory value across all batches
/// - Merges category counts
/// - Finds globally most expensive product
/// - Calculates overall average rating
#[derive(Debug)]
pub struct CsvResultsAggregatorHandler {
    _config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CsvResultsAggregatorHandler {
    fn name(&self) -> &str {
        "aggregate_csv_results"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { _config: config }
    }

    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        tracing::info!(
            step_uuid = %step_uuid,
            "📊 CsvResultsAggregatorHandler: Aggregating CSV batch results"
        );

        // Use centralized batch aggregation scenario detection
        let scenario = tasker_worker::BatchAggregationScenario::detect(
            &step_data.dependency_results,
            "analyze_csv",        // batchable step name
            "process_csv_batch_", // batch worker prefix
        )
        .map_err(|e| anyhow::anyhow!("Failed to detect aggregation scenario: {}", e))?;

        let (
            total_processed,
            total_inventory_value,
            global_category_counts,
            max_price,
            max_price_product,
            overall_avg_rating,
            worker_count,
        ) = match scenario {
            tasker_worker::BatchAggregationScenario::NoBatches { batchable_result } => {
                tracing::info!("Detected NoBatches scenario - no CSV processing occurred");

                let total_rows = batchable_result
                    .result
                    .get("total_rows")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                (total_rows, 0.0, HashMap::new(), 0.0, None, 0.0, 0)
            }
            tasker_worker::BatchAggregationScenario::WithBatches {
                batch_results,
                worker_count,
            } => {
                let mut total_processed: u64 = 0;
                let mut total_inventory_value: f64 = 0.0;
                let mut global_category_counts: HashMap<String, u64> = HashMap::new();
                let mut max_price: f64 = 0.0;
                let mut max_price_product: Option<String> = None;
                let mut total_rating_sum: f64 = 0.0;

                tracing::debug!(
                    worker_count = worker_count,
                    "Aggregating results from CSV batch workers"
                );

                for (step_name, result) in batch_results {
                    let processed = result
                        .result
                        .get("processed_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let inventory_value = result
                        .result
                        .get("total_inventory_value")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    let batch_max_price = result
                        .result
                        .get("max_price")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    let batch_max_product = result
                        .result
                        .get("max_price_product")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let avg_rating = result
                        .result
                        .get("average_rating")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    // Merge category counts
                    if let Some(category_counts) = result
                        .result
                        .get("category_counts")
                        .and_then(|v| v.as_object())
                    {
                        for (category, count) in category_counts {
                            let count_val = count.as_u64().unwrap_or(0);
                            *global_category_counts.entry(category.clone()).or_insert(0) +=
                                count_val;
                        }
                    }

                    total_processed += processed;
                    total_inventory_value += inventory_value;
                    total_rating_sum += avg_rating * (processed as f64); // Weight by batch size

                    // Track global max price
                    if batch_max_price > max_price {
                        max_price = batch_max_price;
                        max_price_product = batch_max_product;
                    }

                    tracing::debug!(
                        step_name = %step_name,
                        processed = processed,
                        inventory_value = format!("${:.2}", inventory_value),
                        "Aggregated batch result"
                    );
                }

                let overall_avg_rating = if total_processed > 0 {
                    total_rating_sum / (total_processed as f64)
                } else {
                    0.0
                };

                (
                    total_processed,
                    total_inventory_value,
                    global_category_counts,
                    max_price,
                    max_price_product,
                    overall_avg_rating,
                    worker_count,
                )
            }
        };

        tracing::info!(
            total_processed = total_processed,
            total_inventory_value = format!("${:.2}", total_inventory_value),
            max_price = format!("${:.2}", max_price),
            max_price_product = max_price_product.as_deref().unwrap_or("None"),
            overall_avg_rating = format!("{:.2}", overall_avg_rating),
            category_count = global_category_counts.len(),
            worker_count = worker_count,
            "CSV results aggregation complete"
        );

        Ok(success_result(
            step_uuid,
            json!({
                "total_processed": total_processed,
                "total_inventory_value": total_inventory_value,
                "category_counts": global_category_counts,
                "max_price": max_price,
                "max_price_product": max_price_product,
                "overall_average_rating": overall_avg_rating,
                "worker_count": worker_count
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("worker_count".to_string(), json!(worker_count)),
                ("total_products".to_string(), json!(total_processed)),
            ])),
        ))
    }
}
