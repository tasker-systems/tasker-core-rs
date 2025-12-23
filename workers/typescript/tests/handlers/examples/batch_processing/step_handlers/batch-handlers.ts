/**
 * Batch Processing Step Handlers for E2E Testing.
 *
 * Implements the TAS-59 batch processing pattern:
 * 1. CsvAnalyzerHandler (batchable): Analyze CSV, create batch configurations
 * 2. CsvBatchProcessorHandler (batch_worker): Process CSV rows in batches
 * 3. CsvResultsAggregatorHandler (deferred_convergence): Aggregate results
 *
 * Matches Ruby and Python batch processing implementations for testing parity.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import { BatchableStepHandler } from '../../../../../src/handler/batchable.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type {
  BatchableResult,
  BatchWorkerConfig,
  StepHandlerResult,
} from '../../../../../src/types/step-handler-result.js';

/**
 * Batchable: Analyze CSV file and create batch worker configurations.
 *
 * For testing purposes, simulates CSV analysis and creates batch
 * configurations based on the input file path.
 */
export class CsvAnalyzerHandler extends BatchableStepHandler {
  static handlerName = 'BatchProcessing.StepHandlers.CsvAnalyzerHandler';
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
    // For testing, we simulate 1000 rows
    const totalRows = 1000;
    const batchSize =
      (context.step?.initialization?.batch_size as number) ?? CsvAnalyzerHandler.DEFAULT_BATCH_SIZE;
    const maxWorkers =
      (context.step?.initialization?.max_workers as number) ??
      CsvAnalyzerHandler.DEFAULT_MAX_WORKERS;

    // Calculate number of batches
    const numBatches = Math.ceil(totalRows / batchSize);
    const actualWorkers = Math.min(numBatches, maxWorkers);

    // Create batch worker configurations
    const batchConfigs: BatchWorkerConfig[] = [];
    for (let i = 0; i < actualWorkers; i++) {
      const startRow = i * batchSize;
      const endRow = Math.min((i + 1) * batchSize, totalRows);

      batchConfigs.push({
        batch_id: `batch_${i + 1}`,
        cursor_start: startRow,
        cursor_end: endRow,
        row_count: endRow - startRow,
        worker_index: i,
        total_workers: actualWorkers,
      });
    }

    return this.batchSuccess(batchConfigs, {
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
 */
export class CsvBatchProcessorHandler extends StepHandler {
  static handlerName = 'BatchProcessing.StepHandlers.CsvBatchProcessorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Get batch configuration from the step context
    const batchConfig = context.step?.batch_config as BatchWorkerConfig | undefined;

    if (!batchConfig) {
      return this.failure('Missing batch configuration', 'batch_error', false);
    }

    const rowCount = batchConfig.row_count ?? 200;
    const batchId = batchConfig.batch_id ?? 'unknown';

    // Simulate processing - mock results
    const validProducts = Math.floor(rowCount * 0.95);
    const invalidProducts = rowCount - validProducts;
    const lowStockItems = Math.floor(validProducts * 0.1);
    const outOfStockItems = Math.floor(validProducts * 0.02);

    return this.success({
      batch_id: batchId,
      rows_processed: rowCount,
      cursor_start: batchConfig.cursor_start,
      cursor_end: batchConfig.cursor_end,
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
 */
export class CsvResultsAggregatorHandler extends StepHandler {
  static handlerName = 'BatchProcessing.StepHandlers.CsvResultsAggregatorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Collect results from all batch worker instances
    // The dependency name 'process_csv_batch_ts' resolves to all worker instances
    const batchResults = context.getAllDependencyResults('process_csv_batch_ts');

    if (!batchResults || batchResults.length === 0) {
      return this.failure('No batch worker results to aggregate', 'aggregation_error', false);
    }

    // Aggregate metrics
    let totalProcessed = 0;
    let totalValid = 0;
    let totalInvalid = 0;
    let totalLowStock = 0;
    let totalOutOfStock = 0;
    const batchSummaries: Array<{ batch_id: string; rows: number }> = [];

    for (const result of batchResults) {
      if (result.result) {
        totalProcessed += (result.result.rows_processed as number) ?? 0;
        totalValid += (result.result.valid_products as number) ?? 0;
        totalInvalid += (result.result.invalid_products as number) ?? 0;
        totalLowStock += (result.result.low_stock_items as number) ?? 0;
        totalOutOfStock += (result.result.out_of_stock_items as number) ?? 0;
        batchSummaries.push({
          batch_id: result.result.batch_id as string,
          rows: (result.result.rows_processed as number) ?? 0,
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
