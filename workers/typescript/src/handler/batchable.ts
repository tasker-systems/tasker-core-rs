import type {
  BatchAnalyzerOutcome,
  BatchWorkerContext,
  BatchWorkerOutcome,
  CursorConfig,
} from '../types/batch';
import { createBatchWorkerContext } from '../types/batch';
import type { StepContext } from '../types/step-context';
import {
  type BatchableResult,
  type BatchWorkerConfig,
  StepHandlerResult,
} from '../types/step-handler-result';
import { StepHandler } from './base';

/**
 * Mixin interface for batch processing capabilities.
 *
 * TypeScript implementation using interface + method binding pattern
 * (since TS doesn't have true mixins like Python/Ruby).
 *
 * Matches Python's Batchable mixin and Ruby's Batchable module (TAS-92 aligned).
 */
export interface Batchable {
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

  getBatchContext(context: StepContext): BatchWorkerContext | null;

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
  (target as T & Batchable).createBatchOutcome = mixin.createBatchOutcome.bind(mixin);
  (target as T & Batchable).createWorkerOutcome = mixin.createWorkerOutcome.bind(mixin);
  (target as T & Batchable).getBatchContext = mixin.getBatchContext.bind(mixin);
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
   * expected by the orchestration layer.
   *
   * @param batchConfigs - Array of batch worker configurations
   * @param metadata - Additional metadata to include in the result
   * @returns A BatchableResult (StepHandlerResult) indicating success
   *
   * @example
   * ```typescript
   * return this.batchSuccess(batchConfigs, {
   *   total_rows: 1000,
   *   analyzed_at: new Date().toISOString(),
   * });
   * ```
   */
  batchSuccess(
    batchConfigs: BatchWorkerConfig[],
    metadata?: Record<string, unknown>
  ): BatchableResult {
    // Convert BatchWorkerConfig[] to the format expected by orchestration
    const cursorConfigs: CursorConfig[] = batchConfigs.map((config) => ({
      startCursor: config.cursor_start,
      endCursor: config.cursor_end,
      stepSize: 1,
      metadata: {
        batch_id: config.batch_id,
        row_count: config.row_count,
        worker_index: config.worker_index,
        total_workers: config.total_workers,
        ...config.metadata,
      },
    }));

    const outcome: BatchAnalyzerOutcome = {
      cursorConfigs,
      totalItems: batchConfigs.reduce((sum, c) => sum + c.row_count, 0),
      batchMetadata: metadata || {},
    };

    return this.batchAnalyzerSuccess(outcome, metadata);
  }
}
