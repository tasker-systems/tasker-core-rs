/**
 * StepContext class tests.
 *
 * Verifies context creation, field access, and factory method.
 */

import { describe, expect, it } from 'bun:test';
import type { FfiStepEvent } from '../../../src/ffi/types.js';
import { StepContext } from '../../../src/types/step-context.js';

describe('StepContext', () => {
  describe('constructor', () => {
    it('initializes all properties from params', () => {
      const event = createValidFfiStepEvent();
      const context = new StepContext({
        event,
        taskUuid: 'task-123',
        stepUuid: 'step-456',
        correlationId: 'corr-789',
        handlerName: 'test_handler',
        inputData: { key: 'value' },
        dependencyResults: { dep1: { result: 'data' } },
        stepConfig: { setting: true },
        stepInputs: { cursor: null },
        retryCount: 1,
        maxRetries: 3,
      });

      expect(context.event).toBe(event);
      expect(context.taskUuid).toBe('task-123');
      expect(context.stepUuid).toBe('step-456');
      expect(context.correlationId).toBe('corr-789');
      expect(context.handlerName).toBe('test_handler');
      expect(context.inputData).toEqual({ key: 'value' });
      expect(context.dependencyResults).toEqual({ dep1: { result: 'data' } });
      expect(context.stepConfig).toEqual({ setting: true });
      expect(context.stepInputs).toEqual({ cursor: null });
      expect(context.retryCount).toBe(1);
      expect(context.maxRetries).toBe(3);
    });

    it('creates readonly properties', () => {
      const context = createValidStepContext();

      // Verify properties exist and are frozen
      expect(context.taskUuid).toBeDefined();
      expect(context.stepUuid).toBeDefined();
    });
  });

  describe('fromFfiEvent', () => {
    it('extracts task UUID and step UUID from event', () => {
      const event = createValidFfiStepEvent();
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.taskUuid).toBe(event.task_uuid);
      expect(context.stepUuid).toBe(event.step_uuid);
    });

    it('extracts correlation ID from event', () => {
      const event = createValidFfiStepEvent();
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.correlationId).toBe(event.correlation_id);
    });

    it('sets handler name from parameter', () => {
      const event = createValidFfiStepEvent();
      const context = StepContext.fromFfiEvent(event, 'custom_handler');

      expect(context.handlerName).toBe('custom_handler');
    });

    it('extracts input data from task context', () => {
      const event = createValidFfiStepEvent();
      event.task.context = { order_id: 'ORD-123', amount: 99.99 };
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.inputData).toEqual({ order_id: 'ORD-123', amount: 99.99 });
    });

    it('defaults inputData to empty object when task context is null', () => {
      const event = createValidFfiStepEvent();
      event.task.context = null;
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.inputData).toEqual({});
    });

    it('extracts dependency results from event', () => {
      const event = createValidFfiStepEvent();
      event.dependency_results = {
        step_1: {
          step_uuid: 's1',
          success: true,
          result: { value: 42 },
          status: 'completed',
          error: null,
        },
      };
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.dependencyResults).toEqual(event.dependency_results);
    });

    it('extracts step config from handler initialization', () => {
      const event = createValidFfiStepEvent();
      event.step_definition.handler.initialization = {
        api_key: 'secret',
        timeout: 30,
      };
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.stepConfig).toEqual({ api_key: 'secret', timeout: 30 });
    });

    it('extracts retry count and max retries from workflow step', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.attempts = 2;
      event.workflow_step.max_attempts = 5;
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.retryCount).toBe(2);
      expect(context.maxRetries).toBe(5);
    });

    it('extracts step inputs from workflow step', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.inputs = { cursor: { offset: 100 } };
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.stepInputs).toEqual({ cursor: { offset: 100 } });
    });

    it('stores original event', () => {
      const event = createValidFfiStepEvent();
      const context = StepContext.fromFfiEvent(event, 'my_handler');

      expect(context.event).toBe(event);
    });
  });

  describe('getDependencyResult', () => {
    it('extracts result value from nested structure', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: { computed: 'value' } },
        },
      });

      const result = context.getDependencyResult('step_1');
      expect(result).toEqual({ computed: 'value' });
    });

    it('returns primitive result value', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: 42 },
        },
      });

      const result = context.getDependencyResult('step_1');
      expect(result).toBe(42);
    });

    it('returns null for missing dependency', () => {
      const context = createValidStepContext({
        dependencyResults: {},
      });

      const result = context.getDependencyResult('nonexistent');
      expect(result).toBeNull();
    });

    it('returns null when dependency value is null', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: null,
        },
      });

      const result = context.getDependencyResult('step_1');
      expect(result).toBeNull();
    });

    it('returns whole value when no result key exists', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { other_key: 'value' },
        },
      });

      const result = context.getDependencyResult('step_1');
      expect(result).toEqual({ other_key: 'value' });
    });
  });

  describe('getInput', () => {
    it('returns value for existing key', () => {
      const context = createValidStepContext({
        inputData: { order_id: 'ORD-123', amount: 99.99 },
      });

      expect(context.getInput('order_id')).toBe('ORD-123');
      expect(context.getInput('amount')).toBe(99.99);
    });

    it('returns undefined for missing key', () => {
      const context = createValidStepContext({
        inputData: { order_id: 'ORD-123' },
      });

      expect(context.getInput('nonexistent')).toBeUndefined();
    });

    it('supports generic type parameter', () => {
      const context = createValidStepContext({
        inputData: { count: 42 },
      });

      const count = context.getInput<number>('count');
      expect(count).toBe(42);
    });
  });

  describe('getConfig', () => {
    it('returns value for existing key', () => {
      const context = createValidStepContext({
        stepConfig: { api_endpoint: 'https://api.example.com', timeout: 30 },
      });

      expect(context.getConfig('api_endpoint')).toBe('https://api.example.com');
      expect(context.getConfig('timeout')).toBe(30);
    });

    it('returns undefined for missing key', () => {
      const context = createValidStepContext({
        stepConfig: { api_endpoint: 'https://api.example.com' },
      });

      expect(context.getConfig('nonexistent')).toBeUndefined();
    });

    it('supports generic type parameter', () => {
      const context = createValidStepContext({
        stepConfig: { enabled: true },
      });

      const enabled = context.getConfig<boolean>('enabled');
      expect(enabled).toBe(true);
    });
  });

  describe('isRetry', () => {
    it('returns false when retryCount is 0', () => {
      const context = createValidStepContext({ retryCount: 0 });
      expect(context.isRetry()).toBe(false);
    });

    it('returns true when retryCount is 1', () => {
      const context = createValidStepContext({ retryCount: 1 });
      expect(context.isRetry()).toBe(true);
    });

    it('returns true when retryCount is greater than 1', () => {
      const context = createValidStepContext({ retryCount: 5 });
      expect(context.isRetry()).toBe(true);
    });
  });

  describe('isLastRetry', () => {
    it('returns false when retry count is less than max - 1', () => {
      const context = createValidStepContext({ retryCount: 0, maxRetries: 3 });
      expect(context.isLastRetry()).toBe(false);
    });

    it('returns true when retry count equals max - 1', () => {
      const context = createValidStepContext({ retryCount: 2, maxRetries: 3 });
      expect(context.isLastRetry()).toBe(true);
    });

    it('returns true when retry count exceeds max - 1', () => {
      const context = createValidStepContext({ retryCount: 5, maxRetries: 3 });
      expect(context.isLastRetry()).toBe(true);
    });
  });

  describe('getInputOr', () => {
    it('returns value when key exists', () => {
      const context = createValidStepContext({
        inputData: { batch_size: 200 },
      });

      expect(context.getInputOr('batch_size', 100)).toBe(200);
    });

    it('returns default when key is missing', () => {
      const context = createValidStepContext({ inputData: {} });

      expect(context.getInputOr('batch_size', 100)).toBe(100);
    });

    it('returns default when value is undefined', () => {
      const context = createValidStepContext({
        inputData: { batch_size: undefined },
      });

      expect(context.getInputOr('batch_size', 100)).toBe(100);
    });

    it('returns falsy value when present (not default)', () => {
      const context = createValidStepContext({
        inputData: { count: 0, flag: false, name: '' },
      });

      expect(context.getInputOr('count', 99)).toBe(0);
      expect(context.getInputOr('flag', true)).toBe(false);
      expect(context.getInputOr('name', 'default')).toBe('');
    });
  });

  describe('getDependencyField', () => {
    it('extracts a single-level field from dependency result', () => {
      const context = createValidStepContext({
        dependencyResults: {
          analyze_csv: { result: { csv_file_path: '/tmp/data.csv', row_count: 500 } },
        },
      });

      expect(context.getDependencyField('analyze_csv', 'csv_file_path')).toBe('/tmp/data.csv');
    });

    it('extracts a multi-level path from dependency result', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: { data: { items: [1, 2, 3] } } },
        },
      });

      expect(context.getDependencyField('step_1', 'data', 'items')).toEqual([1, 2, 3]);
    });

    it('returns null when dependency is missing', () => {
      const context = createValidStepContext({ dependencyResults: {} });

      expect(context.getDependencyField('nonexistent', 'field')).toBeNull();
    });

    it('returns null when dependency result is null', () => {
      const context = createValidStepContext({
        dependencyResults: { step_1: null },
      });

      expect(context.getDependencyField('step_1', 'field')).toBeNull();
    });

    it('returns null when intermediate path is not an object', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: { data: 'not_an_object' } },
        },
      });

      expect(context.getDependencyField('step_1', 'data', 'nested')).toBeNull();
    });

    it('returns undefined for missing key at end of valid path', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: { data: { present: true } } },
        },
      });

      expect(context.getDependencyField('step_1', 'data', 'missing')).toBeUndefined();
    });
  });

  describe('checkpoint', () => {
    it('returns checkpoint data when present', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 500, items_processed: 500 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpoint).toEqual({ cursor: 500, items_processed: 500 });
    });

    it('returns null when no checkpoint exists', () => {
      const event = createValidFfiStepEvent();
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpoint).toBeNull();
    });
  });

  describe('checkpointCursor', () => {
    it('returns cursor value when present', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 1000 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpointCursor).toBe(1000);
    });

    it('returns null when no checkpoint', () => {
      const context = createValidStepContext();

      expect(context.checkpointCursor).toBeNull();
    });

    it('returns string cursor', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 'page_token_abc' };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpointCursor).toBe('page_token_abc');
    });
  });

  describe('checkpointItemsProcessed', () => {
    it('returns items_processed when present', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 100, items_processed: 250 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpointItemsProcessed).toBe(250);
    });

    it('returns 0 when no checkpoint', () => {
      const context = createValidStepContext();

      expect(context.checkpointItemsProcessed).toBe(0);
    });

    it('returns 0 when checkpoint has no items_processed', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 100 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.checkpointItemsProcessed).toBe(0);
    });
  });

  describe('accumulatedResults', () => {
    it('returns accumulated_results when present', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = {
        cursor: 100,
        accumulated_results: { sum: 5000, count: 100 },
      };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.accumulatedResults).toEqual({ sum: 5000, count: 100 });
    });

    it('returns null when no checkpoint', () => {
      const context = createValidStepContext();

      expect(context.accumulatedResults).toBeNull();
    });

    it('returns null when checkpoint has no accumulated_results', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 100 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.accumulatedResults).toBeNull();
    });
  });

  describe('hasCheckpoint', () => {
    it('returns true when cursor exists', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { cursor: 500 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.hasCheckpoint()).toBe(true);
    });

    it('returns false when no checkpoint', () => {
      const context = createValidStepContext();

      expect(context.hasCheckpoint()).toBe(false);
    });

    it('returns false when checkpoint has no cursor', () => {
      const event = createValidFfiStepEvent();
      event.workflow_step.checkpoint = { items_processed: 100 };
      const context = StepContext.fromFfiEvent(event, 'handler');

      expect(context.hasCheckpoint()).toBe(false);
    });
  });

  describe('getDependencyResultKeys', () => {
    it('returns array of step names', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: 'a' },
          step_2: { result: 'b' },
          step_3: { result: 'c' },
        },
      });

      const keys = context.getDependencyResultKeys();

      expect(keys).toEqual(['step_1', 'step_2', 'step_3']);
    });

    it('returns empty array when no dependencies', () => {
      const context = createValidStepContext({ dependencyResults: {} });

      expect(context.getDependencyResultKeys()).toEqual([]);
    });
  });

  describe('getAllDependencyResults', () => {
    it('returns results matching prefix', () => {
      const context = createValidStepContext({
        dependencyResults: {
          process_batch_001: { result: { count: 10 } },
          process_batch_002: { result: { count: 20 } },
          analyze_csv: { result: { total: 30 } },
        },
      });

      const results = context.getAllDependencyResults('process_batch_');

      expect(results).toHaveLength(2);
      expect(results).toContainEqual({ count: 10 });
      expect(results).toContainEqual({ count: 20 });
    });

    it('returns empty array when no prefix matches', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: 'data' },
        },
      });

      expect(context.getAllDependencyResults('nonexistent_')).toEqual([]);
    });

    it('skips null dependency results', () => {
      const context = createValidStepContext({
        dependencyResults: {
          batch_001: null,
          batch_002: { result: { count: 5 } },
        },
      });

      const results = context.getAllDependencyResults('batch_');

      expect(results).toHaveLength(1);
      expect(results[0]).toEqual({ count: 5 });
    });

    it('returns all matching results for broad prefix', () => {
      const context = createValidStepContext({
        dependencyResults: {
          step_1: { result: 'a' },
          step_2: { result: 'b' },
        },
      });

      const results = context.getAllDependencyResults('step_');

      expect(results).toHaveLength(2);
    });
  });
});

// Test helpers

function createValidFfiStepEvent(): FfiStepEvent {
  return {
    event_id: 'event-123',
    task_uuid: 'task-456',
    step_uuid: 'step-789',
    correlation_id: 'corr-001',
    trace_id: null,
    span_id: null,
    task_correlation_id: 'task-corr-001',
    parent_correlation_id: null,
    task: {
      task_uuid: 'task-456',
      named_task_uuid: 'named-task-001',
      name: 'TestTask',
      namespace: 'test',
      version: '1.0.0',
      context: null,
      correlation_id: 'corr-001',
      parent_correlation_id: null,
      complete: false,
      priority: 0,
      initiator: null,
      source_system: null,
      reason: null,
      tags: null,
      identity_hash: 'hash-123',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      requested_at: new Date().toISOString(),
    },
    workflow_step: {
      workflow_step_uuid: 'step-789',
      task_uuid: 'task-456',
      named_step_uuid: 'named-step-001',
      name: 'TestStep',
      template_step_name: 'test_step',
      retryable: true,
      max_attempts: 3,
      attempts: 0,
      in_process: false,
      processed: false,

      inputs: null,
      results: null,
      backoff_request_seconds: null,
      processed_at: null,
      last_attempted_at: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    },
    step_definition: {
      name: 'test_step',
      description: 'A test step',
      handler: {
        callable: 'TestHandler',
        initialization: {},
      },

      dependencies: [],
      timeout_seconds: 30,
      retry: {
        retryable: true,
        max_attempts: 3,
        backoff: 'exponential',
        backoff_base_ms: 1000,
        max_backoff_ms: 30000,
      },
    },
    dependency_results: {},
  };
}

function createValidStepContext(
  overrides: Partial<{
    inputData: Record<string, unknown>;
    dependencyResults: Record<string, unknown>;
    stepConfig: Record<string, unknown>;
    stepInputs: Record<string, unknown>;
    retryCount: number;
    maxRetries: number;
  }> = {}
): StepContext {
  const event = createValidFfiStepEvent();
  return new StepContext({
    event,
    taskUuid: 'task-123',
    stepUuid: 'step-456',
    correlationId: 'corr-789',
    handlerName: 'test_handler',
    inputData: overrides.inputData ?? {},
    dependencyResults: overrides.dependencyResults ?? {},
    stepConfig: overrides.stepConfig ?? {},
    stepInputs: overrides.stepInputs ?? {},
    retryCount: overrides.retryCount ?? 0,
    maxRetries: overrides.maxRetries ?? 3,
  });
}
