/**
 * Tests for batch factory functions and aggregateBatchResults.
 *
 * Covers: noBatches(), createBatches(), isNoBatches(), isCreateBatches(),
 * and aggregateBatchResults() from src/types/batch.ts.
 */

import { describe, expect, test } from 'bun:test';
import type { BatchProcessingOutcome, RustCursorConfig } from '../../../src/types/batch.js';
import {
  aggregateBatchResults,
  createBatches,
  isCreateBatches,
  isNoBatches,
  noBatches,
} from '../../../src/types/batch.js';

describe('noBatches', () => {
  test('should return an object with type no_batches', () => {
    const result = noBatches();

    expect(result).toEqual({ type: 'no_batches' });
  });

  test('should return a fresh object each call', () => {
    const a = noBatches();
    const b = noBatches();

    expect(a).toEqual(b);
    expect(a).not.toBe(b);
  });
});

describe('createBatches', () => {
  test('should create a CreateBatchesOutcome with correct fields', () => {
    const configs: RustCursorConfig[] = [
      { batch_id: '001', start_cursor: 0, end_cursor: 500, batch_size: 500 },
      { batch_id: '002', start_cursor: 500, end_cursor: 1000, batch_size: 500 },
    ];

    const result = createBatches('process_batch', 2, configs, 1000);

    expect(result.type).toBe('create_batches');
    expect(result.worker_template_name).toBe('process_batch');
    expect(result.worker_count).toBe(2);
    expect(result.cursor_configs).toHaveLength(2);
    expect(result.total_items).toBe(1000);
  });

  test('should throw when cursorConfigs length does not match workerCount', () => {
    const configs: RustCursorConfig[] = [
      { batch_id: '001', start_cursor: 0, end_cursor: 500, batch_size: 500 },
    ];

    expect(() => createBatches('process_batch', 3, configs, 1000)).toThrow(
      'cursor_configs length (1) must equal worker_count (3)'
    );
  });

  test('should accept zero workers with empty configs', () => {
    const result = createBatches('process_batch', 0, [], 0);

    expect(result.worker_count).toBe(0);
    expect(result.cursor_configs).toHaveLength(0);
    expect(result.total_items).toBe(0);
  });

  test('should preserve cursor config data', () => {
    const configs: RustCursorConfig[] = [
      { batch_id: 'b1', start_cursor: 'start_token', end_cursor: 'end_token', batch_size: 100 },
    ];

    const result = createBatches('template', 1, configs, 100);

    expect(result.cursor_configs[0].start_cursor).toBe('start_token');
    expect(result.cursor_configs[0].end_cursor).toBe('end_token');
  });
});

describe('isNoBatches', () => {
  test('should return true for NoBatchesOutcome', () => {
    const outcome = noBatches();
    expect(isNoBatches(outcome)).toBe(true);
  });

  test('should return false for CreateBatchesOutcome', () => {
    const configs: RustCursorConfig[] = [
      { batch_id: '001', start_cursor: 0, end_cursor: 100, batch_size: 100 },
    ];
    const outcome = createBatches('template', 1, configs, 100);
    expect(isNoBatches(outcome)).toBe(false);
  });
});

describe('isCreateBatches', () => {
  test('should return true for CreateBatchesOutcome', () => {
    const configs: RustCursorConfig[] = [
      { batch_id: '001', start_cursor: 0, end_cursor: 100, batch_size: 100 },
    ];
    const outcome = createBatches('template', 1, configs, 100);
    expect(isCreateBatches(outcome)).toBe(true);
  });

  test('should return false for NoBatchesOutcome', () => {
    const outcome = noBatches();
    expect(isCreateBatches(outcome)).toBe(false);
  });

  test('should narrow type correctly', () => {
    const outcome: BatchProcessingOutcome = createBatches(
      'template',
      1,
      [{ batch_id: '001', start_cursor: 0, end_cursor: 50, batch_size: 50 }],
      50
    );

    if (isCreateBatches(outcome)) {
      expect(outcome.worker_template_name).toBe('template');
      expect(outcome.worker_count).toBe(1);
    } else {
      throw new Error('Expected CreateBatchesOutcome');
    }
  });
});

describe('aggregateBatchResults', () => {
  test('should return zeroes for empty array', () => {
    const result = aggregateBatchResults([]);

    expect(result.total_processed).toBe(0);
    expect(result.total_succeeded).toBe(0);
    expect(result.total_failed).toBe(0);
    expect(result.total_skipped).toBe(0);
    expect(result.batch_count).toBe(0);
    expect(result.success_rate).toBe(0);
    expect(result.errors).toEqual([]);
    expect(result.error_count).toBe(0);
  });

  test('should skip null and undefined entries', () => {
    const result = aggregateBatchResults([null, undefined, null]);

    expect(result.batch_count).toBe(0);
    expect(result.total_processed).toBe(0);
  });

  test('should aggregate multiple worker results', () => {
    const results = [
      { items_processed: 100, items_succeeded: 90, items_failed: 8, items_skipped: 2, errors: [] },
      {
        items_processed: 50,
        items_succeeded: 45,
        items_failed: 3,
        items_skipped: 2,
        errors: [{ id: 1, msg: 'fail' }],
      },
    ];

    const result = aggregateBatchResults(results);

    expect(result.total_processed).toBe(150);
    expect(result.total_succeeded).toBe(135);
    expect(result.total_failed).toBe(11);
    expect(result.total_skipped).toBe(4);
    expect(result.batch_count).toBe(2);
    expect(result.error_count).toBe(1);
    expect(result.errors).toHaveLength(1);
  });

  test('should calculate success rate', () => {
    const results = [{ items_processed: 100, items_succeeded: 80 }];

    const result = aggregateBatchResults(results);

    expect(result.success_rate).toBe(0.8);
  });

  test('should return success_rate 0 when totalProcessed is 0', () => {
    const results = [{ items_processed: 0, items_succeeded: 0 }];

    const result = aggregateBatchResults(results);

    expect(result.success_rate).toBe(0);
  });

  test('should collect errors from worker results', () => {
    const results = [
      { items_processed: 10, errors: [{ id: 1 }, { id: 2 }] },
      { items_processed: 10, errors: [{ id: 3 }] },
    ];

    const result = aggregateBatchResults(results);

    expect(result.errors).toHaveLength(3);
    expect(result.error_count).toBe(3);
  });

  test('should truncate errors at maxErrors', () => {
    const manyErrors = Array.from({ length: 50 }, (_, i) => ({ id: i }));
    const results = [{ items_processed: 50, errors: manyErrors }];

    const result = aggregateBatchResults(results, 10);

    expect(result.errors).toHaveLength(10);
    expect(result.error_count).toBe(50);
  });

  test('should handle results without errors field', () => {
    const results = [{ items_processed: 100, items_succeeded: 100 }];

    const result = aggregateBatchResults(results);

    expect(result.errors).toEqual([]);
    expect(result.error_count).toBe(0);
  });

  test('should handle mixed null and valid entries', () => {
    const results = [
      null,
      { items_processed: 50, items_succeeded: 50 },
      undefined,
      { items_processed: 30, items_succeeded: 25, items_failed: 5 },
      null,
    ];

    const result = aggregateBatchResults(results);

    expect(result.batch_count).toBe(2);
    expect(result.total_processed).toBe(80);
    expect(result.total_succeeded).toBe(75);
    expect(result.total_failed).toBe(5);
  });

  test('should default missing numeric fields to 0', () => {
    const results = [{ some_other_field: 'value' }];

    const result = aggregateBatchResults(results);

    expect(result.batch_count).toBe(1);
    expect(result.total_processed).toBe(0);
    expect(result.total_succeeded).toBe(0);
    expect(result.total_failed).toBe(0);
    expect(result.total_skipped).toBe(0);
  });
});
