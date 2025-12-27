import type {
  BatchAnalyzerOutcome,
  BatchWorkerContext,
  BatchWorkerOutcome,
  CursorConfig,
} from '../types/batch.js';
import { createBatchWorkerContext } from '../types/batch.js';
import type { StepContext } from '../types/step-context.js';
import {
  type BatchableResult,
  type BatchWorkerConfig,
  StepHandlerResult,
} from '../types/step-handler-result.js';
import { StepHandler } from './base.js';

/**
 * Rust BatchWorkerInputs structure from workflow_step.inputs.
 *
 * This matches the structure created by Rust's BatchProcessingService
 * and stored in workflow_steps.inputs for batch workers.
 *
 * Cross-language standard: Python and Ruby use equivalent structures.
 */
export interface RustBatchWorkerInputs {
  cursor: {
    batch_id: string;
    start_cursor: number;
    end_cursor: number;
    batch_size: number;
  };
  batch_metadata: {
    checkpoint_interval: number;
    cursor_field: string;
    failure_strategy: string;
  };
  is_no_op: boolean;
}

/**
 * Result from aggregating multiple batch worker results.
 *
 * Cross-language standard: matches Python's aggregate_worker_results output.
 */
export interface BatchAggregationResult {
  totalProcessed: number;
  totalSucceeded: number;
  totalFailed: number;
  totalSkipped: number;
  batchCount: number;
  successRate: number;
  errors: Array<Record<string, unknown>>;
  errorCount: number;
}

/**
 * Mixin interface for batch processing capabilities.
 *
 * TypeScript implementation using interface + method binding pattern
 * (since TS doesn't have true mixins like Python/Ruby).
 *
 * Matches Python's Batchable mixin and Ruby's Batchable module (TAS-92 aligned).
 */
export interface Batchable {
  // =========================================================================
  // Cursor Configuration Helpers
  // =========================================================================

  createCursorConfig(
    start: number,
    end: number,
    stepSize?: number,
    metadata?: Record<string, unknown>
  ): CursorConfig;

  createCursorRanges(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    maxBatches?: number
  ): CursorConfig[];

  /**
   * Create cursor configurations for a specific number of workers.
   *
   * Ruby-style method that divides items into worker_count roughly equal ranges.
   * Use this when you know the desired number of workers rather than batch size.
   *
   * Cross-language standard: matches Ruby's create_cursor_configs(total_items, worker_count).
   */
  createCursorConfigs(totalItems: number, workerCount: number): BatchWorkerConfig[];

  // =========================================================================
  // Batch Outcome Builders
  // =========================================================================

  createBatchOutcome(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    batchMetadata?: Record<string, unknown>
  ): BatchAnalyzerOutcome;

  createWorkerOutcome(
    itemsProcessed: number,
    itemsSucceeded?: number,
    itemsFailed?: number,
    itemsSkipped?: number,
    results?: Array<Record<string, unknown>>,
    errors?: Array<Record<string, unknown>>,
    lastCursor?: number | null,
    batchMetadata?: Record<string, unknown>
  ): BatchWorkerOutcome;

  // =========================================================================
  // Batch Context Helpers
  // =========================================================================

  getBatchContext(context: StepContext): BatchWorkerContext | null;

  /**
   * Get Rust batch worker inputs from step context.
   *
   * Returns the BatchWorkerInputs structure from workflow_step.inputs,
   * which contains cursor config, batch metadata, and no-op flag.
   *
   * Cross-language standard: matches Ruby's get_batch_context pattern.
   */
  getBatchWorkerInputs(context: StepContext): Partial<RustBatchWorkerInputs> | null;

  /**
   * Handle no-op placeholder worker scenario.
   *
   * Returns a success result if the worker is a no-op placeholder,
   * otherwise returns null to allow normal processing to continue.
   *
   * Cross-language standard: matches Ruby's handle_no_op_worker.
   */
  handleNoOpWorker(context: StepContext): StepHandlerResult | null;

  // =========================================================================
  // Result Helpers
  // =========================================================================

  batchAnalyzerSuccess(
    outcome: BatchAnalyzerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;

  batchWorkerSuccess(
    outcome: BatchWorkerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult;
}

/**
 * Implementation of Batchable methods.
 *
 * Use this class to add batch processing capabilities to your handlers.
 * The methods can be bound to handler instances or used as a mixin.
 *
 * @example Analyzer using method binding
 * ```typescript
 * class ProductAnalyzer extends StepHandler implements Batchable {
 *   // Bind Batchable methods to this instance
 *   createBatchOutcome = BatchableMixin.prototype.createBatchOutcome.bind(this);
 *   batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess.bind(this);
 *   // ... other required Batchable methods
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const total = context.inputData['product_count'] as number;
 *     const outcome = this.createBatchOutcome(total, 100);
 *     return this.batchAnalyzerSuccess(outcome);
 *   }
 * }
 * ```
 *
 * @example Worker using method binding
 * ```typescript
 * class ProductWorker extends StepHandler implements Batchable {
 *   getBatchContext = BatchableMixin.prototype.getBatchContext.bind(this);
 *   createWorkerOutcome = BatchableMixin.prototype.createWorkerOutcome.bind(this);
 *   batchWorkerSuccess = BatchableMixin.prototype.batchWorkerSuccess.bind(this);
 *   // ... other required Batchable methods
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const batchCtx = this.getBatchContext(context);
 *     if (!batchCtx) {
 *       return this.failure('No batch context found');
 *     }
 *
 *     const results = [];
 *     for (let i = batchCtx.startCursor; i < batchCtx.endCursor; i++) {
 *       results.push(await this.processItem(i));
 *     }
 *
 *     const outcome = this.createWorkerOutcome(results.length, results.length);
 *     return this.batchWorkerSuccess(outcome);
 *   }
 * }
 * ```
 */
export class BatchableMixin implements Batchable {
  /**
   * Create a cursor configuration for a batch range.
   *
   * @param start - Starting cursor position (inclusive)
   * @param end - Ending cursor position (exclusive)
   * @param stepSize - Step size for iteration (default: 1)
   * @param metadata - Additional metadata for this cursor range
   * @returns CursorConfig for the specified range
   */
  createCursorConfig(
    start: number,
    end: number,
    stepSize = 1,
    metadata?: Record<string, unknown>
  ): CursorConfig {
    return {
      startCursor: start,
      endCursor: end,
      stepSize,
      metadata: metadata || {},
    };
  }

  /**
   * Create cursor ranges for batch processing.
   *
   * Divides totalItems into batches of batchSize, optionally limiting
   * the number of batches.
   *
   * @param totalItems - Total number of items to process
   * @param batchSize - Number of items per batch
   * @param stepSize - Step size for iteration (default: 1)
   * @param maxBatches - Maximum number of batches (optional)
   * @returns Array of CursorConfig for each batch
   */
  createCursorRanges(
    totalItems: number,
    batchSize: number,
    stepSize = 1,
    maxBatches?: number
  ): CursorConfig[] {
    if (totalItems === 0) {
      return [];
    }

    let adjustedBatchSize = batchSize;

    // Adjust batch size if max_batches would create more batches
    if (maxBatches && maxBatches > 0) {
      const calculatedBatches = Math.ceil(totalItems / batchSize);
      if (calculatedBatches > maxBatches) {
        adjustedBatchSize = Math.ceil(totalItems / maxBatches);
      }
    }

    const configs: CursorConfig[] = [];
    let start = 0;

    while (start < totalItems) {
      const end = Math.min(start + adjustedBatchSize, totalItems);
      configs.push({
        startCursor: start,
        endCursor: end,
        stepSize,
        metadata: {},
      });
      start = end;
    }

    return configs;
  }

  /**
   * Create cursor configurations for a specific number of workers.
   *
   * Ruby-style method that divides items into worker_count roughly equal ranges.
   * Uses ceiling division to ensure all items are covered.
   *
   * ## Cursor Boundary Math
   *
   * 1. items_per_worker = ceil(total_items / worker_count)
   * 2. For worker i (0-indexed):
   *    - start = i * items_per_worker
   *    - end = min((i + 1) * items_per_worker, total_items)
   *    - batch_size = end - start
   *
   * Example: 1000 items, 3 workers
   *   - items_per_worker = ceil(1000/3) = 334
   *   - Worker 0: start=0, end=334, size=334
   *   - Worker 1: start=334, end=668, size=334
   *   - Worker 2: start=668, end=1000, size=332
   *
   * Cross-language standard: matches Ruby's create_cursor_configs(total_items, worker_count).
   *
   * @param totalItems - Total number of items to process
   * @param workerCount - Number of workers to create configs for (must be > 0)
   * @returns Array of BatchWorkerConfig for each worker
   */
  createCursorConfigs(totalItems: number, workerCount: number): BatchWorkerConfig[] {
    if (workerCount <= 0) {
      throw new Error('workerCount must be > 0');
    }

    if (totalItems === 0) {
      return [];
    }

    const itemsPerWorker = Math.ceil(totalItems / workerCount);
    const configs: BatchWorkerConfig[] = [];

    for (let i = 0; i < workerCount; i++) {
      const startPosition = i * itemsPerWorker;
      const endPosition = Math.min((i + 1) * itemsPerWorker, totalItems);

      // Skip if this worker would have no items
      if (startPosition >= totalItems) {
        break;
      }

      configs.push({
        batch_id: String(i + 1).padStart(3, '0'),
        cursor_start: startPosition,
        cursor_end: endPosition,
        row_count: endPosition - startPosition,
        worker_index: i,
        total_workers: workerCount,
      });
    }

    return configs;
  }

  /**
   * Create a batch analyzer outcome.
   *
   * Convenience method that creates cursor ranges and wraps them
   * in a BatchAnalyzerOutcome.
   *
   * @param totalItems - Total number of items to process
   * @param batchSize - Number of items per batch
   * @param stepSize - Step size for iteration (default: 1)
   * @param batchMetadata - Metadata to pass to all batch workers
   * @returns BatchAnalyzerOutcome ready for batchAnalyzerSuccess
   */
  createBatchOutcome(
    totalItems: number,
    batchSize: number,
    stepSize = 1,
    batchMetadata?: Record<string, unknown>
  ): BatchAnalyzerOutcome {
    const cursorConfigs = this.createCursorRanges(totalItems, batchSize, stepSize);

    return {
      cursorConfigs,
      totalItems,
      batchMetadata: batchMetadata || {},
    };
  }

  /**
   * Create a batch worker outcome.
   *
   * @param itemsProcessed - Total items processed in this batch
   * @param itemsSucceeded - Items that succeeded (default: itemsProcessed)
   * @param itemsFailed - Items that failed (default: 0)
   * @param itemsSkipped - Items that were skipped (default: 0)
   * @param results - Individual item results
   * @param errors - Individual item errors
   * @param lastCursor - Last cursor position processed
   * @param batchMetadata - Additional batch metadata
   * @returns BatchWorkerOutcome ready for batchWorkerSuccess
   */
  createWorkerOutcome(
    itemsProcessed: number,
    itemsSucceeded = 0,
    itemsFailed = 0,
    itemsSkipped = 0,
    results?: Array<Record<string, unknown>>,
    errors?: Array<Record<string, unknown>>,
    lastCursor?: number | null,
    batchMetadata?: Record<string, unknown>
  ): BatchWorkerOutcome {
    return {
      itemsProcessed,
      itemsSucceeded: itemsSucceeded || itemsProcessed,
      itemsFailed,
      itemsSkipped,
      results: results || [],
      errors: errors || [],
      lastCursor: lastCursor ?? null,
      batchMetadata: batchMetadata || {},
    };
  }

  /**
   * Get the batch context from a step context.
   *
   * Looks for batch context in step_config, input_data, or step_inputs.
   *
   * @param context - The step context
   * @returns BatchWorkerContext or null if not found
   */
  getBatchContext(context: StepContext): BatchWorkerContext | null {
    // Look for batch context in step_config or input_data
    let batchData: Record<string, unknown> | undefined;

    if (context.stepConfig) {
      batchData = context.stepConfig.batch_context as Record<string, unknown> | undefined;
    }

    if (!batchData && context.inputData) {
      batchData = context.inputData.batch_context as Record<string, unknown> | undefined;
    }

    // Also check stepInputs (for cursor config from workflow_step.inputs)
    if (!batchData && context.stepInputs) {
      batchData = context.stepInputs.batch_context as Record<string, unknown> | undefined;
    }

    if (!batchData) {
      return null;
    }

    return createBatchWorkerContext(batchData);
  }

  /**
   * Get Rust batch worker inputs from step context.
   *
   * Returns the BatchWorkerInputs structure from workflow_step.inputs,
   * which contains cursor config, batch metadata, and no-op flag.
   *
   * Cross-language standard: matches Ruby's get_batch_context pattern
   * for accessing Rust-provided batch configuration.
   *
   * @param context - The step context
   * @returns BatchWorkerInputs or null if not found
   */
  getBatchWorkerInputs(context: StepContext): Partial<RustBatchWorkerInputs> | null {
    if (!context.stepInputs || Object.keys(context.stepInputs).length === 0) {
      return null;
    }
    return context.stepInputs as Partial<RustBatchWorkerInputs>;
  }

  /**
   * Handle no-op placeholder worker scenario.
   *
   * Returns a success result if the worker is a no-op placeholder
   * (created when a batchable step returns NoBatches), otherwise
   * returns null to allow normal processing to continue.
   *
   * Cross-language standard: matches Ruby's handle_no_op_worker.
   *
   * @param context - The step context
   * @returns Success result if no-op, null otherwise
   *
   * @example
   * ```typescript
   * async call(context: StepContext): Promise<StepHandlerResult> {
   *   const noOpResult = this.handleNoOpWorker(context);
   *   if (noOpResult) {
   *     return noOpResult;
   *   }
   *   // ... normal processing
   * }
   * ```
   */
  handleNoOpWorker(context: StepContext): StepHandlerResult | null {
    const batchInputs = this.getBatchWorkerInputs(context);

    if (!batchInputs?.is_no_op) {
      return null;
    }

    return StepHandlerResult.success({
      batch_id: batchInputs.cursor?.batch_id ?? 'no_op',
      no_op: true,
      processed_count: 0,
      message: 'No batches to process',
      processed_at: new Date().toISOString(),
    });
  }

  /**
   * Create a success result for a batch analyzer.
   *
   * Formats the BatchAnalyzerOutcome in the structure expected by
   * the orchestration layer.
   *
   * @param outcome - The batch analyzer outcome
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult with the batch outcome
   */
  batchAnalyzerSuccess(
    outcome: BatchAnalyzerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const result: Record<string, unknown> = {
      batch_analyzer_outcome: {
        cursor_configs: outcome.cursorConfigs.map((c) => ({
          start_cursor: c.startCursor,
          end_cursor: c.endCursor,
          step_size: c.stepSize,
          metadata: c.metadata,
        })),
        total_items: outcome.totalItems,
        batch_metadata: outcome.batchMetadata,
      },
    };

    // Access success method from the handler this is mixed into
    return StepHandlerResult.success(result, metadata);
  }

  /**
   * Create a success result for a batch worker.
   *
   * Formats the BatchWorkerOutcome in the structure expected by
   * the orchestration layer.
   *
   * @param outcome - The batch worker outcome
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult with the worker outcome
   */
  batchWorkerSuccess(
    outcome: BatchWorkerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    const result: Record<string, unknown> = {
      batch_worker_outcome: {
        items_processed: outcome.itemsProcessed,
        items_succeeded: outcome.itemsSucceeded,
        items_failed: outcome.itemsFailed,
        items_skipped: outcome.itemsSkipped,
        results: outcome.results,
        errors: outcome.errors,
        last_cursor: outcome.lastCursor,
        batch_metadata: outcome.batchMetadata,
      },
    };

    // Access success method from the handler this is mixed into
    return StepHandlerResult.success(result, metadata);
  }

  // =========================================================================
  // Aggregation Helpers (Static Methods)
  // =========================================================================

  /**
   * Aggregate results from multiple batch workers.
   *
   * Use this in an aggregator step to combine results from all
   * batch workers into a single summary.
   *
   * Cross-language standard: matches Python's aggregate_worker_results.
   *
   * @param workerResults - Array of results from batch worker steps
   * @returns Aggregated summary of all batch processing
   *
   * @example
   * ```typescript
   * // In an aggregator handler
   * const workerResults = context.getAllDependencyResults('process_batch_');
   * const summary = BatchableMixin.aggregateWorkerResults(workerResults);
   * return this.success(summary);
   * ```
   */
  static aggregateWorkerResults(
    workerResults: Array<Record<string, unknown> | null>
  ): BatchAggregationResult {
    let totalProcessed = 0;
    let totalSucceeded = 0;
    let totalFailed = 0;
    let totalSkipped = 0;
    const allErrors: Array<Record<string, unknown>> = [];

    for (const result of workerResults) {
      if (result === null || result === undefined) {
        continue;
      }

      totalProcessed += (result.items_processed as number) ?? 0;
      totalSucceeded += (result.items_succeeded as number) ?? 0;
      totalFailed += (result.items_failed as number) ?? 0;
      totalSkipped += (result.items_skipped as number) ?? 0;

      const errors = result.errors as Array<Record<string, unknown>> | undefined;
      if (errors && Array.isArray(errors)) {
        allErrors.push(...errors);
      }
    }

    return {
      totalProcessed,
      totalSucceeded,
      totalFailed,
      totalSkipped,
      batchCount: workerResults.filter((r) => r !== null).length,
      successRate: totalProcessed > 0 ? totalSucceeded / totalProcessed : 0,
      errors: allErrors.slice(0, 100), // Limit errors
      errorCount: allErrors.length,
    };
  }
}

/**
 * Helper function to apply Batchable methods to a handler class.
 *
 * This is a convenience for applying all Batchable methods at once.
 *
 * @example
 * ```typescript
 * class MyBatchHandler extends StepHandler {
 *   constructor() {
 *     super();
 *     applyBatchable(this);
 *   }
 * }
 * ```
 */
export function applyBatchable<T extends object>(target: T): T & Batchable {
  const mixin = new BatchableMixin();

  (target as T & Batchable).createCursorConfig = mixin.createCursorConfig.bind(mixin);
  (target as T & Batchable).createCursorRanges = mixin.createCursorRanges.bind(mixin);
  (target as T & Batchable).createCursorConfigs = mixin.createCursorConfigs.bind(mixin);
  (target as T & Batchable).createBatchOutcome = mixin.createBatchOutcome.bind(mixin);
  (target as T & Batchable).createWorkerOutcome = mixin.createWorkerOutcome.bind(mixin);
  (target as T & Batchable).getBatchContext = mixin.getBatchContext.bind(mixin);
  (target as T & Batchable).getBatchWorkerInputs = mixin.getBatchWorkerInputs.bind(mixin);
  (target as T & Batchable).handleNoOpWorker = mixin.handleNoOpWorker.bind(mixin);
  (target as T & Batchable).batchAnalyzerSuccess = mixin.batchAnalyzerSuccess.bind(mixin);
  (target as T & Batchable).batchWorkerSuccess = mixin.batchWorkerSuccess.bind(mixin);

  return target as T & Batchable;
}

/**
 * Base class for batch-enabled step handlers.
 *
 * Extends StepHandler with batch processing capabilities.
 * Use this class when implementing handlers that need to create
 * batch worker configurations.
 *
 * @example
 * ```typescript
 * export class CsvAnalyzerHandler extends BatchableStepHandler {
 *   static handlerName = 'MyNamespace.CsvAnalyzer';
 *
 *   async call(context: StepContext): Promise<BatchableResult> {
 *     const totalRows = 1000;
 *     const batchConfigs: BatchWorkerConfig[] = [];
 *
 *     for (let i = 0; i < 5; i++) {
 *       batchConfigs.push({
 *         batch_id: `batch_${i + 1}`,
 *         cursor_start: i * 200,
 *         cursor_end: (i + 1) * 200,
 *         row_count: 200,
 *         worker_index: i,
 *         total_workers: 5,
 *       });
 *     }
 *
 *     return this.batchSuccess(batchConfigs, {
 *       total_rows: totalRows,
 *       analyzed_at: new Date().toISOString(),
 *     });
 *   }
 * }
 * ```
 */
export abstract class BatchableStepHandler extends StepHandler implements Batchable {
  private readonly _batchMixin = new BatchableMixin();

  // Delegate Batchable interface methods to mixin
  createCursorConfig(
    start: number,
    end: number,
    stepSize?: number,
    metadata?: Record<string, unknown>
  ): CursorConfig {
    return this._batchMixin.createCursorConfig(start, end, stepSize, metadata);
  }

  createCursorRanges(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    maxBatches?: number
  ): CursorConfig[] {
    return this._batchMixin.createCursorRanges(totalItems, batchSize, stepSize, maxBatches);
  }

  createCursorConfigs(totalItems: number, workerCount: number): BatchWorkerConfig[] {
    return this._batchMixin.createCursorConfigs(totalItems, workerCount);
  }

  createBatchOutcome(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    batchMetadata?: Record<string, unknown>
  ): BatchAnalyzerOutcome {
    return this._batchMixin.createBatchOutcome(totalItems, batchSize, stepSize, batchMetadata);
  }

  createWorkerOutcome(
    itemsProcessed: number,
    itemsSucceeded?: number,
    itemsFailed?: number,
    itemsSkipped?: number,
    results?: Array<Record<string, unknown>>,
    errors?: Array<Record<string, unknown>>,
    lastCursor?: number | null,
    batchMetadata?: Record<string, unknown>
  ): BatchWorkerOutcome {
    return this._batchMixin.createWorkerOutcome(
      itemsProcessed,
      itemsSucceeded,
      itemsFailed,
      itemsSkipped,
      results,
      errors,
      lastCursor,
      batchMetadata
    );
  }

  getBatchContext(context: StepContext): BatchWorkerContext | null {
    return this._batchMixin.getBatchContext(context);
  }

  getBatchWorkerInputs(context: StepContext): Partial<RustBatchWorkerInputs> | null {
    return this._batchMixin.getBatchWorkerInputs(context);
  }

  handleNoOpWorker(context: StepContext): StepHandlerResult | null {
    return this._batchMixin.handleNoOpWorker(context);
  }

  batchAnalyzerSuccess(
    outcome: BatchAnalyzerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._batchMixin.batchAnalyzerSuccess(outcome, metadata);
  }

  batchWorkerSuccess(
    outcome: BatchWorkerOutcome,
    metadata?: Record<string, unknown>
  ): StepHandlerResult {
    return this._batchMixin.batchWorkerSuccess(outcome, metadata);
  }

  /**
   * Create a successful batch analyzer result with batch worker configurations.
   *
   * This is a convenience method that wraps batch configurations in the format
   * expected by the Rust orchestration layer (BatchProcessingOutcome::CreateBatches).
   *
   * @param workerTemplateName - Name of the batch worker template step (e.g., "process_csv_batch_ts")
   * @param batchConfigs - Array of batch worker configurations
   * @param metadata - Additional metadata to include in the result
   * @returns A BatchableResult (StepHandlerResult) indicating success
   *
   * @example
   * ```typescript
   * return this.batchSuccess('process_csv_batch_ts', batchConfigs, {
   *   total_rows: 1000,
   *   analyzed_at: new Date().toISOString(),
   * });
   * ```
   */
  batchSuccess(
    workerTemplateName: string,
    batchConfigs: BatchWorkerConfig[],
    metadata?: Record<string, unknown>
  ): BatchableResult {
    // Convert BatchWorkerConfig[] to the Rust CursorConfig format
    const cursorConfigs = batchConfigs.map((config) => ({
      batch_id: config.batch_id,
      start_cursor: config.cursor_start,
      end_cursor: config.cursor_end,
      batch_size: config.row_count,
    }));

    const totalItems = batchConfigs.reduce((sum, c) => sum + c.row_count, 0);

    // Return in the exact format expected by Rust BatchProcessingOutcome::CreateBatches
    // The orchestration layer extracts batch_processing_outcome from result
    const result: Record<string, unknown> = {
      batch_processing_outcome: {
        type: 'create_batches',
        worker_template_name: workerTemplateName,
        worker_count: batchConfigs.length,
        cursor_configs: cursorConfigs,
        total_items: totalItems,
      },
      ...(metadata || {}),
    };

    return StepHandlerResult.success(result, metadata);
  }

  /**
   * Create a no-batches result when batch processing is not needed.
   *
   * Use this when the batchable handler determines no batch workers are needed.
   *
   * Cross-language standard: matches Ruby's no_batches_outcome(reason:, metadata:)
   * and Python's no_batches_outcome(reason, metadata).
   *
   * @param reason - Human-readable reason why no batches are needed (optional but recommended)
   * @param metadata - Additional metadata to include in the result
   * @returns A BatchableResult (StepHandlerResult) indicating no batches
   *
   * @example
   * ```typescript
   * if (totalItems === 0) {
   *   return this.noBatchesResult('empty_dataset', { total_rows: 0 });
   * }
   * ```
   */
  noBatchesResult(reason?: string, metadata?: Record<string, unknown>): BatchableResult {
    const result: Record<string, unknown> = {
      batch_processing_outcome: {
        type: 'no_batches',
      },
      ...(metadata || {}),
    };

    // Add reason if provided (matches Ruby/Python pattern)
    if (reason) {
      result.reason = reason;
    }

    return StepHandlerResult.success(result, metadata);
  }
}
