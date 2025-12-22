import { beforeEach, describe, expect, test } from 'bun:test';
import { StepHandler } from '../../../src/handler/base';
import type { Batchable } from '../../../src/handler/batchable';
import { applyBatchable, BatchableMixin } from '../../../src/handler/batchable';
import type { BatchAnalyzerOutcome, BatchWorkerOutcome } from '../../../src/types/batch';
import { StepContext } from '../../../src/types/step-context';
import type { StepHandlerResult } from '../../../src/types/step-handler-result';

// =============================================================================
// Test Fixtures
// =============================================================================

/**
 * Simple handler that uses applyBatchable for testing.
 */
class TestBatchHandler extends StepHandler {
  static handlerName = 'test_batch_handler';

  constructor() {
    super();
    applyBatchable(this);
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ processed: true });
  }
}

// =============================================================================
// BatchableMixin Tests
// =============================================================================

describe('BatchableMixin', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  describe('createCursorConfig', () => {
    test('should create cursor config with required fields', () => {
      const config = mixin.createCursorConfig(0, 100);

      expect(config.startCursor).toBe(0);
      expect(config.endCursor).toBe(100);
      expect(config.stepSize).toBe(1);
      expect(config.metadata).toEqual({});
    });

    test('should use custom step size', () => {
      const config = mixin.createCursorConfig(0, 100, 5);

      expect(config.stepSize).toBe(5);
    });

    test('should include metadata', () => {
      const config = mixin.createCursorConfig(0, 100, 1, { source: 'test' });

      expect(config.metadata).toEqual({ source: 'test' });
    });
  });

  describe('createCursorRanges', () => {
    test('should create ranges for even division', () => {
      const ranges = mixin.createCursorRanges(100, 25);

      expect(ranges).toHaveLength(4);
      expect(ranges[0]).toEqual({ startCursor: 0, endCursor: 25, stepSize: 1, metadata: {} });
      expect(ranges[1]).toEqual({ startCursor: 25, endCursor: 50, stepSize: 1, metadata: {} });
      expect(ranges[2]).toEqual({ startCursor: 50, endCursor: 75, stepSize: 1, metadata: {} });
      expect(ranges[3]).toEqual({ startCursor: 75, endCursor: 100, stepSize: 1, metadata: {} });
    });

    test('should handle uneven division', () => {
      const ranges = mixin.createCursorRanges(100, 30);

      expect(ranges).toHaveLength(4);
      expect(ranges[0]).toEqual({ startCursor: 0, endCursor: 30, stepSize: 1, metadata: {} });
      expect(ranges[1]).toEqual({ startCursor: 30, endCursor: 60, stepSize: 1, metadata: {} });
      expect(ranges[2]).toEqual({ startCursor: 60, endCursor: 90, stepSize: 1, metadata: {} });
      expect(ranges[3]).toEqual({ startCursor: 90, endCursor: 100, stepSize: 1, metadata: {} });
    });

    test('should return empty array for zero items', () => {
      const ranges = mixin.createCursorRanges(0, 100);

      expect(ranges).toHaveLength(0);
    });

    test('should use custom step size', () => {
      const ranges = mixin.createCursorRanges(50, 25, 5);

      expect(ranges[0].stepSize).toBe(5);
      expect(ranges[1].stepSize).toBe(5);
    });

    test('should limit batches when maxBatches is specified', () => {
      // 100 items / 10 batch size = 10 batches normally
      // With maxBatches=4, should adjust batch size to 25
      const ranges = mixin.createCursorRanges(100, 10, 1, 4);

      expect(ranges).toHaveLength(4);
      expect(ranges[0]).toEqual({ startCursor: 0, endCursor: 25, stepSize: 1, metadata: {} });
    });

    test('should not adjust when batches are within limit', () => {
      // 100 items / 50 batch size = 2 batches
      // maxBatches=4 should not change anything
      const ranges = mixin.createCursorRanges(100, 50, 1, 4);

      expect(ranges).toHaveLength(2);
    });

    test('should handle single item', () => {
      const ranges = mixin.createCursorRanges(1, 100);

      expect(ranges).toHaveLength(1);
      expect(ranges[0]).toEqual({ startCursor: 0, endCursor: 1, stepSize: 1, metadata: {} });
    });

    test('should handle batch size larger than total', () => {
      const ranges = mixin.createCursorRanges(50, 100);

      expect(ranges).toHaveLength(1);
      expect(ranges[0]).toEqual({ startCursor: 0, endCursor: 50, stepSize: 1, metadata: {} });
    });
  });

  describe('createBatchOutcome', () => {
    test('should create batch analyzer outcome', () => {
      const outcome = mixin.createBatchOutcome(100, 25);

      expect(outcome.totalItems).toBe(100);
      expect(outcome.cursorConfigs).toHaveLength(4);
      expect(outcome.batchMetadata).toEqual({});
    });

    test('should include custom batch metadata', () => {
      const outcome = mixin.createBatchOutcome(100, 25, 1, { source: 'products' });

      expect(outcome.batchMetadata).toEqual({ source: 'products' });
    });

    test('should use custom step size', () => {
      const outcome = mixin.createBatchOutcome(100, 25, 5);

      expect(outcome.cursorConfigs[0].stepSize).toBe(5);
    });
  });

  describe('createWorkerOutcome', () => {
    test('should create worker outcome with minimal fields', () => {
      const outcome = mixin.createWorkerOutcome(100);

      expect(outcome.itemsProcessed).toBe(100);
      expect(outcome.itemsSucceeded).toBe(100); // Defaults to itemsProcessed
      expect(outcome.itemsFailed).toBe(0);
      expect(outcome.itemsSkipped).toBe(0);
      expect(outcome.results).toEqual([]);
      expect(outcome.errors).toEqual([]);
      expect(outcome.lastCursor).toBeNull();
      expect(outcome.batchMetadata).toEqual({});
    });

    test('should track partial success', () => {
      const outcome = mixin.createWorkerOutcome(100, 80, 15, 5);

      expect(outcome.itemsProcessed).toBe(100);
      expect(outcome.itemsSucceeded).toBe(80);
      expect(outcome.itemsFailed).toBe(15);
      expect(outcome.itemsSkipped).toBe(5);
    });

    test('should include results and errors', () => {
      const results = [{ id: 1 }, { id: 2 }];
      const errors = [{ id: 3, error: 'failed' }];

      const outcome = mixin.createWorkerOutcome(3, 2, 1, 0, results, errors);

      expect(outcome.results).toEqual(results);
      expect(outcome.errors).toEqual(errors);
    });

    test('should track last cursor', () => {
      const outcome = mixin.createWorkerOutcome(100, 100, 0, 0, [], [], 99);

      expect(outcome.lastCursor).toBe(99);
    });

    test('should include batch metadata', () => {
      const outcome = mixin.createWorkerOutcome(100, 100, 0, 0, [], [], null, {
        batchIndex: 2,
      });

      expect(outcome.batchMetadata).toEqual({ batchIndex: 2 });
    });
  });

  describe('getBatchContext', () => {
    test('should extract batch context from stepConfig', () => {
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepConfig: {
          batch_context: {
            batch_id: 'batch-123',
            start_cursor: 0,
            end_cursor: 100,
            step_size: 1,
            batch_index: 0,
            total_batches: 4,
          },
        },
      });

      const batchContext = mixin.getBatchContext(context);

      expect(batchContext).not.toBeNull();
      expect(batchContext?.batchId).toBe('batch-123');
      expect(batchContext?.cursorConfig.startCursor).toBe(0);
      expect(batchContext?.cursorConfig.endCursor).toBe(100);
      expect(batchContext?.batchIndex).toBe(0);
      expect(batchContext?.totalBatches).toBe(4);
    });

    test('should extract batch context from inputData', () => {
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: {
          batch_context: {
            batch_id: 'batch-456',
            start_cursor: 50,
            end_cursor: 100,
            step_size: 2,
            batch_index: 1,
            total_batches: 2,
          },
        },
      });

      const batchContext = mixin.getBatchContext(context);

      expect(batchContext).not.toBeNull();
      expect(batchContext?.batchId).toBe('batch-456');
      expect(batchContext?.cursorConfig.startCursor).toBe(50);
      expect(batchContext?.cursorConfig.stepSize).toBe(2);
    });

    test('should extract batch context from stepInputs', () => {
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepInputs: {
          batch_context: {
            batch_id: 'batch-789',
            start_cursor: 0,
            end_cursor: 50,
            step_size: 1,
            batch_index: 0,
            total_batches: 1,
          },
        },
      });

      const batchContext = mixin.getBatchContext(context);

      expect(batchContext).not.toBeNull();
      expect(batchContext?.batchId).toBe('batch-789');
    });

    test('should return null when no batch context found', () => {
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
      });

      const batchContext = mixin.getBatchContext(context);

      expect(batchContext).toBeNull();
    });

    test('should include batch metadata when present', () => {
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepConfig: {
          batch_context: {
            batch_id: 'batch-123',
            start_cursor: 0,
            end_cursor: 100,
            step_size: 1,
            batch_index: 0,
            total_batches: 4,
            batch_metadata: { source: 'products' },
          },
        },
      });

      const batchContext = mixin.getBatchContext(context);

      expect(batchContext?.batchMetadata).toEqual({ source: 'products' });
    });
  });

  describe('batchAnalyzerSuccess', () => {
    test('should create success result with batch outcome', () => {
      const outcome: BatchAnalyzerOutcome = {
        cursorConfigs: [
          { startCursor: 0, endCursor: 50, stepSize: 1, metadata: {} },
          { startCursor: 50, endCursor: 100, stepSize: 1, metadata: {} },
        ],
        totalItems: 100,
        batchMetadata: { source: 'test' },
      };

      const result = mixin.batchAnalyzerSuccess(outcome);

      expect(result.success).toBe(true);
      const analyzerOutcome = result.result?.batch_analyzer_outcome as Record<string, unknown>;
      expect(analyzerOutcome.total_items).toBe(100);
      expect(analyzerOutcome.batch_metadata).toEqual({ source: 'test' });
      const configs = analyzerOutcome.cursor_configs as Array<Record<string, unknown>>;
      expect(configs).toHaveLength(2);
      expect(configs[0].start_cursor).toBe(0);
      expect(configs[0].end_cursor).toBe(50);
    });

    test('should include metadata', () => {
      const outcome: BatchAnalyzerOutcome = {
        cursorConfigs: [],
        totalItems: 0,
        batchMetadata: {},
      };

      const result = mixin.batchAnalyzerSuccess(outcome, { custom: 'value' });

      expect(result.metadata?.custom).toBe('value');
    });
  });

  describe('batchWorkerSuccess', () => {
    test('should create success result with worker outcome', () => {
      const outcome: BatchWorkerOutcome = {
        itemsProcessed: 100,
        itemsSucceeded: 95,
        itemsFailed: 3,
        itemsSkipped: 2,
        results: [{ id: 1 }, { id: 2 }],
        errors: [{ id: 3, error: 'failed' }],
        lastCursor: 99,
        batchMetadata: { batchIndex: 0 },
      };

      const result = mixin.batchWorkerSuccess(outcome);

      expect(result.success).toBe(true);
      const workerOutcome = result.result?.batch_worker_outcome as Record<string, unknown>;
      expect(workerOutcome.items_processed).toBe(100);
      expect(workerOutcome.items_succeeded).toBe(95);
      expect(workerOutcome.items_failed).toBe(3);
      expect(workerOutcome.items_skipped).toBe(2);
      expect(workerOutcome.results).toEqual([{ id: 1 }, { id: 2 }]);
      expect(workerOutcome.errors).toEqual([{ id: 3, error: 'failed' }]);
      expect(workerOutcome.last_cursor).toBe(99);
      expect(workerOutcome.batch_metadata).toEqual({ batchIndex: 0 });
    });

    test('should include metadata', () => {
      const outcome: BatchWorkerOutcome = {
        itemsProcessed: 50,
        itemsSucceeded: 50,
        itemsFailed: 0,
        itemsSkipped: 0,
        results: [],
        errors: [],
        lastCursor: null,
        batchMetadata: {},
      };

      const result = mixin.batchWorkerSuccess(outcome, { duration_ms: 1500 });

      expect(result.metadata?.duration_ms).toBe(1500);
    });
  });
});

// =============================================================================
// applyBatchable Tests
// =============================================================================

describe('applyBatchable', () => {
  test('should add all Batchable methods to target', () => {
    const handler = new TestBatchHandler() as TestBatchHandler & Batchable;

    expect(typeof handler.createCursorConfig).toBe('function');
    expect(typeof handler.createCursorRanges).toBe('function');
    expect(typeof handler.createBatchOutcome).toBe('function');
    expect(typeof handler.createWorkerOutcome).toBe('function');
    expect(typeof handler.getBatchContext).toBe('function');
    expect(typeof handler.batchAnalyzerSuccess).toBe('function');
    expect(typeof handler.batchWorkerSuccess).toBe('function');
  });

  test('should make methods functional', () => {
    const handler = new TestBatchHandler() as TestBatchHandler & Batchable;

    const config = handler.createCursorConfig(0, 100);
    expect(config.startCursor).toBe(0);
    expect(config.endCursor).toBe(100);
  });

  test('should work with createBatchOutcome', () => {
    const handler = new TestBatchHandler() as TestBatchHandler & Batchable;

    const outcome = handler.createBatchOutcome(1000, 100);
    expect(outcome.totalItems).toBe(1000);
    expect(outcome.cursorConfigs).toHaveLength(10);
  });
});

// =============================================================================
// Integration Tests
// =============================================================================

describe('Batchable integration', () => {
  describe('ProductBatchAnalyzer', () => {
    class ProductBatchAnalyzer extends StepHandler implements Batchable {
      static handlerName = 'analyze_products';

      createCursorConfig = BatchableMixin.prototype.createCursorConfig.bind(new BatchableMixin());
      createCursorRanges = BatchableMixin.prototype.createCursorRanges.bind(new BatchableMixin());
      createBatchOutcome = BatchableMixin.prototype.createBatchOutcome.bind(new BatchableMixin());
      createWorkerOutcome = BatchableMixin.prototype.createWorkerOutcome.bind(new BatchableMixin());
      getBatchContext = BatchableMixin.prototype.getBatchContext.bind(new BatchableMixin());
      batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess.bind(
        new BatchableMixin()
      );
      batchWorkerSuccess = BatchableMixin.prototype.batchWorkerSuccess.bind(new BatchableMixin());

      async call(context: StepContext): Promise<StepHandlerResult> {
        const productCount = context.inputData.product_count as number;
        const outcome = this.createBatchOutcome(productCount, 100);
        return this.batchAnalyzerSuccess(outcome);
      }
    }

    test('should create batch outcome from product count', async () => {
      const analyzer = new ProductBatchAnalyzer();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        inputData: { product_count: 500 },
      });

      const result = await analyzer.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.batch_analyzer_outcome as Record<string, unknown>;
      expect(outcome.total_items).toBe(500);
      const configs = outcome.cursor_configs as Array<Record<string, unknown>>;
      expect(configs).toHaveLength(5);
    });
  });

  describe('ProductBatchWorker', () => {
    class ProductBatchWorker extends StepHandler implements Batchable {
      static handlerName = 'process_products';

      createCursorConfig = BatchableMixin.prototype.createCursorConfig.bind(new BatchableMixin());
      createCursorRanges = BatchableMixin.prototype.createCursorRanges.bind(new BatchableMixin());
      createBatchOutcome = BatchableMixin.prototype.createBatchOutcome.bind(new BatchableMixin());
      createWorkerOutcome = BatchableMixin.prototype.createWorkerOutcome.bind(new BatchableMixin());
      getBatchContext = BatchableMixin.prototype.getBatchContext.bind(new BatchableMixin());
      batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess.bind(
        new BatchableMixin()
      );
      batchWorkerSuccess = BatchableMixin.prototype.batchWorkerSuccess.bind(new BatchableMixin());

      async call(context: StepContext): Promise<StepHandlerResult> {
        const batchCtx = this.getBatchContext(context);
        if (!batchCtx) {
          return this.failure('No batch context found');
        }

        const itemsProcessed = batchCtx.cursorConfig.endCursor - batchCtx.cursorConfig.startCursor;
        const outcome = this.createWorkerOutcome(itemsProcessed, itemsProcessed);
        return this.batchWorkerSuccess(outcome);
      }
    }

    test('should process batch from context', async () => {
      const worker = new ProductBatchWorker();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepConfig: {
          batch_context: {
            batch_id: 'batch-123',
            start_cursor: 100,
            end_cursor: 200,
            step_size: 1,
            batch_index: 1,
            total_batches: 5,
          },
        },
      });

      const result = await worker.call(context);

      expect(result.success).toBe(true);
      const outcome = result.result?.batch_worker_outcome as Record<string, unknown>;
      expect(outcome.items_processed).toBe(100);
      expect(outcome.items_succeeded).toBe(100);
    });

    test('should fail when no batch context', async () => {
      const worker = new ProductBatchWorker();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
      });

      const result = await worker.call(context);

      expect(result.success).toBe(false);
      expect(result.errorMessage).toBe('No batch context found');
    });
  });
});
