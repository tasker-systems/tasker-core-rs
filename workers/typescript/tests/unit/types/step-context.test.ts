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
      event.step_definition.handler.initialization = { api_key: 'secret', timeout: 30 };
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
      skippable: false,
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
      system_dependency: null,
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
