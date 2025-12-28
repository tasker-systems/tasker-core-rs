/**
 * Event emitter coherence tests.
 *
 * Verifies that the TaskerEventEmitter works correctly and emits
 * events with proper payloads.
 */

import { beforeEach, describe, expect, it, mock } from 'bun:test';
import {
  type MetricsPayload,
  type StepExecutionCompletedPayload,
  type StepExecutionReceivedPayload,
  type StepExecutionStartedPayload,
  TaskerEventEmitter,
  type WorkerEventPayload,
} from '../../../src/events/event-emitter.js';
import { EventNames } from '../../../src/events/event-names.js';
import type { FfiStepEvent, StepExecutionResult } from '../../../src/ffi/types.js';

describe('TaskerEventEmitter', () => {
  let emitter: TaskerEventEmitter;

  beforeEach(() => {
    emitter = new TaskerEventEmitter();
  });

  describe('construction', () => {
    it('creates a new emitter instance', () => {
      expect(emitter).toBeInstanceOf(TaskerEventEmitter);
    });

    it('generates a unique instance ID', () => {
      const emitter2 = new TaskerEventEmitter();
      expect(emitter.getInstanceId()).not.toBe(emitter2.getInstanceId());
    });

    it('instance ID is a valid UUID format', () => {
      const id = emitter.getInstanceId();
      // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
      expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
    });
  });

  describe('event subscription', () => {
    it('allows subscribing to events', () => {
      const handler = mock(() => {});
      emitter.on('worker.started', handler);
      expect(emitter.listenerCount('worker.started')).toBe(1);
    });

    it('allows multiple subscribers to same event', () => {
      const handler1 = mock(() => {});
      const handler2 = mock(() => {});
      emitter.on('worker.started', handler1);
      emitter.on('worker.started', handler2);
      expect(emitter.listenerCount('worker.started')).toBe(2);
    });

    it('allows unsubscribing from events', () => {
      const handler = mock(() => {});
      emitter.on('worker.started', handler);
      emitter.off('worker.started', handler);
      expect(emitter.listenerCount('worker.started')).toBe(0);
    });

    it('supports once() for single-fire subscriptions', () => {
      const handler = mock(() => {});
      emitter.once('worker.started', handler);

      emitter.emit('worker.started', { timestamp: new Date() });
      emitter.emit('worker.started', { timestamp: new Date() });

      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  describe('step event emission helpers', () => {
    it('emitStepReceived emits with correct payload', () => {
      const handler = mock((_payload: StepExecutionReceivedPayload) => {});
      emitter.on('step.execution.received', handler);

      const mockEvent = createMockFfiStepEvent();
      emitter.emitStepReceived(mockEvent);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.event).toBe(mockEvent);
      expect(payload.receivedAt).toBeInstanceOf(Date);
    });

    it('emitStepStarted emits with correct payload', () => {
      const handler = mock((_payload: StepExecutionStartedPayload) => {});
      emitter.on('step.execution.started', handler);

      emitter.emitStepStarted('event-1', 'step-uuid', 'task-uuid', 'TestHandler');

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.eventId).toBe('event-1');
      expect(payload.stepUuid).toBe('step-uuid');
      expect(payload.taskUuid).toBe('task-uuid');
      expect(payload.handlerName).toBe('TestHandler');
      expect(payload.startedAt).toBeInstanceOf(Date);
    });

    it('emitStepCompleted emits with correct payload', () => {
      const handler = mock((_payload: StepExecutionCompletedPayload) => {});
      emitter.on('step.execution.completed', handler);

      const mockResult = createMockStepExecutionResult();
      emitter.emitStepCompleted('event-1', 'step-uuid', 'task-uuid', mockResult, 150);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.eventId).toBe('event-1');
      expect(payload.stepUuid).toBe('step-uuid');
      expect(payload.taskUuid).toBe('task-uuid');
      expect(payload.result).toBe(mockResult);
      expect(payload.executionTimeMs).toBe(150);
      expect(payload.completedAt).toBeInstanceOf(Date);
    });

    it('emitStepFailed emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('step.execution.failed', handler);

      const error = new Error('Test failure');
      emitter.emitStepFailed('event-1', 'step-uuid', 'task-uuid', error);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.eventId).toBe('event-1');
      expect(payload.error).toBe(error);
      expect(payload.failedAt).toBeInstanceOf(Date);
    });

    it('emitCompletionSent emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('step.completion.sent', handler);

      emitter.emitCompletionSent('event-1', 'step-uuid', true);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.eventId).toBe('event-1');
      expect(payload.stepUuid).toBe('step-uuid');
      expect(payload.success).toBe(true);
      expect(payload.sentAt).toBeInstanceOf(Date);
    });
  });

  describe('worker event emission helpers', () => {
    it('emitWorkerStarted emits with correct payload', () => {
      const handler = mock((_payload: WorkerEventPayload) => {});
      emitter.on('worker.started', handler);

      emitter.emitWorkerStarted('worker-123');

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.workerId).toBe('worker-123');
      expect(payload.timestamp).toBeInstanceOf(Date);
    });

    it('emitWorkerReady emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('worker.ready', handler);

      emitter.emitWorkerReady('worker-123');

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('emitWorkerShutdownStarted emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('worker.shutdown.started', handler);

      emitter.emitWorkerShutdownStarted('worker-123');

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('emitWorkerStopped emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('worker.stopped', handler);

      emitter.emitWorkerStopped('worker-123');

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('emitWorkerError emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('worker.error', handler);

      const error = new Error('Worker error');
      emitter.emitWorkerError(error, { context: 'test' });

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.error).toBe(error);
      expect(payload.context).toEqual({ context: 'test' });
    });
  });

  describe('metrics event emission helpers', () => {
    it('emitMetricsUpdated emits with correct payload', () => {
      const handler = mock((_payload: MetricsPayload) => {});
      emitter.on('metrics.updated', handler);

      const metrics = {
        pending_count: 5,
        starvation_detected: false,
        starving_event_count: 0,
        oldest_pending_age_ms: 100,
        newest_pending_age_ms: 10,
      };
      emitter.emitMetricsUpdated(metrics);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.metrics).toBe(metrics);
      expect(payload.timestamp).toBeInstanceOf(Date);
    });

    it('emitStarvationDetected emits with correct payload', () => {
      const handler = mock(() => {});
      emitter.on('poller.starvation.detected', handler);

      const metrics = {
        pending_count: 10,
        starvation_detected: true,
        starving_event_count: 3,
        oldest_pending_age_ms: 5000,
        newest_pending_age_ms: 100,
      };
      emitter.emitStarvationDetected(metrics);

      expect(handler).toHaveBeenCalledTimes(1);
      const payload = handler.mock.calls[0][0];
      expect(payload.metrics.starvation_detected).toBe(true);
    });
  });
});

describe('EventNames', () => {
  it('exports all expected step event names', () => {
    expect(EventNames.STEP_EXECUTION_RECEIVED).toBe('step.execution.received');
    expect(EventNames.STEP_EXECUTION_STARTED).toBe('step.execution.started');
    expect(EventNames.STEP_EXECUTION_COMPLETED).toBe('step.execution.completed');
    expect(EventNames.STEP_EXECUTION_FAILED).toBe('step.execution.failed');
    expect(EventNames.STEP_COMPLETION_SENT).toBe('step.completion.sent');
    expect(EventNames.STEP_EXECUTION_TIMEOUT).toBe('step.execution.timeout');
  });

  it('exports all expected worker event names', () => {
    expect(EventNames.WORKER_STARTED).toBe('worker.started');
    expect(EventNames.WORKER_READY).toBe('worker.ready');
    expect(EventNames.WORKER_SHUTDOWN_STARTED).toBe('worker.shutdown.started');
    expect(EventNames.WORKER_STOPPED).toBe('worker.stopped');
    expect(EventNames.WORKER_ERROR).toBe('worker.error');
  });

  it('exports all expected poller event names', () => {
    expect(EventNames.POLLER_STARTED).toBe('poller.started');
    expect(EventNames.POLLER_STOPPED).toBe('poller.stopped');
    expect(EventNames.POLLER_CYCLE_COMPLETE).toBe('poller.cycle.complete');
    expect(EventNames.POLLER_STARVATION_DETECTED).toBe('poller.starvation.detected');
    expect(EventNames.POLLER_ERROR).toBe('poller.error');
  });

  it('exports all expected metrics event names', () => {
    expect(EventNames.METRICS_UPDATED).toBe('metrics.updated');
    expect(EventNames.METRICS_ERROR).toBe('metrics.error');
  });
});

// Test helpers

function createMockFfiStepEvent(): FfiStepEvent {
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

function createMockStepExecutionResult(): StepExecutionResult {
  return {
    step_uuid: 'step-789',
    success: true,
    result: { data: 'test' },
    metadata: {
      execution_time_ms: 150,
      worker_id: 'worker-123',
      handler_name: 'TestHandler',
    },
    status: 'completed',
  };
}
