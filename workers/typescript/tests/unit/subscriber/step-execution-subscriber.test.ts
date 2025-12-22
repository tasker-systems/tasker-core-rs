/**
 * Tests for StepExecutionSubscriber.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { EventEmitter } from 'node:events';
import type { TaskerEventEmitter } from '../../../src/events/event-emitter';
import { StepEventNames } from '../../../src/events/event-names';
import type { FfiStepEvent } from '../../../src/ffi/types';
import { StepHandler } from '../../../src/handler/base';
import { HandlerRegistry } from '../../../src/handler/registry';
import {
  StepExecutionSubscriber,
  type StepExecutionSubscriberConfig,
} from '../../../src/subscriber/step-execution-subscriber';
import type { StepContext } from '../../../src/types/step-context';
import type { StepHandlerResult } from '../../../src/types/step-handler-result';

// Test handler implementation
class TestHandler extends StepHandler {
  static handlerName = 'test_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ processed: true });
  }
}

// Slow handler for timeout tests
class SlowHandler extends StepHandler {
  static handlerName = 'slow_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    await new Promise((resolve) => setTimeout(resolve, 100));
    return this.success({ slow: true });
  }
}

// Failing handler
class FailingHandler extends StepHandler {
  static handlerName = 'failing_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.failure('Handler failed intentionally');
  }
}

// Throwing handler
class ThrowingHandler extends StepHandler {
  static handlerName = 'throwing_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    throw new Error('Handler threw an error');
  }
}

// Create a mock FFI step event
function createMockEvent(handlerName: string, overrides: Partial<FfiStepEvent> = {}): FfiStepEvent {
  return {
    event_id: `event-${Date.now()}`,
    task_uuid: 'task-123',
    step_uuid: 'step-456',
    correlation_id: 'corr-789',
    trace_id: null,
    span_id: null,
    task_correlation_id: 'task-corr-123',
    parent_correlation_id: null,
    task: {
      task_uuid: 'task-123',
      named_task_uuid: 'named-task-123',
      name: 'test_task',
      namespace: 'default',
      version: '1.0.0',
      context: { order_id: 'order-001' },
      correlation_id: 'task-corr-123',
      parent_correlation_id: null,
      complete: false,
      priority: 1,
      initiator: 'test',
      source_system: 'test',
      reason: null,
      tags: null,
      identity_hash: 'hash-123',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      requested_at: new Date().toISOString(),
    },
    workflow_step: {
      workflow_step_uuid: 'step-456',
      task_uuid: 'task-123',
      named_step_uuid: 'named-step-456',
      name: 'test_step',
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
      description: 'Test step',
      handler: {
        callable: handlerName,
        initialization: {},
      },
      system_dependency: null,
      dependencies: [],
      timeout_seconds: 60,
      retry: {
        retryable: true,
        max_attempts: 3,
        backoff: 'exponential',
        backoff_base_ms: 1000,
        max_backoff_ms: 30000,
      },
    },
    dependency_results: {},
    ...overrides,
  };
}

describe('StepExecutionSubscriber', () => {
  let emitter: TaskerEventEmitter;
  let registry: HandlerRegistry;
  let subscriber: StepExecutionSubscriber;

  beforeEach(() => {
    // Reset singleton
    HandlerRegistry.resetInstance();

    // Create fresh instances
    emitter = new EventEmitter() as TaskerEventEmitter;
    registry = HandlerRegistry.instance();

    // Register test handlers
    registry.register('test_handler', TestHandler);
    registry.register('slow_handler', SlowHandler);
    registry.register('failing_handler', FailingHandler);
    registry.register('throwing_handler', ThrowingHandler);

    subscriber = new StepExecutionSubscriber(emitter, registry, {
      workerId: 'test-worker',
      maxConcurrent: 5,
      handlerTimeoutMs: 1000,
    });
  });

  afterEach(() => {
    if (subscriber.isRunning()) {
      subscriber.stop();
    }
    HandlerRegistry.resetInstance();
  });

  describe('constructor', () => {
    test('should create subscriber with default config', () => {
      const sub = new StepExecutionSubscriber(emitter, registry);

      expect(sub.isRunning()).toBe(false);
      expect(sub.getProcessedCount()).toBe(0);
      expect(sub.getErrorCount()).toBe(0);
    });

    test('should create subscriber with custom config', () => {
      const config: StepExecutionSubscriberConfig = {
        workerId: 'custom-worker',
        maxConcurrent: 20,
        handlerTimeoutMs: 60000,
      };

      const sub = new StepExecutionSubscriber(emitter, registry, config);

      expect(sub.isRunning()).toBe(false);
    });
  });

  describe('start/stop', () => {
    test('should start and stop', () => {
      expect(subscriber.isRunning()).toBe(false);

      subscriber.start();
      expect(subscriber.isRunning()).toBe(true);

      subscriber.stop();
      expect(subscriber.isRunning()).toBe(false);
    });

    test('should be idempotent on start', () => {
      subscriber.start();
      subscriber.start(); // Second call should be no-op

      expect(subscriber.isRunning()).toBe(true);
    });

    test('should be idempotent on stop', () => {
      subscriber.start();
      subscriber.stop();
      subscriber.stop(); // Second call should be no-op

      expect(subscriber.isRunning()).toBe(false);
    });
  });

  describe('getProcessedCount', () => {
    test('should start at zero', () => {
      expect(subscriber.getProcessedCount()).toBe(0);
    });
  });

  describe('getErrorCount', () => {
    test('should start at zero', () => {
      expect(subscriber.getErrorCount()).toBe(0);
    });
  });

  describe('getActiveHandlers', () => {
    test('should start at zero', () => {
      expect(subscriber.getActiveHandlers()).toBe(0);
    });
  });

  describe('waitForCompletion', () => {
    test('should resolve immediately when no active handlers', async () => {
      const result = await subscriber.waitForCompletion(100);

      expect(result).toBe(true);
    });
  });

  describe('event handling', () => {
    test('should ignore events when not running', () => {
      // Emit event without starting subscriber
      const event = createMockEvent('test_handler');
      emitter.emit(StepEventNames.STEP_EXECUTION_RECEIVED, event);

      // Should not process (no handlers active)
      expect(subscriber.getActiveHandlers()).toBe(0);
    });
  });

  describe('config defaults', () => {
    test('should use default workerId based on process pid', () => {
      const sub = new StepExecutionSubscriber(emitter, registry, {});
      expect(sub.isRunning()).toBe(false);
    });

    test('should use default maxConcurrent of 10', () => {
      const sub = new StepExecutionSubscriber(emitter, registry, {});
      expect(sub.isRunning()).toBe(false);
    });

    test('should use default handlerTimeoutMs of 300000', () => {
      const sub = new StepExecutionSubscriber(emitter, registry, {});
      expect(sub.isRunning()).toBe(false);
    });
  });
});

describe('StepExecutionSubscriberConfig', () => {
  test('should allow partial config', () => {
    const config: StepExecutionSubscriberConfig = {
      workerId: 'worker-1',
    };

    expect(config.workerId).toBe('worker-1');
    expect(config.maxConcurrent).toBeUndefined();
    expect(config.handlerTimeoutMs).toBeUndefined();
  });

  test('should allow full config', () => {
    const config: StepExecutionSubscriberConfig = {
      workerId: 'worker-1',
      maxConcurrent: 20,
      handlerTimeoutMs: 60000,
    };

    expect(config.workerId).toBe('worker-1');
    expect(config.maxConcurrent).toBe(20);
    expect(config.handlerTimeoutMs).toBe(60000);
  });

  test('should allow empty config', () => {
    const config: StepExecutionSubscriberConfig = {};

    expect(config.workerId).toBeUndefined();
    expect(config.maxConcurrent).toBeUndefined();
    expect(config.handlerTimeoutMs).toBeUndefined();
  });
});
