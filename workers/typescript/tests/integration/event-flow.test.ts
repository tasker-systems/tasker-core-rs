/**
 * Integration tests for the complete event flow.
 *
 * These tests verify that the EventPoller → EventEmitter → StepExecutionSubscriber
 * flow works correctly using real (non-mock) components.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { TaskerEventEmitter } from '../../src/events/event-emitter';
import { StepEventNames } from '../../src/events/event-names';
import type { TaskerRuntime } from '../../src/ffi/runtime-interface';
import type { FfiStepEvent } from '../../src/ffi/types';
import { StepHandler } from '../../src/handler/base';
import { HandlerRegistry } from '../../src/handler/registry';
import { StepExecutionSubscriber } from '../../src/subscriber/step-execution-subscriber';
import type { StepContext } from '../../src/types/step-context';
import type { StepHandlerResult } from '../../src/types/step-handler-result';

/**
 * Create a mock TaskerRuntime for testing.
 *
 * The mock runtime simulates a loaded FFI layer that accepts result submissions
 * but doesn't actually call any FFI functions.
 */
function createMockRuntime(): TaskerRuntime {
  return {
    name: 'mock-runtime',
    isLoaded: true,
    load: async () => {},
    unload: () => {},
    getVersion: () => '0.0.0-mock',
    getRustVersion: () => '0.0.0-mock',
    healthCheck: () => true,
    bootstrapWorker: () => ({ success: true, status: 'running', message: 'Mock bootstrap' }),
    isWorkerRunning: () => true,
    getWorkerStatus: () => ({ running: true, state: 'running' }),
    stopWorker: () => ({ success: true, message: 'stopped' }),
    transitionToGracefulShutdown: () => ({ success: true, message: 'shutting down' }),
    pollStepEvents: () => null,
    completeStepEvent: () => true, // Accept result submissions
    getFfiDispatchMetrics: () => ({
      pending_events: 0,
      completed_events: 0,
      failed_events: 0,
      starvation_detected: false,
    }),
    checkStarvationWarnings: () => {},
    cleanupTimeouts: () => {},
    logError: () => {},
    logWarn: () => {},
    logInfo: () => {},
    logDebug: () => {},
    logTrace: () => {},
  };
}

// Create a mock FFI step event
function createMockEvent(handlerName: string): FfiStepEvent {
  return {
    event_id: `event-${Date.now()}-${Math.random()}`,
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
  };
}

describe('Event Flow Integration', () => {
  let emitter: TaskerEventEmitter;
  let registry: HandlerRegistry;
  let mockRuntime: TaskerRuntime;
  let subscriber: StepExecutionSubscriber;

  beforeEach(() => {
    // Create fresh instances for each test (explicit construction)
    emitter = new TaskerEventEmitter();
    registry = new HandlerRegistry();
    mockRuntime = createMockRuntime();
  });

  afterEach(() => {
    if (subscriber?.isRunning()) {
      subscriber.stop();
    }
  });

  test('TaskerEventEmitter instances have unique IDs', () => {
    const emitter1 = new TaskerEventEmitter();
    const emitter2 = new TaskerEventEmitter();

    expect(emitter1.getInstanceId()).not.toBe(emitter2.getInstanceId());
  });

  test('emitStepReceived should emit correct payload format', () => {
    let receivedPayload: unknown = null;

    emitter.on(StepEventNames.STEP_EXECUTION_RECEIVED, (payload) => {
      receivedPayload = payload;
    });

    const event = createMockEvent('test_handler');
    emitter.emitStepReceived(event);

    expect(receivedPayload).not.toBeNull();
    expect((receivedPayload as { event: FfiStepEvent }).event).toBe(event);
    expect((receivedPayload as { receivedAt: Date }).receivedAt).toBeInstanceOf(Date);
  });

  test('StepExecutionSubscriber should receive events via emitStepReceived', async () => {
    let handlerCalled = false;
    let receivedContext: StepContext | null = null;

    // Create tracking handler
    class IntegrationTestHandler extends StepHandler {
      static handlerName = 'integration_test_handler';

      async call(context: StepContext): Promise<StepHandlerResult> {
        handlerCalled = true;
        receivedContext = context;
        return this.success({ integration_test: true });
      }
    }

    registry.register('integration_test_handler', IntegrationTestHandler);

    // Create subscriber with the SAME emitter and mock runtime (explicit injection)
    subscriber = new StepExecutionSubscriber(emitter, registry, mockRuntime, {
      workerId: 'integration-test-worker',
    });
    subscriber.start();

    // Use the emitter's helper method (same as EventPoller does)
    const event = createMockEvent('integration_test_handler');
    emitter.emitStepReceived(event);

    // Wait for async processing
    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(handlerCalled).toBe(true);
    expect(receivedContext).not.toBeNull();
    expect(subscriber.getProcessedCount()).toBe(1);
  });

  test('complete flow: emitter → subscriber → handler with correct context', async () => {
    let capturedStepUuid: string | null = null;
    let capturedTaskUuid: string | null = null;
    let capturedHandlerName: string | null = null;

    class ContextCapturingHandler extends StepHandler {
      static handlerName = 'context_capturing_handler';

      async call(context: StepContext): Promise<StepHandlerResult> {
        capturedStepUuid = context.stepUuid;
        capturedTaskUuid = context.taskUuid;
        capturedHandlerName = context.handlerName;
        return this.success({ captured: true });
      }
    }

    registry.register('context_capturing_handler', ContextCapturingHandler);

    subscriber = new StepExecutionSubscriber(emitter, registry, mockRuntime, {
      workerId: 'context-test-worker',
    });
    subscriber.start();

    const event = createMockEvent('context_capturing_handler');
    event.step_uuid = 'test-step-uuid-12345';
    event.task_uuid = 'test-task-uuid-67890';

    emitter.emitStepReceived(event);

    await new Promise((resolve) => setTimeout(resolve, 100));

    expect(capturedStepUuid).toBe('test-step-uuid-12345');
    expect(capturedTaskUuid).toBe('test-task-uuid-67890');
    expect(capturedHandlerName).toBe('context_capturing_handler');
  });

  test('multiple events should all be processed', async () => {
    let processCount = 0;

    class CountingHandler extends StepHandler {
      static handlerName = 'counting_handler';

      async call(_context: StepContext): Promise<StepHandlerResult> {
        processCount++;
        return this.success({ count: processCount });
      }
    }

    registry.register('counting_handler', CountingHandler);

    subscriber = new StepExecutionSubscriber(emitter, registry, mockRuntime, {
      workerId: 'counting-test-worker',
    });
    subscriber.start();

    // Emit 5 events
    for (let i = 0; i < 5; i++) {
      const event = createMockEvent('counting_handler');
      emitter.emitStepReceived(event);
    }

    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(processCount).toBe(5);
    expect(subscriber.getProcessedCount()).toBe(5);
  });
});
