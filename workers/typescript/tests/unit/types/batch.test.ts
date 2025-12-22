import { describe, expect, test } from 'bun:test';
import { createBatchWorkerContext } from '../../../src/types/batch';
import type { CursorConfig, BatchAnalyzerOutcome, BatchWorkerOutcome } from '../../../src/types/batch';

describe('Batch Types', () => {
  describe('CursorConfig type', () => {
    test('should have correct structure', () => {
      const config: CursorConfig = {
        startCursor: 0,
        endCursor: 100,
        stepSize: 1,
        metadata: { source: 'test' },
      };

      expect(config.startCursor).toBe(0);
      expect(config.endCursor).toBe(100);
      expect(config.stepSize).toBe(1);
      expect(config.metadata).toEqual({ source: 'test' });
    });
  });

  describe('BatchAnalyzerOutcome type', () => {
    test('should have correct structure', () => {
      const outcome: BatchAnalyzerOutcome = {
        cursorConfigs: [
          { startCursor: 0, endCursor: 50, stepSize: 1, metadata: {} },
          { startCursor: 50, endCursor: 100, stepSize: 1, metadata: {} },
        ],
        totalItems: 100,
        batchMetadata: { source: 'products' },
      };

      expect(outcome.cursorConfigs).toHaveLength(2);
      expect(outcome.totalItems).toBe(100);
      expect(outcome.batchMetadata).toEqual({ source: 'products' });
    });

    test('should allow null totalItems', () => {
      const outcome: BatchAnalyzerOutcome = {
        cursorConfigs: [],
        totalItems: null,
        batchMetadata: {},
      };

      expect(outcome.totalItems).toBeNull();
    });
  });

  describe('BatchWorkerOutcome type', () => {
    test('should have correct structure', () => {
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

      expect(outcome.itemsProcessed).toBe(100);
      expect(outcome.itemsSucceeded).toBe(95);
      expect(outcome.itemsFailed).toBe(3);
      expect(outcome.itemsSkipped).toBe(2);
      expect(outcome.results).toHaveLength(2);
      expect(outcome.errors).toHaveLength(1);
      expect(outcome.lastCursor).toBe(99);
    });
  });
});

describe('createBatchWorkerContext', () => {
  test('should create context from flat format', () => {
    const data = {
      batch_id: 'batch-123',
      start_cursor: 0,
      end_cursor: 100,
      step_size: 1,
      batch_index: 0,
      total_batches: 4,
      batch_metadata: { source: 'test' },
    };

    const context = createBatchWorkerContext(data);

    expect(context.batchId).toBe('batch-123');
    expect(context.cursorConfig.startCursor).toBe(0);
    expect(context.cursorConfig.endCursor).toBe(100);
    expect(context.cursorConfig.stepSize).toBe(1);
    expect(context.batchIndex).toBe(0);
    expect(context.totalBatches).toBe(4);
    expect(context.batchMetadata).toEqual({ source: 'test' });
  });

  test('should create context from nested format', () => {
    const data = {
      batch_id: 'batch-456',
      cursor_config: {
        start_cursor: 50,
        end_cursor: 100,
        step_size: 2,
        metadata: { chunk: true },
      },
      batch_index: 1,
      total_batches: 2,
      batch_metadata: {},
    };

    const context = createBatchWorkerContext(data);

    expect(context.batchId).toBe('batch-456');
    expect(context.cursorConfig.startCursor).toBe(50);
    expect(context.cursorConfig.endCursor).toBe(100);
    expect(context.cursorConfig.stepSize).toBe(2);
    expect(context.cursorConfig.metadata).toEqual({ chunk: true });
  });

  test('should provide convenience accessors', () => {
    const data = {
      batch_id: 'batch-789',
      start_cursor: 100,
      end_cursor: 200,
      step_size: 5,
      batch_index: 1,
      total_batches: 2,
    };

    const context = createBatchWorkerContext(data);

    expect(context.startCursor).toBe(100);
    expect(context.endCursor).toBe(200);
    expect(context.stepSize).toBe(5);
  });

  test('should use defaults for missing fields', () => {
    const data = {
      batch_id: 'batch-minimal',
    };

    const context = createBatchWorkerContext(data);

    expect(context.batchId).toBe('batch-minimal');
    expect(context.cursorConfig.startCursor).toBe(0);
    expect(context.cursorConfig.endCursor).toBe(0);
    expect(context.cursorConfig.stepSize).toBe(1);
    expect(context.batchIndex).toBe(0);
    expect(context.totalBatches).toBe(1);
    expect(context.batchMetadata).toEqual({});
  });

  test('should handle empty data', () => {
    const context = createBatchWorkerContext({});

    expect(context.batchId).toBe('');
    expect(context.cursorConfig.startCursor).toBe(0);
    expect(context.totalBatches).toBe(1);
  });
});
