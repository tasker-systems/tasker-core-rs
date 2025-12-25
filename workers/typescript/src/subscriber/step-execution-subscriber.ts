/**
 * Step execution subscriber for TypeScript workers.
 *
 * Subscribes to step execution events from the EventPoller and dispatches
 * them to the appropriate handlers via the HandlerRegistry.
 *
 * Matches Python's StepExecutionSubscriber pattern (TAS-92 aligned).
 */

import type { StepExecutionReceivedPayload, TaskerEventEmitter } from '../events/event-emitter.js';
import { StepEventNames } from '../events/event-names.js';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { FfiStepEvent, StepExecutionResult } from '../ffi/types.js';
import type { StepHandler } from '../handler/base.js';
import { logDebug, logError, logInfo, logWarn } from '../logging/index.js';
import { StepContext } from '../types/step-context.js';
import type { StepHandlerResult } from '../types/step-handler-result.js';

/**
 * Interface for handler registry required by StepExecutionSubscriber.
 */
export interface HandlerRegistryInterface {
  /** Resolve and instantiate a handler by name */
  resolve(name: string): StepHandler | null;
}

/**
 * Configuration for the step execution subscriber.
 */
export interface StepExecutionSubscriberConfig {
  /** Worker ID for result attribution */
  workerId?: string;

  /** Maximum concurrent handler executions (default: 10) */
  maxConcurrent?: number;

  /** Handler execution timeout in milliseconds (default: 300000 = 5 minutes) */
  handlerTimeoutMs?: number;
}

/**
 * Subscribes to step execution events and dispatches them to handlers.
 *
 * This is the critical component that connects the FFI event stream
 * to TypeScript handler execution. It:
 * 1. Listens for step events from the EventPoller via EventEmitter
 * 2. Resolves the appropriate handler from the HandlerRegistry
 * 3. Creates a StepContext from the FFI event
 * 4. Executes the handler
 * 5. Submits the result back to Rust via FFI
 *
 * @example
 * ```typescript
 * const subscriber = new StepExecutionSubscriber(
 *   eventEmitter,
 *   handlerRegistry,
 *   runtime,
 *   { workerId: 'worker-1' }
 * );
 *
 * subscriber.start();
 *
 * // Later...
 * subscriber.stop();
 * ```
 */
export class StepExecutionSubscriber {
  private readonly emitter: TaskerEventEmitter;
  private readonly registry: HandlerRegistryInterface;
  private readonly runtime: TaskerRuntime;
  private readonly workerId: string;
  private readonly maxConcurrent: number;
  private readonly handlerTimeoutMs: number;

  private running = false;
  private activeHandlers = 0;
  private processedCount = 0;
  private errorCount = 0;

  /**
   * Create a new StepExecutionSubscriber.
   *
   * @param emitter - The event emitter to subscribe to (required, no fallback)
   * @param registry - The handler registry for resolving step handlers
   * @param runtime - The FFI runtime for submitting results (required, no fallback)
   * @param config - Optional configuration for execution behavior
   */
  constructor(
    emitter: TaskerEventEmitter,
    registry: HandlerRegistryInterface,
    runtime: TaskerRuntime,
    config: StepExecutionSubscriberConfig = {}
  ) {
    this.emitter = emitter;
    this.registry = registry;
    this.runtime = runtime;
    this.workerId = config.workerId ?? `typescript-worker-${process.pid}`;
    this.maxConcurrent = config.maxConcurrent ?? 10;
    this.handlerTimeoutMs = config.handlerTimeoutMs ?? 300000;
  }

  /**
   * Start subscribing to step execution events.
   */
  start(): void {
    if (this.running) {
      logWarn('StepExecutionSubscriber already running', {
        component: 'subscriber',
      });
      return;
    }

    this.running = true;
    this.processedCount = 0;
    this.errorCount = 0;

    // Subscribe to step events
    this.emitter.on(
      StepEventNames.STEP_EXECUTION_RECEIVED,
      (payload: StepExecutionReceivedPayload) => {
        // Extract the event from the payload wrapper
        this.handleEvent(payload.event);
      }
    );

    logInfo('StepExecutionSubscriber started', {
      component: 'subscriber',
      operation: 'start',
      worker_id: this.workerId,
    });
  }

  /**
   * Stop subscribing to step execution events.
   *
   * Note: Does not wait for in-flight handlers to complete.
   * Use waitForCompletion() if you need to wait.
   */
  stop(): void {
    if (!this.running) {
      return;
    }

    this.running = false;
    this.emitter.removeAllListeners(StepEventNames.STEP_EXECUTION_RECEIVED);

    logInfo('StepExecutionSubscriber stopped', {
      component: 'subscriber',
      operation: 'stop',
      processed_count: String(this.processedCount),
      error_count: String(this.errorCount),
    });
  }

  /**
   * Check if the subscriber is running.
   */
  isRunning(): boolean {
    return this.running;
  }

  /**
   * Get the count of events processed.
   */
  getProcessedCount(): number {
    return this.processedCount;
  }

  /**
   * Get the count of errors encountered.
   */
  getErrorCount(): number {
    return this.errorCount;
  }

  /**
   * Get the count of currently active handlers.
   */
  getActiveHandlers(): number {
    return this.activeHandlers;
  }

  /**
   * Wait for all active handlers to complete.
   *
   * @param timeoutMs - Maximum time to wait (default: 30000)
   * @returns True if all handlers completed, false if timeout
   */
  async waitForCompletion(timeoutMs = 30000): Promise<boolean> {
    const startTime = Date.now();
    const checkInterval = 100;

    while (this.activeHandlers > 0) {
      if (Date.now() - startTime > timeoutMs) {
        logWarn('Timeout waiting for handlers to complete', {
          component: 'subscriber',
          active_handlers: String(this.activeHandlers),
        });
        return false;
      }
      await new Promise((resolve) => setTimeout(resolve, checkInterval));
    }

    return true;
  }

  /**
   * Handle a step execution event.
   */
  private handleEvent(event: FfiStepEvent): void {
    if (!this.running) {
      logWarn('Received event while stopped, ignoring', {
        component: 'subscriber',
        event_id: event.event_id,
      });
      return;
    }

    // Check concurrency limit
    if (this.activeHandlers >= this.maxConcurrent) {
      logWarn('Max concurrent handlers reached, event will be re-polled', {
        component: 'subscriber',
        active_handlers: String(this.activeHandlers),
        max_concurrent: String(this.maxConcurrent),
      });
      // Don't process - event stays in FFI queue and will be re-polled
      return;
    }

    // Process asynchronously
    this.processEvent(event).catch((error) => {
      logError('Unhandled error processing event', {
        component: 'subscriber',
        event_id: event.event_id,
        error_message: error instanceof Error ? error.message : String(error),
      });
    });
  }

  /**
   * Process a step execution event.
   */
  private async processEvent(event: FfiStepEvent): Promise<void> {
    this.activeHandlers++;
    const startTime = Date.now();

    try {
      // Extract handler name from step definition
      const handlerName = this.extractHandlerName(event);
      if (!handlerName) {
        await this.submitErrorResult(event, 'No handler name found in step definition', startTime);
        return;
      }

      logDebug('Processing step event', {
        component: 'subscriber',
        event_id: event.event_id,
        step_uuid: event.step_uuid,
        handler_name: handlerName,
      });

      // Emit started event
      this.emitter.emit(StepEventNames.STEP_EXECUTION_STARTED, {
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        handlerName,
        timestamp: new Date(),
      });

      // Resolve handler from registry
      const handler = this.registry.resolve(handlerName);
      if (!handler) {
        await this.submitErrorResult(event, `Handler not found: ${handlerName}`, startTime);
        return;
      }

      // Create context from FFI event
      const context = StepContext.fromFfiEvent(event, handlerName);

      // Execute handler with timeout
      const result = await this.executeWithTimeout(
        () => handler.call(context),
        this.handlerTimeoutMs
      );

      const executionTimeMs = Date.now() - startTime;

      // Submit result to Rust
      await this.submitResult(event, result, executionTimeMs);

      // Emit completed/failed event
      if (result.success) {
        this.emitter.emit(StepEventNames.STEP_EXECUTION_COMPLETED, {
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          handlerName,
          executionTimeMs,
          timestamp: new Date(),
        });
      } else {
        this.emitter.emit(StepEventNames.STEP_EXECUTION_FAILED, {
          eventId: event.event_id,
          stepUuid: event.step_uuid,
          handlerName,
          error: result.errorMessage,
          executionTimeMs,
          timestamp: new Date(),
        });
      }

      this.processedCount++;
    } catch (error) {
      this.errorCount++;
      const errorMessage = error instanceof Error ? error.message : String(error);

      logError('Handler execution failed', {
        component: 'subscriber',
        event_id: event.event_id,
        step_uuid: event.step_uuid,
        error_message: errorMessage,
      });

      await this.submitErrorResult(event, errorMessage, startTime);

      this.emitter.emit(StepEventNames.STEP_EXECUTION_FAILED, {
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        error: errorMessage,
        executionTimeMs: Date.now() - startTime,
        timestamp: new Date(),
      });
    } finally {
      this.activeHandlers--;
    }
  }

  /**
   * Execute a function with a timeout.
   */
  private async executeWithTimeout<T>(fn: () => Promise<T>, timeoutMs: number): Promise<T> {
    return Promise.race([
      fn(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error(`Handler execution timed out after ${timeoutMs}ms`)),
          timeoutMs
        )
      ),
    ]);
  }

  /**
   * Extract handler name from FFI event.
   *
   * The handler name is in step_definition.handler.callable
   */
  private extractHandlerName(event: FfiStepEvent): string | null {
    const stepDefinition = event.step_definition;
    if (!stepDefinition) {
      return null;
    }

    const handler = stepDefinition.handler;
    if (!handler) {
      return null;
    }

    return handler.callable || null;
  }

  /**
   * Submit a success result via FFI.
   */
  private async submitResult(
    event: FfiStepEvent,
    result: StepHandlerResult,
    executionTimeMs: number
  ): Promise<void> {
    if (!this.runtime.isLoaded) {
      logError('Cannot submit result: runtime not available', {
        component: 'subscriber',
        event_id: event.event_id,
      });
      return;
    }

    // Build the execution result, only adding error if not successful
    const executionResult: StepExecutionResult = {
      step_uuid: event.step_uuid,
      success: result.success,
      result: result.result ?? {},
      metadata: {
        execution_time_ms: executionTimeMs,
        worker_id: this.workerId,
        handler_name: this.extractHandlerName(event) ?? 'unknown',
        attempt_number: event.workflow_step?.attempts ?? 1,
        ...result.metadata,
      },
      status: result.success ? 'completed' : 'failed',
    };

    // Only add error field when not successful
    if (!result.success) {
      executionResult.error = {
        message: result.errorMessage ?? 'Unknown error',
        error_type: result.errorType ?? 'handler_error',
        retryable: result.retryable,
        status_code: null,
        backtrace: null,
      };
    }

    try {
      this.runtime.completeStepEvent(event.event_id, executionResult);

      this.emitter.emit(StepEventNames.STEP_COMPLETION_SENT, {
        eventId: event.event_id,
        stepUuid: event.step_uuid,
        success: result.success,
        timestamp: new Date(),
      });

      logDebug('Step result submitted', {
        component: 'subscriber',
        event_id: event.event_id,
        step_uuid: event.step_uuid,
        success: String(result.success),
        execution_time_ms: String(executionTimeMs),
      });
    } catch (error) {
      logError('Failed to submit step result', {
        component: 'subscriber',
        event_id: event.event_id,
        error_message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Submit an error result via FFI.
   */
  private async submitErrorResult(
    event: FfiStepEvent,
    errorMessage: string,
    startTime: number
  ): Promise<void> {
    if (!this.runtime.isLoaded) {
      logError('Cannot submit error result: runtime not available', {
        component: 'subscriber',
        event_id: event.event_id,
      });
      return;
    }

    const executionTimeMs = Date.now() - startTime;

    const executionResult: StepExecutionResult = {
      step_uuid: event.step_uuid,
      success: false,
      result: {},
      metadata: {
        execution_time_ms: executionTimeMs,
        worker_id: this.workerId,
      },
      status: 'error',
      error: {
        message: errorMessage,
        error_type: 'handler_error',
        retryable: true,
        status_code: null,
        backtrace: null,
      },
    };

    try {
      this.runtime.completeStepEvent(event.event_id, executionResult);
      this.errorCount++;

      logDebug('Error result submitted', {
        component: 'subscriber',
        event_id: event.event_id,
        step_uuid: event.step_uuid,
        error_message: errorMessage,
      });
    } catch (error) {
      logError('Failed to submit error result', {
        component: 'subscriber',
        event_id: event.event_id,
        error_message: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
