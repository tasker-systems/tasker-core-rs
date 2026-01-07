/**
 * Event emitter for TypeScript workers.
 *
 * Provides a type-safe event emitter wrapper that works across
 * all supported runtimes (Bun, Node.js, Deno).
 */

import { EventEmitter } from 'eventemitter3';
import type { FfiDispatchMetrics, FfiStepEvent, StepExecutionResult } from '../ffi/types.js';
import {
  MetricsEventNames,
  PollerEventNames,
  StepEventNames,
  WorkerEventNames,
} from './event-names.js';

/**
 * Event payload types
 */
export interface StepExecutionReceivedPayload {
  event: FfiStepEvent;
  receivedAt: Date;
}

export interface StepExecutionStartedPayload {
  eventId: string;
  stepUuid: string;
  taskUuid: string;
  handlerName: string;
  startedAt: Date;
}

export interface StepExecutionCompletedPayload {
  eventId: string;
  stepUuid: string;
  taskUuid: string;
  result: StepExecutionResult;
  executionTimeMs: number;
  completedAt: Date;
}

export interface StepExecutionFailedPayload {
  eventId: string;
  stepUuid: string;
  taskUuid: string;
  error: Error;
  failedAt: Date;
}

export interface StepCompletionSentPayload {
  eventId: string;
  stepUuid: string;
  success: boolean;
  sentAt: Date;
}

/** TAS-125: Checkpoint yield event payload */
export interface StepCheckpointYieldSentPayload {
  eventId: string;
  stepUuid: string;
  cursor: unknown;
  itemsProcessed: number;
  timestamp: Date;
}

export interface WorkerEventPayload {
  workerId?: string;
  timestamp: Date;
  message?: string;
}

export interface WorkerErrorPayload {
  error: Error;
  timestamp: Date;
  context?: Record<string, unknown>;
}

export interface PollerCyclePayload {
  eventsProcessed: number;
  cycleNumber: number;
  timestamp: Date;
}

export interface MetricsPayload {
  metrics: FfiDispatchMetrics;
  timestamp: Date;
}

/**
 * Event map for type-safe event handling
 */
export interface TaskerEventMap {
  // Step events
  'step.execution.received': StepExecutionReceivedPayload;
  'step.execution.started': StepExecutionStartedPayload;
  'step.execution.completed': StepExecutionCompletedPayload;
  'step.execution.failed': StepExecutionFailedPayload;
  'step.completion.sent': StepCompletionSentPayload;
  'step.execution.timeout': StepExecutionFailedPayload;
  // TAS-125: Checkpoint yield event
  'step.checkpoint_yield.sent': StepCheckpointYieldSentPayload;

  // Worker events
  'worker.started': WorkerEventPayload;
  'worker.ready': WorkerEventPayload;
  'worker.shutdown.started': WorkerEventPayload;
  'worker.stopped': WorkerEventPayload;
  'worker.error': WorkerErrorPayload;

  // Poller events
  'poller.started': WorkerEventPayload;
  'poller.stopped': WorkerEventPayload;
  'poller.cycle.complete': PollerCyclePayload;
  'poller.starvation.detected': MetricsPayload;
  'poller.error': WorkerErrorPayload;

  // Metrics events
  'metrics.updated': MetricsPayload;
  'metrics.error': WorkerErrorPayload;
}

/**
 * Type-safe event emitter for Tasker events
 */
export class TaskerEventEmitter extends EventEmitter<TaskerEventMap> {
  private readonly instanceId: string;

  constructor() {
    super();
    this.instanceId = crypto.randomUUID();
  }

  /**
   * Get the unique instance ID for this emitter
   */
  getInstanceId(): string {
    return this.instanceId;
  }

  /**
   * Emit a step execution received event
   */
  emitStepReceived(event: FfiStepEvent): void {
    this.emit(StepEventNames.STEP_EXECUTION_RECEIVED, {
      event,
      receivedAt: new Date(),
    });
  }

  /**
   * Emit a step execution started event
   */
  emitStepStarted(eventId: string, stepUuid: string, taskUuid: string, handlerName: string): void {
    this.emit(StepEventNames.STEP_EXECUTION_STARTED, {
      eventId,
      stepUuid,
      taskUuid,
      handlerName,
      startedAt: new Date(),
    });
  }

  /**
   * Emit a step execution completed event
   */
  emitStepCompleted(
    eventId: string,
    stepUuid: string,
    taskUuid: string,
    result: StepExecutionResult,
    executionTimeMs: number
  ): void {
    this.emit(StepEventNames.STEP_EXECUTION_COMPLETED, {
      eventId,
      stepUuid,
      taskUuid,
      result,
      executionTimeMs,
      completedAt: new Date(),
    });
  }

  /**
   * Emit a step execution failed event
   */
  emitStepFailed(eventId: string, stepUuid: string, taskUuid: string, error: Error): void {
    this.emit(StepEventNames.STEP_EXECUTION_FAILED, {
      eventId,
      stepUuid,
      taskUuid,
      error,
      failedAt: new Date(),
    });
  }

  /**
   * Emit a step completion sent event
   */
  emitCompletionSent(eventId: string, stepUuid: string, success: boolean): void {
    this.emit(StepEventNames.STEP_COMPLETION_SENT, {
      eventId,
      stepUuid,
      success,
      sentAt: new Date(),
    });
  }

  /**
   * Emit a worker started event
   */
  emitWorkerStarted(workerId?: string): void {
    this.emit(WorkerEventNames.WORKER_STARTED, {
      workerId,
      timestamp: new Date(),
      message: 'Worker started',
    });
  }

  /**
   * Emit a worker ready event
   */
  emitWorkerReady(workerId?: string): void {
    this.emit(WorkerEventNames.WORKER_READY, {
      workerId,
      timestamp: new Date(),
      message: 'Worker ready to process events',
    });
  }

  /**
   * Emit a worker shutdown started event
   */
  emitWorkerShutdownStarted(workerId?: string): void {
    this.emit(WorkerEventNames.WORKER_SHUTDOWN_STARTED, {
      workerId,
      timestamp: new Date(),
      message: 'Graceful shutdown initiated',
    });
  }

  /**
   * Emit a worker stopped event
   */
  emitWorkerStopped(workerId?: string): void {
    this.emit(WorkerEventNames.WORKER_STOPPED, {
      workerId,
      timestamp: new Date(),
      message: 'Worker stopped',
    });
  }

  /**
   * Emit a worker error event
   */
  emitWorkerError(error: Error, context?: Record<string, unknown>): void {
    this.emit(WorkerEventNames.WORKER_ERROR, {
      error,
      timestamp: new Date(),
      context,
    });
  }

  /**
   * Emit a metrics updated event
   */
  emitMetricsUpdated(metrics: FfiDispatchMetrics): void {
    this.emit(MetricsEventNames.METRICS_UPDATED, {
      metrics,
      timestamp: new Date(),
    });
  }

  /**
   * Emit a starvation detected event
   */
  emitStarvationDetected(metrics: FfiDispatchMetrics): void {
    this.emit(PollerEventNames.POLLER_STARVATION_DETECTED, {
      metrics,
      timestamp: new Date(),
    });
  }
}
