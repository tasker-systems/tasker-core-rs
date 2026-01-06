/**
 * TAS-125: Checkpoint Yield Step Handlers for E2E Testing.
 *
 * Demonstrates checkpoint yielding functionality:
 * 1. CheckpointYieldAnalyzerHandler (batchable): Create a single batch for checkpoint testing
 * 2. CheckpointYieldWorkerHandler (batch_worker): Process items with checkpoint yields
 * 3. CheckpointYieldAggregatorHandler (deferred_convergence): Aggregate final output
 *
 * Checkpoint Yield Pattern:
 * - Handler processes items in chunks
 * - At each checkpoint interval, calls checkpointYield()
 * - Checkpoint data (cursor, items_processed, accumulated_results) is persisted
 * - Step is re-dispatched and resumes from the checkpoint
 * - When all items processed, returns success (completes the step)
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import { BatchableStepHandler } from '../../../../../src/handler/batchable.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type {
  BatchableResult,
  StepHandlerResult,
} from '../../../../../src/types/step-handler-result.js';

/**
 * TAS-125: Checkpoint Yield Analyzer Handler
 *
 * Creates a single batch for testing checkpoint yielding.
 * Configuration from task context:
 *   - total_items: Number of items to process (default: 100)
 *
 * Returns batch_processing_outcome with a single batch covering all items.
 */
export class CheckpointYieldAnalyzerHandler extends BatchableStepHandler {
  static handlerName = 'checkpoint_yield.step_handlers.CheckpointYieldAnalyzerHandler';
  static handlerVersion = '1.0.0';

  private static readonly DEFAULT_TOTAL_ITEMS = 100;

  async call(context: StepContext): Promise<BatchableResult> {
    // Get configuration from task context
    const totalItems =
      (context.inputData?.total_items as number) ??
      (context.stepConfig?.total_items as number) ??
      CheckpointYieldAnalyzerHandler.DEFAULT_TOTAL_ITEMS;

    // Get worker template name from step definition initialization
    const workerTemplateName =
      (context.stepConfig?.worker_template_name as string) ?? 'checkpoint_yield_batch_ts';

    if (totalItems <= 0) {
      // No items to process - return no_batches outcome
      return this.noBatchesResult('no_items_to_process', {
        total_items: 0,
        analyzed_at: new Date().toISOString(),
      });
    }

    // Create a single batch for all items (checkpoint yielding handles chunking)
    const batchConfigs = this.createCursorConfigs(totalItems, 1); // 1 worker for checkpoint test

    return this.batchSuccess(workerTemplateName, batchConfigs, {
      test_type: 'checkpoint_yield',
      total_items: totalItems,
      analyzed_at: new Date().toISOString(),
    });
  }
}

/**
 * TAS-125: Checkpoint Yield Worker Handler
 *
 * Processes items with checkpoint yielding to test the TAS-125
 * checkpoint persistence and re-dispatch mechanism.
 *
 * Configuration from task context:
 *   - items_per_checkpoint: Items before checkpoint yield (default: 25)
 *   - fail_after_items: Fail after processing this many items (optional)
 *   - fail_on_attempt: Only fail on this attempt number (default: 1)
 *   - permanent_failure: If true, fail with non-retryable error (default: false)
 *
 * Checkpoint behavior:
 *   - After processing items_per_checkpoint items, calls checkpointYield()
 *   - Checkpoint persists cursor position and accumulated results
 *   - On resume, continues from checkpoint cursor with accumulated results
 */
/** Configuration for checkpoint yield worker */
interface WorkerConfig {
  itemsPerCheckpoint: number;
  failAfterItems: number | undefined;
  failOnAttempt: number;
  permanentFailure: boolean;
}

/** Processing state for checkpoint yield worker */
interface ProcessingState {
  startCursor: number;
  accumulated: { running_total: number; item_ids: string[] };
  totalProcessed: number;
  currentAttempt: number;
}

export class CheckpointYieldWorkerHandler extends BatchableStepHandler {
  static handlerName = 'checkpoint_yield.step_handlers.CheckpointYieldWorkerHandler';
  static handlerVersion = '1.0.0';

  private static readonly DEFAULT_ITEMS_PER_CHECKPOINT = 25;

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Check for no-op placeholder
    const noOpResult = this.handleNoOpWorker(context);
    if (noOpResult) {
      return noOpResult;
    }

    // Get batch worker inputs
    const batchInputs = this.getBatchWorkerInputs(context);
    const cursor = batchInputs?.cursor;

    if (!cursor) {
      return this.failure('No batch inputs found', 'batch_error', false);
    }

    const config = this.getWorkerConfig(context);
    const state = this.getProcessingState(context, cursor.start_cursor);
    const endCursor = cursor.end_cursor;

    return this.processItems(config, state, endCursor);
  }

  private getWorkerConfig(context: StepContext): WorkerConfig {
    return {
      itemsPerCheckpoint:
        (context.inputData?.items_per_checkpoint as number) ??
        (context.stepConfig?.items_per_checkpoint as number) ??
        CheckpointYieldWorkerHandler.DEFAULT_ITEMS_PER_CHECKPOINT,
      failAfterItems: context.inputData?.fail_after_items as number | undefined,
      failOnAttempt: (context.inputData?.fail_on_attempt as number) ?? 1,
      permanentFailure: (context.inputData?.permanent_failure as boolean) ?? false,
    };
  }

  private getProcessingState(context: StepContext, defaultStartCursor: number): ProcessingState {
    const workflowStep = context.event?.workflow_step as Record<string, unknown> | undefined;
    const checkpoint = workflowStep?.checkpoint as Record<string, unknown> | undefined;
    const currentAttempt = ((workflowStep?.attempts as number) ?? 0) + 1;

    if (checkpoint) {
      return {
        startCursor: (checkpoint.cursor as number) ?? defaultStartCursor,
        accumulated: (checkpoint.accumulated_results as ProcessingState['accumulated']) ?? {
          running_total: 0,
          item_ids: [],
        },
        totalProcessed: (checkpoint.items_processed as number) ?? 0,
        currentAttempt,
      };
    }

    return {
      startCursor: defaultStartCursor,
      accumulated: { running_total: 0, item_ids: [] },
      totalProcessed: 0,
      currentAttempt,
    };
  }

  private processItems(
    config: WorkerConfig,
    state: ProcessingState,
    endCursor: number
  ): StepHandlerResult {
    let currentCursor = state.startCursor;
    let itemsInChunk = 0;
    let totalProcessed = state.totalProcessed;
    const accumulated = state.accumulated;

    while (currentCursor < endCursor) {
      // Check for failure injection
      if (this.shouldInjectFailure(config, totalProcessed, state.currentAttempt)) {
        return this.injectFailure(totalProcessed, currentCursor, config.permanentFailure);
      }

      // Process one item
      const itemResult = this.processItem(currentCursor);
      accumulated.running_total += itemResult.value;
      accumulated.item_ids.push(itemResult.id);

      currentCursor += 1;
      itemsInChunk += 1;
      totalProcessed += 1;

      // Check if we should yield a checkpoint
      if (itemsInChunk >= config.itemsPerCheckpoint && currentCursor < endCursor) {
        return this.checkpointYield(currentCursor, totalProcessed, accumulated);
      }
    }

    // All items processed - return success
    return this.success({
      items_processed: totalProcessed,
      items_succeeded: totalProcessed,
      items_failed: 0,
      batch_metadata: {
        ...accumulated,
        final_cursor: currentCursor,
        checkpoints_used: Math.floor(totalProcessed / config.itemsPerCheckpoint),
      },
    });
  }

  private shouldInjectFailure(
    config: WorkerConfig,
    totalProcessed: number,
    currentAttempt: number
  ): boolean {
    return (
      config.failAfterItems !== undefined &&
      totalProcessed >= config.failAfterItems &&
      currentAttempt === config.failOnAttempt
    );
  }

  private processItem(cursor: number): { id: string; value: number } {
    return {
      id: `item_${String(cursor).padStart(4, '0')}`,
      value: cursor + 1,
    };
  }

  private injectFailure(
    itemsProcessed: number,
    cursor: number,
    permanent: boolean
  ): StepHandlerResult {
    const errorType = permanent ? 'PermanentError' : 'RetryableError';
    const failureType = permanent ? 'permanent' : 'transient';
    const message = `Injected ${failureType} failure after ${itemsProcessed} items`;

    return this.failure(message, errorType, !permanent, {
      items_processed: itemsProcessed,
      cursor_at_failure: cursor,
      failure_type: failureType,
    });
  }
}

/**
 * TAS-125: Checkpoint Yield Aggregator Handler
 *
 * Aggregates results from checkpoint yield batch workers.
 * Collects final output for E2E test verification.
 */
export class CheckpointYieldAggregatorHandler extends StepHandler {
  static handlerName = 'checkpoint_yield.step_handlers.CheckpointYieldAggregatorHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Get results from batch workers
    const batchResults = context.getAllDependencyResults(
      'checkpoint_yield_batch_ts'
    ) as Array<Record<string, unknown> | null>;

    // Handle no batches scenario (check analyze step result)
    const analyzeResult = context.getDependencyResult('analyze_items_ts') as Record<
      string,
      unknown
    > | null;
    const outcome = analyzeResult?.batch_processing_outcome as Record<string, unknown> | undefined;

    if (outcome?.type === 'no_batches') {
      return this.success({
        total_processed: 0,
        running_total: 0,
        test_passed: true,
        scenario: 'no_batches',
      });
    }

    if (!batchResults || batchResults.length === 0) {
      return this.failure('No batch worker results to aggregate', 'aggregation_error', false);
    }

    // Aggregate from batch results
    let totalProcessed = 0;
    let runningTotal = 0;
    const allItemIds: string[] = [];
    let checkpointsUsed = 0;

    for (const result of batchResults) {
      if (!result) continue;

      totalProcessed += (result.items_processed as number) ?? 0;
      const batchMetadata = result.batch_metadata as Record<string, unknown> | undefined;
      if (batchMetadata) {
        runningTotal += (batchMetadata.running_total as number) ?? 0;
        const itemIds = batchMetadata.item_ids as string[] | undefined;
        if (itemIds) {
          allItemIds.push(...itemIds);
        }
        checkpointsUsed += (batchMetadata.checkpoints_used as number) ?? 0;
      }
    }

    // NOTE: Unlike Ruby where `result:` is a keyword argument,
    // TypeScript success() takes the result object directly.
    // Do NOT wrap in { result: { ... } } to avoid double nesting.
    return this.success({
      total_processed: totalProcessed,
      running_total: runningTotal,
      item_count: allItemIds.length,
      checkpoints_used: checkpointsUsed,
      worker_count: batchResults.length,
      test_passed: true,
      scenario: 'with_batches',
    });
  }
}
