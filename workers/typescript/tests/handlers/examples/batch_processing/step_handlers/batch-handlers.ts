/**
 * Batch Processing Step Handlers for E2E Testing.
 *
 * Implements the TAS-59 batch processing pattern:
 * 1. CsvAnalyzerHandler (batchable): Analyze CSV, create batch configurations
 * 2. CsvBatchProcessorHandler (batch_worker): Process CSV rows in batches
 * 3. CsvResultsAggregatorHandler (deferred_convergence): Aggregate results
 *
 * Matches Ruby and Python batch processing implementations for testing parity.
 *
 * NOTE: These handlers demonstrate proper use of batchable helper methods
 * from BatchableStepHandler. The inline logic has been moved to the base class.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import { BatchableStepHandler } from '../../../../../src/handler/batchable.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { BatchableResult, StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Batchable: Analyze CSV file and create batch worker configurations.
 *
 * For testing purposes, simulates CSV analysis and creates batch
 * configurations based on the input file path.
 *
 * Demonstrates use of:
 * - createCursorConfigs() - Ruby-style helper for creating worker configs
 * - noBatchesResult() - Cross-language standard no-batches outcome
 * - batchSuccess() - Create batches outcome with worker template
 */
export class CsvAnalyzerHandler extends BatchableStepHandler {
  static handlerName = 'batch_processing.step_handlers.CsvAnalyzerHandler';
  static handlerVersion = '1.0.0';

  private static readonly DEFAULT_BATCH_SIZE = 200;
  private static readonly DEFAULT_MAX_WORKERS = 5;

  async call(context: StepContext): Promise<BatchableResult> {
    const csvFilePath = context.getInput<string>('csv_file_path');
    const analysisMode = context.getInput<string>('analysis_mode') ?? 'inventory';

    if (!csvFilePath) {
      return this.failure(
        'Missing required input: csv_file_path',
        'validation_error',
        false
      ) as BatchableResult;
    }

    // Simulate CSV analysis - in real scenario, would read the file
    // For testing: check if this is the "empty" file scenario
    const isEmptyFile = csvFilePath.includes('empty');
    const totalRows = isEmptyFile ? 0 : 1000;

    // Use context.stepConfig (from handler.initialization in the YAML template)
    const batchSize =
      (context.stepConfig?.batch_size as number) ?? CsvAnalyzerHandler.DEFAULT_BATCH_SIZE;
    const maxWorkers =
      (context.stepConfig?.max_workers as number) ?? CsvAnalyzerHandler.DEFAULT_MAX_WORKERS;

    // Handle empty file case - return no batches result
    // Cross-language standard: matches Ruby's no_batches_outcome(reason:, metadata:)
    if (totalRows === 0) {
      return this.noBatchesResult('empty_dataset', {
        csv_file_path: csvFilePath,
        analysis_mode: analysisMode,
        total_rows: 0,
        analyzed_at: new Date().toISOString(),
      });
    }

    // Calculate number of workers based on batch size and max workers
    const numBatches = Math.ceil(totalRows / batchSize);
    const actualWorkers = Math.min(numBatches, maxWorkers);

    // Use createCursorConfigs() helper - matches Ruby's create_cursor_configs
    // This divides totalRows into actualWorkers roughly equal ranges
    const batchConfigs = this.createCursorConfigs(totalRows, actualWorkers);

    // Pass the worker template name that orchestration will use to create workers
    return this.batchSuccess('process_csv_batch_ts', batchConfigs, {
      csv_file_path: csvFilePath,
      analysis_mode: analysisMode,
      total_rows: totalRows,
      batch_size: batchSize,
      num_batches: actualWorkers,
      analyzed_at: new Date().toISOString(),
    });
  }
}

/**
 * Batch Worker: Process a batch of CSV rows.
 *
 * For testing purposes, simulates row processing and returns
 * mock inventory analysis results.
 *
 * Demonstrates use of:
 * - handleNoOpWorker() - Cross-language standard no-op handling
 * - getBatchWorkerInputs() - Access Rust-provided batch configuration
 */
export class CsvBatchProcessorHandler extends BatchableStepHandler {
  static handlerName = 'batch_processing.step_handlers.CsvBatchProcessorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Cross-language standard: check for no-op worker first
    // Matches Ruby's handle_no_op_worker pattern
    const noOpResult = this.handleNoOpWorker(context);
    if (noOpResult) {
      return noOpResult;
    }

    // Get batch worker inputs from stepInputs
    // Cross-language standard: matches Ruby's get_batch_context pattern
    const batchInputs = this.getBatchWorkerInputs(context);
    const cursor = batchInputs?.cursor;

    if (!cursor) {
      return this.failure('Missing batch cursor configuration', 'batch_error', false);
    }

    const rowCount = cursor.batch_size ?? 200;
    const batchId = cursor.batch_id ?? 'unknown';

    // Simulate processing - mock results
    const validProducts = Math.floor(rowCount * 0.95);
    const invalidProducts = rowCount - validProducts;
    const lowStockItems = Math.floor(validProducts * 0.1);
    const outOfStockItems = Math.floor(validProducts * 0.02);

    return this.success({
      batch_id: batchId,
      rows_processed: rowCount,
      cursor_start: cursor.start_cursor,
      cursor_end: cursor.end_cursor,
      valid_products: validProducts,
      invalid_products: invalidProducts,
      low_stock_items: lowStockItems,
      out_of_stock_items: outOfStockItems,
      processed_at: new Date().toISOString(),
    });
  }
}

/**
 * Deferred Convergence: Aggregate results from all batch workers.
 *
 * This aggregator uses domain-specific aggregation logic (product metrics).
 * For generic aggregation, use BatchableMixin.aggregateWorkerResults().
 *
 * Cross-language standard: matches Ruby's aggregate_batch_worker_results
 * pattern with custom block for domain-specific aggregation.
 */
export class CsvResultsAggregatorHandler extends StepHandler {
  static handlerName = 'batch_processing.step_handlers.CsvResultsAggregatorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Collect results from all batch worker instances
    // getAllDependencyResults already unwraps the 'result' field, so we get inner values directly
    const batchResults = context.getAllDependencyResults('process_csv_batch_ts') as Array<Record<
      string,
      unknown
    > | null>;

    if (!batchResults || batchResults.length === 0) {
      return this.failure('No batch worker results to aggregate', 'aggregation_error', false);
    }

    // Domain-specific aggregation for product inventory metrics
    // This is handler-specific logic that cannot be generalized
    let totalProcessed = 0;
    let totalValid = 0;
    let totalInvalid = 0;
    let totalLowStock = 0;
    let totalOutOfStock = 0;
    const batchSummaries: Array<{ batch_id: string; rows: number }> = [];

    for (const result of batchResults) {
      if (result) {
        totalProcessed += (result.rows_processed as number) ?? 0;
        totalValid += (result.valid_products as number) ?? 0;
        totalInvalid += (result.invalid_products as number) ?? 0;
        totalLowStock += (result.low_stock_items as number) ?? 0;
        totalOutOfStock += (result.out_of_stock_items as number) ?? 0;
        batchSummaries.push({
          batch_id: result.batch_id as string,
          rows: (result.rows_processed as number) ?? 0,
        });
      }
    }

    return this.success({
      total_processed: totalProcessed,
      worker_count: batchResults.length,
      valid_products: totalValid,
      invalid_products: totalInvalid,
      low_stock_items: totalLowStock,
      out_of_stock_items: totalOutOfStock,
      batch_summaries: batchSummaries,
      aggregated_at: new Date().toISOString(),
    });
  }
}
