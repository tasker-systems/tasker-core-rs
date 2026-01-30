import { beforeEach, describe, expect, test } from 'bun:test';
import { StepHandler } from '../../../src/handler/base';
import type { Batchable } from '../../../src/handler/batchable';
import {
  applyBatchable,
  BatchableMixin,
  BatchableStepHandler,
} from '../../../src/handler/batchable';
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

// =============================================================================
// BatchAggregationScenario Tests (TAS-112)
// =============================================================================

describe('BatchAggregationScenario', () => {
  describe('detectAggregationScenario', () => {
    test('should detect NoBatches scenario from batch_processing_outcome', () => {
      const { detectAggregationScenario } = require('../../../src/handler/batchable');

      const dependencyResults = {
        analyze_csv: {
          batch_processing_outcome: { type: 'no_batches' },
          reason: 'empty_dataset',
        },
      };

      const scenario = detectAggregationScenario(
        dependencyResults,
        'analyze_csv',
        'process_csv_batch_'
      );

      expect(scenario.isNoBatches).toBe(true);
      expect(scenario.workerCount).toBe(0);
      expect(scenario.batchResults).toEqual({});
      expect(scenario.batchableResult.reason).toBe('empty_dataset');
    });

    test('should detect WithBatches scenario', () => {
      const { detectAggregationScenario } = require('../../../src/handler/batchable');

      const dependencyResults = {
        analyze_csv: {
          batch_processing_outcome: { type: 'create_batches', worker_count: 3 },
        },
        process_csv_batch_001: { items_processed: 100, items_succeeded: 98 },
        process_csv_batch_002: { items_processed: 100, items_succeeded: 100 },
        process_csv_batch_003: { items_processed: 50, items_succeeded: 50 },
      };

      const scenario = detectAggregationScenario(
        dependencyResults,
        'analyze_csv',
        'process_csv_batch_'
      );

      expect(scenario.isNoBatches).toBe(false);
      expect(scenario.workerCount).toBe(3);
      expect(Object.keys(scenario.batchResults)).toHaveLength(3);
      expect(scenario.batchResults.process_csv_batch_001.items_succeeded).toBe(98);
      expect(scenario.batchResults.process_csv_batch_002.items_succeeded).toBe(100);
    });

    test('should throw error when batchable step is missing', () => {
      const { detectAggregationScenario } = require('../../../src/handler/batchable');

      const dependencyResults = {
        process_csv_batch_001: { items_processed: 100 },
      };

      expect(() => {
        detectAggregationScenario(dependencyResults, 'analyze_csv', 'process_csv_batch_');
      }).toThrow('Missing batchable step dependency');
    });

    test('should throw error when no workers found without NoBatches outcome', () => {
      const { detectAggregationScenario } = require('../../../src/handler/batchable');

      const dependencyResults = {
        analyze_csv: {
          batch_processing_outcome: { type: 'create_batches', worker_count: 3 },
        },
        // No batch workers present
      };

      expect(() => {
        detectAggregationScenario(dependencyResults, 'analyze_csv', 'process_csv_batch_');
      }).toThrow('No batch workers found');
    });

    test('should handle wrapped result objects', () => {
      const { detectAggregationScenario } = require('../../../src/handler/batchable');

      const dependencyResults = {
        analyze_csv: {
          result: {
            batch_processing_outcome: { type: 'no_batches' },
          },
        },
      };

      const scenario = detectAggregationScenario(
        dependencyResults,
        'analyze_csv',
        'process_csv_batch_'
      );

      expect(scenario.isNoBatches).toBe(true);
    });
  });

  describe('aggregation helper methods', () => {
    let mixin: BatchableMixin;

    beforeEach(() => {
      mixin = new BatchableMixin();
    });

    test('detectAggregationScenario method delegates correctly', () => {
      const dependencyResults = {
        analyze_csv: {
          batch_processing_outcome: { type: 'no_batches' },
        },
      };

      const scenario = mixin.detectAggregationScenario(
        dependencyResults,
        'analyze_csv',
        'process_csv_batch_'
      );

      expect(scenario.isNoBatches).toBe(true);
    });

    test('noBatchesAggregationResult creates correct result', () => {
      const result = mixin.noBatchesAggregationResult({ total_processed: 0, total_value: 0.0 });

      expect(result.success).toBe(true);
      expect(result.result?.worker_count).toBe(0);
      expect(result.result?.scenario).toBe('no_batches');
      expect(result.result?.total_processed).toBe(0);
      expect(result.result?.total_value).toBe(0.0);
    });

    test('aggregateBatchWorkerResults handles NoBatches scenario', () => {
      const scenario = {
        isNoBatches: true,
        batchableResult: { reason: 'empty' },
        batchResults: {},
        workerCount: 0,
      };

      const result = mixin.aggregateBatchWorkerResults(scenario, { total: 0 });

      expect(result.success).toBe(true);
      expect(result.result?.worker_count).toBe(0);
      expect(result.result?.scenario).toBe('no_batches');
      expect(result.result?.total).toBe(0);
    });

    test('aggregateBatchWorkerResults handles WithBatches scenario with custom aggregation', () => {
      const scenario = {
        isNoBatches: false,
        batchableResult: { total_items: 250 },
        batchResults: {
          process_batch_001: { count: 100 },
          process_batch_002: { count: 100 },
          process_batch_003: { count: 50 },
        },
        workerCount: 3,
      };

      const aggregationFn = (batchResults: Record<string, Record<string, unknown>>) => {
        let total = 0;
        for (const result of Object.values(batchResults)) {
          total += (result.count as number) || 0;
        }
        return { total_processed: total };
      };

      const result = mixin.aggregateBatchWorkerResults(
        scenario,
        { total_processed: 0 },
        aggregationFn
      );

      expect(result.success).toBe(true);
      expect(result.result?.worker_count).toBe(3);
      expect(result.result?.scenario).toBe('with_batches');
      expect(result.result?.total_processed).toBe(250);
    });

    test('aggregateBatchWorkerResults uses default aggregation when no function provided', () => {
      const scenario = {
        isNoBatches: false,
        batchableResult: {},
        batchResults: {
          process_batch_001: { count: 100 },
          process_batch_002: { count: 50 },
        },
        workerCount: 2,
      };

      const result = mixin.aggregateBatchWorkerResults(scenario);

      expect(result.success).toBe(true);
      expect(result.result?.worker_count).toBe(2);
      expect(result.result?.scenario).toBe('with_batches');
      expect(result.result?.batch_results).toBeDefined();
      expect(Object.keys(result.result?.batch_results as object)).toHaveLength(2);
    });
  });

  // TAS-125: Checkpoint Yield Tests
  describe('checkpointYield', () => {
    let mixin: BatchableMixin;

    beforeEach(() => {
      mixin = new BatchableMixin();
    });

    test('should create checkpoint yield result with integer cursor', () => {
      const result = mixin.checkpointYield(5000, 5000);

      expect(result.success).toBe(true);
      expect(result.result?.type).toBe('checkpoint_yield');
      expect(result.result?.cursor).toBe(5000);
      expect(result.result?.items_processed).toBe(5000);
      expect(result.metadata?.checkpoint_yield).toBe(true);
      expect(result.metadata?.batch_worker).toBe(true);
    });

    test('should create checkpoint yield result with string cursor (pagination token)', () => {
      const result = mixin.checkpointYield('eyJsYXN0X2lkIjoiOTk5In0=', 100);

      expect(result.success).toBe(true);
      expect(result.result?.cursor).toBe('eyJsYXN0X2lkIjoiOTk5In0=');
      expect(result.result?.items_processed).toBe(100);
    });

    test('should create checkpoint yield result with object cursor (complex pagination)', () => {
      const complexCursor = { page: 5, partition: 'A', lastTimestamp: '2024-01-15T10:00:00Z' };
      const result = mixin.checkpointYield(complexCursor, 250);

      expect(result.success).toBe(true);
      expect(result.result?.cursor).toEqual(complexCursor);
      expect((result.result?.cursor as Record<string, unknown>).page).toBe(5);
    });

    test('should include accumulated results when provided', () => {
      const result = mixin.checkpointYield(7500, 7500, {
        runningTotal: 375000.5,
        processedCount: 7500,
      });

      expect(result.success).toBe(true);
      expect(result.result?.accumulated_results).toEqual({
        runningTotal: 375000.5,
        processedCount: 7500,
      });
    });

    test('should not include accumulated_results when not provided', () => {
      const result = mixin.checkpointYield(1000, 1000);

      expect(result.success).toBe(true);
      expect(result.result?.accumulated_results).toBeUndefined();
    });
  });
});

// =============================================================================
// TAS-125: Checkpoint Yield with applyBatchable
// =============================================================================

describe('Checkpoint Yield with applyBatchable', () => {
  test('should add checkpointYield method to handler', () => {
    const handler = new TestBatchHandler();

    expect(typeof (handler as Batchable).checkpointYield).toBe('function');
  });

  test('should work correctly on handler instance', () => {
    const handler = new TestBatchHandler();
    const batchableHandler = handler as Batchable;

    const result = batchableHandler.checkpointYield(3000, 3000, {
      partialSum: 150000,
    });

    expect(result.success).toBe(true);
    expect(result.result?.type).toBe('checkpoint_yield');
    expect(result.result?.cursor).toBe(3000);
    expect(result.result?.items_processed).toBe(3000);
    expect(result.result?.accumulated_results).toEqual({ partialSum: 150000 });
  });
});

// =============================================================================
// TAS-125: Checkpoint Yield Error Scenarios
// =============================================================================

describe('Checkpoint Yield Error Scenarios', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  test('should handle zero items processed (edge case)', () => {
    const result = mixin.checkpointYield(0, 0);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBe(0);
    expect(result.result?.items_processed).toBe(0);
  });

  test('should handle null cursor', () => {
    const result = mixin.checkpointYield(null as unknown as number, 0);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBeNull();
  });

  test('should handle undefined cursor', () => {
    const result = mixin.checkpointYield(undefined as unknown as number, 100);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBeUndefined();
  });

  test('should handle empty object cursor', () => {
    const result = mixin.checkpointYield({}, 100);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toEqual({});
  });

  test('should handle empty string cursor', () => {
    const result = mixin.checkpointYield('', 0);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBe('');
  });

  test('should handle very large items processed', () => {
    const largeCount = Number.MAX_SAFE_INTEGER;
    const result = mixin.checkpointYield(largeCount, largeCount);

    expect(result.success).toBe(true);
    expect(result.result?.items_processed).toBe(largeCount);
  });

  test('should handle deeply nested accumulated results', () => {
    const nestedResults = {
      level1: {
        level2: {
          level3: {
            level4: {
              value: 12345,
              items: [1, 2, 3, 4, 5],
            },
          },
        },
      },
    };
    const result = mixin.checkpointYield(5000, 5000, nestedResults);

    expect(result.success).toBe(true);
    const accumulated = result.result?.accumulated_results as Record<string, unknown>;
    expect(
      (
        ((accumulated.level1 as Record<string, unknown>).level2 as Record<string, unknown>)
          .level3 as Record<string, unknown>
      ).level4
    ).toEqual({ value: 12345, items: [1, 2, 3, 4, 5] });
  });

  test('should handle special characters in string cursor', () => {
    const specialCursor = 'cursor_with_émojis_🎉_and_中文';
    const result = mixin.checkpointYield(specialCursor, 100);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBe(specialCursor);
  });

  test('should handle boolean cursor (edge case)', () => {
    // Unusual but technically valid JSON value
    const result = mixin.checkpointYield(true as unknown as number, 100);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toBe(true);
  });

  test('should handle array cursor', () => {
    const arrayCursor = [1, 2, 3, 'last_id'];
    const result = mixin.checkpointYield(arrayCursor as unknown as Record<string, unknown>, 100);

    expect(result.success).toBe(true);
    expect(result.result?.cursor).toEqual(arrayCursor);
  });
});

// =============================================================================
// TAS-125: Batchable Validation Error Scenarios
// =============================================================================

describe('Batchable Validation Errors', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  test('should return empty array for zero items', () => {
    const ranges = mixin.createCursorRanges(0, 100);

    expect(ranges).toHaveLength(0);
  });

  test('should handle single item correctly', () => {
    const ranges = mixin.createCursorRanges(1, 100);

    expect(ranges).toHaveLength(1);
    expect(ranges[0].startCursor).toBe(0);
    expect(ranges[0].endCursor).toBe(1);
  });

  test('should throw error for zero batch size', () => {
    expect(() => mixin.createCursorRanges(100, 0)).toThrow('batchSize must be > 0');
  });

  test('should throw error for negative batch size', () => {
    expect(() => mixin.createCursorRanges(100, -10)).toThrow('batchSize must be > 0');
  });

  test('should handle batch size larger than total', () => {
    const ranges = mixin.createCursorRanges(50, 1000);

    expect(ranges).toHaveLength(1);
    expect(ranges[0].startCursor).toBe(0);
    expect(ranges[0].endCursor).toBe(50);
  });

  test('should handle very large total items', () => {
    const largeTotal = 1000000;
    const ranges = mixin.createCursorRanges(largeTotal, 100000);

    expect(ranges).toHaveLength(10);
    expect(ranges[0].startCursor).toBe(0);
    expect(ranges[0].endCursor).toBe(100000);
    expect(ranges[9].startCursor).toBe(900000);
    expect(ranges[9].endCursor).toBe(1000000);
  });

  test('should return null for getBatchContext when no context exists', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
    });

    const batchContext = mixin.getBatchContext(context);

    expect(batchContext).toBeNull();
  });

  test('should return null for getBatchContext with empty stepConfig', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepConfig: {},
    });

    const batchContext = mixin.getBatchContext(context);

    expect(batchContext).toBeNull();
  });

  test('should handle malformed batch_context gracefully', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepConfig: {
        batch_context: {
          // Missing required fields
          batch_id: 'batch-123',
        },
      },
    });

    // Should return a context with default values for missing fields
    const batchContext = mixin.getBatchContext(context);

    // Implementation may return null or fill in defaults
    if (batchContext !== null) {
      expect(batchContext.batchId).toBe('batch-123');
    }
  });
});

// =============================================================================
// createCursorConfigs Tests (TAS-92 Ruby-style worker division)
// =============================================================================

describe('BatchableMixin.createCursorConfigs', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  test('should divide items evenly across workers', () => {
    const configs = mixin.createCursorConfigs(1000, 5);

    expect(configs).toHaveLength(5);
    expect(configs[0].cursor_start).toBe(0);
    expect(configs[0].cursor_end).toBe(200);
    expect(configs[0].row_count).toBe(200);
    expect(configs[0].batch_id).toBe('001');
    expect(configs[0].worker_index).toBe(0);
    expect(configs[0].total_workers).toBe(5);

    expect(configs[4].cursor_start).toBe(800);
    expect(configs[4].cursor_end).toBe(1000);
    expect(configs[4].row_count).toBe(200);
    expect(configs[4].batch_id).toBe('005');
    expect(configs[4].worker_index).toBe(4);
  });

  test('should handle uneven division (1000 items, 3 workers)', () => {
    const configs = mixin.createCursorConfigs(1000, 3);

    expect(configs).toHaveLength(3);
    // ceil(1000/3) = 334
    expect(configs[0].cursor_start).toBe(0);
    expect(configs[0].cursor_end).toBe(334);
    expect(configs[0].row_count).toBe(334);

    expect(configs[1].cursor_start).toBe(334);
    expect(configs[1].cursor_end).toBe(668);
    expect(configs[1].row_count).toBe(334);

    expect(configs[2].cursor_start).toBe(668);
    expect(configs[2].cursor_end).toBe(1000);
    expect(configs[2].row_count).toBe(332);
  });

  test('should stop early when more workers than items', () => {
    const configs = mixin.createCursorConfigs(5, 10);

    // ceil(5/10) = 1 item per worker, only 5 workers should be created
    expect(configs).toHaveLength(5);
    expect(configs[0].row_count).toBe(1);
    expect(configs[4].cursor_start).toBe(4);
    expect(configs[4].cursor_end).toBe(5);
  });

  test('should return empty array for zero items', () => {
    const configs = mixin.createCursorConfigs(0, 5);

    expect(configs).toHaveLength(0);
  });

  test('should throw for workerCount <= 0', () => {
    expect(() => mixin.createCursorConfigs(100, 0)).toThrow('workerCount must be > 0');
    expect(() => mixin.createCursorConfigs(100, -1)).toThrow('workerCount must be > 0');
  });

  test('should handle single worker', () => {
    const configs = mixin.createCursorConfigs(1000, 1);

    expect(configs).toHaveLength(1);
    expect(configs[0].cursor_start).toBe(0);
    expect(configs[0].cursor_end).toBe(1000);
    expect(configs[0].row_count).toBe(1000);
  });

  test('should pad batch_id with leading zeros', () => {
    const configs = mixin.createCursorConfigs(100, 2);

    expect(configs[0].batch_id).toBe('001');
    expect(configs[1].batch_id).toBe('002');
  });
});

// =============================================================================
// getBatchWorkerInputs Tests (TAS-112/TAS-123)
// =============================================================================

describe('BatchableMixin.getBatchWorkerInputs', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  test('should return stepInputs when present with cursor/batch_metadata', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepInputs: {
        cursor: { batch_id: '001', start_cursor: 0, end_cursor: 100, batch_size: 100 },
        batch_metadata: { cursor_field: 'id', failure_strategy: 'continue_on_failure' },
        is_no_op: false,
      },
    });

    const inputs = mixin.getBatchWorkerInputs(context);

    expect(inputs).not.toBeNull();
    expect(inputs?.is_no_op).toBe(false);
    expect(inputs?.cursor).toBeDefined();
  });

  test('should return null for empty stepInputs', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepInputs: {},
    });

    const inputs = mixin.getBatchWorkerInputs(context);

    expect(inputs).toBeNull();
  });

  test('should return null when no stepInputs', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
    });

    const inputs = mixin.getBatchWorkerInputs(context);

    expect(inputs).toBeNull();
  });
});

// =============================================================================
// handleNoOpWorker Tests (TAS-112/TAS-123)
// =============================================================================

describe('BatchableMixin.handleNoOpWorker', () => {
  let mixin: BatchableMixin;

  beforeEach(() => {
    mixin = new BatchableMixin();
  });

  test('should return success result for no-op worker', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepInputs: {
        cursor: { batch_id: 'no_op', start_cursor: 0, end_cursor: 0, batch_size: 0 },
        batch_metadata: { cursor_field: 'id', failure_strategy: 'continue_on_failure' },
        is_no_op: true,
      },
    });

    const result = mixin.handleNoOpWorker(context);

    expect(result).not.toBeNull();
    expect(result?.success).toBe(true);
    expect(result?.result?.no_op).toBe(true);
    expect(result?.result?.processed_count).toBe(0);
    expect(result?.result?.message).toBe('No batches to process');
  });

  test('should return null when is_no_op is false', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepInputs: {
        cursor: { batch_id: '001', start_cursor: 0, end_cursor: 100, batch_size: 100 },
        is_no_op: false,
      },
    });

    const result = mixin.handleNoOpWorker(context);

    expect(result).toBeNull();
  });

  test('should return null when no stepInputs', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
    });

    const result = mixin.handleNoOpWorker(context);

    expect(result).toBeNull();
  });

  test('should extract batch_id from cursor', () => {
    const context = new StepContext({
      taskUuid: 'task-1',
      stepUuid: 'step-1',
      stepInputs: {
        cursor: { batch_id: 'my_batch' },
        is_no_op: true,
      },
    });

    const result = mixin.handleNoOpWorker(context);

    expect(result?.result?.batch_id).toBe('my_batch');
  });
});

// =============================================================================
// BatchableStepHandler Tests (TAS-112/TAS-123)
// =============================================================================

describe('BatchableStepHandler', () => {
  /**
   * Concrete subclass for testing abstract BatchableStepHandler.
   */
  class TestBatchableHandler extends BatchableStepHandler {
    static handlerName = 'test_batchable_handler';

    async call(_context: StepContext): Promise<StepHandlerResult> {
      return this.success({ processed: true });
    }
  }

  describe('batchSuccess', () => {
    test('should convert BatchWorkerConfig[] to RustCursorConfig[]', () => {
      const handler = new TestBatchableHandler();
      const batchConfigs = [
        {
          batch_id: '001',
          cursor_start: 0,
          cursor_end: 500,
          row_count: 500,
          worker_index: 0,
          total_workers: 2,
        },
        {
          batch_id: '002',
          cursor_start: 500,
          cursor_end: 1000,
          row_count: 500,
          worker_index: 1,
          total_workers: 2,
        },
      ];

      const result = handler.batchSuccess('process_batch_ts', batchConfigs);

      expect(result.success).toBe(true);
      const outcome = result.result?.batch_processing_outcome as Record<string, unknown>;
      expect(outcome.type).toBe('create_batches');
      expect(outcome.worker_template_name).toBe('process_batch_ts');
      expect(outcome.worker_count).toBe(2);
      const configs = outcome.cursor_configs as Array<Record<string, unknown>>;
      expect(configs).toHaveLength(2);
      expect(configs[0].batch_id).toBe('001');
      expect(configs[0].start_cursor).toBe(0);
      expect(configs[0].end_cursor).toBe(500);
      expect(configs[0].batch_size).toBe(500);
    });

    test('should calculate totalItems from row_count sum', () => {
      const handler = new TestBatchableHandler();
      const batchConfigs = [
        {
          batch_id: '001',
          cursor_start: 0,
          cursor_end: 300,
          row_count: 300,
          worker_index: 0,
          total_workers: 3,
        },
        {
          batch_id: '002',
          cursor_start: 300,
          cursor_end: 600,
          row_count: 300,
          worker_index: 1,
          total_workers: 3,
        },
        {
          batch_id: '003',
          cursor_start: 600,
          cursor_end: 800,
          row_count: 200,
          worker_index: 2,
          total_workers: 3,
        },
      ];

      const result = handler.batchSuccess('template', batchConfigs);

      const outcome = result.result?.batch_processing_outcome as Record<string, unknown>;
      expect(outcome.total_items).toBe(800);
    });

    test('should include metadata in result', () => {
      const handler = new TestBatchableHandler();
      const batchConfigs = [
        {
          batch_id: '001',
          cursor_start: 0,
          cursor_end: 100,
          row_count: 100,
          worker_index: 0,
          total_workers: 1,
        },
      ];

      const result = handler.batchSuccess('template', batchConfigs, { analyzed_at: '2025-01-01' });

      expect(result.result?.analyzed_at).toBe('2025-01-01');
      expect(result.metadata?.analyzed_at).toBe('2025-01-01');
    });
  });

  describe('noBatchesResult', () => {
    test('should return batch_processing_outcome with type no_batches', () => {
      const handler = new TestBatchableHandler();

      const result = handler.noBatchesResult();

      expect(result.success).toBe(true);
      const outcome = result.result?.batch_processing_outcome as Record<string, unknown>;
      expect(outcome.type).toBe('no_batches');
    });

    test('should include reason when provided', () => {
      const handler = new TestBatchableHandler();

      const result = handler.noBatchesResult('empty_dataset');

      expect(result.result?.reason).toBe('empty_dataset');
    });

    test('should not include reason when not provided', () => {
      const handler = new TestBatchableHandler();

      const result = handler.noBatchesResult();

      expect(result.result?.reason).toBeUndefined();
    });

    test('should include metadata when provided', () => {
      const handler = new TestBatchableHandler();

      const result = handler.noBatchesResult('no_data', { total_rows: 0 });

      expect(result.result?.total_rows).toBe(0);
      expect(result.metadata?.total_rows).toBe(0);
    });
  });

  describe('delegates to BatchableMixin', () => {
    test('createCursorConfigs delegates correctly', () => {
      const handler = new TestBatchableHandler();
      const configs = handler.createCursorConfigs(100, 2);

      expect(configs).toHaveLength(2);
      expect(configs[0].cursor_start).toBe(0);
      expect(configs[0].cursor_end).toBe(50);
    });

    test('getBatchWorkerInputs delegates correctly', () => {
      const handler = new TestBatchableHandler();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepInputs: { cursor: { batch_id: '001' }, is_no_op: false },
      });

      const inputs = handler.getBatchWorkerInputs(context);
      expect(inputs).not.toBeNull();
      expect(inputs?.is_no_op).toBe(false);
    });

    test('handleNoOpWorker delegates correctly', () => {
      const handler = new TestBatchableHandler();
      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
        stepInputs: { is_no_op: true, cursor: { batch_id: 'noop' } },
      });

      const result = handler.handleNoOpWorker(context);
      expect(result).not.toBeNull();
      expect(result?.result?.no_op).toBe(true);
    });
  });
});
